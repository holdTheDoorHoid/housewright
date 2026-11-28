//! Regression tests for the findings of the Phase 6 security review
//! (docs/security-reviews/2026-11-28-phase6-review.md). Each test plays
//! the attack or failure the review describes against the real server
//! code and checks that it no longer works.

mod common;

use common::*;
use housewright_core::hlc::Hlc;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::oplog::{DELETED_FIELD, Op, RECORD_FIELD};
use housewright_core::store::WallClock;
use housewright_crypto::batch::{SealedBatch, ZERO_HASH, batch_hash, new_batch_id, seal_batch};
use housewright_crypto::encoding::unb64;
use housewright_crypto::wire::BatchBody;
use housewright_crypto::{DeviceSecret, KeyChain, SpaceId};
use housewright_sync::{Invitee, PairState, SyncClient, SyncError};
use serde_json::json;

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
fn my_id(d: &mut Device) -> String {
    d.client
        .devices(&d.store)
        .unwrap()
        .into_iter()
        .find(|x| x.this_device)
        .unwrap()
        .id
}
fn now(lab: &Lab) -> u64 {
    lab.clock.now_ms()
}
fn op(
    author: MemberId,
    kind: EntityKind,
    entity: &str,
    field: &str,
    value: serde_json::Value,
    wall: u64,
) -> Op {
    Op {
        id: OpId::new(),
        hlc: Hlc {
            wall_ms: wall,
            counter: 0,
        },
        author,
        kind,
        entity: entity.into(),
        field: field.into(),
        value,
    }
}
fn body(ops: &[Op]) -> Vec<u8> {
    serde_json::to_vec(&BatchBody {
        roster_version: 1,
        roster_hash: String::new(),
        ops: serde_json::to_value(ops).unwrap(),
    })
    .unwrap()
}
/// What a hostile server does: store a batch it was never sent.
fn inject(lab: &Lab, sp: SpaceId, b: &SealedBatch, server_seq: Option<i64>, received: u64) {
    let db = lab.server_db();
    let seq = server_seq.unwrap_or_else(|| {
        db.query_row(
            "SELECT COALESCE(MAX(seq),0)+1 FROM batches WHERE space = ?1",
            [sp.to_string()],
            |r| r.get(0),
        )
        .unwrap()
    });
    db.execute(
        "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![sp.to_string(), seq, b.header.batch_id, format!("forged-{}", b.header.batch_id), b.header.device_seq as i64, received as i64, serde_json::to_string(b).unwrap()],
    )
    .unwrap();
}
fn room_id(n: u128) -> String {
    RoomId(uuid::Uuid::from_u128(n)).to_string()
}

struct House {
    a: Device,
    b: Device,
    e: Device,
    key: String,
}

/// Ana (owner, with a second device E) and Ben (member).
fn house(lab: &Lab) -> House {
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
    House { a, b, e, key }
}

