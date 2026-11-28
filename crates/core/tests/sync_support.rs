//! The core's support for sync (Phase 6): the clock guard, the list of
//! operations still to push, sync state, and adopting a household.

use housewright_core::hlc::Hlc;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::store::{MAX_CLOCK_AHEAD_MS, ManualClock, Store};
use uuid::Uuid;

fn store(author: u128, clock: &ManualClock) -> Store {
    let mut s = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    s.set_author(MemberId(Uuid::from_u128(author))).unwrap();
    s
}

fn room(id: u128, name: &str) -> Room {
    Room {
        id: RoomId(Uuid::from_u128(id)),
        property: PropertyId(Uuid::from_u128(2)),
        floor: None,
        name: name.into(),
    }
}

#[test]
fn clock_guard_refuses_ops_far_ahead_of_the_reference() {
    let now = 1_800_000_000_000;
    let honest = ManualClock::new(now);
    let wrong = ManualClock::new(now + 3 * 365 * 24 * 3600 * 1000); // three years ahead
    let mut a = store(1, &wrong);
    a.put(&room(1, "Kitchen")).unwrap();
    let mut b = store(2, &honest);
    let r = b.apply_synced_ops(&a.ops().unwrap(), now).unwrap();
    assert_eq!(r.applied, 0);
    assert_eq!(r.too_far_ahead, 3);
    assert!(
        b.get::<Room>(&room(1, "").id.to_string())
            .unwrap()
            .is_none()
    );
    // The receiver's clock was not dragged into the future.
    assert!(b.hlc().wall_ms <= now + MAX_CLOCK_AHEAD_MS);

    // Within the allowance: accepted, and the decision does not depend on
    // the receiver's own clock (a receiver whose clock is behind agrees).
    let slightly = ManualClock::new(now + MAX_CLOCK_AHEAD_MS - 1000);
    let mut c = store(3, &slightly);
    c.put(&room(5, "Hall")).unwrap();
    let behind = ManualClock::new(now - 3_600_000);
    let mut d = store(4, &behind);
    let r = d.apply_synced_ops(&c.ops().unwrap(), now).unwrap();
    assert_eq!(r.applied, 3);
    assert_eq!(r.too_far_ahead, 0);
    let mut e = store(5, &honest);
    assert_eq!(
        e.apply_synced_ops(&c.ops().unwrap(), now).unwrap().applied,
        3
    );
    assert_eq!(
        d.snapshot().unwrap().entities,
        e.snapshot().unwrap().entities
    );
}

#[test]
fn synced_ops_are_not_pushed_back() {
    let clock = ManualClock::new(1000);
    let mut a = store(1, &clock);
    let mut b = store(2, &clock);
    a.put(&room(1, "Kitchen")).unwrap();
    assert_eq!(a.unsynced_count(0).unwrap(), 3);
    b.apply_synced_ops(&a.ops().unwrap(), 1000).unwrap();
    assert_eq!(
        b.unsynced_count(0).unwrap(),
        0,
        "received ops are not b's to push"
    );
    clock.advance(10);
    b.put(&room(1, "Big kitchen")).unwrap();
    let mine = b.unsynced_ops(0, 100).unwrap();
    assert_eq!(mine.len(), 1);
    let (seq, op) = &mine[0];
    assert_eq!(op.field, "name");
    assert_eq!(b.unsynced_ops(*seq, 100).unwrap().len(), 0);
    // Re-applying is idempotent and still not pushable.
    let r = b.apply_synced_ops(&a.ops().unwrap(), 1000).unwrap();
    assert_eq!(r.duplicates, 3);
    assert_eq!(b.unsynced_count(0).unwrap(), 1);
}

#[test]
fn sync_state_and_adopting_a_household() {
    let clock = ManualClock::new(1000);
    let mut first = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    let (h, owner) = first
        .init_household("Home", "Ana", "America/New_York")
        .unwrap();
    let mut joined = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    assert!(
        joined.adopt_household(owner.id).is_err(),
        "nothing has arrived yet"
    );
    joined
        .apply_synced_ops(&first.ops().unwrap(), 1000)
        .unwrap();
    let got = joined.adopt_household(owner.id).unwrap();
    assert_eq!(got.id, h.id);
    assert_eq!(joined.household().unwrap().name, "Home");
    assert_eq!(joined.author().unwrap(), owner.id);
    assert!(joined.adopt_household(owner.id).is_err());

    joined.sync_set("server", "https://example.org").unwrap();
    joined.sync_set_many(&[("a", "1"), ("b", "2")]).unwrap();
    assert_eq!(joined.sync_get("a").unwrap().as_deref(), Some("1"));
    joined.sync_remove("a").unwrap();
    assert_eq!(joined.sync_get("a").unwrap(), None);
    joined.sync_forget().unwrap();
    assert_eq!(joined.sync_get("server").unwrap(), None);
    assert_eq!(joined.household().unwrap().id, h.id, "data stays");
    let _ = Hlc::ZERO;
}
