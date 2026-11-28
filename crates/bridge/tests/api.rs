//! The bridge's local API: authentication, every path, and that nothing
//! sensitive ever comes out (D-41, D-117).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use housewright_bridge::http::{Shared, router, token_matches};
use housewright_bridge::*;
use housewright_core::model::{Completion, Member, MemberKind};
use housewright_core::store::ManualClock;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const TOKEN: &str = "test-token-0123456789abcdefghijklmnop";
const DAY_MS: u64 = 86_400_000;

fn demo() -> (Shared, ManualClock) {
    // 2026-11-28 12:00 UTC.
    let clock = ManualClock::new(1_795_867_200_000);
    let mut b = Bridge::in_memory(Some(Box::new(clock.clone()))).unwrap();
    b.seed_demo().unwrap();
    (Shared::new(b, TOKEN), clock)
}

async fn send(
    s: &Shared,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut r = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    let req = if method == "GET" {
        r.body(Body::empty()).unwrap()
    } else {
        r.header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let res = router(s.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

async fn ok(s: &Shared, method: &str, path: &str, body: Value) -> Value {
    let (st, v) = send(s, method, path, Some(TOKEN), body).await;
    assert_eq!(st, StatusCode::OK, "{method} {path}: {v}");
    assert_no_secrets(&v);
    v
}

/// The demo household's sensitive values all contain SECRET.
fn assert_no_secrets(v: &Value) {
    let t = v.to_string();
    assert!(!t.contains("SECRET"), "sensitive data in a response: {t}");
}

fn task<'a>(tasks: &'a Value, title: &str) -> &'a Value {
    tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == title)
        .unwrap_or_else(|| panic!("no task {title}"))
}

const ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/v1/status"),
    ("GET", "/api/v1/snapshot"),
    ("GET", "/api/v1/tasks"),
    ("GET", "/api/v1/items"),
    ("GET", "/api/v1/members"),
    ("POST", "/api/v1/complete"),
    ("POST", "/api/v1/snooze"),
    ("POST", "/api/v1/readings"),
    ("POST", "/api/v1/events"),
    ("POST", "/api/v1/devices/match"),
    ("POST", "/api/v1/devices/import"),
    ("POST", "/api/v1/pairing"),
    ("GET", "/api/v1/pairing"),
    ("POST", "/api/v1/pairing/confirm"),
    ("POST", "/api/v1/sync"),
];

#[test]
fn token_compare() {
    let h: [u8; 32] = Sha256::digest(TOKEN.as_bytes()).into();
    assert!(token_matches(&h, TOKEN));
    assert!(!token_matches(&h, ""));
    assert!(!token_matches(&h, &TOKEN[..TOKEN.len() - 1]));
    assert!(!token_matches(&h, &format!("{TOKEN}x")));
    assert!(!token_matches(&h, &TOKEN.to_uppercase()));
}

