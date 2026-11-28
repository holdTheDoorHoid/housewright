//! Sync end to end against the real server code: convergence, pairing,
//! removal, recovery, and every attack the design claims to stop.

mod common;

use common::*;
use housewright_core::ids::*;
use housewright_core::model::{Fact, Member, Role, Sensitivity};
use housewright_crypto::batch::{SealedBatch, open_batch};
use housewright_crypto::encoding::unb64;
use housewright_crypto::{DeviceSecret, KeyChain};
use housewright_sync::{Invitee, PairState, SyncError};

fn two_devices(lab: &Lab) -> (Device, Device) {
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
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
    (a, b)
}

#[test]
fn pair_edit_offline_and_converge() {
    let lab = Lab::new("converge");
    let (mut a, mut b) = two_devices(&lab);
    // The new device adopted the household and acts as Ben.
    assert_eq!(b.store.household().unwrap().name, "Our home");
    let ben: Member = b
        .store
        .fetch(&b.store.author().unwrap().to_string())
        .unwrap();
    assert_eq!(ben.name, "Ben");
    assert_eq!(ben.role, Role::Member);

    a.room(1, "Kitchen");
    a.sync();
    b.sync();
    assert_eq!(b.room_name(1).as_deref(), Some("Kitchen"));

    // Both go offline and edit; then reconnect.
    a.set_online(false);
    b.set_online(false);
    lab.clock.advance(1000);
    a.room(1, "Big kitchen");
    lab.clock.advance(1000);
    b.room(2, "Garage");
    assert!(matches!(a.try_sync(), Err(SyncError::Offline(_))));
    a.set_online(true);
    b.set_online(true);
    a.sync();
    b.sync();
    a.sync();
    assert_eq!(a.entities(), b.entities());
    assert_eq!(b.room_name(1).as_deref(), Some("Big kitchen"));
    assert_eq!(a.room_name(2).as_deref(), Some("Garage"));

    // A photo attached on one shows up on the other.
    let photo = b"\xff\xd8 not really a jpeg, but bytes all the same".repeat(50);
    let att = a.attach_photo(&photo);
    let r = a.sync();
    assert_eq!(r.files_up, 1);
    let r = b.sync();
    assert_eq!(r.files_down, 1);
    assert!(has_blob(&b, &att.sha256));
    assert_eq!(a.entities(), b.entities());

    // Devices list: both, named, with last seen.
    let list = a.client.devices(&a.store).unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        list.iter()
            .any(|d| d.name == "Ben's phone" && d.member_name == "Ben" && d.last_seen_ms.is_some())
    );
}

#[test]
fn server_holds_no_plaintext() {
    let lab = Lab::new("plaintext");
    let (mut a, mut b) = two_devices(&lab);
    a.room(1, "Zanzibar-pantry-1234");
    let f = Fact {
        id: FactId::new(),
        property: PropertyId(uuid::Uuid::from_u128(7)),
        category: "security".into(),
        label: "Alarm code".into(),
        value: "Q7-SECRET-ALARM-8812".into(),
        sensitivity: Sensitivity::Sensitive,
        key: None,
        room: None,
    };
    a.store.put(&f).unwrap();
    a.attach_photo(b"PHOTO-BYTES-MARKER-55555 inside a picture of the water heater");
    a.sync();
    b.room(3, "Xylophone-attic-9876");
    b.sync();
    a.sync();
    lab.restart(); // flush the WAL into the main file too
    let bytes = lab.server_bytes();
    assert!(bytes.len() > 10_000, "the server stored something");
    for needle in [
        "Zanzibar-pantry-1234",
        "Q7-SECRET-ALARM-8812",
        "Alarm code",
        "PHOTO-BYTES-MARKER-55555",
        "Xylophone-attic-9876",
        "Our home",
        "Ben's phone",
        "Ana's phone",
        "boiler.jpg",
    ] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
            "the server's files contain '{needle}'"
        );
    }
    // Member ids are visible (by design: they group devices), but not names.
    let hh = a.store.household().unwrap().id.to_string();
    assert!(
        !bytes.windows(hh.len()).any(|w| w == hh.as_bytes()),
        "the household's own id is not on the server"
    );
}

