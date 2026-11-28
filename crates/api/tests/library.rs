//! The appliance library in the app (D-147 to D-152): the shipped
//! snapshot, matching, the thing's page, the manufacturer-or-generic
//! choice, updates and manuals. No test touches the network.

use std::collections::BTreeMap;

use housewright_api::library::shipped_library;
use housewright_api::{Api, bundled_library};
use housewright_core::appliances::bundle::{self, Bundle, MANIFEST_FORMAT, Manifest};
use housewright_core::appliances::matching::{self, Confidence};
use housewright_core::appliances::{Library, normalise_model};
use housewright_core::blobs::SqliteBlobs;
use housewright_core::store::{ManualClock, Store};
use serde_json::{Value, json};

const NOW_MS: u64 = 1_795_780_800_000;

fn setup() -> Api {
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut a = Api::with_blobs(store, Box::new(SqliteBlobs::in_memory().unwrap())).unwrap();
    call(
        &mut a,
        "init",
        json!({"property_name": "Home", "your_name": "Alex", "time_zone": "America/New_York"}),
    );
    a
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn lib() -> Library {
    shipped_library(&bundled_library().unwrap()).unwrap()
}

// -- the shipped snapshot ----------------------------------------------------

#[test]
fn the_shipped_snapshot_loads_without_a_problem() {
    let templates = bundled_library().unwrap();
    let l = lib();
    assert!(l.skipped.is_empty(), "left out: {:?}", l.skipped);
    assert!(l.problems.is_empty(), "problems: {:#?}", l.problems);
    assert!(l.families.len() >= 150, "{} families", l.families.len());
    assert!(l.brands.len() >= 80);
    assert_eq!(l.meta.licence, "CC BY 4.0");
    assert_eq!(l.meta.commit.len(), 40);
    // Every family's template is one of the app's (or it has none yet).
    for (id, f) in &l.families {
        if let Some(t) = l.template_of(f) {
            assert!(templates.get(&t).is_some(), "{id} extends unknown '{t}'");
        }
    }
    // Every override names a task its template has (the library has no
    // whole new tasks yet; the app offers only overrides).
    for (id, f) in &l.families {
        let Some(t) = l.template_of(f) else { continue };
        let r = templates.resolve(&t).unwrap();
        for o in &f.tasks {
            assert!(
                r.tasks.iter().any(|x| x.key == o.key),
                "{id}: task '{}'",
                o.key
            );
        }
    }
    // Strict loading (what an update must pass) accepts it too.
    let b = Bundle::from_gz(housewright_api::library_bundle_bytes()).unwrap();
    Library::load(&b, &templates, true).unwrap();
}

#[test]
fn the_snapshot_is_small_and_holds_no_documents() {
    let gz = housewright_api::library_bundle_bytes();
    assert!(gz.len() < 200 * 1024, "{} bytes compiled in", gz.len());
    let b = Bundle::from_gz(gz).unwrap();
    for (path, text) in &b.files {
        assert!(path.ends_with(".toml"), "{path}");
        assert!(!text.contains("%PDF"), "{path}");
    }
}

// -- matching ---------------------------------------------------------------

/// Every real model number the library records finds its own family.
#[test]
fn every_recorded_example_finds_its_family() {
    let l = lib();
    let mut weak = Vec::new();
    let mut n = 0;
    for (id, f) in &l.families {
        let brand = &l.brands[&f.brand].name;
        for ex in &f.model_examples {
            if f.model_patterns.is_empty() {
                continue;
            }
            n += 1;
            let t = l.template_of(f);
            let r = matching::find(&l, brand, ex, t.as_deref());
            let pos = r.families.iter().position(|m| &m.family == id);
            match pos {
                Some(0) if r.families[0].confidence == Confidence::Model => {}
                Some(p) => weak.push(format!("{id}: {ex} -> #{p} {:?}", r.families[p].confidence)),
                None if !f
                    .model_patterns
                    .iter()
                    .any(|p| matching::pattern_matches(p, ex)) =>
                {
                    // The library's own warning ("matches none of the
                    // patterns"); not the app's to fix.
                }
                None => panic!("{id}: example {ex} did not find its family: {r:?}"),
            }
        }
    }
    assert!(n > 100, "only {n} examples");
    eprintln!(
        "{} of {n} examples are not a strong unique match:\n{}",
        weak.len(),
        weak.join("\n")
    );
    // Ties between families are real (one series sold under two names);
    // they must stay rare.
    assert!(weak.len() * 5 < n, "{} weak of {n}", weak.len());
}

fn first(l: &Library, make: &str, model: &str, t: Option<&str>) -> Option<matching::FamilyMatch> {
    matching::find(l, make, model, t)
        .families
        .into_iter()
        .next()
}

#[test]
fn brands_are_matched_by_label_spelling() {
    let l = lib();
    for make in ["A. O. Smith", "AO SMITH", "a.o. smith"] {
        let r = matching::find(&l, make, "", Some("water-heater"));
        assert_eq!(r.brands, vec!["ao-smith".to_string()], "{make}");
    }
    // A company suffix: only a possible match.
    let r = matching::find(&l, "Whirlpool Corp.", "", None);
    assert_eq!(r.brands, vec!["whirlpool".to_string()]);
    // Unknown brand, or no make at all: nothing, even with a model number
    // that fits a pattern.
    assert!(
        matching::find(&l, "Acme", "XG50T12HE40U0", None)
            .families
            .is_empty()
    );
    assert!(
        matching::find(&l, "", "XG50T12HE40U0", None)
            .families
            .is_empty()
    );
    // Two companies share a name: the kind of thing tells them apart.
    let r = matching::find(&l, "Bosch", "", Some("dishwasher"));
    assert!(
        r.families.iter().all(|f| f.family.starts_with("bosch/")),
        "{r:?}"
    );
    let r = matching::find(&l, "Bosch", "", Some("boiler"));
    assert!(
        r.families
            .iter()
            .all(|f| f.family.starts_with("bosch-thermotechnology/")),
        "{r:?}"
    );
}

/// Find a family with a model example and a pattern, for the tricky cases.
fn sample(l: &Library, pred: impl Fn(&str) -> bool) -> (String, String, String, Option<String>) {
    for (id, f) in &l.families {
        for ex in &f.model_examples {
            let n = normalise_model(ex);
            if pred(&n)
                && f.model_patterns
                    .iter()
                    .any(|p| matching::pattern_matches(p, ex))
            {
                let r =
                    matching::find(l, &l.brands[&f.brand].name, ex, l.template_of(f).as_deref());
                if r.families
                    .first()
                    .is_some_and(|m| &m.family == id && m.confidence == Confidence::Model)
                {
                    return (
                        id.clone(),
                        l.brands[&f.brand].name.clone(),
                        ex.clone(),
                        l.template_of(f),
                    );
                }
            }
        }
    }
    panic!("no sample");
}

#[test]
fn ocr_confusions_are_tried_but_only_suggested() {
    let l = lib();
    // A model with a 0 where a digit must be, read as the letter O.
    let mut checked = 0;
    for (id, f) in &l.families {
        let brand = &l.brands[&f.brand].name;
        let t = l.template_of(f);
        for ex in &f.model_examples {
            let n = normalise_model(ex);
            if !n.contains('0') || n.len() < 5 {
                continue;
            }
            let misread = n.replacen('0', "O", 1);
            let Some(m) = first(&l, brand, &misread, t.as_deref()) else {
                continue;
            };
            if m.family != *id || m.corrected_model.is_none() {
                continue; // the O fits a letter position too: no swap needed
            }
            assert_eq!(m.confidence, Confidence::Possible, "{id} {misread}");
            assert_eq!(m.corrected_model.as_deref(), Some(n.as_str()));
            assert!(
                m.reasons.iter().any(|r| r.contains("0 and O")),
                "{:?}",
                m.reasons
            );
            checked += 1;
        }
    }
    assert!(checked >= 3, "only {checked} examples with a swappable 0");
    let (_, brand, ex, t) = sample(&l, |n| n.len() >= 5);

    // Lower case and spacing do not matter.
    let spaced = format!(" {} ", ex.to_lowercase().replace("", " ").trim());
    assert_eq!(
        first(&l, &brand, &spaced, t.as_deref()).unwrap().confidence,
        Confidence::Model
    );

    // Whatever needed a swap is never strong.
    let (_, brand, ex, t) = sample(&l, |n| n.chars().filter(|c| *c == '0').count() >= 3);
    let bad = normalise_model(&ex).replace('0', "O");
    for m in matching::find(&l, &brand, &bad, t.as_deref()).families {
        assert!(m.corrected_model.is_none() || m.confidence != Confidence::Model);
    }
}

#[test]
fn prefix_collisions_prefer_the_more_specific_pattern() {
    // A synthetic library: two families of one brand, one pattern a prefix
    // of the other.
    let mut b = Bundle::from_gz(housewright_api::library_bundle_bytes()).unwrap();
    b.files.retain(|p, _| !p.starts_with("families/rheem/"));
    let fam = |id: &str, pat: &str| {
        format!(
            "format = 1\nid = \"{id}\"\nbrand = \"rheem\"\nname = \"{id}\"\ncategory = \"water-heater-tank\"\nstatus = \"current\"\nmodel_patterns = [\"{pat}\"]\n\n[[manuals]]\nkind = \"support-page\"\ntitle = \"Support\"\nurl = \"https://www.rheem.com/help-and-support/\"\nsource = \"https://www.rheem.com/help-and-support/\"\n\n[[sources]]\nurl = \"https://www.rheem.com/help-and-support/\"\nchecked = 2026-11-28\n"
        )
    };
    b.files
        .insert("families/rheem/broad.toml".into(), fam("broad", "XG##*"));
    b.files.insert(
        "families/rheem/narrow.toml".into(),
        fam("narrow", "XG50T##*"),
    );
    b.files
        .insert("families/rheem/twin.toml".into(), fam("twin", "XE##*"));
    let l = Library::load(&b, &bundled_library().unwrap(), false).unwrap();
    let r = matching::find(&l, "Rheem", "XG50T12HE40U0", Some("water-heater"));
    assert_eq!(r.families[0].family, "rheem/narrow");
    assert_eq!(r.families[0].confidence, Confidence::Model);
    assert_eq!(r.families[1].family, "rheem/broad");
    assert_eq!(r.families[1].confidence, Confidence::Possible);
    // "XE" vs "XG": a different literal letter is not an OCR confusion.
    assert!(r.families.iter().all(|f| f.family != "rheem/twin"));
    // Too short to compare: the brand's families, by name.
    let r = matching::find(&l, "Rheem", "XG", Some("water-heater"));
    assert!(r.families.iter().all(|f| f.confidence == Confidence::Brand));
    // Two equally specific patterns: neither is strong.
    b.files
        .insert("families/rheem/broad2.toml".into(), fam("broad2", "XG##*"));
    let l = Library::load(&b, &bundled_library().unwrap(), false).unwrap();
    let r = matching::find(&l, "Rheem", "XG40ABC", Some("water-heater"));
    assert!(
        r.families
            .iter()
            .all(|f| f.confidence == Confidence::Possible),
        "{r:?}"
    );
}

// -- on a thing ---------------------------------------------------------------

/// A family with a task override, for the page and the choice.
fn family_with_override(l: &Library) -> (String, String, String, String) {
    for (id, f) in &l.families {
        if let (Some(t), Some(o)) = (
            l.template_of(f),
            f.tasks
                .iter()
                .find(|o| o.schedule.is_some() && !o.unverified),
        ) {
            let ex = f.model_examples.first().cloned().unwrap_or_default();
            return (id.clone(), t, o.key.clone(), ex);
        }
    }
    panic!("no family with an override");
}

#[test]
fn a_thing_gets_suggestions_then_its_family_and_the_choice() {
    let l = lib();
    let (fid, template, key, example) = family_with_override(&l);
    let f = &l.families[&fid];
    let brand = l.brands[&f.brand].name.clone();
    let mut a = setup();

    // Typing make and model while adding: a suggestion, nothing applied.
    let m = call(
        &mut a,
        "library_match",
        json!({"make": brand, "model": example, "template": template}),
    );
    assert!(!m["families"].as_array().unwrap().is_empty(), "{m}");
    let added = call(
        &mut a,
        "add_from_template",
        json!({"template": template, "make": brand, "model": example}),
    );
    let item = added["item_id"].as_str().unwrap().to_string();
    let d = call(&mut a, "item", json!({"id": item}));
    assert!(d["maker"].is_null());
    assert!(
        d["family_suggestion"]["families"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["id"] == fid.as_str())
    );

    // "None of these" stops suggestions until the model changes.
    let d = call(
        &mut a,
        "set_family",
        json!({"item_id": item, "family": "none"}),
    );
    assert!(d["family_suggestion"].is_null());
    call(
        &mut a,
        "update_item",
        json!({"id": item, "model": format!("{example}X")}),
    );
    let d = call(&mut a, "item", json!({"id": item}));
    assert!(d["maker"].is_null());

    // A family of another kind is refused.
    let other = l
        .families
        .iter()
        .find(|(_, x)| l.template_of(x).is_some_and(|t| t != template))
        .unwrap()
        .0;
    assert!(
        a.call("set_family", json!({"item_id": item, "family": other}))
            .is_err()
    );

    // Accept: the page shows links, facts and the task choice.
    let d = call(
        &mut a,
        "set_family",
        json!({"item_id": item, "family": fid}),
    );
    let maker = &d["maker"];
    assert_eq!(maker["family"]["id"], fid.as_str());
    assert!(
        maker["family"]["credit"]
            .as_str()
            .unwrap()
            .contains("CC BY 4.0")
    );
    let g = maker["guidance"].as_array().unwrap();
    let g = g
        .iter()
        .find(|g| g["key"] == key.as_str())
        .expect("override offered");
    assert!(g["following"].is_null());
    assert_ne!(g["generic"], g["maker"]);
    let task_id = g["task_id"].as_str().unwrap().to_string();

    // Choose the maker's: the task changes and says where it came from.
    let d = call(
        &mut a,
        "choose_guidance",
        json!({"task_id": task_id, "follow": "maker"}),
    );
    let g = d["maker"]["guidance"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["key"] == key.as_str())
        .unwrap()
        .clone();
    assert_eq!(g["following"], "maker");
    let t = call(&mut a, "task", json!({"id": task_id}));
    assert_eq!(t["source"], format!("manufacturer guidance ({brand})"));
    assert!(t["source_link"].as_str().unwrap().starts_with("https://"));
    assert_eq!(t["card"]["schedule_text"], g["maker"]);

    // And back to the generic one.
    call(
        &mut a,
        "choose_guidance",
        json!({"task_id": task_id, "follow": "generic"}),
    );
    let t = call(&mut a, "task", json!({"id": task_id}));
    assert!(t["source_link"].is_null());
    assert_eq!(t["card"]["schedule_text"], g["generic"]);

    // The choice and the family survive an export and import.
    let text = call(&mut a, "export", json!({}))["json"]
        .as_str()
        .unwrap()
        .to_string();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert!(
        v["entities"]["item"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["data"]["family"] == fid.as_str())
    );
    assert!(
        v["entities"]["task"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["data"]["guidance"]["follows"] == "generic")
    );
    let store = Store::open_in_memory(Box::new(ManualClock::new(NOW_MS))).unwrap();
    let mut b = Api::new(store).unwrap();
    call(&mut b, "import", json!({"json": text}));
    let d = call(&mut b, "item", json!({"id": item}));
    assert_eq!(d["maker"]["family"]["id"], fid.as_str());
}

#[test]
fn consumables_are_structured_data() {
    let l = lib();
    let (fid, f) = l
        .families
        .iter()
        .find(|(_, f)| f.consumables.iter().any(|c| c.part.is_some()))
        .unwrap();
    let template = l.template_of(f).unwrap();
    let mut a = setup();
    let item = call(
        &mut a,
        "add_from_template",
        json!({"template": template, "family": fid}),
    )["item_id"]
        .as_str()
        .unwrap()
        .to_string();
    let s = call(&mut a, "supplies", json!({}));
    let s = s.as_array().unwrap();
    assert!(!s.is_empty());
    assert_eq!(s[0]["item_id"], item.as_str());
    assert!(s.iter().any(|x| x["consumable"]["part"].is_string()));
}

// -- manuals ----------------------------------------------------------------

#[test]
fn only_the_familys_own_pdf_links_can_be_saved() {
    let l = lib();
    let (fid, f) = l
        .families
        .iter()
        .find(|(_, f)| {
            l.template_of(f).is_some() && f.manuals.iter().any(|m| m.kind == "manual-pdf")
        })
        .unwrap();
    let pdf = f
        .manuals
        .iter()
        .find(|m| m.kind == "manual-pdf")
        .unwrap()
        .url
        .clone();
    let mut a = setup();
    let item = call(
        &mut a,
        "add_from_template",
        json!({"template": l.template_of(f).unwrap(), "family": fid}),
    )["item_id"]
        .as_str()
        .unwrap()
        .to_string();
    let src = call(
        &mut a,
        "manual_source",
        json!({"item_id": item, "url": pdf}),
    );
    assert!(
        src["name"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .ends_with(".pdf")
    );
    assert!(!src["domains"].as_array().unwrap().is_empty());
    assert!(
        a.call(
            "manual_source",
            json!({"item_id": item, "url": "https://evil.example/x.pdf"})
        )
        .is_err()
    );
    if let Some(page) = f.manuals.iter().find(|m| m.kind != "manual-pdf") {
        assert!(
            a.call("manual_source", json!({"item_id": item, "url": page.url}))
                .is_err()
        );
    }
    // Not a PDF: refused; a PDF: kept as the thing's manual, with its source.
    assert!(
        a.call_upload(
            "attach_manual",
            json!({"item_id": item, "url": pdf}),
            b"<html>"
        )
        .is_err()
    );
    let att = a
        .call_upload(
            "attach_manual",
            json!({"item_id": item, "url": pdf}),
            b"%PDF-1.4\n%%EOF",
        )
        .unwrap();
    assert_eq!(att["kind"], "manual");
    assert_eq!(att["source"], pdf.as_str());
}

// -- updates ------------------------------------------------------------------

/// A newer release made from the shipped files, as `library-sync bundle`
/// would publish it.
fn release(version: &str, edit: impl Fn(&mut BTreeMap<String, String>)) -> (String, Vec<u8>) {
    let mut b = Bundle::from_gz(housewright_api::library_bundle_bytes()).unwrap();
    let snap = b.files["SNAPSHOT.toml"].clone();
    let old = snap
        .lines()
        .find(|l| l.starts_with("version"))
        .unwrap()
        .to_string();
    b.files.insert(
        "SNAPSHOT.toml".into(),
        snap.replace(&old, &format!("version = \"{version}\"")),
    );
    edit(&mut b.files);
    let gz = b.to_gz();
    let commit = lib().meta.commit;
    let m = Manifest {
        format: MANIFEST_FORMAT.into(),
        library_format: 1,
        version: version.into(),
        commit,
        published: "2026-12-01".into(),
        bundle: format!("library-{version}.json.gz"),
        sha256: bundle::sha256_hex(&gz),
        size: gz.len() as u64,
    };
    (serde_json::to_string(&m).unwrap(), gz)
}

#[test]
fn a_valid_update_is_used_and_kept() {
    let mut a = setup();
    let s = call(&mut a, "library", json!({}));
    assert_eq!(s["from_update"], false);
    assert!(
        s["manifest_url"]
            .as_str()
            .unwrap()
            .starts_with("https://raw.githubusercontent.com/holdTheDoorHoid/housewright-library/")
    );
    let (manifest, gz) = release("2099.1.1", |_| {});
    let c = call(&mut a, "library_check", json!({"manifest": manifest}));
    assert_eq!(c["newer"], true);
    assert!(
        c["bundle_url"]
            .as_str()
            .unwrap()
            .ends_with("/library-2099.1.1.json.gz")
    );
    let s = a
        .call_upload("library_import", json!({"manifest": manifest}), &gz)
        .unwrap();
    assert_eq!(s["version"], "2099.1.1");
    assert_eq!(s["from_update"], true);
    // Not newer any more.
    let c = call(&mut a, "library_check", json!({"manifest": manifest}));
    assert_eq!(c["newer"], false);
    // Back to the shipped one.
    let s = call(&mut a, "library_reset", json!({}));
    assert_eq!(s["from_update"], false);
}

#[test]
fn tampered_or_bad_updates_change_nothing() {
    let mut a = setup();
    let before = call(&mut a, "library", json!({}))["version"].clone();
    let (manifest, gz) = release("2099.1.1", |_| {});

    // One byte changed.
    let mut bad = gz.clone();
    let n = bad.len() / 2;
    bad[n] ^= 1;
    let e = a
        .call_upload("library_import", json!({"manifest": manifest}), &bad)
        .unwrap_err();
    assert!(e.message.contains("checksum"), "{e}");

    // A different file entirely, with its own (matching) manifest but a
    // link off the brand's site: the checks refuse it.
    let (m2, gz2) = release("2099.1.2", |files| {
        let k = files
            .keys()
            .find(|k| k.starts_with("families/"))
            .unwrap()
            .clone();
        let t = files[&k].clone();
        let first_url = t
            .split("url = \"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_string();
        files.insert(k, t.replace(&first_url, "https://evil.example/manual.pdf"));
    });
    let e = a
        .call_upload("library_import", json!({"manifest": m2}), &gz2)
        .unwrap_err();
    assert!(e.message.contains("failed its checks"), "{e}");

    // A plain-http link.
    let (m3, gz3) = release("2099.1.3", |files| {
        let k = files
            .keys()
            .find(|k| k.starts_with("brands/"))
            .unwrap()
            .clone();
        let t = files[&k].replace("website = \"https://", "website = \"http://");
        files.insert(k, t);
    });
    assert!(
        a.call_upload("library_import", json!({"manifest": m3}), &gz3)
            .is_err()
    );

    // An older version.
    let (m4, gz4) = release("2000.1.1", |_| {});
    assert!(
        a.call_upload("library_import", json!({"manifest": m4}), &gz4)
            .unwrap_err()
            .message
            .contains("not newer")
    );

    // A manifest that lies about the version inside.
    let (_, gz5) = release("2099.1.5", |_| {});
    let lying = serde_json::to_string(&Manifest {
        version: "2099.9.9".into(),
        bundle: "library-2099.9.9.json.gz".into(),
        ..serde_json::from_str::<Manifest>(&release("2099.1.5", |_| {}).0).unwrap()
    })
    .unwrap();
    assert!(
        a.call_upload("library_import", json!({"manifest": lying}), &gz5)
            .is_err()
    );

    // Not JSON at all.
    assert!(
        a.call("library_check", json!({"manifest": "<html>404</html>"}))
            .is_err()
    );

    assert_eq!(call(&mut a, "library", json!({}))["version"], before);
    assert_eq!(call(&mut a, "library", json!({}))["from_update"], false);
}

#[test]
fn a_kept_update_is_used_after_a_restart() {
    let dir = std::env::temp_dir().join(format!("hw-lib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db.sqlite");
    {
        let store = Store::open(&path).unwrap();
        let mut a = Api::new(store).unwrap();
        call(
            &mut a,
            "init",
            json!({"property_name": "Home", "your_name": "Alex", "time_zone": "America/New_York"}),
        );
        let (manifest, gz) = release("2099.1.1", |_| {});
        a.call_upload("library_import", json!({"manifest": manifest}), &gz)
            .unwrap();
    }
    let mut a = Api::new(Store::open(&path).unwrap()).unwrap();
    assert_eq!(call(&mut a, "library", json!({}))["version"], "2099.1.1");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Optional fields the library added after snapshot d623c10: they parse, and an
/// unconfirmed pattern is only ever a suggestion to check.
#[test]
fn unconfirmed_patterns_only_ever_suggest() {
    let mut l = lib();
    // A family whose recorded example matches strongly today.
    let (fid, example, brand) = l
        .families
        .iter()
        .find_map(|(id, f)| {
            let ex = f.model_examples.first()?.clone();
            let b = l.brands.get(&f.brand)?.name.clone();
            let m = first(&l, &b, &ex, None)?;
            (m.family == *id && m.confidence == matching::Confidence::Model)
                .then(|| (id.clone(), ex, b))
        })
        .expect("a family with a strong example");
    let f = l.families.get_mut(&fid).unwrap();
    f.unconfirmed_patterns = std::mem::take(&mut f.model_patterns);
    let m = first(&l, &brand, &example, None).expect("still suggested");
    assert_eq!(m.family, fid);
    assert_eq!(
        m.confidence,
        matching::Confidence::Possible,
        "{:?}",
        m.reasons
    );

    // The new optional fields are accepted by the parser.
    let fam: housewright_core::appliances::model::Family = toml::from_str(
        r#"
format = 1
id = "x"
brand = "acme"
name = "Acme X"
category = "water-heater"
status = "current"
unconfirmed_patterns = ["AX##"]
pattern_sources = ["https://acme.example/models"]
"#,
    )
    .unwrap();
    assert_eq!(fam.unconfirmed_patterns, vec!["AX##"]);
}
