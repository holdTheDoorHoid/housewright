//! Regression tests for the second verification pass (P1, P2, P3, L-a).
//! P1 to P3 port the reviewer's attack tests (`verify/tests/v.rs`) with the
//! assertions turned around.

mod common;

use common::*;
use housewright_core::model::*;
use housewright_core::store::WallClock;
use housewright_crypto::batch::{batch_hash, new_batch_id, seal_batch};
use housewright_crypto::encoding::unb64;
use housewright_crypto::roster::{DeviceRole, RosterEntry, SignedRoster};
use housewright_crypto::wire::BatchBody;
use housewright_crypto::{DeviceSecret, KeyChain, SpaceId};
use housewright_sync::{HttpRequest, Invitee, SyncClient, SyncError};

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
fn signed(lab: &Lab, s: &DeviceSecret, method: &str, path: &str, body: Vec<u8>) -> u16 {
    let auth =
        housewright_crypto::request::sign_request(s, method, path, lab.clock.now_ms(), &body)
            .unwrap();
    let req = HttpRequest {
        method: method.into(),
        url: format!("{URL}{path}"),
        headers: auth
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        body,
    };
    send_to(&lab.rt, &lab.app.lock().unwrap().clone(), req)
        .unwrap()
        .status
}
fn push_raw(lab: &Lab, d: &mut Device, body: &BatchBody) -> u16 {
    let s = secret(d);
    let sp = space(d);
    let seq: u64 =
        serde_json::from_str(&d.store.sync_get("next_device_seq").unwrap().unwrap()).unwrap();
    let head: String =
        serde_json::from_str(&d.store.sync_get("own_head").unwrap().unwrap()).unwrap();
    let b = seal_batch(
        &s,
        &chain(d),
        sp,
        new_batch_id().unwrap(),
        seq,
        &head,
        lab.clock.now_ms(),
        &serde_json::to_vec(body).unwrap(),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&housewright_crypto::wire::PushOps {
        batches: vec![b.clone()],
    })
    .unwrap();
    let st = signed(lab, &s, "POST", &format!("/v1/s/{sp}/ops"), bytes);
    d.store
        .sync_set("next_device_seq", &(seq + 1).to_string())
        .unwrap();
    d.store
        .sync_set(
            "own_head",
            &serde_json::to_string(&batch_hash(&b).unwrap()).unwrap(),
        )
        .unwrap();
    d.store.sync_set("acked_seq", &seq.to_string()).unwrap();
    d.store
        .sync_set(
            &format!("sent:{seq:020}"),
            &serde_json::json!({"batch": b, "upto": 0}).to_string(),
        )
        .unwrap();
    st
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

/// P1: a stolen owner phone (no paper) signs a bogus "newer" roster and
/// ships it as a claim. It no longer freezes anyone.
#[test]
fn p1_a_bogus_signed_claim_freezes_nobody() {
    let lab = Lab::new("p1");
    let (mut a, mut b, mut e, _k) = house(&lab);
    let mut body = rosters(&e).last().unwrap().parse().unwrap();
    body.version = 999_999;
    let claim = SignedRoster::sign(&body, &secret(&e)).unwrap();
    assert_eq!(
        push_raw(
            &lab,
            &mut e,
            &BatchBody {
                rosters: vec![claim],
                ops: serde_json::json!([])
            }
        ),
        200
    );
    a.room(50, "after");
    a.try_sync().expect("Ana's phone still sends");
    a.try_sync().expect("and keeps sending");
    b.sync();
    assert_eq!(b.room_name(50).as_deref(), Some("after"));
}

/// P1: a real (verifiable) newer version that the stolen phone keeps from
/// the server does pause sending, says so plainly, and the person can
/// clear it with OK.
#[test]
fn p1_a_verified_but_unpublished_version_can_be_cleared() {
    let lab = Lab::new("p1b");
    let (mut a, _b, mut e, _k) = house(&lab);
    let rs = rosters(&e);
    let latest = rs.last().unwrap();
    let mut next = latest
        .parse()
        .unwrap()
        .successor(latest, lab.clock.now_ms());
    let ghost = DeviceSecret::generate().unwrap();
    next.devices.push(RosterEntry {
        device: ghost.public(),
        member: "ghost".into(),
        role: DeviceRole::Owner,
        added_in: next.version,
        label: vec![0; 4],
    });
    next.sort();
    let mut claims = rs.clone();
    claims.push(SignedRoster::sign(&next, &secret(&e)).unwrap());
    assert_eq!(
        push_raw(
            &lab,
            &mut e,
            &BatchBody {
                rosters: claims,
                ops: serde_json::json!([])
            }
        ),
        200
    );
    a.room(60, "kept until OK");
    assert!(matches!(a.try_sync(), Err(SyncError::RosterHidden)));
    let st = a.client.status(&a.store).unwrap();
    assert!(
        st.notices
            .iter()
            .any(|n| n.text.contains("AnaTablet") && n.text.contains("press OK")),
        "{:?}",
        st.notices
    );
    SyncClient::dismiss_notices(&a.store).unwrap();
    a.try_sync().expect("sending again after OK");
}

/// P2: a member marks a good file bad and uploads garbage under its name.
/// The original copy survives and a new device opens the file.
#[test]
fn p2_a_member_cannot_replace_a_file() {
    let lab = Lab::new("p2");
    let (mut a, mut b, _e, _k) = house(&lab);
    let att = a.attach_photo(b"the only copy of the boiler warranty photo");
    a.sync();
    b.sync();
    let sp = space(&b);
    let name = housewright_crypto::blob::blob_name(&chain(&b), sp, &att.sha256).unwrap();
    let dir = lab.dir.join("blobs").join(sp.to_string());
    let before: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|f| std::fs::read(f.unwrap().path()).unwrap())
        .collect();
    let s = secret(&b);
    assert_eq!(
        signed(
            &lab,
            &s,
            "POST",
            &format!("/v1/s/{sp}/blobs/{name}/bad"),
            vec![]
        ),
        200
    );
    assert_eq!(
        signed(
            &lab,
            &s,
            "PUT",
            &format!("/v1/s/{sp}/blobs/{name}"),
            vec![0u8; 4200]
        ),
        200
    );
    let after: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|f| std::fs::read(f.unwrap().path()).unwrap())
        .collect();
    assert!(
        before.iter().all(|f| after.contains(f)),
        "the original copy is untouched"
    );
    // Ana's phone is lost; a new phone (restored with the recovery key
    // would be the same) opens the file from the original copy.
    let mut d = lab.device("Dee");
    pair(&mut a, &mut d, Invitee::MyDevice);
    a.set_online(false);
    assert!(
        d.client
            .fetch_file(&d.store, &mut d.blobs, &att.sha256)
            .unwrap()
            .is_some()
    );
}