#[tokio::test]
async fn every_route_needs_the_token() {
    let (s, _) = demo();
    let (st, v) = send(&s, "GET", "/api/v1/health", None, Value::Null).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["service"], "housewright-bridge");
    for (m, p) in ROUTES {
        for tok in [None, Some("wrong"), Some(""), Some(&TOKEN[1..])] {
            let (st, v) = send(&s, m, p, tok, json!({})).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED, "{m} {p} with {tok:?}");
            assert_eq!(v["error"]["code"], "unauthorized");
        }
    }
    // A wrong scheme is refused too.
    let req = Request::builder()
        .uri("/api/v1/status")
        .header("authorization", format!("Basic {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router(s.clone()).oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let (st, _) = send(&s, "GET", "/nothing", Some(TOKEN), Value::Null).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reading_paths_show_nothing_sensitive() {
    let (s, _) = demo();
    for p in [
        "/api/v1/status",
        "/api/v1/snapshot",
        "/api/v1/tasks",
        "/api/v1/items",
        "/api/v1/members",
    ] {
        ok(&s, "GET", p, Value::Null).await;
    }
    let snap = ok(&s, "GET", "/api/v1/snapshot", Value::Null).await;
    assert_eq!(snap["status"]["paired"], true);
    assert_eq!(snap["status"]["member"]["home_assistant"], true);
    // States are one of the six.
    for t in snap["tasks"].as_array().unwrap() {
        assert!(STATES.contains(&t["state"].as_str().unwrap()), "{t}");
        assert_eq!(t["active"], true);
    }
    // The sensitive item cannot be reached by name either.
    let (st, v) = send(
        &s,
        "POST",
        "/api/v1/events",
        Some(TOKEN),
        json!({"item": "SECRET-SAFE-NAME", "event": "leak"}),
    )
    .await;
    // Not found, exactly like a name that does not exist (the message
    // only repeats what was asked).
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(v["error"]["message"], "No thing called 'SECRET-SAFE-NAME'.");
    let (st, _) = send(
        &s,
        "POST",
        "/api/v1/complete",
        Some(TOKEN),
        json!({"task": "SECRET-SAFE-TASK"}),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // Bad input is a 400 in the same shape.
    let (st, v) = send(
        &s,
        "POST",
        "/api/v1/complete",
        Some(TOKEN),
        json!({"nope": 1}),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"]["code"], "invalid");
}

#[tokio::test]
async fn complete_for_a_person_says_via_home_assistant() {
    let (s, _) = demo();
    let t = ok(
        &s,
        "POST",
        "/api/v1/complete",
        json!({"task": "test the smoke alarm", "member": "ben"}),
    )
    .await;
    assert_eq!(t["state"], "ok");
    assert_eq!(t["last_done"], "2026-11-28");
    // Without a member: "Home Assistant".
    ok(
        &s,
        "POST",
        "/api/v1/complete",
        json!({"task": "Inspect the anode rod"}),
    )
    .await;
    let (st, _) = send(
        &s,
        "POST",
        "/api/v1/complete",
        Some(TOKEN),
        json!({"task": "Inspect the anode rod", "member": "Nobody"}),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let mut b = s.bridge.lock().unwrap();
    let api = b.api();
    let ha: Member = api
        .store()
        .list::<Member>()
        .unwrap()
        .into_iter()
        .find(|m| m.kind == MemberKind::HomeAssistant)
        .unwrap();
    let ben = api
        .store()
        .list::<Member>()
        .unwrap()
        .into_iter()
        .find(|m| m.name == "Ben")
        .unwrap();
    let cs: Vec<Completion> = api.store().list().unwrap();
    assert_eq!(cs.len(), 2);
    // The record is the bridge's (D-104); Ben is its word for who did it.
    assert!(cs.iter().all(|c| c.by == ha.id));
    assert!(cs.iter().any(|c| c.done_by == Some(ben.id)));
    let h = api.call("history", Value::Null).unwrap().to_string();
    assert!(
        h.contains("Ben marked 'Test the smoke alarm' done on 2026-11-28, via Home Assistant"),
        "{h}"
    );
    assert!(
        h.contains("Marked 'Inspect the anode rod' done on 2026-11-28, via Home Assistant"),
        "{h}"
    );
}

#[tokio::test]
async fn snooze_to_a_day_or_by_days() {
    let (s, _) = demo();
    let t = ok(
        &s,
        "POST",
        "/api/v1/snooze",
        json!({"task": "Test the smoke alarm", "days": 3}),
    )
    .await;
    assert_eq!(t["state"], "snoozed");
    assert_eq!(t["due"], "2026-12-01");
    let t = ok(
        &s,
        "POST",
        "/api/v1/snooze",
        json!({"task": "Inspect the anode rod", "until": "2026-12-24"}),
    )
    .await;
    assert_eq!(t["due"], "2026-12-24");
    for bad in [
        json!({"task": "Inspect the anode rod", "until": "2026-11-28"}),
        json!({"task": "Inspect the anode rod", "until": "2028-01-01"}),
    ] {
        let (st, _) = send(&s, "POST", "/api/v1/snooze", Some(TOKEN), bad).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn meter_readings_make_a_usage_task_due() {
    let (s, clock) = demo();
    let r = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "loads", "value": 100}),
    )
    .await;
    assert_eq!(r["stored"], true);
    let t = &r["tasks"][0];
    assert_eq!(t["title"], "Run a washer cleaning cycle");
    assert_eq!(t["state"], "ok");
    // Throttled pushes: unchanged and too soon are skipped.
    let r = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "loads", "value": 100, "throttle": true}),
    )
    .await;
    assert_eq!(
        (r["stored"].clone(), r["reason"].clone()),
        (json!(false), json!("unchanged"))
    );
    let r = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "Loads washed", "value": 120, "throttle": true}),
    )
    .await;
    assert_eq!(r["reason"], "too soon");
    clock.advance(DAY_MS);
    let r = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "loads", "value": 120, "throttle": true}),
    )
    .await;
    assert_eq!(r["stored"], true);
    // One more cycle, as a delta; then past the threshold of 40.
    clock.advance(DAY_MS);
    let r = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "loads", "value": 1, "delta": true}),
    )
    .await;
    assert_eq!(r["value"], 121.0);
    clock.advance(DAY_MS);
    // A counter that restarted from zero keeps counting up (D-121).
    let r = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "loads", "value": 25}),
    )
    .await;
    assert_eq!(r["value"], 146.0);
    assert_eq!(r["tasks"][0]["state"], "due");
    assert_eq!(r["tasks"][0]["usage_used"], 46.0);
    for bad in [
        json!({"item": "Washing machine", "meter": "hours", "value": 1}),
        json!({"item": "Fridge", "meter": "loads", "value": 1}),
    ] {
        let (st, v) = send(&s, "POST", "/api/v1/readings", Some(TOKEN), bad).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{v}");
    }
    let (st, _) = send(
        &s,
        "POST",
        "/api/v1/readings",
        Some(TOKEN),
        json!({"item": "Washing machine", "meter": "loads", "value": -1}),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let mut b = s.bridge.lock().unwrap();
    let h = b.api().call("history", Value::Null).unwrap().to_string();
    assert!(
        h.contains("Recorded a reading of 146 on meter 'Loads washed', via Home Assistant"),
        "{h}"
    );
}