/// H1: the reviewer's `forge/` scenario, end to end. An owner's second
/// device is removed; it (holding its signing key and the epoch-0 key)
/// and the server then inject batches under numbers it used before, with
/// operations in another member's name. Nobody accepts them: not a device
/// that already had the real batches, and not a device that joins later
/// and is shown the forged ones first.
#[test]
fn h1_a_removed_device_and_the_server_cannot_forge_batches() {
    let lab = Lab::new("h1");
    let House {
        mut a,
        mut b,
        mut e,
        key,
    } = house(&lab);
    e.room(10, "Tablet shelf");
    e.sync();
    e.room(11, "Tablet nook");
    e.sync();
    a.sync();
    b.sync();
    let e_secret = secret(&e);
    let e_chain0 = chain(&e);
    let sp = space(&e);
    let ben = b.store.author().unwrap();
    let e_id = my_id(&mut e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &e_id, Some(&key))
        .unwrap();
    b.sync();

    // Forged after removal: rename Ana's kitchen, delete E's own shelf,
    // all in Ben's name, at numbers 1 and 2 (both <= last_seq).
    let forged_ops = vec![
        op(
            ben,
            EntityKind::Room,
            &room_id(1),
            "name",
            json!("FORGED"),
            now(&lab),
        ),
        op(
            ben,
            EntityKind::Room,
            &room_id(10),
            DELETED_FIELD,
            json!(true),
            now(&lab),
        ),
    ];
    let f1 = seal_batch(
        &e_secret,
        &e_chain0,
        sp,
        new_batch_id().unwrap(),
        1,
        ZERO_HASH,
        now(&lab),
        &body(&forged_ops),
    )
    .unwrap();
    let f2 = seal_batch(
        &e_secret,
        &e_chain0,
        sp,
        new_batch_id().unwrap(),
        2,
        &batch_hash(&f1).unwrap(),
        now(&lab),
        &body(&forged_ops),
    )
    .unwrap();
    inject(&lab, sp, &f1, None, now(&lab));
    inject(&lab, sp, &f2, None, now(&lab));
    let r = b.sync();
    assert!(r.tampered + r.rejected_batches >= 2, "{r:?}");
    assert_eq!(b.room_name(1).as_deref(), Some("Kitchen"));
    assert_eq!(b.room_name(10).as_deref(), Some("Tablet shelf"));

    // A device that joins later, shown the forged batches FIRST.
    {
        let db = lab.server_db();
        db.execute(
            "UPDATE batches SET seq = seq + 1000 WHERE space = ?1",
            [sp.to_string()],
        )
        .unwrap();
        for (i, f) in [&f1, &f2].iter().enumerate() {
            db.execute(
                "UPDATE batches SET seq = ?2 WHERE space = ?1 AND batch_id = ?3",
                rusqlite::params![sp.to_string(), i as i64 + 1, f.header.batch_id],
            )
            .unwrap();
        }
    }
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    assert_eq!(
        c.room_name(1).as_deref(),
        Some("Kitchen"),
        "the forgery was not applied"
    );
    assert_eq!(
        c.room_name(10).as_deref(),
        Some("Tablet shelf"),
        "the real batches still were"
    );
    assert_eq!(c.room_name(11).as_deref(), Some("Tablet nook"));
    a.sync();
    assert_eq!(a.entities(), c.entities());
}

/// H1 (second part): nobody, an owner included, writes in another
/// member's name.
#[test]
fn h1_an_owner_device_cannot_write_in_another_members_name() {
    let lab = Lab::new("h1b");
    let House { mut a, mut b, .. } = house(&lab);
    let ben = b.store.author().unwrap();
    a.store.set_author(ben).unwrap();
    a.room(7, "Written as Ben by Ana's phone");
    a.sync();
    let r = b.sync();
    assert!(r.rejected_batches >= 1);
    assert_eq!(b.room_name(7), None);
}

