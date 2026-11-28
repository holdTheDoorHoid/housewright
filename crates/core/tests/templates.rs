//! Every template shipped in `templates/` parses and validates.

use std::path::PathBuf;

use housewright_core::template::{Library, MAX_STEPS, MIN_STEPS};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates")
}

#[test]
fn every_shipped_template_validates() {
    let lib = Library::load_dir(dir()).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        lib.len() >= 6,
        "expected the starter set, found {}",
        lib.len()
    );
    let ids: Vec<&str> = lib.ids().collect();
    for want in [
        "smoke-alarm",
        "co-alarm",
        "hvac-air-filter",
        "water-heater",
        "washing-machine",
        "clothes-dryer",
        "refrigerator",
        "gutters",
        "sump-pump",
        "hose-bibs",
    ] {
        assert!(ids.contains(&want), "missing {want}");
    }
    for id in lib.ids() {
        let t = lib.resolve(id).unwrap();
        assert!(!t.tasks.is_empty(), "{id} has no tasks");
        for task in &t.tasks {
            assert!((MIN_STEPS..=MAX_STEPS).contains(&task.steps.len()));
            assert!(!task.source.contains("http"), "{id}: no URLs in sources");
        }
    }
}

#[test]
fn every_file_in_the_directory_is_a_template() {
    let count = std::fs::read_dir(dir())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "toml")
        })
        .count();
    assert_eq!(count, Library::load_dir(dir()).unwrap().len());
}

/// The fixed list of categories, with the labels the interface shows
/// (`app/src/lib/labels.ts` must match).
pub const CATEGORIES: [(&str, &str); 8] = [
    ("safety", "Safety"),
    ("heating_cooling", "Heating and cooling"),
    ("plumbing", "Plumbing and water"),
    ("appliances", "Appliances"),
    ("electrical", "Electrical and power"),
    ("exterior", "Outside the house"),
    ("structure", "Roof, attic and structure"),
    ("yard", "Yard, garden and pool"),
];

#[test]
fn template_quality() {
    use housewright_core::schedule::Schedule;
    let lib = Library::load_dir(dir()).unwrap();
    assert!(
        (35..=50).contains(&lib.len()),
        "the library has {} templates",
        lib.len()
    );
    let cats: Vec<&str> = CATEGORIES.iter().map(|c| c.0).collect();
    let mut titles = std::collections::BTreeSet::new();
    for id in lib.ids() {
        let t = lib.resolve(id).unwrap();
        let cat = t.category.as_deref().unwrap_or("none");
        assert!(
            cats.contains(&cat),
            "{id}: category '{cat}' is not in the list"
        );
        assert!(t.description.is_some(), "{id}: no description");
        // Up to eight, since per-fuel variants count separately.
        assert!(
            (1..=8).contains(&t.tasks.len()),
            "{id}: {} tasks",
            t.tasks.len()
        );
        for task in &t.tasks {
            let at = format!("{id}/{}", task.key);
            assert!((3..=7).contains(&task.steps.len()), "{at}: steps");
            assert!(
                task.steps.iter().all(|s| s.trim().len() >= 10),
                "{at}: a step is too short"
            );
            assert!(
                !task.why.trim().is_empty() && task.why.len() <= 200,
                "{at}: why"
            );
            assert!(
                task.why.trim_end().ends_with('.'),
                "{at}: why should be a sentence"
            );
            assert!(!task.source.trim().is_empty(), "{at}: source");
            for text in task
                .steps
                .iter()
                .chain([&task.why, &task.source, &task.title])
            {
                assert!(
                    !text.contains("http") && !text.contains("www."),
                    "{at}: no URLs"
                );
                assert!(!text.contains('%'), "{at}: no statistics");
            }
            let days = task.schedule.period_days();
            if !matches!(task.schedule, Schedule::Usage { .. }) {
                assert!((7..=3650).contains(&days), "{at}: interval of {days} days");
            }
            assert!(
                task.minutes.is_some_and(|m| (1..=480).contains(&m)),
                "{at}: minutes"
            );
            // A reminder to book a professional says who to hire first.
            if task.title.starts_with("Book ") {
                let first = task.steps[0].to_lowercase();
                assert!(
                    ["hire", "book", "call", "contact", "arrange", "schedule"]
                        .iter()
                        .any(|w| first.contains(w)),
                    "{at}: first step should say who to book: {}",
                    task.steps[0]
                );
            }
            titles.insert((id.to_string(), task.key.clone()));
        }
    }
}