#[tokio::test]
async fn events_trigger_tasks() {
    let (s, clock) = demo();
    let e = ok(
        &s,
        "POST",
        "/api/v1/events",
        json!({"item": "Smoke alarm", "event": "Battery low"}),
    )
    .await;
    assert_eq!(e["recorded"], true);
    assert_eq!(e["event"], "battery_low");
    assert_eq!(
        e["switched_on"].as_array().unwrap().len(),
        1,
        "the suggestion was switched on"
    );
    let t = &e["tasks"][0];
    assert_eq!(
        t["title"],
        "Change the smoke alarm battery after a low-battery alert"
    );
    assert_eq!(t["state"], "due");
    assert_eq!(t["event"], "battery_low");
    // The same event again within ten minutes is one event.
    let again = ok(
        &s,
        "POST",
        "/api/v1/events",
        json!({"item": "Smoke alarm", "event": "battery_low"}),
    )
    .await;
    assert_eq!(again["recorded"], false);
    // Done, then quiet; a later event fires it again.
    ok(&s, "POST", "/api/v1/complete", json!({"task": t["id"]})).await;
    let tasks = ok(&s, "GET", "/api/v1/tasks", Value::Null).await;
    assert_eq!(
        task(
            &tasks,
            "Change the smoke alarm battery after a low-battery alert"
        )["state"],
        "ok"
    );
    clock.advance(DAY_MS);
    let e = ok(
        &s,
        "POST",
        "/api/v1/events",
        json!({"item": "Smoke alarm", "event": "battery_low"}),
    )
    .await;
    assert_eq!(e["tasks"][0]["state"], "due");
    assert_eq!(e["switched_on"].as_array().unwrap().len(), 0);
    // An event no task follows is still recorded, and says so.
    let e = ok(
        &s,
        "POST",
        "/api/v1/events",
        json!({"item": "Tank water heater", "event": "battery_low"}),
    )
    .await;
    assert!(e["message"].as_str().unwrap().contains("No task"), "{e}");
    let (st, _) = send(
        &s,
        "POST",
        "/api/v1/events",
        Some(TOKEN),
        json!({"item": "Smoke alarm", "event": "<script>"}),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let items = ok(&s, "GET", "/api/v1/items", Value::Null).await;
    let alarm = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "Smoke alarm")
        .unwrap();
    assert_eq!(alarm["events"], json!(["battery_low"]));
    // The event counter is not listed as a meter.
    assert_eq!(alarm["meters"], json!([]));
}

