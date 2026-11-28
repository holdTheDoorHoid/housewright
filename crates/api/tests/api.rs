//! The command surface end to end, as the interface drives it.

use housewright_api::{Api, COMMANDS};
use housewright_core::store::{ManualClock, Store};
use serde_json::{Value, json};

/// 2026-11-27 12:00 UTC.
const NOW_MS: u64 = 1_795_780_800_000;
const DAY_MS: u64 = 86_400_000;

fn api() -> (Api, ManualClock) {
    let clock = ManualClock::new(NOW_MS);
    let store = Store::open_in_memory(Box::new(clock.clone())).unwrap();
    (Api::new(store).unwrap(), clock)
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn setup() -> (Api, ManualClock) {
    let (mut a, clock) = api();
    let s = call(
        &mut a,
        "init",
        json!({"property_name": "Home", "your_name": "Alex", "time_zone": "America/New_York"}),
    );
    assert_eq!(s["initialized"], true);
    assert_eq!(s["household_name"], "Alex's household");
    (a, clock)
}

#[test]
fn commands_need_a_household_first() {
    let (mut a, _) = api();
    assert_eq!(call(&mut a, "status", Value::Null)["initialized"], false);
    let e = a.call("today", Value::Null).unwrap_err();
    assert_eq!(e.code, "not_initialized");
}

#[test]
fn every_listed_command_is_dispatched() {
    let (mut a, _) = setup();
    for cmd in COMMANDS {
        if let Err(e) = a.call(cmd, json!({})) {
            assert!(
                !e.message.contains("unknown command"),
                "{cmd} is listed but not dispatched"
            );
        }
    }
    assert!(
        a.call("nope", json!({}))
            .unwrap_err()
            .message
            .contains("unknown command")
    );
}

#[test]
fn today_flow_add_start_small_done_snooze_review() {
    let (mut a, clock) = setup();
    let templates = call(&mut a, "templates", Value::Null);
    assert!(templates.as_array().unwrap().len() >= 10);

    let added = call(
        &mut a,
        "add_from_template",
        json!({"template": "smoke-alarm", "new_room": "Hallway"}),
    );
    let tasks = added["tasks"].as_array().unwrap();
    assert!(!tasks.is_empty());
    assert!(
        tasks.iter().all(|t| t["active"] == false),
        "P3: start switched off"
    );

    // Nothing is on yet, so Today is empty but says suggestions exist.
    let today = call(&mut a, "today", Value::Null);
    assert_eq!(today["active_count"], 0);
    assert!(today["suggested_count"].as_u64().unwrap() > 0);

    let on = call(
        &mut a,
        "start_small",
        json!({"item_id": added["item_id"], "n": 1}),
    );
    assert_eq!(on.as_array().unwrap().len(), 1);
    let id = on[0]["id"].as_str().unwrap().to_string();

    // A task never done is due when added, so it shows on Today.
    let today = call(&mut a, "today", Value::Null);
    let first = &today["tasks"][0];
    assert_eq!(first["id"], id.as_str());
    assert_eq!(first["room_name"], "Hallway");
    assert_eq!(today["lapsed"], false);

    // Why it matters and steps are on the detail (P2, P9).
    let detail = call(&mut a, "task", json!({"id": id}));
    assert!(detail["why"].as_str().is_some());
    assert!(detail["steps"].as_array().unwrap().len() >= 3);

    // One tap: no notes needed (P1). Afterwards an optional note.
    let done = call(&mut a, "complete", json!({"id": id}));
    assert_ne!(done["card"]["state"], "due");
    call(
        &mut a,
        "annotate",
        json!({"completion_id": done["completion_id"], "text": "Battery fine"}),
    );
    let detail = call(&mut a, "task", json!({"id": id}));
    assert_eq!(detail["completions"][0]["notes"][0], "Battery fine");
    assert!(
        call(&mut a, "today", Value::Null)["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["id"] != id.as_str())
    );

    // Undo is a voiding record, not a deletion.
    call(
        &mut a,
        "void_completion",
        json!({"id": done["completion_id"]}),
    );
    let detail = call(&mut a, "task", json!({"id": id}));
    assert_eq!(detail["completions"][0]["voided"], true);

    // Three snoozes ask for a review (P7).
    let opts = call(&mut a, "snooze_options", Value::Null);
    assert_eq!(opts[1]["label"], "This weekend"); // 2026-11-27 is a Friday
    assert_eq!(opts[1]["date"], "2026-11-28");
    let mut card = Value::Null;
    for k in 0..3 {
        clock.advance(DAY_MS);
        card = call(
            &mut a,
            "snooze",
            json!({"id": id, "until": format!("2026-12-0{}", k + 3)}),
        );
    }
    assert_eq!(card["needs_review"], true);
    let card = call(
        &mut a,
        "review",
        json!({"id": id, "action": "change_interval", "every": "2 months"}),
    );
    assert!(card["schedule_text"].as_str().unwrap().contains("2 months"));
    let card = call(
        &mut a,
        "review",
        json!({"id": id, "action": "not_applicable"}),
    );
    assert_eq!(card["state"], "not_applicable");

    // History names who did what, newest first, without values (D-58).
    let history = call(&mut a, "history", Value::Null);
    let texts: Vec<&str> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["text"].as_str().unwrap())
        .collect();
    assert!(texts.iter().any(|t| t.contains("done")), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("Battery fine")));
}

