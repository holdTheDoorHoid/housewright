//! The sync engine: set up, sync, devices, removal, recovery.

use std::collections::BTreeSet;

use housewright_core::blobs::{BlobStore, referenced};
use housewright_core::ids::MemberId;
use housewright_core::model::{Attachment, Member, Role};
use housewright_core::oplog::Op;
use housewright_core::store::{MAX_CLOCK_AHEAD_MS, Store};
use housewright_crypto::batch::{SealedBatch, new_batch_id, open_batch, seal_batch};
use housewright_crypto::blob::{blob_name, open_blob, seal_blob};
use housewright_crypto::recovery::{Anchor, RecoveryKey};
use housewright_crypto::request::{H_SERVER_TIME, sign_request};
use housewright_crypto::roster::{
    DeviceRole, RemovedEntry, RosterBody, RosterEntry, SignedRoster, open_label, seal_label,
    verify_chain, verify_next,
};
use housewright_crypto::wire::*;
use housewright_crypto::wrap::{KeyEnvelope, unwrap, wrap};
use housewright_crypto::{DeviceId, DevicePublic, DeviceSecret, KeyChain, SpaceId};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;

use crate::state::{self, Pending, State, SyncConfig, SyncStatus};
use crate::{HttpRequest, HttpResponse, Result, SyncError, Transport, check_server_url};

/// Operations per pushed batch, and a soft size cap for its plaintext.
const BATCH_OPS: usize = 400;
const BATCH_BYTES: usize = 512 * 1024;
/// Files larger than this are fetched when opened, not during sync.
const PREFETCH_MAX: u64 = 4 * 1024 * 1024;

/// What one sync did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub pushed_ops: usize,
    pub pulled_ops: usize,
    pub rejected_batches: usize,
    pub refused_future_ops: usize,
    pub files_up: usize,
    pub files_down: usize,
}

/// A device in the household, for the devices list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub member: String,
    pub member_name: String,
    pub role: DeviceRole,
    pub this_device: bool,
    pub added_ms: Option<u64>,
    pub last_seen_ms: Option<u64>,
}

/// The client. Holds the transport and anything that lives only in memory
/// (a pairing in progress, the measured clock offset).
pub struct SyncClient {
    transport: Box<dyn Transport>,
    /// Server time minus local time, learned from responses.
    offset_ms: i64,
    pub(crate) invite: Option<crate::pairing::InviteSession>,
    pub(crate) join: Option<crate::pairing::JoinSession>,
}

pub(crate) fn now_ms(store: &Store) -> u64 {
    store.now().as_millisecond().max(0) as u64
}

fn to_json<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).expect("protocol types serialise")
}

pub(crate) fn from_json<T: DeserializeOwned>(r: &HttpResponse) -> Result<T> {
    serde_json::from_slice(&r.body)
        .map_err(|e| SyncError::Invalid(format!("the server sent something unexpected: {e}")))
}

pub(crate) fn role_of(r: Role) -> DeviceRole {
    match r {
        Role::Owner => DeviceRole::Owner,
        Role::Member => DeviceRole::Member,
        Role::Limited => DeviceRole::Limited,
    }
}

