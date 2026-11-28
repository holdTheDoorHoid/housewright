//! Regression tests for the verification pass of the security review
//! (N1 to N6, L6). The N1, N2 and N3 tests port the reviewer's attack
//! tests (`verify/tests/v.rs`) with the assertions turned around: what the
//! reviewer showed happening must no longer happen.

mod common;

use common::*;
use housewright_core::hlc::Hlc;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::oplog::Op;
use housewright_core::store::WallClock;
use housewright_crypto::batch::{SealedBatch, new_batch_id, seal_batch};
use housewright_crypto::encoding::unb64;
use housewright_crypto::roster::SignedRoster;
use housewright_crypto::wire::BatchBody;
use housewright_crypto::{DeviceSecret, KeyChain, SpaceId};
use housewright_sync::{Invitee, PairState, SyncClient};

fn secret(d: &Device) -> DeviceSecret {
    DeviceSecret::from_bytes(&unb64(&d.store.sync_get("device_secret").unwrap().unwrap()).unwrap())
        .unwrap()
}
fn chain(d: &Device) -> KeyChain {
    KeyChain::from_bytes(&unb64(&d.store.sync_get("chain").unwrap().unwrap()).unwrap()).unwrap()
}
fn space(d: &Device) -> SpaceId {
    let c: serde_json::Value =
        serde_json::from_str(&d.store.sync_get("config").unwrap().unwrap()).unwrap();
    c["space"].as_str().unwrap().parse().unwrap()
}
fn rosters(d: &Device) -> Vec<SignedRoster> {
    serde_json::from_str(&d.store.sync_get("rosters").unwrap().unwrap()).unwrap()
}
fn my_id(d: &mut Device) -> String {
    d.client
        .devices(&d.store)
        .unwrap()
        .into_iter()
        .find(|x| x.this_device)
        .unwrap()
        .id
}
fn house(lab: &Lab) -> (Device, Device, Device, String) {
    let mut a = lab.device("Ana");
    a.init("Ana");
    let key = a.setup();
    a.room(1, "Kitchen");
    a.sync();
    let mut b = lab.device("Ben");
    pair(
        &mut a,
        &mut b,
        Invitee::NewPerson {
            name: "Ben".into(),
            role: Role::Member,
        },
    );
    let mut e = lab.device("AnaTablet");
    pair(&mut a, &mut e, Invitee::MyDevice);
    (a, b, e, key)
}
/// Push a validly signed, correctly chained batch as device `d` (what a
/// modified app does), with any body.
fn push_raw(lab: &Lab, d: &mut Device, body: &BatchBody) -> u16 {
    let s = secret(d);
    let sp = space(d);
    let seq: u64 =
        serde_json::from_str(&d.store.sync_get("next_device_seq").unwrap().unwrap()).unwrap();
    let head: String =
        serde_json::from_str(&d.store.sync_get("own_head").unwrap().unwrap()).unwrap();
    let now = lab.clock.now_ms();
    let b = seal_batch(
        &s,
        &chain(d),
        sp,
        new_batch_id().unwrap(),
        seq,
        &head,
        now,
        &serde_json::to_vec(body).unwrap(),
    )
    .unwrap();
    let status = post_batch(lab, &s, sp, &b);
    // Keep the device's own bookkeeping in step, as the modified app would.
    let hash = housewright_crypto::batch::batch_hash(&b).unwrap();
    d.store
        .sync_set("next_device_seq", &(seq + 1).to_string())
        .unwrap();
    d.store
        .sync_set("own_head", &serde_json::to_string(&hash).unwrap())
        .unwrap();
    d.store.sync_set("acked_seq", &seq.to_string()).unwrap();
    d.store
        .sync_set(
            &format!("sent:{seq:020}"),
            &serde_json::json!({"batch": b, "upto": 0}).to_string(),
        )
        .unwrap();
    status
}
fn post_batch(lab: &Lab, s: &DeviceSecret, sp: SpaceId, b: &SealedBatch) -> u16 {
    let bytes = serde_json::to_vec(&housewright_crypto::wire::PushOps {
        batches: vec![b.clone()],
    })
    .unwrap();
    let path = format!("/v1/s/{sp}/ops");
    let now = lab.clock.now_ms();
    let auth = housewright_crypto::request::sign_request(s, "POST", &path, now, &bytes).unwrap();
    let req = housewright_sync::HttpRequest {
        method: "POST".into(),
        url: format!("{URL}{path}"),
        headers: auth
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        body: bytes,
    };
    send_to(&lab.rt, &lab.app.lock().unwrap().clone(), req)
        .unwrap()
        .status
}
fn blob_count(lab: &Lab, sp: SpaceId) -> i64 {
    lab.server_db()
        .query_row(
            "SELECT COUNT(*) FROM blobs WHERE space = ?1",
            [sp.to_string()],
            |r| r.get(0),
        )
        .unwrap()
}

