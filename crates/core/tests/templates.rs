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