/// H2: big batches, a page too big for a device, and a stored batch too
/// big to send never stop sync.
#[test]
fn h2_large_batches_never_wedge_sync() {
    let lab = Lab::new("h2");
    let House { mut a, mut b, .. } = house(&lab);
    // 24 changes of about 450 KB each: one batch each, about 14 MB of
    // stored batches, more than one server page.
    let big = "x".repeat(450 * 1024);
    for i in 0..24u128 {
        a.room(100 + i, &format!("{i}{big}"));
    }
    a.sync();
    // A phone that cannot take answers over 2 MB.
    b.max_response
        .store(2 * 1024 * 1024, std::sync::atomic::Ordering::SeqCst);
    b.sync();
    assert_eq!(a.entities(), b.entities());

    // A single change too big to send stays on the device, said plainly;
    // the rest still syncs.
    a.room(200, &"y".repeat(1024 * 1024));
    a.room(201, "After the huge one");
    a.sync();
    assert_eq!(a.client.status(&a.store).unwrap().unsent_too_large, 1);
    b.sync();
    assert_eq!(b.room_name(201).as_deref(), Some("After the huge one"));

    // A stored batch larger than the server will send (written straight
    // into its database) is skipped, not fetched forever.
    let sp = space(&a);
    let db = lab.server_db();
    db.execute(
        "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data) VALUES (?1, (SELECT MAX(seq)+1 FROM batches WHERE space = ?1), 'huge', 'huge', 1, 0, ?2)",
        rusqlite::params![sp.to_string(), "z".repeat(3 * 1024 * 1024)],
    )
    .unwrap();
    let r = b.sync();
    assert_eq!(r.skipped_too_large, 1);
    a.room(202, "Still syncing");
    a.sync();
    b.sync();
    assert_eq!(b.room_name(202).as_deref(), Some("Still syncing"));

    // The server refuses a batch over its size cap outright.
    let s = secret(&a);
    let huge = seal_batch(
        &s,
        &chain(&a),
        sp,
        new_batch_id().unwrap(),
        999,
        ZERO_HASH,
        now(&lab),
        &vec![7u8; 1100 * 1024],
    )
    .unwrap();
    let bytes = serde_json::to_vec(&housewright_crypto::wire::PushOps {
        batches: vec![huge],
    })
    .unwrap();
    let path = format!("/v1/s/{sp}/ops");
    let auth =
        housewright_crypto::request::sign_request(&s, "POST", &path, now(&lab), &bytes).unwrap();
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
    let res = send_to(&lab.rt, &lab.app.lock().unwrap().clone(), req).unwrap();
    assert_eq!(res.status, 413);
}

/// M2: a member cannot sign a record in someone else's name, nor replace
/// someone else's record with a backdated one.
#[test]
fn m2_records_are_bound_to_their_author() {
    let lab = Lab::new("m2");
    let House { mut a, mut b, .. } = house(&lab);
    let ana = a.store.author().unwrap();
    let ben = b.store.author().unwrap();
    let note = Note {
        id: NoteId::new(),
        about: EntityRef {
            kind: EntityKind::Room,
            id: room_id(1),
        },
        by: ana,
        at: jiff::Timestamp::from_second(1_800_000_000).unwrap(),
        text: Flagged {
            value: "Ana's real note".into(),
            sensitivity: Sensitivity::Normal,
        },
    };
    a.store.append(&note).unwrap();
    a.sync();
    b.sync();
    // Ben's device: a note claiming Ana wrote it.
    let fake = Note {
        id: NoteId::new(),
        by: ana,
        text: Flagged {
            value: "Ana did not write this".into(),
            sensitivity: Sensitivity::Normal,
        },
        ..note.clone()
    };
    b.store.append(&fake).unwrap();
    // And a backdated rewrite of Ana's real note.
    let mut v = serde_json::to_value(&note).unwrap();
    v["text"]["value"] = "rewritten by Ben".into();
    v["by"] = ben.to_string().into();
    let rewrite = op(
        ben,
        EntityKind::Note,
        &note.id.to_string(),
        RECORD_FIELD,
        v,
        0,
    );
    b.store.apply_remote_ops(&[rewrite]).unwrap(); // as if made on Ben's phone
    b.store.sync_remove("push_cursor").unwrap();
    b.sync();
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    for d in [&a, &c] {
        assert!(
            d.store.get::<Note>(&fake.id.to_string()).unwrap().is_none(),
            "forged attribution refused"
        );
        assert_eq!(
            d.store
                .fetch::<Note>(&note.id.to_string())
                .unwrap()
                .text
                .value,
            "Ana's real note"
        );
    }
}

