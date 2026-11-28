//! A server and devices in one process: the real axum router driven
//! through `tower::oneshot`, real SQLite files in a temporary folder, and
//! a clock every party shares (and the tests can move).
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use housewright_core::blobs::{BlobStore, MemoryBlobs};
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::records::NewFile;
use housewright_core::store::{ManualClock, Store, WallClock};
use housewright_server::{AppState, Config};
use housewright_sync::{HttpRequest, HttpResponse, Invitee, PairState, SyncClient, Transport};
use http_body_util::BodyExt;
use tower::ServiceExt;

pub const URL: &str = "http://127.0.0.1:1";

pub struct Lab {
    pub quota: std::sync::atomic::AtomicU64,
    pub dir: PathBuf,
    pub rt: Arc<tokio::runtime::Runtime>,
    pub app: Arc<Mutex<Router>>,
    pub clock: ManualClock,
    pub log: Arc<Mutex<Vec<HttpRequest>>>,
}

impl Lab {
    pub fn new(name: &str) -> Lab {
        let dir = std::env::temp_dir().join(format!("hw-sync-{name}-{}", uuid::Uuid::now_v7()));
        let clock = ManualClock::new(1_800_000_000_000);
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );
        let lab = Lab {
            quota: std::sync::atomic::AtomicU64::new(u64::MAX),
            dir,
            rt,
            app: Arc::new(Mutex::new(Router::new())),
            clock,
            log: Default::default(),
        };
        lab.restart();
        lab
    }

    /// Stop the server (dropping its database handle) and start it again
    /// on the same files.
    pub fn restart(&self) {
        *self.app.lock().unwrap() = Router::new();
        let mut cfg = Config::new(&self.dir);
        let c = self.clock.clone();
        cfg.clock = Arc::new(move || c.now_ms());
        let q = self.quota.load(Ordering::SeqCst);
        if q != u64::MAX {
            cfg.quota_bytes = q;
        }
        *self.app.lock().unwrap() = housewright_server::router(AppState::open(cfg).unwrap());
    }

    pub fn transport(&self, online: Arc<AtomicBool>) -> LabTransport {
        LabTransport {
            rt: self.rt.clone(),
            app: self.app.clone(),
            online,
            fail_after: Arc::new(AtomicUsize::new(usize::MAX)),
            max_response: Arc::new(AtomicUsize::new(usize::MAX)),
            log: self.log.clone(),
        }
    }

    pub fn device(&self, name: &str) -> Device {
        self.device_with_clock(name, self.clock.clone())
    }

    pub fn device_with_clock(&self, name: &str, clock: ManualClock) -> Device {
        let online = Arc::new(AtomicBool::new(true));
        let t = self.transport(online.clone());
        let fail_after = t.fail_after.clone();
        let max_response = t.max_response.clone();
        Device {
            name: name.into(),
            store: Store::open_in_memory(Box::new(clock.clone())).unwrap(),
            blobs: MemoryBlobs::default(),
            client: SyncClient::new(Box::new(t)),
            online,
            fail_after,
            max_response,
            clock,
        }
    }

    /// Every byte the server keeps on disk.
    pub fn server_bytes(&self) -> Vec<u8> {
        let mut all = Vec::new();
        for f in housewright_server::data_files(&self.dir) {
            all.extend(std::fs::read(f).unwrap_or_default());
        }
        all
    }

    pub fn server_db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.dir.join("housewright-server.db")).unwrap()
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub struct LabTransport {
    rt: Arc<tokio::runtime::Runtime>,
    app: Arc<Mutex<Router>>,
    online: Arc<AtomicBool>,
    /// Fail (as if the connection dropped) after this many more requests.
    pub fail_after: Arc<AtomicUsize>,
    /// Refuse answers bigger than this (a small phone's limit).
    pub max_response: Arc<AtomicUsize>,
    log: Arc<Mutex<Vec<HttpRequest>>>,
}

impl Transport for LabTransport {
    fn send(&mut self, req: HttpRequest) -> Result<HttpResponse, String> {
        if !self.online.load(Ordering::SeqCst) {
            return Err("offline".into());
        }
        let left = self.fail_after.load(Ordering::SeqCst);
        if left == 0 {
            return Err("connection reset".into());
        }
        if left != usize::MAX {
            self.fail_after.store(left - 1, Ordering::SeqCst);
        }
        self.log.lock().unwrap().push(req.clone());
        let r = send_to(&self.rt, &self.app.lock().unwrap().clone(), req)?;
        if r.body.len() > self.max_response.load(Ordering::SeqCst) {
            return Err("response too large".into());
        }
        Ok(r)
    }
}

pub fn send_to(
    rt: &tokio::runtime::Runtime,
    app: &Router,
    req: HttpRequest,
) -> Result<HttpResponse, String> {
    let path = req.url.strip_prefix(URL).unwrap_or(&req.url).to_string();
    let mut b = axum::http::Request::builder()
        .method(req.method.as_str())
        .uri(path);
    for (k, v) in &req.headers {
        b = b.header(k, v);
    }
    let request = b.body(axum::body::Body::from(req.body)).unwrap();
    rt.block_on(async {
        // The router's error type is Infallible, so this pattern is irrefutable.
        let Ok(res) = app.clone().oneshot(request).await;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = res
            .into_body()
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes()
            .to_vec();
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    })
}