fn stored_batches(lab: &Lab) -> Vec<(String, i64, String)> {
    let db = lab.server_db();
    let mut s = db
        .prepare("SELECT space, seq, data FROM batches ORDER BY space, seq")
        .unwrap();
    s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn set_batch(lab: &Lab, space: &str, seq: i64, b: &SealedBatch) {
    lab.server_db()
        .execute(
            "UPDATE batches SET data = ?3 WHERE space = ?1 AND seq = ?2",
            rusqlite::params![space, seq, serde_json::to_string(b).unwrap()],
        )
        .unwrap();
}

#[test]
fn a_malicious_server_cannot_tamper_with_or_move_operations() {
    let lab = Lab::new("tamper");
    let (mut a, mut b) = two_devices(&lab);
    b.sync();
    a.room(1, "Kitchen");
    a.sync();
    let (space, seq, data) = stored_batches(&lab).into_iter().last().unwrap();
    let original: SealedBatch = serde_json::from_str(&data).unwrap();

    // Flip a ciphertext bit.
    let mut t = original.clone();
    t.ct[10] ^= 1;
    set_batch(&lab, &space, seq, &t);
    let r = b.sync();
    assert_eq!(r.rejected_batches, 1);
    assert_eq!(b.room_name(1), None, "nothing applied");

    // Another household's batch dropped into this household.
    let mut c = lab.device("Cy");
    c.init("Cy");
    c.setup();
    c.room(5, "Cy's den");
    c.sync();
    let (other_space, other_seq, other) = stored_batches(&lab)
        .into_iter()
        .find(|(s, _, _)| *s != space)
        .unwrap();
    let foreign: SealedBatch = serde_json::from_str(&other).unwrap();
    let db = lab.server_db();
    db.execute(
        "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data) VALUES (?1, 999, 'x', 'y', 999, 0, ?2)",
        rusqlite::params![space, serde_json::to_string(&foreign).unwrap()],
    )
    .unwrap();
    let r = b.sync();
    assert_eq!(
        r.rejected_batches, 1,
        "a batch from another household is refused"
    );
    assert_eq!(b.room_name(5), None);
    let _ = (other_space, other_seq);

    // Restore the honest batch: it applies (the cursor has moved on, so
    // re-deliver it as a new row, as a server would after a hiccup).
    db.execute(
        "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data) VALUES (?1, 1000, 'z', 'y', 1000, ?2, ?3)",
        rusqlite::params![space, lab.clock.now_ms_for_test(), serde_json::to_string(&original).unwrap()],
    )
    .unwrap();
    b.sync();
    assert_eq!(b.room_name(1).as_deref(), Some("Kitchen"));
}

trait NowForTest {
    fn now_ms_for_test(&self) -> i64;
}
impl NowForTest for housewright_core::store::ManualClock {
    fn now_ms_for_test(&self) -> i64 {
        use housewright_core::store::WallClock;
        self.now_ms() as i64
    }
}

#[test]
fn the_server_refuses_bad_signatures_and_replays() {
    let lab = Lab::new("replay");
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
    a.room(1, "Kitchen");
    a.sync();
    let log = lab.log.lock().unwrap().clone();
    let push = log
        .iter()
        .rev()
        .find(|r| r.method == "POST" && r.url.ends_with("/ops"))
        .unwrap()
        .clone();
    let pull = log
        .iter()
        .rev()
        .find(|r| r.method == "GET" && r.url.contains("/ops?"))
        .unwrap()
        .clone();
    let app = lab.app.lock().unwrap().clone();

    // The exact same request again: refused as a replay.
    for req in [push.clone(), pull.clone()] {
        let res = send_to(&lab.rt, &app, req).unwrap();
        assert_eq!(res.status, 401);
        assert!(String::from_utf8_lossy(&res.body).contains("replay"));
    }
    // A changed body under the old signature: refused.
    let mut changed = push.clone();
    changed.body.push(b' ');
    for h in changed.headers.iter_mut() {
        if h.0 == "hw-nonce" {
            h.1 = "00".repeat(16);
        }
    }
    let res = send_to(&lab.rt, &app, changed).unwrap();
    assert_eq!(res.status, 401);
    // An old request (outside the five-minute window): refused, with the
    // server's time so an honest client can tell its clock is off.
    lab.clock.advance(10 * 60 * 1000);
    let res = send_to(&lab.rt, &app, pull).unwrap();
    assert_eq!(res.status, 401);
    assert!(String::from_utf8_lossy(&res.body).contains("clock_skew"));
    // A tampered batch is refused by the server itself.
    let mut body: housewright_crypto::wire::PushOps = serde_json::from_slice(&push.body).unwrap();
    body.batches[0].ct[0] ^= 1;
    body.batches[0].header.batch_id = "ab".repeat(16);
    let secret = device_secret(&a);
    let bytes = serde_json::to_vec(&body).unwrap();
    let path = push.url.strip_prefix(URL).unwrap();
    use housewright_core::store::WallClock;
    let auth = housewright_crypto::request::sign_request(
        &secret,
        "POST",
        path,
        lab.clock.now_ms(),
        &bytes,
    )
    .unwrap();
    let mut req = push.clone();
    req.body = bytes;
    req.headers = auth
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let res = send_to(&lab.rt, &app, req).unwrap();
    assert_eq!(res.status, 400, "{}", String::from_utf8_lossy(&res.body));
}

fn device_secret(d: &Device) -> DeviceSecret {
    DeviceSecret::from_bytes(&unb64(&d.store.sync_get("device_secret").unwrap().unwrap()).unwrap())
        .unwrap()
}

fn device_chain(d: &Device) -> KeyChain {
    KeyChain::from_bytes(&unb64(&d.store.sync_get("chain").unwrap().unwrap()).unwrap()).unwrap()
}

#[test]
fn a_removed_device_cannot_pull_or_read_new_data() {
    let lab = Lab::new("remove");
    let (mut a, mut b) = two_devices(&lab);
    b.room(4, "Ben's workshop");
    b.sync();
    a.sync();
    let b_id = b
        .client
        .devices(&b.store)
        .unwrap()
        .into_iter()
        .find(|d| d.this_device)
        .unwrap()
        .id;
    let old_chain = device_chain(&b);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &b_id, None)
        .unwrap();
    assert_eq!(
        device_chain(&a).current_epoch(),
        old_chain.current_epoch() + 1,
        "the key rotated"
    );
    a.room(1, "New secret room");
    a.sync();
    // B is refused by the server.
    let e = b.try_sync().unwrap_err();
    assert!(matches!(e, SyncError::Removed), "{e:?}");
    assert!(b.client.status(&b.store).unwrap().removed);
    assert_eq!(b.room_name(1), None);
    // Even with the ciphertext in hand (a leaked server copy), B's keys
    // cannot open anything from the new epoch.
    let (space, _, data) = stored_batches(&lab).into_iter().last().unwrap();
    let batch: SealedBatch = serde_json::from_str(&data).unwrap();
    assert_eq!(batch.header.epoch, old_chain.current_epoch() + 1);
    let a_pub = device_secret(&a).public();
    assert!(open_batch(&batch, space.parse().unwrap(), &old_chain, &a_pub).is_err());
    // A still has everything, including what B wrote before removal.
    assert_eq!(a.room_name(4).as_deref(), Some("Ben's workshop"));
    // And a third device paired later reads old and new data.
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    assert_eq!(c.room_name(4).as_deref(), Some("Ben's workshop"));
    assert_eq!(c.room_name(1).as_deref(), Some("New secret room"));
}