/// M3: a stolen phone alone cannot remove the owner's other devices or
/// replace the recovery key, and every device is told about changes.
#[test]
fn m3_takeover_needs_the_recovery_key_and_is_announced() {
    let lab = Lab::new("m3");
    let House {
        mut a,
        mut b,
        mut e,
        key,
    } = house(&lab);
    let a_id = my_id(&mut a);
    // E is "the stolen phone".
    let err = e
        .client
        .remove_device(&mut e.store, &mut e.blobs, &a_id, None)
        .unwrap_err();
    assert!(err.plain().contains("recovery key"), "{}", err.plain());
    let wrong = housewright_crypto::recovery::RecoveryKey::generate()
        .unwrap()
        .display()
        .to_string();
    assert!(
        e.client
            .remove_device(&mut e.store, &mut e.blobs, &a_id, Some(&wrong))
            .is_err()
    );
    assert!(e.client.new_recovery_key(&mut e.store, &wrong).is_err());
    assert_eq!(a.client.devices(&a.store).unwrap().len(), 3);
    // Ben's (member) device was told when the tablet was added.
    b.sync();
    let notices = b.client.status(&b.store).unwrap().notices;
    assert!(
        notices
            .iter()
            .any(|n| n.text.contains("AnaTablet's phone") && n.text.contains("added")),
        "{notices:?}"
    );
    // With the key, the owner removes the tablet; everyone hears of it.
    let e_id = my_id(&mut e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &e_id, Some(&key))
        .unwrap();
    b.sync();
    let notices = b.client.status(&b.store).unwrap().notices;
    assert!(
        notices.iter().any(|n| n.text.contains("removed")),
        "{notices:?}"
    );
    SyncClient::dismiss_notices(&b.store).unwrap();
    assert!(b.client.status(&b.store).unwrap().notices.is_empty());
}

/// M1: a device that learns (from another device's batch) of a roster
/// version the server is hiding from it stops sealing new changes.
#[test]
fn m1_a_hidden_roster_version_stops_sealing() {
    let lab = Lab::new("m1");
    let House {
        mut a,
        mut b,
        e: _e,
        ..
    } = house(&lab);
    // A adds another device: roster version N. The server hides N from
    // everyone but A (deleted after A posted it).
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
    a.room(30, "Gossip carrier");
    a.sync();
    b.room(31, "Ben's change while the roster is hidden");
    let e = b.try_sync().unwrap_err();
    assert!(matches!(e, SyncError::RosterHidden), "{e:?}");
    assert_eq!(
        b.room_name(30).as_deref(),
        Some("Gossip carrier"),
        "pulling still works"
    );
    a.sync();
    assert_eq!(
        a.room_name(31),
        None,
        "nothing was sealed while the list was hidden"
    );
    // The server stops hiding it: everything flows again.
    db.execute(
        "INSERT INTO rosters (space, version, data) VALUES (?1, ?2, ?3)",
        rusqlite::params![sp.to_string(), n, row],
    )
    .unwrap();
    b.sync();
    a.sync();
    assert_eq!(
        a.room_name(31).as_deref(),
        Some("Ben's change while the roster is hidden")
    );
}

/// L2: garbage uploaded under a file's name is dropped and the real file
/// uploaded again; one bad file does not stop the others.
#[test]
fn l2_a_squatted_file_heals() {
    let lab = Lab::new("l2");
    let House { mut a, mut b, .. } = house(&lab);
    let att = a.attach_photo(b"the real photo bytes, nothing else");
    a.sync();
    // The server's copy is replaced with garbage.
    let sp = space(&a);
    let dir = lab.dir.join("blobs").join(sp.to_string());
    for f in std::fs::read_dir(&dir).unwrap() {
        std::fs::write(f.unwrap().path(), b"HWB1 garbage garbage garbage garbage").unwrap();
    }
    let r = b.sync();
    assert_eq!(r.files_failed, 1);
    assert!(!has_blob(&b, &att.sha256));
    let r = a.sync();
    assert_eq!(r.files_up, 1, "uploaded again");
    b.sync();
    assert!(has_blob(&b, &att.sha256));
}