pub struct Device {
    pub name: String,
    pub store: Store,
    pub blobs: MemoryBlobs,
    pub client: SyncClient,
    pub online: Arc<AtomicBool>,
    pub fail_after: Arc<AtomicUsize>,
    pub max_response: Arc<AtomicUsize>,
    pub clock: ManualClock,
}

impl Device {
    pub fn init(&mut self, owner: &str) -> Member {
        let (_, m) = self
            .store
            .init_household("Our home", owner, "America/New_York")
            .unwrap();
        m
    }

    pub fn setup(&mut self) -> String {
        self.client
            .setup(
                &mut self.store,
                URL,
                &format!("{}'s phone", self.name),
                None,
            )
            .unwrap()
            .to_string()
    }

    pub fn sync(&mut self) -> housewright_sync::SyncReport {
        self.client.sync(&mut self.store, &mut self.blobs).unwrap()
    }

    pub fn try_sync(&mut self) -> housewright_sync::Result<housewright_sync::SyncReport> {
        self.client.sync(&mut self.store, &mut self.blobs)
    }

    pub fn set_online(&self, on: bool) {
        self.online.store(on, Ordering::SeqCst);
    }

    pub fn room(&mut self, id: u128, name: &str) {
        let r = Room {
            outline: None,
            id: RoomId(uuid::Uuid::from_u128(id)),
            property: PropertyId(uuid::Uuid::from_u128(7)),
            floor: None,
            name: name.into(),
        };
        self.store.put(&r).unwrap();
    }

    /// Like `room`, but editing a deleted room is allowed to fail.
    pub fn try_room(&mut self, id: u128, name: &str) {
        let r = Room {
            outline: None,
            id: RoomId(uuid::Uuid::from_u128(id)),
            property: PropertyId(uuid::Uuid::from_u128(7)),
            floor: None,
            name: name.into(),
        };
        let _ = self.store.put(&r);
    }

    pub fn room_name(&self, id: u128) -> Option<String> {
        self.store
            .get::<Room>(&RoomId(uuid::Uuid::from_u128(id)).to_string())
            .unwrap()
            .map(|r| r.name)
    }

    pub fn attach_photo(&mut self, bytes: &[u8]) -> Attachment {
        let r = Room {
            outline: None,
            id: RoomId(uuid::Uuid::from_u128(99)),
            property: PropertyId(uuid::Uuid::from_u128(7)),
            floor: None,
            name: "Utility".into(),
        };
        self.store.put(&r).unwrap();
        self.store
            .add_attachment(
                &mut self.blobs,
                NewFile {
                    about: EntityRef {
                        kind: EntityKind::Room,
                        id: r.id.to_string(),
                    },
                    kind: AttachmentKind::Photo,
                    name: "boiler.jpg".into(),
                    media_type: "image/jpeg".into(),
                    bytes,
                    thumb: None,
                    width: None,
                    height: None,
                    sensitivity: Sensitivity::Normal,
                    source: None,
                },
            )
            .unwrap()
    }

    pub fn entities(
        &self,
    ) -> std::collections::BTreeMap<EntityKind, Vec<housewright_core::store::RawEntity>> {
        self.store.snapshot().unwrap().entities
    }
}

/// Pair `new` into `owner`'s household; both people say the numbers match.
pub fn pair(owner: &mut Device, new: &mut Device, who: Invitee) {
    let inv = owner.client.invite_start(&mut owner.store, who).unwrap();
    assert_eq!(
        owner.client.invite_poll(&owner.store).unwrap(),
        PairState::Waiting
    );
    new.client
        .join_start(&new.store, URL, &inv.code, &format!("{}'s phone", new.name))
        .unwrap();
    let a = owner.client.invite_poll(&owner.store).unwrap();
    let b = new
        .client
        .join_poll(&mut new.store, &mut new.blobs)
        .unwrap();
    let (
        PairState::Compare {
            safety_number: na, ..
        },
        PairState::Compare {
            safety_number: nb, ..
        },
    ) = (&a, &b)
    else {
        panic!("expected numbers to compare, got {a:?} / {b:?}");
    };
    assert_eq!(na, nb, "honest devices show the same number");
    assert_eq!(
        new.client
            .join_confirm(&mut new.store, &mut new.blobs, true)
            .unwrap(),
        PairState::WaitingForOther
    );
    assert_eq!(
        owner
            .client
            .invite_confirm(&mut owner.store, &mut owner.blobs, true)
            .unwrap(),
        PairState::Done
    );
    assert_eq!(
        new.client
            .join_poll(&mut new.store, &mut new.blobs)
            .unwrap(),
        PairState::Done
    );
}

pub fn has_blob(d: &Device, hash: &str) -> bool {
    d.blobs.has(hash).unwrap()
}
