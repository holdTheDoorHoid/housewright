//! End to end through the repository: templates, start small, completion,
//! snooze, voiding, roles, audit and export/import.

use std::path::PathBuf;

use housewright_core::audit::audit_log;
use housewright_core::export::{self, Export};
use housewright_core::fields::{CustomValue, FieldValue};
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::schedule::{Climate, TaskState};
use housewright_core::store::{ManualClock, Store};
use housewright_core::template::Library;
use jiff::Timestamp;
use jiff::civil::date;
use jiff::tz::TimeZone;

/// 2026-11-27 12:00 UTC.
const NOW_MS: u64 = 1_795_780_800_000;

fn lib() -> Library {
    Library::load_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates")).unwrap()
}

fn now(clock: &ManualClock) -> Timestamp {
    use housewright_core::store::WallClock;
    Timestamp::from_millisecond(clock.now_ms() as i64).unwrap()
}

struct Home {
    store: Store,
    clock: ManualClock,
    owner: Member,
    property: Property,
}

fn home() -> Home {
    let clock = ManualClock::new(NOW_MS);
    let mut store = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    let (_, owner) = store
        .init_household("The Testers", "Alex", "America/New_York")
        .unwrap();
    let property = Property {
        id: PropertyId::new(),
        name: "Home".into(),
        address: Some(Flagged::sensitive("1 Example St".into())),
        time_zone: "America/New_York".into(),
        climate: Climate {
            first_frost_doy: Some(300),
            ..Climate::default()
        },
    };
    store.put(&property).unwrap();
    Home {
        store,
        clock,
        owner,
        property,
    }
}

fn item(h: &Home, name: &str, template: &str) -> Item {
    Item {
        id: ItemId::new(),
        property: h.property.id,
        room: None,
        parent: None,
        name: name.into(),
        category: None,
        make: None,
        model: None,
        serial: Some(Flagged::normal("SN1".into())),
        purchased: None,
        installed: Some(date(2024, 1, 1)),
        warranty: None,
        location_note: None,
        template: Some(template.into()),
        extra_fields: vec![],
        custom: Default::default(),
        sensitivity: Sensitivity::Normal,
        price: Default::default(),
        value: Default::default(),
    }
}

fn populated() -> Home {
    let mut h = home();
    let lib = lib();
    let owner = h.owner.id;
    for (name, tid) in [
        ("Hall smoke alarm", "smoke-alarm"),
        ("Hall CO alarm", "co-alarm"),
        ("Gutters", "gutters"),
        ("Fridge", "refrigerator"),
    ] {
        let it = item(&h, name, tid);
        let t = lib.resolve(tid).unwrap();
        h.store
            .add_item_from_template(&it, &t, owner, date(2026, 11, 27))
            .unwrap();
    }
    let mut wh = item(&h, "Water heater", "water-heater");
    wh.custom.insert(
        "fuel".into(),
        CustomValue {
            value: FieldValue::Choice("gas".into()),
            sensitivity: Sensitivity::Normal,
        },
    );
    let tasks = h
        .store
        .add_item_from_template(
            &wh,
            &lib.resolve("water-heater").unwrap(),
            owner,
            date(2026, 11, 27),
        )
        .unwrap();
    assert!(
        tasks.iter().any(|t| t.title.contains("flue")),
        "gas-only task included"
    );
    h
}