#[test]
fn welcome_back_after_a_lapse() {
    let (mut a, clock) = setup();
    let added = call(
        &mut a,
        "add_from_template",
        json!({"template": "hvac-air-filter"}),
    );
    call(&mut a, "start_small", json!({"item_id": added["item_id"]}));
    call(&mut a, "today", Value::Null);
    clock.advance(120 * DAY_MS);
    let today = call(&mut a, "today", Value::Null);
    assert_eq!(today["lapsed"], true);
    assert!(
        !today["welcome_back"]["focus"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Seeing it once is enough; dismissing marks the person as back.
    call(&mut a, "mark_seen", Value::Null);
    assert_eq!(call(&mut a, "today", Value::Null)["lapsed"], false);
}

#[test]
fn reminder_plan_respects_quiet_hours() {
    let (mut a, _) = setup();
    let added = call(
        &mut a,
        "add_from_template",
        json!({"template": "smoke-alarm"}),
    );
    call(&mut a, "start_small", json!({"item_id": added["item_id"]}));
    // Done today, so the next one is due in the future and gets reminders.
    for t in call(&mut a, "today", Value::Null)["tasks"]
        .as_array()
        .unwrap()
        .clone()
    {
        call(&mut a, "complete", json!({"id": t["id"]}));
    }
    call(
        &mut a,
        "set_reminder_prefs",
        json!({"prefs": {"quiet_hours": {"start": "21:00", "end": "10:15"}}}),
    );
    let plan = call(&mut a, "reminder_plan", json!({"horizon_days": 400}));
    let plan = plan.as_array().unwrap();
    assert!(!plan.is_empty());
    for r in plan {
        let at = jiff::Timestamp::from_millisecond(r["at_ms"].as_i64().unwrap()).unwrap();
        let local = at.in_tz("America/New_York").unwrap();
        assert_eq!((local.hour(), local.minute()), (10, 15));
    }
    let status = call(&mut a, "status", Value::Null);
    assert_eq!(status["me"]["reminders"]["quiet_hours"]["end"], "10:15:00");

    let test = call(&mut a, "test_reminder", json!({"in_seconds": 30}));
    assert_eq!(test["at_ms"].as_i64().unwrap(), NOW_MS as i64 + 30_000);
}

#[test]
fn export_then_import_into_a_fresh_database() {
    let (mut a, _) = setup();
    call(
        &mut a,
        "add_from_template",
        json!({"template": "sump-pump"}),
    );
    let out = call(&mut a, "export", Value::Null);
    assert!(out["filename"].as_str().unwrap().ends_with(".json"));
    let (mut b, _) = api();
    let s = call(&mut b, "import", json!({"json": out["json"]}));
    assert_eq!(s["initialized"], true);
    assert_eq!(
        call(&mut b, "items", Value::Null).as_array().unwrap().len(),
        1
    );
    // Importing over an existing household is refused.
    assert!(a.call("import", json!({"json": out["json"]})).is_err());
}

#[test]
fn call_json_wraps_results_and_errors() {
    let (mut a, _) = api();
    let ok: Value = serde_json::from_str(&a.call_json("status", "")).unwrap();
    assert_eq!(ok["ok"]["initialized"], false);
    let err: Value = serde_json::from_str(&a.call_json("status", "{not json")).unwrap();
    assert_eq!(err["error"]["code"], "invalid");
}