/// N1: a member's made-up roster claims (a huge version, a wrong version 1)
/// no longer freeze anyone; they are counted as that member's tampering.
#[test]
fn n1_a_members_false_roster_claim_freezes_nobody() {
    let lab = Lab::new("vn1");
    let (mut a, mut b, _e, _k) = house(&lab);
    let ben = secret(&b);
    // A huge version, "signed" by Ben (who is no owner).
    let mut body = rosters(&a).last().unwrap().parse().unwrap();
    body.version = u64::MAX;
    let fake_high = SignedRoster::sign(&body, &ben).unwrap();
    assert_eq!(
        push_raw(
            &lab,
            &mut b,
            &BatchBody {
                roster: Some(fake_high),
                ops: serde_json::json!([])
            }
        ),
        200
    );
    // A different "version 1" (a fake fork).
    let mut g = rosters(&a)[0].parse().unwrap();
    g.made_ms += 1;
    let fake_fork = SignedRoster::sign(&g, &ben).unwrap();
    assert_eq!(
        push_raw(
            &lab,
            &mut b,
            &BatchBody {
                roster: Some(fake_fork),
                ops: serde_json::json!([])
            }
        ),
        200
    );
    a.room(50, "after the poison");
    let r1 = a.try_sync().expect("Ana's phone still sends");
    assert_eq!(r1.bad_claims, 2);
    a.try_sync().expect("and keeps sending");
    let st = a.client.status(&a.store).unwrap();
    assert!(!st.fork);
    assert_eq!(st.bad_claims, 2);
    b.sync();
    assert_eq!(b.room_name(50).as_deref(), Some("after the poison"));
}

