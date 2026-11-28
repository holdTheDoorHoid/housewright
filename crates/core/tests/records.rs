//! Phase 4 at the store level: attachments, blob garbage collection, the
//! full backup, completion details and older exports.

use housewright_core::backup;
use housewright_core::blobs::{self, BlobStore, MemoryBlobs};
use housewright_core::costs;
use housewright_core::export;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::records::NewFile;
use housewright_core::schedule::{Climate, Interval, Schedule};
use housewright_core::store::{ManualClock, Store};
use jiff::civil::date;

const NOW_MS: u64 = 1_795_780_800_000;

struct Home {
    store: Store,
    property: PropertyId,
    item: ItemId,
    task: TaskId,
}

fn home() -> Home {
    let mut store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let (_, owner) = store
        .init_household("Testers", "Alex", "America/New_York")
        .unwrap();
    let property = PropertyId::new();
    store
        .put(&Property {
            id: property,
            name: "Home".into(),
            address: None,
            time_zone: "America/New_York".into(),
            climate: Climate::default(),
        })
        .unwrap();
    let item = ItemId::new();
    store
        .put(&Item {
            id: item,
            property,
            room: None,
            parent: None,
            name: "Water heater".into(),
            category: None,
            make: None,
            model: None,
            serial: None,
            purchased: None,
            installed: None,
            warranty: None,
            location_note: None,
            template: None,
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Sensitivity::Normal,
            price: Default::default(),
            value: Default::default(),
        })
        .unwrap();
    let task = TaskId::new();
    store
        .put(&Task {
            id: task,
            property,
            item: Some(item),
            room: None,
            title: "Flush the tank".into(),
            owner: owner.id,
            priority: housewright_core::priority::PriorityClass::PropertyLoss,
            schedule: Schedule::Floating {
                every: "1 year".parse::<Interval>().unwrap(),
            },
            window: None,
            start: date(2026, 1, 1),
            steps: vec![],
            why: None,
            minutes: None,
            completion_fields: vec![],
            template: None,
            active: true,
            paused: false,
            not_applicable: false,
        })
        .unwrap();
    Home {
        store,
        property,
        item,
        task,
    }
}

fn file<'a>(about: EntityRef, bytes: &'a [u8], thumb: Option<&'a [u8]>) -> NewFile<'a> {
    NewFile {
        about,
        kind: AttachmentKind::Photo,
        name: "x.jpg".into(),
        media_type: "image/jpeg".into(),
        bytes,
        thumb,
        width: None,
        height: None,
        sensitivity: Sensitivity::Normal,
    }
}

#[test]
fn blobs_are_shared_and_collected_only_when_unused() {
    let mut h = home();
    let mut b = MemoryBlobs::default();
    let about = EntityRef {
        kind: EntityKind::Item,
        id: h.item.to_string(),
    };
    let a1 = h
        .store
        .add_attachment(&mut b, file(about.clone(), b"photo", Some(b"thumb")))
        .unwrap();
    let a2 = h
        .store
        .add_attachment(&mut b, file(about.clone(), b"photo", None))
        .unwrap();
    assert_eq!(a1.sha256, a2.sha256);
    assert_eq!(b.list().unwrap().len(), 2, "photo once, thumb once");
    // A stray blob (say, from a failed save) is collected.
    b.put(b"orphan").unwrap();
    assert_eq!(blobs::collect_garbage(&h.store, &mut b).unwrap(), 1);
    assert_eq!(
        h.store.delete_attachment(&mut b, a1.id).unwrap(),
        1,
        "thumb freed"
    );
    assert!(b.has(&a2.sha256).unwrap());
    assert_eq!(h.store.delete_attachment(&mut b, a2.id).unwrap(), 1);
    assert!(b.list().unwrap().is_empty());
    // The tombstones are in the log; the records stay readable as deleted.
    assert!(
        h.store
            .raw(EntityKind::Attachment, &a1.id.to_string())
            .unwrap()
            .unwrap()
            .deleted
    );
    // Too big, or empty, is refused.
    let big = vec![0u8; housewright_core::records::MAX_DOCUMENT_BYTES + 1];
    assert!(
        h.store
            .add_attachment(&mut b, file(about.clone(), &big, None))
            .is_err()
    );
    assert!(
        h.store
            .add_attachment(&mut b, file(about, b"", None))
            .is_err()
    );
}

