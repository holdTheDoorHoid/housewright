//! Phase 9 through the command surface: contractors and their jobs,
//! warranties and their notices, and recall checks against a recorded CPSC
//! response (no network).

use housewright_api::Api;
use housewright_core::blobs::SqliteBlobs;
use housewright_core::store::{ManualClock, Store};
use serde_json::{Value, json};

/// 2026-11-27 12:00 UTC (07:00 in New York).
const NOW_MS: u64 = 1_795_780_800_000;
const DAY_MS: u64 = 86_400_000;
const FIXTURE: &str = include_str!("../../core/tests/fixtures/cpsc/recalls-2026-10.json");

fn setup() -> (Api, ManualClock) {
    let clock = ManualClock::new(NOW_MS);
    let store = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    let mut a = Api::with_blobs(store, Box::new(SqliteBlobs::in_memory().unwrap())).unwrap();
    call(
        &mut a,
        "init",
        json!({"property_name": "Home", "your_name": "Alex", "time_zone": "America/New_York"}),
    );
    (a, clock)
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn add(a: &mut Api, template: &str) -> String {
    call(a, "add_from_template", json!({"template": template}))["item_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn notices(a: &mut Api) -> Vec<Value> {
    call(a, "reminder_plan", json!({"horizon_days": 400}))
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "notice")
        .cloned()
        .collect()
}

#[test]
fn a_contractor_with_licence_rating_and_jobs() {
    let (mut a, _) = setup();
    call(
        &mut a,
        "save_contractor",
        json!({"name": "Ana Ruiz", "company": "Ruiz Plumbing", "trades": ["plumbing", "water heaters"],
               "phone": "+1 215 555 0101", "email": "ana@ruiz.example", "website": "ruiz.example",
               "service_area": "Philadelphia", "licence_number": "PL-4411",
               "licence_expires": "2026-12-20", "insurance_expires": "2027-06-01",
               "private_rating": 5, "would_hire_again": "yes", "private_notes": "Fair and quick"}),
    );
    let list = call(&mut a, "contractors", Value::Null);
    let c = &list[0];
    let id = c["id"].as_str().unwrap().to_string();
    assert_eq!(c["website"], "https://ruiz.example");
    assert_eq!(c["licence"]["number"], "PL-4411");
    assert_eq!(c["licence"]["state"], "ending_soon");
    assert_eq!(c["insurance"]["state"], "ok");
    assert_eq!(c["private"]["rating"], 5);
    assert_eq!(c["private"]["would_hire_again"], true);
    // A bad website or rating is refused plainly.
    assert!(
        a.call(
            "save_contractor",
            json!({"id": id, "name": "Ana Ruiz", "website": "javascript:alert(1)"})
        )
        .is_err()
    );
    assert!(
        a.call(
            "save_contractor",
            json!({"id": id, "name": "Ana Ruiz", "private_rating": 9})
        )
        .is_err()
    );

    // The binder's "Who to call" form leaves the new fields alone.
    call(
        &mut a,
        "save_contractor",
        json!({"id": id, "name": "Ana Ruiz", "trades": ["plumbing"], "phone": "+1 215 555 0101",
               "emergency": true}),
    );
    let c = &call(&mut a, "contractors", Value::Null)[0];
    assert_eq!(c["company"], "Ruiz Plumbing");
    assert_eq!(c["private"]["notes"], "Fair and quick");
    assert_eq!(c["licence"]["number"], "PL-4411");

    // A job: Done first (P1), then "who did it" and the cost.
    let heater = add(&mut a, "water-heater");
    let task = call(&mut a, "item", json!({"id": heater}))["tasks"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = call(&mut a, "complete", json!({"id": task}));
    let cid = done["completion_id"].as_str().unwrap();
    let t = call(
        &mut a,
        "add_details",
        json!({"completion_id": cid, "contractor_id": id, "costs": {"contractor": "180", "parts": "20.50"}}),
    );
    assert_eq!(
        t["completions"][0]["contractor_name"],
        "Ana Ruiz, Ruiz Plumbing"
    );
    // Saving again without a change adds nothing; "" says no contractor.
    call(
        &mut a,
        "add_details",
        json!({"completion_id": cid, "contractor_id": id}),
    );
    let page = call(&mut a, "contractor", json!({"id": id}));
    assert_eq!(page["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(page["jobs"][0]["total"], 20050);
    assert_eq!(page["jobs"][0]["item_id"], heater);
    assert_eq!(page["contractor"]["spent"], 20050);
    assert_eq!(page["contractor"]["jobs"], 1);
    call(
        &mut a,
        "add_details",
        json!({"completion_id": cid, "contractor_id": ""}),
    );
    let page = call(&mut a, "contractor", json!({"id": id}));
    assert_eq!(page["jobs"].as_array().unwrap().len(), 0);
    // An undone job does not count either.
    call(
        &mut a,
        "add_details",
        json!({"completion_id": cid, "contractor_id": id}),
    );
    call(&mut a, "void_completion", json!({"id": cid}));
    let page = call(&mut a, "contractor", json!({"id": id}));
    assert_eq!(page["contractor"]["spent"], 0);
}

#[test]
fn licence_and_insurance_notices_only_for_use_again() {
    let (mut a, _) = setup();
    for (name, again) in [
        ("Use Again Co", "yes"),
        ("Never Again Co", "no"),
        ("Unsure Co", ""),
    ] {
        call(
            &mut a,
            "save_contractor",
            json!({"name": name, "licence_number": "L-1", "licence_expires": "2027-02-15",
                   "would_hire_again": again}),
        );
    }
    let n = notices(&mut a);
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(n[0]["title"], "Use Again Co: licence ends Feb 15, 2027");
    assert!(n[0]["open"].as_str().unwrap().starts_with("contractor/"));
    // 30 days before, at 9:00 New York time (14:00 UTC).
    let at = jiff::Timestamp::from_millisecond(n[0]["at_ms"].as_i64().unwrap()).unwrap();
    assert_eq!(at.to_string(), "2027-01-16T14:00:00Z");
}

#[test]
fn warranty_from_a_length_its_view_and_two_notices_in_the_window() {
    let (mut a, clock) = setup();
    let furnace = add(&mut a, "furnace");
    // Five years from the purchase date: ends 2027-01-10.
    let r = call(
        &mut a,
        "update_item",
        json!({"id": furnace, "purchased": "2022-01-10",
               "warranty": {"months": 60, "provider": "Carrier", "register_url": "www.example.com/register"},
               "price": "4,200", "value": "2500"}),
    );
    let w = &r["detail"]["warranty"];
    assert_eq!(w["ends"], "2027-01-10");
    assert_eq!(w["state"], "ending_soon");
    assert_eq!(w["register_url"], "https://www.example.com/register");
    assert_eq!(r["detail"]["price"]["minor"], 420000);
    assert_eq!(r["detail"]["value"]["minor"], 250000);
    // A different purchase date moves the end with it.
    let r = call(
        &mut a,
        "update_item",
        json!({"id": furnace, "purchased": "2022-03-01"}),
    );
    assert_eq!(r["detail"]["warranty"]["ends"], "2027-03-01");
    // A link that is not https is refused.
    assert!(a
        .call("update_item", json!({"id": furnace, "warranty": {"ends": "2027-03-01", "register_url": "http://x.example"}}))
        .is_err());

    let dryer = add(&mut a, "clothes-dryer");
    call(
        &mut a,
        "update_item",
        json!({"id": dryer, "warranty": {"ends": "2026-01-01"}}),
    );
    let washer = add(&mut a, "dishwasher");
    call(
        &mut a,
        "update_item",
        json!({"id": washer, "warranty": {"ends": "2030-01-01"}}),
    );
    let w = call(&mut a, "warranties", Value::Null);
    // The furnace now ends in 94 days: still active, soonest first.
    assert_eq!(w["active"][0]["item_id"], furnace);
    assert_eq!(w["active"][1]["item_id"], washer);
    assert_eq!(w["ended"][0]["item_id"], dryer);
    assert!(w["ending_soon"].as_array().unwrap().is_empty());

    // Quiet hours until 10:00 and a Saturday morning window.
    call(
        &mut a,
        "set_reminder_prefs",
        json!({"prefs": {"quiet_hours": {"start": "21:00", "end": "10:00"},
                          "maintenance_window": {"days": ["saturday"], "start": "09:00", "end": "12:00"}}}),
    );
    let n = notices(&mut a);
    let furnace_notices: Vec<&Value> = n
        .iter()
        .filter(|x| x["open"] == format!("item/{furnace}"))
        .collect();
    // 60 days before Mar 1 is Dec 31 (a Thursday): moved to Saturday Jan 2,
    // 10:00 (quiet hours end), New York time = 15:00 UTC.
    assert_eq!(furnace_notices.len(), 2, "{n:?}");
    let at = |x: &Value| {
        jiff::Timestamp::from_millisecond(x["at_ms"].as_i64().unwrap())
            .unwrap()
            .to_string()
    };
    assert_eq!(at(furnace_notices[0]), "2027-01-02T15:00:00Z");
    assert!(
        furnace_notices[0]["title"]
            .as_str()
            .unwrap()
            .contains("warranty ends in 60 days")
    );
    assert!(
        furnace_notices[0]["body"]
            .as_str()
            .unwrap()
            .contains("Book a check-up")
    );
    // The washer's notices are years away (beyond the horizon), the dryer's over.
    assert!(n.iter().all(|x| x["open"] != format!("item/{dryer}")));
    // A notice never repeats once its moment has passed.
    clock.set(NOW_MS + 40 * DAY_MS);
    let n = notices(&mut a);
    let left: Vec<&Value> = n
        .iter()
        .filter(|x| x["open"] == format!("item/{furnace}"))
        .collect();
    assert_eq!(left.len(), 1);
    assert!(left[0]["title"].as_str().unwrap().contains("in a week"));
}

#[test]
fn recall_checks_are_off_until_switched_on_and_send_only_dates() {
    let (mut a, _) = setup();
    let r = call(&mut a, "recalls", Value::Null);
    assert_eq!(r["enabled"], false);
    assert!(r["next"].as_array().unwrap().is_empty());
    assert!(
        a.call(
            "recall_import",
            json!({"json": FIXTURE, "start": "2026-10-01", "end": "2026-11-27"})
        )
        .is_err()
    );
    let r = call(&mut a, "set_recalls", json!({"on": true}));
    let next = r["next"].as_array().unwrap();
    assert!(next.len() >= 10);
    for w in next {
        let url = w["url"].as_str().unwrap();
        let q = url
            .strip_prefix("https://www.saferproducts.gov/RestWebServices/Recall?")
            .unwrap();
        let keys: Vec<&str> = q
            .split('&')
            .map(|kv| kv.split('=').next().unwrap())
            .collect();
        assert_eq!(keys, ["format", "RecallDateStart", "RecallDateEnd"]);
    }
}

#[test]
fn a_recall_match_with_evidence_and_not_mine() {
    let (mut a, _) = setup();
    let heater = add(&mut a, "furnace");
    call(
        &mut a,
        "update_item",
        json!({"id": heater, "name": "Space heater", "make": "Dreo", "model": "DR-HSH015"}),
    );
    call(&mut a, "set_recalls", json!({"on": true}));
    let out = call(
        &mut a,
        "recall_import",
        json!({"json": FIXTURE, "start": "2026-10-01", "end": "2026-11-27"}),
    );
    assert_eq!(out["received"], 7);
    let r = call(&mut a, "recalls", Value::Null);
    assert_eq!(r["synced_to"], "2026-11-27");
    assert_eq!(r["stored"], 7);
    // Up to date today: nothing more to fetch.
    assert!(r["next"].as_array().unwrap().is_empty());
    let m = &r["matches"][0];
    assert_eq!(m["item_id"], heater);
    assert_eq!(m["level"], "model");
    assert_eq!(m["recall"]["number"], "27054");
    assert!(m["recall"]["url"].as_str().unwrap().starts_with("https://"));
    assert!(m["evidence"].as_array().unwrap().len() >= 2);
    assert_eq!(call(&mut a, "today", Value::Null)["recall_matches"], 1);

    let r = call(
        &mut a,
        "dismiss_recall",
        json!({"item_id": heater, "number": "27054"}),
    );
    assert!(r["matches"].as_array().unwrap().is_empty());
    let back = r["dismissed"][0]["dismissal_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(call(&mut a, "today", Value::Null)["recall_matches"], 0);
    let r = call(&mut a, "undismiss_recall", json!({"id": back}));
    assert_eq!(r["matches"].as_array().unwrap().len(), 1);

    // Switching off forgets the downloaded list on this device.
    let r = call(&mut a, "set_recalls", json!({"on": false}));
    assert_eq!(r["stored"], 0);
    assert!(r["matches"].as_array().unwrap().is_empty());
    assert_eq!(call(&mut a, "today", Value::Null)["recall_matches"], 0);
}

#[test]
fn private_contractor_fields_are_left_out_for_limited_members() {
    let (mut a, _) = setup();
    call(
        &mut a,
        "save_contractor",
        json!({"name": "Ana Ruiz", "private_rating": 2, "private_notes": "Late twice"}),
    );
    // Make the device's member limited.
    let me = a.store().author().unwrap();
    let mut m: housewright_core::model::Member = a.store().fetch(&me.to_string()).unwrap();
    m.role = housewright_core::model::Role::Limited;
    a.store_mut().put(&m).unwrap();
    let c = &call(&mut a, "contractors", Value::Null)[0];
    assert!(c["private"].is_null());
    assert!(!c.to_string().contains("Late twice"));
}