/// N1: a real newer roster (signed by an owner) that the server hides
/// does stop sealing; once it arrives, the device sends again by itself.
#[test]
fn n1_a_verified_hidden_roster_clears_when_it_arrives() {
    let lab = Lab::new("vn1b");
    let (mut a, mut b, _e, _k) = house(&lab);
    let mut d = lab.device("Dee");
    pair(&mut a, &mut d, Invitee::MyDevice);
    let sp = space(&a);
    let db = lab.server_db();
    let n: i64 = db
        .query_row(
            "SELECT MAX(version) FROM rosters WHERE space = ?1",
            [sp.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    let row: String = db
        .query_row(
            "SELECT data FROM rosters WHERE space = ?1 AND version = ?2",
            rusqlite::params![sp.to_string(), n],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "DELETE FROM rosters WHERE space = ?1 AND version = ?2",
        rusqlite::params![sp.to_string(), n],
    )
    .unwrap();
    db.execute_batch(&format!("CREATE TRIGGER hide AFTER INSERT ON rosters WHEN NEW.version = {n} BEGIN DELETE FROM rosters WHERE version = {n}; END;")).unwrap();
    a.room(30, "carrier");
    a.sync();
    b.room(31, "waits");
    assert!(b.try_sync().is_err());
    db.execute_batch("DROP TRIGGER hide;").unwrap();
    db.execute(
        "INSERT INTO rosters (space, version, data) VALUES (?1, ?2, ?3)",
        rusqlite::params![sp.to_string(), n, row],
    )
    .unwrap();
    b.sync();
    assert!(b.client.status(&b.store).unwrap().newest_roster_seen <= n as u64);
    a.sync();
    assert_eq!(a.room_name(31).as_deref(), Some("waits"));
}

/// N2: one batch the server lost no longer cuts a device off: the author
/// sends it again, the others fill the gap and get everything after it.
#[test]
fn n2_one_lost_batch_is_recovered() {
    let lab = Lab::new("vn2");
    let (mut a, mut b, mut e, _k) = house(&lab);
    b.sync();
    e.room(10, "e1");
    e.sync();
    e.room(11, "e2");
    e.sync();
    e.room(12, "e3");
    e.sync();
    let eid = my_id(&mut e);
    let sp = space(&e);
    let n = lab
        .server_db()
        .execute(
            "DELETE FROM batches WHERE space = ?1 AND device = ?2 AND device_seq = (SELECT MIN(device_seq)+1 FROM batches WHERE space = ?1 AND device = ?2)",
            rusqlite::params![sp.to_string(), eid],
        )
        .unwrap();
    assert_eq!(n, 1);
    b.sync();
    assert!(
        !b.client.status(&b.store).unwrap().missing_from.is_empty(),
        "said plainly"
    );
    e.room(13, "e4 much later");
    e.sync();
    e.room(14, "e5 much later");
    e.sync();
    b.sync();
    a.sync();
    for d in [&a, &b] {
        for (id, name) in [(11, "e2"), (12, "e3"), (14, "e5 much later")] {
            assert_eq!(d.room_name(id).as_deref(), Some(name));
        }
    }
    assert!(b.client.status(&b.store).unwrap().missing_from.is_empty());
    assert_eq!(a.entities(), b.entities());
}

/// N2: a server restored from an old backup: every device sends again
/// what the server lost, and everyone converges; a new device can still
/// join and get everything.
#[test]
fn n2_a_server_restored_from_backup_recovers() {
    let lab = Lab::new("vn2b");
    let (mut a, mut b, mut e, _k) = house(&lab);
    b.sync();
    e.sync();
    // The backup.
    *lab.app.lock().unwrap() = axum::Router::new();
    let backup = lab.dir.with_extension("backup");
    copy_dir(&lab.dir, &backup);
    lab.restart();
    a.room(20, "after the backup, Ana");
    a.sync();
    b.room(21, "after the backup, Ben");
    b.sync();
    e.room(22, "after the backup, tablet");
    e.sync();
    // The disk dies; the operator restores the backup.
    *lab.app.lock().unwrap() = axum::Router::new();
    std::fs::remove_dir_all(&lab.dir).unwrap();
    copy_dir(&backup, &lab.dir);
    lab.restart();
    for _ in 0..2 {
        a.sync();
        b.sync();
        e.sync();
    }
    assert_eq!(a.entities(), b.entities());
    assert_eq!(b.entities(), e.entities());
    assert_eq!(b.room_name(20).as_deref(), Some("after the backup, Ana"));
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    assert_eq!(c.room_name(22).as_deref(), Some("after the backup, tablet"));
    let _ = std::fs::remove_dir_all(&backup);
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// N3: a device with old keys opening a file sealed under a newer key
/// never deletes it; it fetches the keys and opens it. And a member
/// reporting a good file as bad loses nothing.
#[test]
fn n3_a_good_file_is_never_lost() {
    let lab = Lab::new("vn3");
    let (mut a, mut b, mut e, key) = house(&lab);
    b.sync();
    let eid = my_id(&mut e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &eid, Some(&key))
        .unwrap();
    let att = a.attach_photo(b"photo of the boiler, sealed under epoch 1");
    a.sync();
    let sp = space(&a);
    assert_eq!(blob_count(&lab, sp), 1);
    let got = b
        .client
        .fetch_file(&b.store, &mut b.blobs, &att.sha256)
        .unwrap();
    assert!(
        got.is_some(),
        "Ben's phone fetched the new keys and opened it"
    );
    assert_eq!(blob_count(&lab, sp), 1, "nothing deleted");
    // A member reports the good file as bad: it is only marked; Ana's
    // phone (which has it) sends it again, and it is still there.
    let s = secret(&b);
    let name = housewright_crypto::blob::blob_name(&chain(&b), sp, &att.sha256).unwrap();
    let path = format!("/v1/s/{sp}/blobs/{name}/bad");
    let auth =
        housewright_crypto::request::sign_request(&s, "POST", &path, lab.clock.now_ms(), b"")
            .unwrap();
    let req = housewright_sync::HttpRequest {
        method: "POST".into(),
        url: format!("{URL}{path}"),
        headers: auth
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        body: vec![],
    };
    assert_eq!(
        send_to(&lab.rt, &lab.app.lock().unwrap().clone(), req)
            .unwrap()
            .status,
        200
    );
    assert_eq!(blob_count(&lab, sp), 1);
    let r = a.sync();
    assert_eq!(r.files_up, 1);
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    assert!(
        c.client
            .fetch_file(&c.store, &mut c.blobs, &att.sha256)
            .unwrap()
            .is_some()
    );
}

/// N5: batches naming a removed device but not signed by it are never
/// kept, so a server cannot fill a device's storage with them.
#[test]
fn n5_unsigned_batches_for_a_removed_device_are_not_held() {
    let lab = Lab::new("vn5");
    let (mut a, mut b, mut e, key) = house(&lab);
    e.room(10, "tablet");
    e.sync();
    e.room(11, "tablet 2");
    e.sync();
    b.sync();
    let eid = my_id(&mut e);
    let e_chain = chain(&e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &eid, Some(&key))
        .unwrap();
    let sp = space(&a);
    let db = lab.server_db();
    // The server lost the tablet's second batch, so a new device keeps the
    // first one waiting for the chain to complete...
    db.execute(
        "DELETE FROM batches WHERE space = ?1 AND device = ?2 AND device_seq = 2",
        rusqlite::params![sp.to_string(), eid],
    )
    .unwrap();
    // ...and the server adds 20 batches "from the tablet" signed by
    // someone else.
    let impostor = DeviceSecret::generate().unwrap();
    for i in 0..20u64 {
        let mut f = seal_batch(
            &impostor,
            &e_chain,
            sp,
            new_batch_id().unwrap(),
            1 + i % 2,
            housewright_crypto::batch::ZERO_HASH,
            lab.clock.now_ms(),
            b"{\"ops\":[]}",
        )
        .unwrap();
        f.header.device = eid.parse().unwrap();
        db.execute(
            "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data) VALUES (?1, (SELECT MAX(seq)+1 FROM batches WHERE space = ?1), ?2, ?3, ?4, 0, ?5)",
            rusqlite::params![sp.to_string(), f.header.batch_id, format!("x{i}"), (100 + i) as i64, serde_json::to_string(&f).unwrap()],
        )
        .unwrap();
    }
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    assert_eq!(
        c.store.sync_keys("held:").unwrap().len(),
        1,
        "only the tablet's own batch waits"
    );
}

/// N6: the founding device cannot use its history exception after setup,
/// even with a backdated clock.
#[test]
fn n6_the_founder_cannot_backdate_ops_in_someone_elses_name() {
    let lab = Lab::new("vn6");
    let (mut a, mut b, _e, _k) = house(&lab);
    let ben = b.store.author().unwrap();
    let op = Op {
        id: OpId::new(),
        hlc: Hlc {
            wall_ms: 1,
            counter: 0,
        },
        author: ben,
        kind: EntityKind::Room,
        entity: RoomId(uuid::Uuid::from_u128(1)).to_string(),
        field: "name".into(),
        value: "Renamed 'by Ben' with a 1970 clock".into(),
    };
    assert_eq!(
        push_raw(
            &lab,
            &mut a,
            &BatchBody {
                roster: None,
                ops: serde_json::to_value([op]).unwrap()
            }
        ),
        200
    );
    let r = b.sync();
    assert!(r.rejected_batches >= 1);
    assert_eq!(b.room_name(1).as_deref(), Some("Kitchen"));
}

/// L6: the new device's "numbers match" is bound to the safety number's
/// inputs; a constant (what anyone with the code could send) is not
/// accepted.
#[test]
fn l6_a_constant_confirmation_is_not_accepted() {
    let lab = Lab::new("vl6");
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
    let inv = a
        .client
        .invite_start(&mut a.store, Invitee::MyDevice)
        .unwrap();
    let mut n = lab.device("New");
    n.client
        .join_start(&n.store, URL, &inv.code, "New phone")
        .unwrap();
    a.client.invite_poll(&a.store).unwrap();
    let code = housewright_crypto::pairing::PairingCode::parse(&inv.code).unwrap();
    let sealed = code
        .seal(housewright_crypto::pairing::Part::Confirm, b"numbers match")
        .unwrap();
    let body =
        serde_json::to_vec(&housewright_crypto::wire::JoinConfirm { confirm: sealed }).unwrap();
    let req = housewright_sync::HttpRequest {
        method: "POST".into(),
        url: format!("{URL}/v1/invitations/{}/confirm", code.invitation_id()),
        headers: vec![],
        body,
    };
    assert_eq!(
        send_to(&lab.rt, &lab.app.lock().unwrap().clone(), req)
            .unwrap()
            .status,
        200
    );
    let PairState::Compare {
        other_confirmed, ..
    } = a.client.invite_poll(&a.store).unwrap()
    else {
        panic!()
    };
    assert!(!other_confirmed);
    assert!(
        a.client
            .invite_confirm(&mut a.store, &mut a.blobs, true)
            .is_err()
    );
    let _ = SyncClient::is_set_up(&n.store);
}