#[test]
fn recovery_key_restores_everything_on_a_fresh_device() {
    let lab = Lab::new("recovery");
    let mut a = lab.device("Ana");
    a.init("Ana");
    let key = a.setup();
    a.room(1, "Kitchen");
    let att = a.attach_photo(b"the boiler's rating plate, photographed");
    a.sync();
    // Every device is lost. A fresh one, the server address and the paper.
    let mut fresh = lab.device("Fresh");
    // A typo is caught before anything is sent.
    let mut typo = key.clone().into_bytes();
    typo[2] = if typo[2] == b'A' { b'B' } else { b'A' };
    assert!(
        fresh
            .client
            .restore(
                &mut fresh.store,
                &mut fresh.blobs,
                URL,
                std::str::from_utf8(&typo).unwrap(),
                "New phone"
            )
            .is_err()
    );
    let r = fresh
        .client
        .restore(
            &mut fresh.store,
            &mut fresh.blobs,
            URL,
            &key.to_lowercase(),
            "New phone",
        )
        .unwrap();
    assert!(r.pulled_ops > 0);
    assert_eq!(fresh.entities(), a.entities());
    assert!(has_blob(&fresh, &att.sha256));
    assert_eq!(fresh.store.household().unwrap().name, "Our home");
    // The restored device is an owner and can remove the lost one.
    let lost = fresh
        .client
        .devices(&fresh.store)
        .unwrap()
        .into_iter()
        .find(|d| !d.this_device)
        .unwrap();
    fresh
        .client
        .remove_device(&mut fresh.store, &mut fresh.blobs, &lost.id, Some(&key))
        .unwrap();
    assert!(matches!(a.try_sync(), Err(SyncError::Removed)));
    // A replaced recovery key stops working.
    let new_key = fresh
        .client
        .new_recovery_key(&mut fresh.store, &key)
        .unwrap();
    let mut again = lab.device("Again");
    assert!(
        again
            .client
            .restore(&mut again.store, &mut again.blobs, URL, &key, "x")
            .is_err()
    );
    let mut ok = lab.device("Ok");
    ok.client
        .restore(&mut ok.store, &mut ok.blobs, URL, &new_key, "y")
        .unwrap();
    assert_eq!(ok.room_name(1).as_deref(), Some("Kitchen"));
}

