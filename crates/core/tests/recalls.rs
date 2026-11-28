//! Recall matching against a recorded CPSC response (no network).

use std::collections::BTreeSet;

use housewright_core::ids::{ItemId, PropertyId};
use housewright_core::model::Item;
use housewright_core::recalls::*;

const FIXTURE: &str = include_str!("fixtures/cpsc/recalls-2026-10.json");

fn item(name: &str, make: Option<&str>, model: Option<&str>) -> Item {
    Item {
        id: ItemId::new(),
        property: PropertyId::new(),
        room: None,
        parent: None,
        name: name.into(),
        category: None,
        make: make.map(Into::into),
        model: model.map(Into::into),
        serial: None,
        purchased: None,
        installed: None,
        warranty: None,
        location_note: None,
        template: None,
        extra_fields: vec![],
        custom: Default::default(),
        sensitivity: Default::default(),
        price: None,
        value: None,
    }
}

fn one(i: &Item) -> Vec<RecallMatch> {
    let r = parse_cpsc(FIXTURE).unwrap();
    matches(std::slice::from_ref(i), &r, &BTreeSet::new())
}

#[test]
fn parses_the_recorded_response() {
    let r = parse_cpsc(FIXTURE).unwrap();
    assert_eq!(r.len(), 7);
    let dreo = r.iter().find(|x| x.number == "27054").unwrap();
    assert!(dreo.url.starts_with("https://") && dreo.url.contains("cpsc.gov/Recalls/"));
    assert!(dreo.products[0].contains("DR-HSH015"));
    assert!(dreo.companies.iter().any(|c| c.contains("DREO")));
    assert!(!dreo.remedy.is_empty());
    // Round-trips as stored on the device.
    let back: Recall = serde_json::from_str(&serde_json::to_string(dreo).unwrap()).unwrap();
    assert_eq!(&back, dreo);
}

#[test]
fn bad_input_is_refused_or_skipped() {
    assert!(parse_cpsc("not json").is_err());
    assert!(parse_cpsc("{}").is_err());
    // No number, no date, or a link that is not https: skipped.
    let r = parse_cpsc(
        r#"[{"RecallNumber":"","RecallDate":"2026-01-01T00:00:00","URL":"https://x.example"},
            {"RecallNumber":"1","RecallDate":"","URL":"https://x.example"},
            {"RecallNumber":"2","RecallDate":"2026-01-01T00:00:00","URL":"javascript:alert(1)"},
            {"RecallNumber":"3","RecallDate":"2026-01-01T00:00:00","URL":"https://x.example"}]"#,
    )
    .unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].number, "3");
}

#[test]
fn exact_model_with_brand() {
    let m = one(&item("Space heater", Some("Dreo"), Some("DR HSH015")));
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].number, "27054");
    assert_eq!(m[0].level, MatchLevel::Model);
    assert!(m[0].evidence[0].contains("brand"));
    assert!(m[0].evidence[1].contains("DR-HSH015"), "{:?}", m[0].evidence);
}

#[test]
fn model_in_two_words_and_in_the_description() {
    let m = one(&item("Speaker", Some("Bose"), Some("S1 Pro")));
    assert_eq!(m[0].level, MatchLevel::Model);
    assert_eq!(m[0].number, "27065");
    let m = one(&item("Pressure washer", None, Some("HWT12P")));
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].level, MatchLevel::Model);
    assert!(m[0].evidence.iter().any(|e| e.contains("description")), "{:?}", m[0].evidence);
}

#[test]
fn series_needs_the_brand() {
    let m = one(&item("Boiler", Some("NTI"), Some("FTVN199")));
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].level, MatchLevel::ModelSeries);
    assert!(one(&item("Boiler", Some("Weil-McLain"), Some("FTVN199"))).is_empty());
}

#[test]
fn same_brand_and_kind_is_worth_a_look() {
    let m = one(&item("Space heater", Some("DREO"), Some("DR-HSP004")));
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].level, MatchLevel::BrandAndKind);
    assert!(m[0].evidence.iter().any(|e| e.contains("not named")));
    // Same brand, different kind of thing: nothing.
    assert!(one(&item("Humidifier", Some("Dreo"), Some("HM311"))).is_empty());
}

#[test]
fn conservative_without_a_brand() {
    // A short model with no brand is too likely to be a coincidence.
    assert!(one(&item("Generator", None, Some("5000"))).is_empty());
    // Nothing to compare.
    assert!(one(&item("Generator", None, None)).is_empty());
    // Brand that is a different company.
    assert!(one(&item("Dishwasher", Some("Bosch"), Some("SHX78CM5N"))).is_empty());
}

#[test]
fn not_mine_is_respected() {
    let i = item("Generator", Some("Cummins"), Some("P5000iDF"));
    let r = parse_cpsc(FIXTURE).unwrap();
    assert_eq!(matches(std::slice::from_ref(&i), &r, &BTreeSet::new()).len(), 1);
    let gone: BTreeSet<_> = [(i.id, "27087".to_string())].into();
    assert!(matches(std::slice::from_ref(&i), &r, &gone).is_empty());
}
