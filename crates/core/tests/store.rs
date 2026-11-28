//! Storage and merge semantics, case by case.

use housewright_core::hlc::Hlc;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::oplog::{DELETED_FIELD, Op};
use housewright_core::store::{ManualClock, Store};
use jiff::Timestamp;
use serde_json::json;
use uuid::Uuid;

fn store(author: u128, clock: &ManualClock) -> Store {
    let mut s = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    s.set_author(MemberId(Uuid::from_u128(author))).unwrap();
    s
}

fn room(name: &str) -> Room {
    Room {
        id: RoomId(Uuid::from_u128(1)),
        property: PropertyId(Uuid::from_u128(2)),
        floor: None,
        name: name.into(),
    }
}

fn completion(task: u128) -> Completion {
    Completion {
        id: CompletionId::new(),
        task: TaskId(Uuid::from_u128(task)),
        by: MemberId(Uuid::from_u128(1)),
        at: Timestamp::from_second(1_800_000_000).unwrap(),
        notes: None,
        cost: None,
        values: Default::default(),
        contractor: None,
        attachments: vec![],
        voids: None,
    }
}

#[test]
fn put_emits_only_changed_fields_and_null_clears() {
    let c = ManualClock::new(10);
    let mut s = store(1, &c);
    let mut r = room("Kitchen");
    assert_eq!(s.put(&r).unwrap(), 3); // id, property, name
    assert_eq!(s.put(&r).unwrap(), 0);
    r.name = "Big kitchen".into();
    assert_eq!(s.put(&r).unwrap(), 1);
    r.floor = Some(FloorId(Uuid::from_u128(9)));
    assert_eq!(s.put(&r).unwrap(), 1);
    r.floor = None;
    assert_eq!(s.put(&r).unwrap(), 1);
    let back: Room = s.fetch(&r.id.to_string()).unwrap();
    assert_eq!(back, r);
    assert_eq!(s.ops().unwrap().len(), 6);
    assert_eq!(s.list::<Room>().unwrap(), vec![r]);
}

#[test]
fn last_writer_wins_per_field_with_author_tie_break() {
    let clock = ManualClock::new(100);
    let mut a = store(1, &clock);
    let mut b = store(2, &clock);
    let base = room("Kitchen");
    a.put(&base).unwrap();
    b.apply_remote_ops(&a.ops().unwrap()).unwrap();

    // Concurrent edits to different fields both survive.
    let mut ra = base.clone();
    ra.name = "Galley".into();
    let mut rb = base.clone();
    rb.floor = Some(FloorId(Uuid::from_u128(7)));
    a.put(&ra).unwrap();
    b.put(&rb).unwrap();
    a.apply_remote_ops(&b.ops().unwrap()).unwrap();
    b.apply_remote_ops(&a.ops().unwrap()).unwrap();
    let merged: Room = a.fetch(&base.id.to_string()).unwrap();
    assert_eq!(merged.name, "Galley");
    assert_eq!(merged.floor, Some(FloorId(Uuid::from_u128(7))));
    assert_eq!(a.snapshot().unwrap(), b.snapshot().unwrap());

    // Same field, and (since the shared clock has not moved) the same HLC
    // wall time: higher author id wins on both sides.
    let mut ra = merged.clone();
    ra.name = "From A".into();
    let mut rb = merged.clone();
    rb.name = "From B".into();
    a.put(&ra).unwrap();
    b.put(&rb).unwrap();
    a.apply_remote_ops(&b.ops().unwrap()).unwrap();
    b.apply_remote_ops(&a.ops().unwrap()).unwrap();
    let ha = a.fetch::<Room>(&base.id.to_string()).unwrap().name;
    let hb = b.fetch::<Room>(&base.id.to_string()).unwrap().name;
    assert_eq!(ha, hb);
    assert_eq!(ha, "From B");
}