#[test]
fn pairing_with_a_wrong_code_or_mismatched_numbers_fails() {
    let lab = Lab::new("pairing");
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
    let inv = a
        .client
        .invite_start(&mut a.store, Invitee::MyDevice)
        .unwrap();
    let mut b = lab.device("Ben");
    // Wrong code (right invitation, wrong secret): cannot even read it.
    let mut wrong: Vec<char> = inv.code.chars().collect();
    let last = wrong.len() - 1;
    wrong[last] = if wrong[last] == 'A' { 'B' } else { 'A' };
    let wrong: String = wrong.into_iter().collect();
    let e = b
        .client
        .join_start(&b.store, URL, &wrong, "Ben's phone")
        .unwrap_err();
    assert!(e.plain().contains("doesn't match"), "{}", e.plain());
    // Right code; the person says the numbers differ.
    b.client
        .join_start(&b.store, URL, &inv.code, "Ben's phone")
        .unwrap();
    let PairState::Compare { .. } = a.client.invite_poll(&a.store).unwrap() else {
        panic!()
    };
    let PairState::Compare { .. } = b.client.join_poll(&mut b.store, &mut b.blobs).unwrap() else {
        panic!()
    };
    assert!(matches!(
        b.client
            .join_confirm(&mut b.store, &mut b.blobs, false)
            .unwrap(),
        PairState::Failed { .. }
    ));
    assert!(matches!(
        a.client.invite_poll(&a.store).unwrap(),
        PairState::Failed { .. }
    ));
    assert!(!housewright_sync::SyncClient::is_set_up(&b.store).unwrap());
    assert_eq!(
        a.client.devices(&a.store).unwrap().len(),
        1,
        "nothing was added"
    );
    // And on the owner's side: "they don't match" adds nobody.
    let inv = a
        .client
        .invite_start(&mut a.store, Invitee::MyDevice)
        .unwrap();
    let mut c = lab.device("Cy");
    c.client
        .join_start(&c.store, URL, &inv.code, "Cy's phone")
        .unwrap();
    a.client.invite_poll(&a.store).unwrap();
    assert!(matches!(
        a.client
            .invite_confirm(&mut a.store, &mut a.blobs, false)
            .unwrap(),
        PairState::Failed { .. }
    ));
    assert!(matches!(
        c.client.join_poll(&mut c.store, &mut c.blobs).unwrap(),
        PairState::Failed { .. }
    ));
    assert_eq!(a.client.devices(&a.store).unwrap().len(), 1);
}