impl SyncClient {
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            offset_ms: 0,
            invite: None,
            join: None,
        }
    }

    pub fn is_set_up(store: &Store) -> Result<bool> {
        state::is_set_up(store)
    }

    /// One HTTP exchange; signs it when `signer` is given. Retries once
    /// with a corrected clock if the server says ours is off.
    pub(crate) fn call(
        &mut self,
        store: &Store,
        server: &str,
        signer: Option<&DeviceSecret>,
        method: &str,
        path: &str,
        body: Vec<u8>,
    ) -> Result<HttpResponse> {
        for attempt in 0..2 {
            let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
            if let Some(s) = signer {
                let t = (now_ms(store) as i64 + self.offset_ms).max(0) as u64;
                let auth = sign_request(s, method, path, t, &body)?;
                headers.extend(auth.headers().map(|(k, v)| (k.to_string(), v)));
            }
            let res = self
                .transport
                .send(HttpRequest {
                    method: method.to_string(),
                    url: format!("{server}{path}"),
                    headers,
                    body: body.clone(),
                })
                .map_err(SyncError::Offline)?;
            if let Some(t) = res
                .header(H_SERVER_TIME)
                .and_then(|v| v.parse::<u64>().ok())
            {
                self.offset_ms = t as i64 - now_ms(store) as i64;
            }
            if (200..300).contains(&res.status) {
                return Ok(res);
            }
            let err = SyncError::from_response(&res);
            if let SyncError::Server { code, .. } = &err
                && code == "clock_skew"
                && attempt == 0
            {
                continue;
            }
            return Err(err);
        }
        unreachable!("the loop returns")
    }

    /// The measured clock difference, when it is worth telling anyone.
    fn clock_offset(&self) -> Option<i64> {
        (self.offset_ms.unsigned_abs() > 60_000).then_some(self.offset_ms)
    }

    // -- status -----------------------------------------------------------

    pub fn config(store: &Store) -> Result<Option<SyncConfig>> {
        match store.sync_get("config")? {
            Some(v) => Ok(Some(
                serde_json::from_str(&v).map_err(|e| SyncError::Invalid(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    pub fn status(&self, store: &Store) -> Result<SyncStatus> {
        let mut s = state::load_status(store)?;
        if s.clock_offset_ms.is_none() {
            s.clock_offset_ms = self.clock_offset();
        }
        Ok(s)
    }

    /// Local operations still waiting to be sent.
    pub fn pending_count(store: &Store) -> Result<u64> {
        let cursor: i64 = store
            .sync_get("push_cursor")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(store.unsynced_count(cursor)?)
    }

    pub fn confirm_recovery_saved(store: &Store) -> Result<()> {
        let mut s = state::load_status(store)?;
        s.recovery_confirmed = true;
        state::save_status(store, &s)
    }

    /// "Stop syncing on this device": forget the keys and settings here.
    /// The household's data stays on the device.
    pub fn forget(&mut self, store: &Store) -> Result<()> {
        self.invite = None;
        self.join = None;
        store.sync_forget()?;
        Ok(())
    }

    // -- setting up -------------------------------------------------------

    /// Check that an address is a Housewright server.
    pub fn health(&mut self, store: &Store, server: &str) -> Result<Health> {
        let server = check_server_url(server)?;
        let r = self.call(store, &server, None, "GET", "/v1/health", vec![])?;
        from_json(&r).map_err(|_| {
            SyncError::Invalid(
                "That address answered, but it is not a Housewright sync server.".into(),
            )
        })
    }

    /// Turn sync on for this household (the first device). Returns the
    /// recovery key, which is not stored anywhere and must be shown now.
    pub fn setup(
        &mut self,
        store: &mut Store,
        server: &str,
        device_name: &str,
        registration_token: Option<&str>,
    ) -> Result<Zeroizing<String>> {
        if state::is_set_up(store)? {
            return Err(SyncError::Invalid(
                "Sync is already set up on this device.".into(),
            ));
        }
        let server = check_server_url(server)?;
        let me_member = store.author()?;
        let member: Member = store.fetch(&me_member.to_string())?;
        if member.role != Role::Owner {
            return Err(SyncError::Invalid(
                "Only the household's owner can set up sync.".into(),
            ));
        }
        self.health(store, &server)?;
        let secret = DeviceSecret::generate()?;
        let recovery = RecoveryKey::generate()?;
        let rec = recovery.identity();
        let space = SpaceId::random()?;
        let chain = KeyChain::generate()?;
        let me = secret.public();
        let mut body = RosterBody {
            v: 1,
            space,
            version: 1,
            epoch: 0,
            prev: "0".repeat(64),
            genesis: String::new(),
            devices: vec![
                RosterEntry {
                    device: me.clone(),
                    member: me_member.to_string(),
                    role: DeviceRole::Owner,
                    added_in: 1,
                    label: seal_label(&chain, space, me.id, device_name)?,
                },
                RosterEntry {
                    device: rec.public(),
                    member: me_member.to_string(),
                    role: DeviceRole::Recovery,
                    added_in: 1,
                    label: seal_label(&chain, space, rec.id(), "Recovery key")?,
                },
            ],
            removed: vec![],
            made_ms: now_ms(store),
        };
        body.sort();
        let roster = SignedRoster::sign(&body, &secret)?;
        let envelopes = vec![
            wrap(&secret, space, &me, &chain)?,
            wrap(&secret, space, &rec.public(), &chain)?,
        ];
        let anchor = recovery.seal_anchor(
            space,
            &Anchor {
                genesis: roster.hash(),
            },
        )?;
        let req = CreateSpace {
            space,
            roster: roster.clone(),
            envelopes,
            anchor,
            registration_token: registration_token
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        };
        self.call(
            store,
            &server,
            Some(&secret),
            "POST",
            "/v1/spaces",
            to_json(&req),
        )?;
        let st = State {
            config: SyncConfig {
                server,
                space,
                device: me.id,
                member: me_member.to_string(),
                role: DeviceRole::Owner,
            },
            secret,
            chain,
            rosters: vec![roster],
            bodies: vec![body],
            cursor: 0,
            push_cursor: 0,
            next_device_seq: 1,
            pending: None,
            seen: Default::default(),
            status: SyncStatus::default(),
        };
        st.save(store)?;
        Ok(recovery.display())
    }

    // -- syncing ----------------------------------------------------------

    /// Push, pull and move files. Safe to call at any time, as often as
    /// wanted; safe to interrupt.
    pub fn sync(&mut self, store: &mut Store, blobs: &mut dyn BlobStore) -> Result<SyncReport> {
        let mut st = State::load(store)?;
        let out = self.sync_inner(store, blobs, &mut st);
        st.status.clock_offset_ms = self.clock_offset();
        match &out {
            Ok(r) => {
                st.status.last_sync_ms = Some(now_ms(store));
                st.status.last_error = None;
                st.status.last_error_code = None;
                st.status.clock_ahead = false;
                st.status.rejected_batches += r.rejected_batches as u64;
                st.status.refused_future_ops += r.refused_future_ops as u64;
            }
            Err(e) => {
                st.status.last_error = Some(e.plain());
                st.status.last_error_code = Some(e.code().to_string());
                st.status.clock_ahead = matches!(e, SyncError::ClockAhead);
                if matches!(e, SyncError::Removed) {
                    st.status.removed = true;
                }
            }
        }
        st.save(store)?;
        out
    }

    fn sync_inner(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        st: &mut State,
    ) -> Result<SyncReport> {
        if st.status.removed {
            return Err(SyncError::Removed);
        }
        let mut report = SyncReport::default();
        self.refresh(store, st)?;
        // A wrong clock does not stop sync: requests are signed with the
        // corrected time, pulling is safe, and the server holds back
        // changes stamped in the future (ClockAhead). The interface warns
        // from `clock_offset_ms`.
        // Pull first, so a long-offline device has the latest roster
        // and keys before sealing anything; then push.
        self.pull(store, st, &mut report)?;
        if store.meta("household_id")?.is_none() {
            let member: MemberId = st
                .config
                .member
                .parse()
                .map_err(|_| SyncError::Invalid("bad member id".into()))?;
            store.adopt_household(member)?;
        }
        if st.config.role != DeviceRole::Recovery {
            self.push(store, st, &mut report)?;
            self.sync_files(store, blobs, st, &mut report)?;
        }
        Ok(report)
    }

    /// Fetch newer roster versions and keys; check them all.
    pub(crate) fn refresh(&mut self, store: &Store, st: &mut State) -> Result<()> {
        let path = format!(
            "/v1/s/{}/roster?after={}",
            st.config.space,
            st.latest().version
        );
        let server = st.config.server.clone();
        let r = self.call(store, &server, Some(&st.secret), "GET", &path, vec![])?;
        let list: RosterList = from_json(&r)?;
        for next in list.rosters {
            let prev_signed = st.rosters.last().expect("non-empty");
            let body = verify_next(prev_signed, st.latest(), &next)?;
            st.rosters.push(next);
            st.bodies.push(body);
        }
        if st.latest().find(st.config.device).is_none() {
            st.status.removed = true;
            return Err(SyncError::Removed);
        }
        if st.latest().epoch > st.chain.current_epoch() {
            self.fetch_keys(store, st)?;
        }
        Ok(())
    }

    /// Open the newest key envelope for this device.
    pub(crate) fn fetch_keys(&mut self, store: &Store, st: &mut State) -> Result<()> {
        let path = format!("/v1/s/{}/keys", st.config.space);
        let server = st.config.server.clone();
        let r = self.call(store, &server, Some(&st.secret), "GET", &path, vec![])?;
        let list: KeyList = from_json(&r)?;
        let want = st.latest().epoch;
        let env = list
            .envelopes
            .iter()
            .filter(|e| e.space == st.config.space)
            .max_by_key(|e| e.epoch)
            .ok_or_else(|| SyncError::Invalid("no keys for this device yet".into()))?;
        let chain = open_envelope(&st.secret, &st.bodies, env)?;
        if !st.chain.is_prefix_of(&chain) {
            return Err(SyncError::Invalid(
                "the new household keys do not extend the old ones".into(),
            ));
        }
        if chain.current_epoch() < want {
            return Err(SyncError::Invalid(
                "the household's newest keys have not arrived yet".into(),
            ));
        }
        st.chain = chain;
        Ok(())
    }

    fn push(&mut self, store: &mut Store, st: &mut State, report: &mut SyncReport) -> Result<()> {
        let path = format!("/v1/s/{}/ops", st.config.space);
        let server = st.config.server.clone();
        loop {
            if let Some(p) = st.pending.clone() {
                let body = to_json(&PushOps {
                    batches: vec![p.batch.clone()],
                });
                self.call(store, &server, Some(&st.secret), "POST", &path, body)?;
                st.push_cursor = p.upto;
                st.pending = None;
                st.save(store)?;
            }
            let ops = store.unsynced_ops(st.push_cursor, BATCH_OPS)?;
            if ops.is_empty() {
                return Ok(());
            }
            let mut chosen: Vec<Op> = Vec::new();
            let mut size = 0;
            let mut upto = st.push_cursor;
            for (seq, op) in ops {
                size += serde_json::to_vec(&op).map(|v| v.len()).unwrap_or(0);
                chosen.push(op);
                upto = seq;
                if size > BATCH_BYTES {
                    break;
                }
            }
            let max_wall = chosen.iter().map(|o| o.hlc.wall_ms).max().unwrap_or(0);
            let plaintext = Zeroizing::new(to_json(&chosen));
            let batch = seal_batch(
                &st.secret,
                &st.chain,
                st.config.space,
                new_batch_id()?,
                st.next_device_seq,
                max_wall,
                &plaintext,
            )?;
            report.pushed_ops += chosen.len();
            // Saved before sending: a retry sends these same bytes.
            st.pending = Some(Pending { batch, upto });
            st.next_device_seq += 1;
            st.save(store)?;
        }
    }

    fn pull(&mut self, store: &mut Store, st: &mut State, report: &mut SyncReport) -> Result<()> {
        let server = st.config.server.clone();
        loop {
            let path = format!(
                "/v1/s/{}/ops?after={}&limit=200",
                st.config.space, st.cursor
            );
            let r = self.call(store, &server, Some(&st.secret), "GET", &path, vec![])?;
            let page: PullOps = from_json(&r)?;
            for p in &page.batches {
                if p.batch.header.device != st.config.device {
                    match self.accept(store, st, p)? {
                        Some(ops) => {
                            let a = store.apply_synced_ops(&ops, p.received_ms)?;
                            report.pulled_ops += a.applied;
                            report.refused_future_ops += a.too_far_ahead;
                        }
                        None => report.rejected_batches += 1,
                    }
                    let seen = st.seen.entry(p.batch.header.device).or_insert(0);
                    *seen = (*seen).max(p.batch.header.device_seq);
                }
                st.cursor = p.seq;
            }
            st.save(store)?;
            if !page.more {
                return Ok(());
            }
        }
    }

    /// Every check a pulled batch must pass. `Ok(None)` = refused (and
    /// skipped, the same way on every device); `Err` = stop and retry later.
    fn accept(&mut self, store: &Store, st: &mut State, p: &Pulled) -> Result<Option<Vec<Op>>> {
        let h = &p.batch.header;
        let mut author = find_author(st.latest(), h.device, h.device_seq);
        if author.is_none() {
            // Perhaps added since we last looked.
            self.refresh(store, st)?;
            author = find_author(st.latest(), h.device, h.device_seq);
        }
        let Some(author) = author else {
            return Ok(None);
        };
        if author.role == DeviceRole::Recovery {
            return Ok(None);
        }
        if h.epoch > st.chain.current_epoch() {
            self.refresh(store, st)?;
            self.fetch_keys(store, st)?;
        }
        let Ok(plain) = open_batch(&p.batch, st.config.space, &st.chain, &author.device) else {
            return Ok(None);
        };
        let plain = Zeroizing::new(plain);
        let Ok(ops) = serde_json::from_slice::<Vec<Op>>(&plain) else {
            return Ok(None);
        };
        // The clock guard, at batch level: the claimed maximum must be
        // honest and not ahead of when the server received the batch.
        if h.max_wall_ms > p.received_ms.saturating_add(MAX_CLOCK_AHEAD_MS)
            || ops.iter().any(|o| o.hlc.wall_ms > h.max_wall_ms)
        {
            return Ok(None);
        }
        // Only an owner's device may write in another member's name.
        if author.role != DeviceRole::Owner
            && ops.iter().any(|o| o.author.to_string() != author.member)
        {
            return Ok(None);
        }
        Ok(Some(ops))
    }

    fn sync_files(
        &mut self,
        store: &Store,
        blobs: &mut dyn BlobStore,
        st: &mut State,
        report: &mut SyncReport,
    ) -> Result<()> {
        let wanted = referenced(store)?;
        if wanted.is_empty() {
            return Ok(());
        }
        let server = st.config.server.clone();
        let space = st.config.space;
        // Upload what we have and the server lacks.
        let local: Vec<String> = wanted
            .iter()
            .filter(|h| blobs.has(h).unwrap_or(false))
            .cloned()
            .collect();
        for chunk in local.chunks(500) {
            let names: Vec<String> = chunk
                .iter()
                .map(|h| blob_name(&st.chain, space, h))
                .collect::<std::result::Result<_, _>>()?;
            let path = format!("/v1/s/{space}/blobs/missing");
            let r = self.call(
                store,
                &server,
                Some(&st.secret),
                "POST",
                &path,
                to_json(&BlobNames {
                    names: names.clone(),
                }),
            )?;
            let missing: BTreeSet<String> = from_json::<BlobNames>(&r)?.names.into_iter().collect();
            for (hash, name) in chunk.iter().zip(&names) {
                if missing.contains(name)
                    && let Some(bytes) = blobs.get(hash)?
                {
                    let sealed = seal_blob(&st.chain, space, name, &bytes)?;
                    let path = format!("/v1/s/{space}/blobs/{name}");
                    self.call(store, &server, Some(&st.secret), "PUT", &path, sealed)?;
                    report.files_up += 1;
                }
            }
        }
        // Download thumbnails and smaller files we lack; big ones wait
        // until someone opens them.
        let mut small = BTreeSet::new();
        for a in store.list::<Attachment>()? {
            if let Some(t) = a.thumb_sha256 {
                small.insert(t);
            }
            if a.size <= PREFETCH_MAX {
                small.insert(a.sha256);
            }
        }
        for hash in wanted.iter().filter(|h| small.contains(*h)) {
            if blobs.has(hash)? {
                continue;
            }
            if self.download(store, blobs, st, hash)?.is_some() {
                report.files_down += 1;
            }
        }
        Ok(())
    }

    fn download(
        &mut self,
        store: &Store,
        blobs: &mut dyn BlobStore,
        st: &State,
        hash: &str,
    ) -> Result<Option<Vec<u8>>> {
        let name = blob_name(&st.chain, st.config.space, hash)?;
        let path = format!("/v1/s/{}/blobs/{name}", st.config.space);
        let server = st.config.server.clone();
        let r = match self.call(store, &server, Some(&st.secret), "GET", &path, vec![]) {
            Ok(r) => r,
            // Not uploaded yet by the device that has it.
            Err(SyncError::Server { status: 404, .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let bytes = open_blob(&st.chain, st.config.space, &name, &r.body, hash)?;
        let got = blobs.put(&bytes)?;
        debug_assert_eq!(got, hash);
        Ok(Some(bytes))
    }

    /// A file that is not on this device yet: fetch it now (when it is
    /// opened). `Ok(None)` if sync is off or nobody has uploaded it.
    pub fn fetch_file(
        &mut self,
        store: &Store,
        blobs: &mut dyn BlobStore,
        hash: &str,
    ) -> Result<Option<Vec<u8>>> {
        if !state::is_set_up(store)? {
            return Ok(None);
        }
        let st = State::load(store)?;
        if st.status.removed {
            return Ok(None);
        }
        self.download(store, blobs, &st, hash)
    }

    // -- devices ----------------------------------------------------------

    /// The household's devices, from the verified roster, with "last seen"
    /// from the server when it can be reached.
    pub fn devices(&mut self, store: &Store) -> Result<Vec<DeviceInfo>> {
        let mut st = State::load(store)?;
        let online = self.refresh(store, &mut st).is_ok();
        st.save(store)?;
        let mut seen = Vec::new();
        if online {
            let path = format!("/v1/s/{}/devices", st.config.space);
            let server = st.config.server.clone();
            if let Ok(r) = self.call(store, &server, Some(&st.secret), "GET", &path, vec![]) {
                seen = from_json::<DeviceList>(&r)?.devices;
            }
        }
        let latest = st.latest();
        let mut out = Vec::new();
        for e in &latest.devices {
            if e.role == DeviceRole::Recovery {
                continue;
            }
            let s = seen.iter().find(|d| d.id == e.device.id);
            let member_name =
                store.member_name(e.member.parse().unwrap_or(MemberId(Default::default())));
            out.push(DeviceInfo {
                id: e.device.id.to_string(),
                name: open_label(&st.chain, latest.space, e.device.id, &e.label)
                    .unwrap_or_else(|_| "A device".into()),
                member: e.member.clone(),
                member_name,
                role: e.role,
                this_device: e.device.id == st.config.device,
                added_ms: s.map(|d| d.added_ms),
                last_seen_ms: s.and_then(|d| d.last_seen_ms),
            });
        }
        Ok(out)
    }

    /// Remove a device (an owner's action): the server stops serving it,
    /// and the household key rotates, so it cannot read anything new.
    pub fn remove_device(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        device: &str,
    ) -> Result<()> {
        let target: DeviceId = device
            .parse()
            .map_err(|_| SyncError::Invalid("bad device id".into()))?;
        // Catch up first, so we know the last batch the device sent.
        let _ = self.sync(store, blobs);
        let mut st = State::load(store)?;
        if st.config.role != DeviceRole::Owner {
            return Err(SyncError::Invalid(
                "Only an owner can remove devices.".into(),
            ));
        }
        if target == st.config.device {
            return Err(SyncError::Invalid(
                "Use \"Stop syncing on this device\" to leave from this device.".into(),
            ));
        }
        for attempt in 0..2 {
            self.refresh(store, &mut st)?;
            let latest = st.latest().clone();
            let entry = latest
                .find(target)
                .filter(|e| e.role != DeviceRole::Recovery)
                .cloned()
                .ok_or_else(|| SyncError::Invalid("That device is not in the household.".into()))?;
            let new_chain = st.chain.rotated()?;
            let mut next = latest.successor(st.rosters.last().expect("non-empty"), now_ms(store));
            next.devices.retain(|e| e.device.id != target);
            next.removed.push(RemovedEntry {
                entry,
                removed_in: next.version,
                last_seq: st.seen.get(&target).copied().unwrap_or(0),
            });
            next.epoch += 1;
            next.sort();
            match self.publish_roster(store, &mut st, next, &new_chain, None, None) {
                Ok(()) => {
                    st.chain = new_chain;
                    st.save(store)?;
                    return Ok(());
                }
                Err(SyncError::Server { code, .. }) if code == "conflict" && attempt == 0 => {
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("the loop returns")
    }

    /// Sign and send a new roster version with envelopes for `chain` to
    /// every device that needs them; on success, append it locally.
    pub(crate) fn publish_roster(
        &mut self,
        store: &Store,
        st: &mut State,
        body: RosterBody,
        chain: &KeyChain,
        anchor: Option<Vec<u8>>,
        invitation: Option<String>,
    ) -> Result<()> {
        let prev = st.latest().clone();
        let signed = SignedRoster::sign(&body, &st.secret)?;
        let body = verify_next(st.rosters.last().expect("non-empty"), &prev, &signed)?;
        let rotated = body.epoch != prev.epoch;
        let mut envelopes: Vec<KeyEnvelope> = Vec::new();
        for e in &body.devices {
            if rotated || prev.find(e.device.id).is_none() {
                envelopes.push(wrap(&st.secret, body.space, &e.device, chain)?);
            }
        }
        let req = UpdateRoster {
            roster: signed.clone(),
            envelopes,
            anchor,
            invitation,
        };
        let path = format!("/v1/s/{}/roster", body.space);
        let server = st.config.server.clone();
        self.call(
            store,
            &server,
            Some(&st.secret),
            "POST",
            &path,
            to_json(&req),
        )?;
        st.rosters.push(signed);
        st.bodies.push(body);
        Ok(())
    }

    /// Replace the recovery key (if the paper is lost or was seen). The
    /// old one stops working and the household key rotates.
    pub fn new_recovery_key(&mut self, store: &mut Store) -> Result<Zeroizing<String>> {
        let mut st = State::load(store)?;
        if st.config.role != DeviceRole::Owner {
            return Err(SyncError::Invalid(
                "Only an owner can make a new recovery key.".into(),
            ));
        }
        self.refresh(store, &mut st)?;
        let latest = st.latest().clone();
        let old = latest
            .recovery()
            .cloned()
            .ok_or_else(|| SyncError::Invalid("no recovery key".into()))?;
        let recovery = RecoveryKey::generate()?;
        let rec = recovery.identity();
        let new_chain = st.chain.rotated()?;
        let mut next = latest.successor(st.rosters.last().expect("non-empty"), now_ms(store));
        next.devices.retain(|e| e.device.id != old.device.id);
        next.removed.push(RemovedEntry {
            entry: old,
            removed_in: next.version,
            last_seq: 0,
        });
        next.devices.push(RosterEntry {
            device: rec.public(),
            member: st.config.member.clone(),
            role: DeviceRole::Recovery,
            added_in: next.version,
            label: seal_label(&new_chain, latest.space, rec.id(), "Recovery key")?,
        });
        next.epoch += 1;
        next.sort();
        let genesis = st.rosters[0].hash();
        let anchor = recovery.seal_anchor(latest.space, &Anchor { genesis })?;
        self.publish_roster(store, &mut st, next, &new_chain, Some(anchor), None)?;
        st.chain = new_chain;
        st.status.recovery_confirmed = false;
        st.save(store)?;
        Ok(recovery.display())
    }

    /// On a fresh device: get everything back with the recovery key.
    pub fn restore(
        &mut self,
        store: &mut Store,
        blobs: &mut dyn BlobStore,
        server: &str,
        recovery_text: &str,
        device_name: &str,
    ) -> Result<SyncReport> {
        if state::is_set_up(store)? || store.meta("household_id")?.is_some() {
            return Err(SyncError::Invalid(
                "Restoring needs a device with no household on it yet.".into(),
            ));
        }
        let server = check_server_url(server)?;
        let recovery = RecoveryKey::parse(recovery_text)?;
        let rid = recovery.identity();
        let r = self.call(store, &server, Some(&rid), "GET", "/v1/recovery", vec![])
            .map_err(|e| match e {
                SyncError::Server { code, .. } if code == "unknown_device" || code == "revoked" => SyncError::Invalid(
                    "This server does not know that recovery key. Check the key and the server address (a newer recovery key may have replaced it).".into(),
                ),
                e => e,
            })?;
        let view: RecoveryView = from_json(&r)?;
        let anchor = recovery.open_anchor(view.space, &view.anchor)?;
        let path = format!("/v1/s/{}/roster?after=0", view.space);
        let list: RosterList =
            from_json(&self.call(store, &server, Some(&rid), "GET", &path, vec![])?)?;
        let bodies = verify_chain(&list.rosters)?;
        if list.rosters[0].hash() != anchor.genesis {
            return Err(SyncError::Crypto(
                housewright_crypto::CryptoError::Rejected(
                    "the server's household history does not match this recovery key".into(),
                ),
            ));
        }
        let latest = bodies.last().expect("non-empty").clone();
        let rec_entry = latest
            .find(rid.id())
            .filter(|e| e.role == DeviceRole::Recovery)
            .cloned()
            .ok_or_else(|| {
                SyncError::Invalid("That recovery key was replaced by a newer one.".into())
            })?;
        let path = format!("/v1/s/{}/keys", view.space);
        let keys: KeyList =
            from_json(&self.call(store, &server, Some(&rid), "GET", &path, vec![])?)?;
        let env = keys
            .envelopes
            .iter()
            .max_by_key(|e| e.epoch)
            .ok_or_else(|| SyncError::Invalid("no keys were left for the recovery key".into()))?;
        let chain = open_envelope(&rid, &bodies, env)?;
        if chain.current_epoch() != latest.epoch {
            return Err(SyncError::Invalid(
                "the household's newest keys are missing".into(),
            ));
        }
        // Add this device, signed by the recovery identity.
        let secret = DeviceSecret::generate()?;
        let me = secret.public();
        let mut rec_state = State {
            config: SyncConfig {
                server: server.clone(),
                space: view.space,
                device: rid.id(),
                member: rec_entry.member.clone(),
                role: DeviceRole::Recovery,
            },
            secret: rid,
            chain: chain.clone(),
            rosters: list.rosters,
            bodies,
            cursor: 0,
            push_cursor: 0,
            next_device_seq: 1,
            pending: None,
            seen: Default::default(),
            status: SyncStatus::default(),
        };
        let mut next =
            latest.successor(rec_state.rosters.last().expect("non-empty"), now_ms(store));
        next.devices.push(RosterEntry {
            device: me.clone(),
            member: rec_entry.member.clone(),
            role: DeviceRole::Owner,
            added_in: next.version,
            label: seal_label(&chain, view.space, me.id, device_name)?,
        });
        next.sort();
        self.publish_roster(store, &mut rec_state, next, &chain, None, None)?;
        let st = State {
            config: SyncConfig {
                server,
                space: view.space,
                device: me.id,
                member: rec_entry.member,
                role: DeviceRole::Owner,
            },
            secret,
            chain,
            rosters: rec_state.rosters.clone(),
            bodies: rec_state.bodies.clone(),
            cursor: 0,
            push_cursor: 0,
            next_device_seq: 1,
            pending: None,
            seen: Default::default(),
            status: SyncStatus {
                recovery_confirmed: true,
                ..Default::default()
            },
        };
        st.save(store)?;
        self.sync(store, blobs)
    }
}

pub(crate) fn find_author(latest: &RosterBody, id: DeviceId, seq: u64) -> Option<RosterEntry> {
    if let Some(e) = latest.find(id) {
        return Some(e.clone());
    }
    latest
        .find_removed(id)
        .filter(|r| seq <= r.last_seq)
        .map(|r| r.entry.clone())
}

/// Open an envelope, checking its sender was a manager in the roster
/// version that started its epoch.
pub(crate) fn open_envelope(
    me: &DeviceSecret,
    bodies: &[RosterBody],
    env: &KeyEnvelope,
) -> Result<KeyChain> {
    let first = bodies
        .iter()
        .find(|b| b.epoch == env.epoch)
        .ok_or_else(|| SyncError::Invalid("keys for an epoch the roster does not have".into()))?;
    let latest = bodies.last().expect("non-empty");
    let sender: DevicePublic = first
        .find(env.sender)
        .or_else(|| latest.find(env.sender))
        .filter(|e| e.role.can_manage())
        .map(|e| e.device.clone())
        .or_else(|| {
            // Wrapped by a manager added later in the same epoch.
            bodies
                .iter()
                .filter(|b| b.epoch == env.epoch)
                .find_map(|b| {
                    b.find(env.sender)
                        .filter(|e| e.role.can_manage())
                        .map(|e| e.device.clone())
                })
        })
        .ok_or_else(|| SyncError::Invalid("keys were not sent by an owner".into()))?;
    Ok(unwrap(me, env, &sender)?)
}

/// The batch type, re-exported for tests.
pub type Batch = SealedBatch;