#[tokio::test]
async fn devices_become_things_with_suggested_tasks() {
    let (s, _) = demo();
    let devices = json!({"devices": [
        {"id": "ha-1", "name": "Dishwasher", "manufacturer": "Bosch", "model": "SHPM88Z75N", "area": "Kitchen"},
        {"id": "ha-2", "name": "Upstairs smoke detector", "manufacturer": "Google", "model": "Nest Protect", "area": "Upstairs"},
        {"id": "ha-3", "name": "Living room lamp", "manufacturer": "Signify", "model": "LCT015", "area": "Living room"},
        {"id": "ha-4", "name": "Washing machine", "manufacturer": null, "model": null, "area": null}
    ]});
    let m = ok(&s, "POST", "/api/v1/devices/match", devices.clone()).await;
    let m = m.as_array().unwrap();
    assert_eq!(m[0]["template"], "dishwasher");
    assert_eq!(m[1]["template"], "smoke-alarm");
    assert_eq!(m[2]["template"], Value::Null);
    assert!(m[3]["existing_item"].is_string(), "already recorded");
    let out = ok(&s, "POST", "/api/v1/devices/import", devices).await;
    let added = out["added"].as_array().unwrap();
    assert_eq!(added.len(), 2);
    assert!(added[0]["suggested_tasks"].as_u64().unwrap() > 0);
    let skipped: Vec<&str> = out["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["reason"].as_str().unwrap())
        .collect();
    assert_eq!(skipped, ["no matching kind of thing", "already recorded"]);
    let items = ok(&s, "GET", "/api/v1/items", Value::Null).await;
    let dw = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "Dishwasher")
        .unwrap();
    assert_eq!(dw["make"], "Bosch");
    assert_eq!(dw["room_name"], "Kitchen");
    // Imported tasks are suggestions: none switched on.
    let tasks = ok(&s, "GET", "/api/v1/tasks", Value::Null).await;
    assert!(
        !tasks
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["item_name"] == "Dishwasher")
    );
    // Importing again adds nothing.
    let again = ok(&s, "POST", "/api/v1/devices/import", json!({"devices": [{"id": "ha-1", "name": "Dishwasher", "manufacturer": "Bosch", "model": "SHPM88Z75N"}]})).await;
    assert_eq!(again["added"], json!([]));
}

#[tokio::test]
async fn an_unpaired_bridge_says_so() {
    let b = Bridge::in_memory(None).unwrap();
    let s = Shared::new(b, TOKEN);
    let st = ok(&s, "GET", "/api/v1/status", Value::Null).await;
    assert_eq!(st["paired"], false);
    let (code, v) = send(&s, "GET", "/api/v1/snapshot", Some(TOKEN), Value::Null).await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(v["error"]["code"], "not_paired");
}