#[test]
fn a_middleman_with_the_code_is_caught_by_the_safety_number() {
    // The worst case for pairing: the server is hostile *and* has the
    // pairing code (say, it saw a photo of the QR). It shows the new
    // device a fake offer with its own keys, and shows the owner a fake
    // answer with other keys of its own. Because the owner reveals r only
    // after the fake answer is fixed, the two screens show different
    // numbers, and the person stops.
    use housewright_crypto::encoding::hex;
    use housewright_crypto::pairing::{PairingCode, Part, commitment};
    use housewright_crypto::wire::{Joiner, Offer};
    let lab = Lab::new("mitm");
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
    let inv = a
        .client
        .invite_start(&mut a.store, Invitee::MyDevice)
        .unwrap();
    let code = PairingCode::parse(&inv.code).unwrap();
    let id = code.invitation_id();
    let db = lab.server_db();
    let get = |col: &str| -> Vec<u8> {
        db.query_row(
            &format!("SELECT {col} FROM invitations WHERE id = ?1"),
            [&id],
            |r| r.get(0),
        )
        .unwrap()
    };
    // 1. The server swaps the offer for one naming its own key.
    let real: Offer =
        serde_json::from_slice(&code.open(Part::Offer, &get("offer")).unwrap()).unwrap();
    let m1 = DeviceSecret::generate().unwrap();
    let r_m = [42u8; 32];
    let mut fake = real.clone();
    fake.inviter = m1.public();
    fake.commit = hex(&commitment(&r_m));
    let fake_offer = code
        .seal(Part::Offer, &serde_json::to_vec(&fake).unwrap())
        .unwrap();
    db.execute(
        "UPDATE invitations SET offer = ?2 WHERE id = ?1",
        rusqlite::params![id, fake_offer],
    )
    .unwrap();
    // 2. Ben's device joins (it reads the fake offer).
    let mut ben = lab.device("Ben");
    ben.client
        .join_start(&ben.store, URL, &inv.code, "Ben's phone")
        .unwrap();
    // 3. The server swaps Ben's answer for one with its second key.
    let m2 = DeviceSecret::generate().unwrap();
    let fake_join = code
        .seal(
            Part::Request,
            &serde_json::to_vec(&Joiner {
                device: m2.public(),
                label: "Ben's phone".into(),
            })
            .unwrap(),
        )
        .unwrap();
    db.execute(
        "UPDATE invitations SET request = ?2 WHERE id = ?1",
        rusqlite::params![id, fake_join],
    )
    .unwrap();
    // 4. Ana's device sees "Ben", reveals r.
    let PairState::Compare {
        safety_number: on_ana,
        ..
    } = a.client.invite_poll(&a.store).unwrap()
    else {
        panic!()
    };
    // 5. The server swaps the reveal for its own r (matching its commit).
    let fake_reveal = code.seal(Part::Reveal, &r_m).unwrap();
    db.execute(
        "UPDATE invitations SET reveal = ?2 WHERE id = ?1",
        rusqlite::params![id, fake_reveal],
    )
    .unwrap();
    let PairState::Compare {
        safety_number: on_ben,
        ..
    } = ben
        .client
        .join_poll(&mut ben.store, &mut ben.blobs)
        .unwrap()
    else {
        panic!()
    };
    assert_ne!(on_ana, on_ben, "the numbers differ, so the person stops");
    // Even if both people tapped "match" by mistake, Ben's device refuses
    // the household: the roster adds the attacker's key, not Ben's.
    ben.client
        .join_confirm(&mut ben.store, &mut ben.blobs, true)
        .unwrap();
    a.client
        .invite_confirm(&mut a.store, &mut a.blobs, true)
        .unwrap();
    assert!(
        ben.client
            .join_poll(&mut ben.store, &mut ben.blobs)
            .is_err()
    );
    assert!(!housewright_sync::SyncClient::is_set_up(&ben.store).unwrap());
}