#[test]
fn start_small_switches_on_life_safety_first() {
    let mut h = populated();
    let all: Vec<Task> = h.store.list().unwrap();
    assert!(all.len() > 10);
    assert!(all.iter().all(|t| !t.active));
    let picked = h.store.start_small(5, now(&h.clock)).unwrap();
    assert_eq!(picked.len(), 5);
    for id in &picked {
        let t: Task = h.store.fetch(&id.to_string()).unwrap();
        assert!(t.active);
        assert_eq!(
            t.priority,
            housewright_core::priority::PriorityClass::LifeSafety
        );
    }
    let active = h
        .store
        .ranked_tasks(h.owner.id, now(&h.clock), false)
        .unwrap();
    assert_eq!(active.len(), 5);
    // History reads in plain words, not field names.
    let log = audit_log(&h.store, &TimeZone::get("America/New_York").unwrap()).unwrap();
    assert!(
        log.iter()
            .any(|e| e.text.starts_with("Alex switched on task '")),
        "{:#?}",
        log.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
    assert!(!log.iter().any(|e| e.text.contains("changed active")));
}

#[test]
fn complete_snooze_void_and_audit() {
    let mut h = populated();
    let task = h
        .store
        .list::<Task>()
        .unwrap()
        .into_iter()
        .find(|t| t.title == "Test the smoke alarm")
        .unwrap();
    let st = h.store.task_status(&task, now(&h.clock)).unwrap();
    assert_eq!(st.eval.state, TaskState::Due);

    // One tap, no notes.
    let c = h.store.complete_task(task.id, None, None).unwrap();
    let st = h.store.task_status(&task, now(&h.clock)).unwrap();
    assert_eq!(st.eval.state, TaskState::Scheduled);
    assert_eq!(st.eval.window.unwrap().due, date(2026, 12, 27));

    // Logged by mistake: void it, and the task is owed again.
    h.clock.advance(60_000);
    h.store.void_completion(c.id, Some("wrong alarm")).unwrap();
    let st = h.store.task_status(&task, now(&h.clock)).unwrap();
    assert_eq!(st.eval.state, TaskState::Due);
    assert!(
        h.store.void_completion(c.id, None).is_ok(),
        "voiding twice is harmless"
    );

    for d in [1, 3, 5] {
        h.store
            .snooze_task(task.id, date(2026, 12, d), None)
            .unwrap();
    }
    let st = h.store.task_status(&task, now(&h.clock)).unwrap();
    assert_eq!(st.eval.state, TaskState::Snoozed);
    assert!(st.eval.needs_review);

    let log = audit_log(&h.store, &TimeZone::get("America/New_York").unwrap()).unwrap();
    let texts: Vec<&str> = log.iter().map(|e| e.text.as_str()).collect();
    assert!(
        texts.contains(&"Alex added household 'The Testers'"),
        "{texts:#?}"
    );
    assert!(
        texts.contains(&"Alex marked 'Test the smoke alarm' done on 2026-11-27"),
        "{texts:#?}"
    );
    assert!(texts.contains(&"Alex snoozed 'Test the smoke alarm' until 2026-12-05"));
    assert!(texts.iter().any(|t| t.contains("cancelled a completion")));
    assert!(
        texts
            .iter()
            .any(|t| t == &"Alex added item 'Hall smoke alarm'")
    );
    // Values never appear in the audit trail.
    assert!(!texts.iter().any(|t| t.contains("1 Example St")));
}

#[test]
fn limited_members_see_only_their_tasks() {
    let mut h = populated();
    let kid = Member {
        id: MemberId::new(),
        name: "Sam".into(),
        role: Role::Limited,
        kind: Default::default(),
        contact: None,
        reminders: None,
    };
    h.store.put(&kid).unwrap();
    let mut t = h.store.list::<Task>().unwrap().remove(0);
    t.owner = kid.id;
    h.store.put(&t).unwrap();
    let seen = h.store.visible_tasks(kid.id).unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].id, t.id);
    assert!(h.store.visible_tasks(h.owner.id).unwrap().len() > 1);
}

fn without_time(mut e: Export) -> Export {
    e.exported_at = Timestamp::UNIX_EPOCH;
    e
}

#[test]
fn export_import_with_history_is_lossless() {
    let mut h = populated();
    let task = h.store.list::<Task>().unwrap().remove(0);
    h.store.complete_task(task.id, None, Some("done")).unwrap();
    let room = Room {
        outline: None,
        id: RoomId::new(),
        property: h.property.id,
        floor: None,
        name: "Spare".into(),
    };
    h.store.put(&room).unwrap();
    h.store
        .delete(EntityKind::Room, &room.id.to_string())
        .unwrap();

    let e = export::export(&h.store).unwrap();
    let json = export::to_json(&e).unwrap();
    let parsed = export::from_json(&json).unwrap();
    assert_eq!(parsed, e);

    let mut copy = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let report = export::import(&mut copy, &parsed, None).unwrap();
    assert!(report.with_history);
    assert_eq!(copy.author().unwrap(), h.owner.id);
    assert_eq!(copy.snapshot().unwrap(), h.store.snapshot().unwrap());
    assert_eq!(
        without_time(export::export(&copy).unwrap()),
        without_time(e.clone())
    );
    // The copy keeps working.
    copy.complete_task(task.id, None, None).unwrap();

    // Importing twice, or into a non-empty database, is refused.
    assert!(export::import(&mut copy, &parsed, None).is_err());
}

#[test]
fn state_only_import_rebuilds_the_same_entities() {
    let h = populated();
    let mut e = export::export(&h.store).unwrap();
    e.operations.clear();
    let mut copy = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let report = export::import(&mut copy, &e, None).unwrap();
    assert!(!report.with_history);
    let a = copy.snapshot().unwrap().entities;
    let b = h.store.snapshot().unwrap().entities;
    assert_eq!(a, b);
}

#[test]
fn import_rejects_tampered_history_and_foreign_formats() {
    let h = populated();
    let mut e = export::export(&h.store).unwrap();
    e.entities
        .get_mut(&EntityKind::Item)
        .unwrap()
        .first_mut()
        .unwrap()
        .data
        .insert("name".into(), serde_json::json!("Tampered"));
    let mut copy = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    assert!(export::import(&mut copy, &e, None).is_err());

    assert!(export::from_json(r#"{"format":"other","version":1}"#).is_err());
}