/// The shapes Home Assistant relies on, kept in a file the Python tests
/// read (`integrations/home-assistant/tests/fixtures/bridge-schema.json`).
/// `UPDATE_FIXTURES=1 cargo test -p housewright-bridge` rewrites it.
#[tokio::test]
async fn schema_fixture_is_current() {
    let (s, _) = demo();
    let snap = ok(&s, "GET", "/api/v1/snapshot", Value::Null).await;
    let reading = ok(
        &s,
        "POST",
        "/api/v1/readings",
        json!({"item": "Washing machine", "meter": "loads", "value": 1}),
    )
    .await;
    let event = ok(
        &s,
        "POST",
        "/api/v1/events",
        json!({"item": "Washing machine", "event": "leak"}),
    )
    .await;
    let matched = ok(
        &s,
        "POST",
        "/api/v1/devices/match",
        json!({"devices": [{"id": "x", "name": "Dryer"}]}),
    )
    .await;
    let imported = ok(
        &s,
        "POST",
        "/api/v1/devices/import",
        json!({"devices": [{"id": "x", "name": "Dryer"}]}),
    )
    .await;
    let keys = |v: &Value| -> Value {
        let mut k: Vec<&String> = v.as_object().unwrap().keys().collect();
        k.sort();
        json!(k)
    };
    let schema = json!({
        "_comment": "Written by crates/bridge/tests/api.rs (schema_fixture_is_current). Keys of each object the bridge returns.",
        "status": keys(&snap["status"]),
        "status.sync": keys(&snap["status"]["sync"]),
        "member": keys(&snap["members"][0]),
        "task": keys(&snap["tasks"][0]),
        "item": keys(&snap["items"][0]),
        "meter": keys(&snap["items"].as_array().unwrap().iter().find(|i| !i["meters"].as_array().unwrap().is_empty()).unwrap()["meters"][0]),
        "snapshot": keys(&snap),
        "reading": keys(&reading),
        "event": keys(&event),
        "device_match": keys(&matched[0]),
        "import": keys(&imported),
        "imported": keys(&imported["added"][0]),
        "states": STATES,
    });
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../integrations/home-assistant/tests/fixtures/bridge-schema.json"
    );
    let text = serde_json::to_string_pretty(&schema).unwrap() + "\n";
    if std::env::var("UPDATE_FIXTURES").is_ok() {
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, &text).unwrap();
    }
    let on_disk = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        on_disk, text,
        "run UPDATE_FIXTURES=1 cargo test -p housewright-bridge"
    );
}

/// Contractors (D-28) never reach Home Assistant: not their names, not the
/// household's private rating or notes, not what a job cost (Phase 9).
#[tokio::test]
async fn contractors_and_their_private_fields_never_come_out() {
    let clock = ManualClock::new(1_795_867_200_000);
    let mut b = Bridge::in_memory(Some(Box::new(clock.clone()))).unwrap();
    b.seed_demo().unwrap();
    let api = b.api();
    api.call(
        "save_contractor",
        json!({"name": "PRIVATE-CONTRACTOR Ruiz", "company": "PRIVATE-COMPANY",
               "phone": "555-0199", "private_rating": 1, "would_hire_again": "no",
               "private_notes": "PRIVATE-NOTE never again", "licence_number": "PRIVATE-LIC-1"}),
    )
    .unwrap();
    let k = api.call("contractors", Value::Null).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let t = api.call("today", Value::Null).unwrap();
    let task = t["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .chain(t["coming_up"].as_array().unwrap())
        .next()
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = api.call("complete", json!({"id": task})).unwrap();
    api.call(
        "add_details",
        json!({"completion_id": done["completion_id"], "contractor_id": k,
               "costs": {"contractor": "987.65"}}),
    )
    .unwrap();
    let s = Shared::new(b, TOKEN);
    for p in [
        "/api/v1/status",
        "/api/v1/snapshot",
        "/api/v1/tasks",
        "/api/v1/items",
        "/api/v1/members",
    ] {
        let v = ok(&s, "GET", p, Value::Null).await;
        let text = v.to_string();
        for bad in ["PRIVATE", "Ruiz", "555-0199", "987", "never again"] {
            assert!(!text.contains(bad), "{p} shows {bad}: {text}");
        }
    }
}