#[test]
fn clock_guard_end_to_end() {
    let lab = Lab::new("clock");
    let mut a = lab.device("Ana");
    a.init("Ana");
    a.setup();
    a.sync();
    let mut b = lab.device_with_clock(
        "Ben",
        housewright_core::store::ManualClock::new(lab.clock_now()),
    );
    pair(
        &mut a,
        &mut b,
        Invitee::NewPerson {
            name: "Ben".into(),
            role: Role::Member,
        },
    );
    // B's clock is an hour fast. Its requests still authenticate (the
    // client corrects request times by the server's), but its changes
    // are held back and the person is told why.
    b.clock.set(lab.clock_now() + 3_600_000);
    b.room(7, "Made with a wrong clock");
    let e = b.try_sync().unwrap_err();
    assert!(matches!(e, SyncError::ClockAhead), "{e:?}");
    let st = b.client.status(&b.store).unwrap();
    assert!(st.clock_ahead);
    assert!(st.last_error.unwrap().contains("clock"));
    assert!(st.clock_offset_ms.unwrap() < -3_000_000);
    a.sync();
    assert_eq!(a.room_name(7), None, "nothing from the future arrived");
    assert!(
        a.store.hlc().wall_ms < lab.clock_now() + 300_000,
        "A's clock was not dragged ahead"
    );
    // Once real time catches up, the held changes go through.
    lab.clock.advance(3_600_000);
    b.clock.set(lab.clock_now());
    b.sync();
    a.sync();
    assert_eq!(a.room_name(7).as_deref(), Some("Made with a wrong clock"));

    // A server that lies about when it received a batch (to slip a
    // future clock in) is caught by every receiver the same way.
    a.room(8, "Honest");
    a.sync();
    let (space, seq, _) = stored_batches(&lab).into_iter().last().unwrap();
    lab.server_db()
        .execute(
            "UPDATE batches SET received_ms = received_ms - 3600000 WHERE space = ?1 AND seq = ?2",
            rusqlite::params![space, seq],
        )
        .unwrap();
    let r = b.sync();
    assert_eq!(r.rejected_batches, 1);
    assert_eq!(b.room_name(8), None);
}

#[test]
fn a_member_cannot_write_in_someone_elses_name() {
    let lab = Lab::new("author");
    let (mut a, mut b) = two_devices(&lab);
    // Ben's device writes an op claiming to be Ana (forging the author).
    let ana = a.store.author().unwrap();
    b.store.set_author(ana).unwrap();
    b.room(9, "Forged by Ben");
    b.sync();
    a.sync();
    assert_eq!(
        a.room_name(9),
        None,
        "refused: members write only as themselves"
    );
    assert!(a.client.status(&a.store).unwrap().rejected_batches >= 1);
}

#[test]
fn server_restart_and_dropped_connections_mid_sync_are_safe() {
    let lab = Lab::new("restart");
    let (mut a, mut b) = two_devices(&lab);
    for i in 0..30u128 {
        a.room(100 + i, &format!("Room {i}"));
    }
    // The connection drops after the push is sent but before the reply
    // could be read (fail on the request after the push).
    for cut in 1..6 {
        a.fail_after.store(cut, std::sync::atomic::Ordering::SeqCst);
        let _ = a.try_sync();
        lab.restart();
        a.fail_after
            .store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
        a.room(200 + cut as u128, "after a cut");
    }
    a.sync();
    b.sync();
    a.sync();
    assert_eq!(a.entities(), b.entities());
    // No batch was stored twice.
    let db = lab.server_db();
    let dup: i64 = db
        .query_row("SELECT COUNT(*) FROM (SELECT batch_id FROM batches GROUP BY space, batch_id HAVING COUNT(*) > 1)", [], |r| r.get(0))
        .unwrap();
    assert_eq!(dup, 0);
    assert_eq!(
        housewright_sync::SyncClient::pending_count(&a.store).unwrap(),
        0
    );
}

trait LabNow {
    fn clock_now(&self) -> u64;
}
impl LabNow for Lab {
    fn clock_now(&self) -> u64 {
        use housewright_core::store::WallClock;
        self.clock.now_ms()
    }
}