#[test]
fn zip_backup_round_trip() {
    let mut h = home();
    let mut b = MemoryBlobs::default();
    let about = EntityRef {
        kind: EntityKind::Item,
        id: h.item.to_string(),
    };
    let a = h
        .store
        .add_attachment(
            &mut b,
            file(about, b"\xFF\xD8\xFFjpeg bytes", Some(b"small")),
        )
        .unwrap();
    let c = h.store.complete_task(h.task, None, None).unwrap();
    h.store
        .add_completion_detail(
            c.id,
            Some(vec![CostLine {
                kind: CostKind::Contractor,
                amount: costs::money(25000, "USD"),
            }]),
            Default::default(),
        )
        .unwrap();
    let zip = backup::write(&h.store, &b).unwrap();
    assert!(backup::is_zip(&zip));

    let mut copy = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut cb = MemoryBlobs::default();
    let r = backup::restore(&mut copy, &mut cb, &zip).unwrap();
    assert!(r.import.with_history);
    assert_eq!(r.blobs, 2);
    assert_eq!(r.missing, 0);
    assert_eq!(copy.snapshot().unwrap(), h.store.snapshot().unwrap());
    assert_eq!(
        cb.get(&a.sha256).unwrap().unwrap(),
        b"\xFF\xD8\xFFjpeg bytes"
    );
    let s = costs::summarise(&copy).unwrap();
    assert_eq!(s.total.contractor, 25000);
    assert_eq!(s.by_property[0].key, h.property);
}

#[test]
fn completion_details_are_records_and_later_ones_win() {
    let mut h = home();
    let c = h.store.complete_task(h.task, None, None).unwrap();
    let line = |minor| {
        Some(vec![CostLine {
            kind: CostKind::Parts,
            amount: costs::money(minor, "USD"),
        }])
    };
    h.store
        .add_completion_detail(c.id, line(1000), Default::default())
        .unwrap();
    // Two edits on one device in the same millisecond still order by id.
    h.store
        .add_completion_detail(c.id, line(1200), Default::default())
        .unwrap();
    // A detail with no costs leaves the costs alone.
    h.store
        .add_completion_detail(c.id, None, Default::default())
        .unwrap();
    let d = h.store.completion_details(c.id).unwrap();
    assert_eq!(d.len(), 3);
    assert_eq!(costs::effective_costs(&c, &d)[0].amount.minor, 1200);
    assert!(
        h.store
            .delete(EntityKind::CompletionDetail, &d[0].id.to_string())
            .is_err()
    );
    // Negative costs are refused.
    assert!(
        h.store
            .add_completion_detail(c.id, line(-5), Default::default())
            .is_err()
    );
}

#[test]
fn a_phase_3_export_still_imports() {
    let text = include_str!("fixtures/export-v2-phase3.json");
    let e = export::from_json(text).unwrap();
    assert_eq!(e.version, 2);
    let mut s = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let r = export::import(&mut s, &e, None).unwrap();
    assert!(r.with_history);
    let completions: Vec<Completion> = s.list().unwrap();
    assert_eq!(completions.len(), 1);
    let h: Household = s.household().unwrap();
    assert_eq!(h.currency(), "USD");
    // And it exports again as the current version.
    let again = export::export(&s).unwrap();
    assert_eq!(again.version, export::EXPORT_VERSION);
}

#[test]
fn a_phase_2_export_still_imports_with_records() {
    let text = include_str!("fixtures/export-v1-phase2.json");
    let e = export::from_json(text).unwrap();
    let mut s = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    export::import(&mut s, &e, None).unwrap();
    assert_eq!(s.schema_version().unwrap(), 7);
    assert!(costs::summarise(&s).unwrap().by_year.is_empty());
}