#[test]
fn causally_later_edit_wins_even_with_a_slow_clock() {
    let fast = ManualClock::new(1_000_000);
    let slow = ManualClock::new(5);
    let mut a = store(9, &fast);
    let mut b = store(1, &slow);
    a.put(&room("A's name")).unwrap();
    b.apply_remote_ops(&a.ops().unwrap()).unwrap();
    // B saw A's edit, so B's next HLC is past it despite B's clock.
    b.put(&room("B's later name")).unwrap();
    a.apply_remote_ops(&b.ops().unwrap()).unwrap();
    assert_eq!(
        a.fetch::<Room>(&room("").id.to_string()).unwrap().name,
        "B's later name"
    );
    assert!(
        b.hlc()
            > Hlc {
                wall_ms: 1_000_000,
                counter: 0
            }
    );
}

#[test]
fn delete_wins_over_concurrent_and_later_edits() {
    let clock = ManualClock::new(100);
    let mut a = store(1, &clock);
    let mut b = store(2, &clock);
    a.put(&room("Kitchen")).unwrap();
    b.apply_remote_ops(&a.ops().unwrap()).unwrap();
    a.delete(EntityKind::Room, &room("").id.to_string())
        .unwrap();
    clock.advance(1000);
    b.put(&room("Renamed after the delete")).unwrap();
    a.apply_remote_ops(&b.ops().unwrap()).unwrap();
    b.apply_remote_ops(&a.ops().unwrap()).unwrap();
    assert!(a.get::<Room>(&room("").id.to_string()).unwrap().is_none());
    assert_eq!(a.snapshot().unwrap(), b.snapshot().unwrap());
    assert!(a.list::<Room>().unwrap().is_empty());
    // Editing a deleted entity locally is refused.
    assert!(a.put(&room("x")).is_err());
}

#[test]
fn completions_are_append_only() {
    let clock = ManualClock::new(100);
    let mut s = store(1, &clock);
    let c = completion(5);
    s.append(&c).unwrap();
    assert!(s.append(&c).is_err(), "cannot rewrite a completion");
    assert!(s.put(&c).is_err(), "cannot field-edit a completion");
    assert!(s.delete(EntityKind::Completion, &c.id.to_string()).is_err());
    assert!(s.append(&room("x")).is_err(), "rooms are not append-only");

    // Hostile or buggy remote ops are ignored deterministically.
    let field_edit = Op {
        id: OpId::new(),
        hlc: Hlc {
            wall_ms: 999_999,
            counter: 0,
        },
        author: MemberId(Uuid::from_u128(3)),
        kind: EntityKind::Completion,
        entity: c.id.to_string(),
        field: "task".into(),
        value: json!(Uuid::from_u128(77).to_string()),
    };
    let tombstone = Op {
        id: OpId::new(),
        field: DELETED_FIELD.into(),
        value: json!(true),
        ..field_edit.clone()
    };
    s.apply_remote_ops(&[field_edit, tombstone]).unwrap();
    assert_eq!(s.fetch::<Completion>(&c.id.to_string()).unwrap(), c);
}

#[test]
fn file_store_persists_hlc_author_and_schema() {
    let dir = std::env::temp_dir().join(format!("hw-test-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("h.db");
    let hlc;
    {
        let mut s = Store::open(&path).unwrap();
        s.set_author(MemberId(Uuid::from_u128(4))).unwrap();
        s.put(&room("Hall")).unwrap();
        hlc = s.hlc();
        assert_eq!(s.schema_version().unwrap(), 2);
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.hlc(), hlc);
    assert_eq!(s.author().unwrap(), MemberId(Uuid::from_u128(4)));
    assert_eq!(s.list::<Room>().unwrap().len(), 1);
    assert_eq!(s.schema_version().unwrap(), 2);
    drop(s);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn incomplete_remote_entities_read_as_absent() {
    let clock = ManualClock::new(100);
    let mut a = store(1, &clock);
    let mut b = store(2, &clock);
    a.put(&room("Kitchen")).unwrap();
    let ops = a.ops().unwrap();
    // Only the name arrives at first.
    let name_only: Vec<Op> = ops.iter().filter(|o| o.field == "name").cloned().collect();
    b.apply_remote_ops(&name_only).unwrap();
    assert!(b.get::<Room>(&room("").id.to_string()).unwrap().is_none());
    assert!(b.list::<Room>().unwrap().is_empty());
    b.apply_remote_ops(&ops).unwrap();
    assert!(b.get::<Room>(&room("").id.to_string()).unwrap().is_some());
}