/// L3b: an authentic batch this version cannot read is kept for later,
/// not dropped.
#[test]
fn l3_an_unreadable_batch_is_kept_for_after_an_update() {
    let lab = Lab::new("l3");
    let House { a, mut b, .. } = house(&lab);
    let s = secret(&a);
    let sp = space(&a);
    let head: String =
        serde_json::from_str(&a.store.sync_get("own_head").unwrap().unwrap()).unwrap();
    let seq: u64 = a
        .store
        .sync_get("next_device_seq")
        .unwrap()
        .unwrap()
        .parse()
        .unwrap();
    // "From a newer app": a body this version does not understand.
    let future = br#"{"roster_version":1,"roster_hash":"","ops":[{"kind":"solar_panel"}]}"#;
    let bt = seal_batch(
        &s,
        &chain(&a),
        sp,
        new_batch_id().unwrap(),
        seq,
        &head,
        now(&lab),
        future,
    )
    .unwrap();
    inject(&lab, sp, &bt, None, now(&lab));
    let r = b.sync();
    assert_eq!(r.unreadable, 1);
    assert_eq!(r.rejected_batches, 0);
    assert_eq!(b.store.sync_keys("unread:").unwrap().len(), 1);
}

/// L4: a batch sealed before this device learned of a removal is sealed
/// again under the new key before it is sent.
#[test]
fn l4_a_batch_sealed_before_a_removal_is_resealed() {
    let lab = Lab::new("l4");
    let House {
        mut a,
        mut b,
        mut e,
        key,
    } = house(&lab);
    b.room(40, "Sealed under the old key");
    // B seals, then the connection drops before the push arrives
    // (requests: roster, pull, push).
    b.fail_after.store(2, std::sync::atomic::Ordering::SeqCst);
    let _ = b.try_sync();
    b.fail_after
        .store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
    let e_id = my_id(&mut e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &e_id, Some(&key))
        .unwrap();
    b.sync();
    let db = lab.server_db();
    let data: String = db
        .query_row(
            "SELECT data FROM batches ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let last: SealedBatch = serde_json::from_str(&data).unwrap();
    assert_eq!(last.header.epoch, 1, "re-sealed under the new epoch");
    a.sync();
    assert_eq!(a.room_name(40).as_deref(), Some("Sealed under the old key"));
}

/// L6: the owner's "yes" only counts after the new device said yes.
#[test]
fn l6_the_owner_cannot_finish_before_the_new_device_confirms() {
    let lab = Lab::new("l6");
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
    let _ = n.client.join_poll(&mut n.store, &mut n.blobs).unwrap();
    n.client
        .join_confirm(&mut n.store, &mut n.blobs, true)
        .unwrap();
    let PairState::Compare {
        other_confirmed, ..
    } = a.client.invite_poll(&a.store).unwrap()
    else {
        panic!()
    };
    assert!(other_confirmed);
    assert_eq!(
        a.client
            .invite_confirm(&mut a.store, &mut a.blobs, true)
            .unwrap(),
        PairState::Done
    );
}

/// L7: "stop syncing here" leaves no copy of the device key in the file.
#[test]
fn l7_stop_syncing_erases_the_device_key_from_the_file() {
    let lab = Lab::new("l7");
    let path = lab.dir.join("device-l7.db");
    let mut d = lab.device("Ana");
    d.store = housewright_core::store::Store::open_with_clock(&path, Box::new(lab.clock.clone()))
        .unwrap();
    d.init("Ana");
    d.setup();
    d.sync();
    let secret = d.store.sync_get("device_secret").unwrap().unwrap();
    let chain = d.store.sync_get("chain").unwrap().unwrap();
    d.client.forget(&d.store).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend(std::fs::read(path.with_extension("db-wal")).unwrap_or_default());
    for s in [&secret, &chain] {
        assert!(
            !bytes.windows(s.len()).any(|w| w == s.as_bytes()),
            "a key is still in the file"
        );
    }
}

/// M4: a household cannot store more than the server's quota.
#[test]
fn m4_the_household_quota_is_enforced() {
    let lab = Lab::new("m4");
    lab.quota
        .store(400 * 1024, std::sync::atomic::Ordering::SeqCst);
    lab.restart();
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
    a.sync();
    for i in 0..4u128 {
        a.room(300 + i, &"q".repeat(150 * 1024));
    }
    let e = a.try_sync().unwrap_err();
    assert_eq!(e.code(), "quota", "{e:?}");
}