/// P3: a server restored from a backup taken before a removal: everyone
/// converges, and the removed device is revoked again.
#[test]
fn p3_a_restore_across_a_removal_converges() {
    let lab = Lab::new("p3");
    let (mut a, mut b, mut e, key) = house(&lab);
    b.sync();
    e.room(9, "tablet before");
    e.sync();
    a.sync();
    b.sync();
    *lab.app.lock().unwrap() = axum::Router::new();
    let backup = lab.dir.with_extension("backup");
    copy_dir(&lab.dir, &backup);
    lab.restart();
    e.room(10, "tablet after backup");
    e.sync();
    a.sync();
    let eid = my_id(&mut e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &eid, Some(&key))
        .unwrap();
    b.sync();
    b.room(21, "ben after removal");
    b.sync();
    *lab.app.lock().unwrap() = axum::Router::new();
    std::fs::remove_dir_all(&lab.dir).unwrap();
    copy_dir(&backup, &lab.dir);
    lab.restart();
    for _ in 0..3 {
        a.sync();
        b.sync();
    }
    let revoked: i64 = lab
        .server_db()
        .query_row(
            "SELECT revoked FROM devices WHERE id = ?1",
            [eid.clone()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(revoked, 1, "the removed tablet is revoked again");
    assert!(matches!(e.try_sync(), Err(SyncError::Removed)));
    assert_eq!(a.room_name(21).as_deref(), Some("ben after removal"));
    assert_eq!(a.room_name(10).as_deref(), Some("tablet after backup"));
    assert_eq!(a.entities(), b.entities());
    let _ = std::fs::remove_dir_all(&backup);
}

/// L-a: a removed device and the server fill every candidate slot with
/// forks of its own chain before its real batches arrive; the real ones
/// are still applied (fetched by number while walking back from the
/// recorded hash).
#[test]
fn l_a_forks_cannot_block_a_removed_devices_real_batches() {
    let lab = Lab::new("la");
    let (mut a, _b, mut e, key) = house(&lab);
    e.room(10, "tablet one");
    e.sync();
    e.room(11, "tablet two");
    e.sync();
    a.sync();
    let es = secret(&e);
    let ec = chain(&e);
    let sp = space(&e);
    let eid = my_id(&mut e);
    a.client
        .remove_device(&mut a.store, &mut a.blobs, &eid, Some(&key))
        .unwrap();
    let db = lab.server_db();
    db.execute(
        "UPDATE batches SET seq = seq + 1000 WHERE space = ?1",
        [sp.to_string()],
    )
    .unwrap();
    let mut server_seq = 1;
    for n in 1..=2u64 {
        for i in 0..5 {
            let f = seal_batch(
                &es,
                &ec,
                sp,
                new_batch_id().unwrap(),
                n,
                housewright_crypto::batch::ZERO_HASH,
                lab.clock.now_ms() + i,
                b"{\"ops\":[]}",
            )
            .unwrap();
            db.execute(
                "INSERT INTO batches (space, seq, batch_id, device, device_seq, received_ms, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![sp.to_string(), server_seq, f.header.batch_id, format!("fork{server_seq}"), n as i64, lab.clock.now_ms() as i64, serde_json::to_string(&f).unwrap()],
            )
            .unwrap();
            server_seq += 1;
        }
    }
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    assert_eq!(c.room_name(10).as_deref(), Some("tablet one"));
    assert_eq!(c.room_name(11).as_deref(), Some("tablet two"));
}

/// Q1: a stolen owner phone re-signing the current roster (a "fork") can
/// pause sending once; after the person's OK it never pauses it again.
#[test]
fn q1_a_fork_claim_pauses_at_most_once_per_device() {
    let lab = Lab::new("q1");
    let (mut a, _b, mut e, _k) = house(&lab);
    let mut c = lab.device("Cy");
    pair(&mut a, &mut c, Invitee::MyDevice);
    e.sync();
    let mut paused = 0;
    for i in 0..3u64 {
        let rs = rosters(&e);
        let mut body = rs.last().unwrap().parse().unwrap();
        body.made_ms += 1 + i;
        let claim = SignedRoster::sign(&body, &secret(&e)).unwrap();
        assert_eq!(
            push_raw(
                &lab,
                &mut e,
                &BatchBody {
                    rosters: vec![claim],
                    ops: serde_json::json!([])
                }
            ),
            200
        );
        a.room(60 + i as u128, "x");
        if a.try_sync().is_err() {
            paused += 1;
        }
        SyncClient::dismiss_notices(&a.store).unwrap();
    }
    assert!(paused <= 1, "paused {paused} times");
    a.try_sync().expect("sending after the last OK");
}

/// Q2: replacing one's own file copy with a bigger one counts against the
/// quota.
#[test]
fn q2_overwriting_ones_own_copy_respects_the_quota() {
    let lab = Lab::new("q2");
    let (_a, b, _e, _k) = house(&lab);
    let sp = space(&b);
    let s = secret(&b);
    let names: Vec<String> = (0..4u32).map(|i| format!("{:032x}", i + 1)).collect();
    for n in &names {
        assert_eq!(
            signed(
                &lab,
                &s,
                "PUT",
                &format!("/v1/s/{sp}/blobs/{n}"),
                vec![1u8; 64]
            ),
            200
        );
    }
    let db = lab.server_db();
    db.execute(
        "UPDATE blob_copies SET size = 2147483000 WHERE space = ?1 AND name = ?2",
        rusqlite::params![sp.to_string(), names[0]],
    )
    .unwrap();
    let st_new = signed(
        &lab,
        &s,
        "PUT",
        &format!("/v1/s/{sp}/blobs/{:032x}", 99),
        vec![2u8; 4096],
    );
    let st_over = signed(
        &lab,
        &s,
        "PUT",
        &format!("/v1/s/{sp}/blobs/{}", names[1]),
        vec![3u8; 20 * 1024 * 1024],
    );
    assert_eq!(st_new, 507);
    assert_eq!(
        st_over, 507,
        "growing one's own copy past the quota is refused"
    );
    // Replacing it with something the same size or smaller still works.
    assert_eq!(
        signed(
            &lab,
            &s,
            "PUT",
            &format!("/v1/s/{sp}/blobs/{}", names[1]),
            vec![4u8; 64]
        ),
        200
    );
}

/// Low: `restore: true` does not let a removal skip the check that the
/// device has not sent more than the removal names.
#[test]
fn restore_flag_does_not_reopen_the_removal_race() {
    use housewright_crypto::roster::RemovedEntry;
    use housewright_crypto::wire::UpdateRoster;
    let lab = Lab::new("rr");
    let (mut a, _b, mut e, key) = house(&lab);
    e.room(10, "one");
    e.sync();
    a.sync();
    e.room(11, "sent after Ana last synced");
    e.sync();
    let rs = rosters(&a);
    let latest_signed = rs.last().unwrap();
    let latest = latest_signed.parse().unwrap();
    let eid = my_id(&mut e);
    let heads: serde_json::Value =
        serde_json::from_str(&a.store.sync_get("heads").unwrap().unwrap()).unwrap();
    let h = &heads[&eid];
    let mut next = latest.successor(latest_signed, lab.clock.now_ms());
    let pos = next
        .devices
        .iter()
        .position(|d| d.device.id.to_string() == eid)
        .unwrap();
    let entry = next.devices.remove(pos);
    next.removed.push(RemovedEntry {
        entry,
        removed_in: next.version,
        last_seq: h["seq"].as_u64().unwrap(),
        last_hash: h["hash"].as_str().unwrap().to_string(),
    });
    next.epoch += 1;
    next.sort();
    let s = secret(&a);
    let mut signed_next = SignedRoster::sign(&next, &s).unwrap();
    signed_next.cosign(
        &housewright_crypto::recovery::RecoveryKey::parse(&key)
            .unwrap()
            .identity(),
    );
    let new_chain = chain(&a).rotated().unwrap();
    let sp = space(&a);
    let envelopes = next
        .devices
        .iter()
        .map(|d| housewright_crypto::wrap::wrap(&s, sp, &d.device, &new_chain).unwrap())
        .collect();
    let req = UpdateRoster {
        roster: signed_next,
        envelopes,
        anchor: None,
        invitation: None,
        restore: true,
    };
    let st = signed(
        &lab,
        &s,
        "POST",
        &format!("/v1/s/{sp}/roster"),
        serde_json::to_vec(&req).unwrap(),
    );
    assert_eq!(
        st, 409,
        "the tablet sent more than the removal names: refused even with restore"
    );
}
