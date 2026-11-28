//! Sync through the command surface, over a real local socket: set up,
//! show the recovery key once, pair, a large file fetched when opened,
//! remove, the sealed volume's recovery page.

use std::net::SocketAddr;

use housewright_api::Api;
use housewright_core::store::{Store, SystemClock};
use housewright_server::{AppState, Config, Listen, serve};
use housewright_sync::native::UreqTransport;
use serde_json::{Value, json};

fn server() -> (String, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("hw-api-sync-{}", uuid::Uuid::now_v7()));
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let state = AppState::open(Config::new(&dir)).unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(serve(state, "", Listen::Plain(addr)))
            .unwrap();
    });
    let url = format!("http://127.0.0.1:{port}");
    for _ in 0..50 {
        if std::net::TcpStream::connect(addr).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    (url, dir)
}

fn api() -> Api {
    let mut a = Api::new(Store::open_in_memory(Box::new(SystemClock)).unwrap()).unwrap();
    a.set_transport(Box::new(UreqTransport::default()));
    a
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

#[test]
fn sync_through_the_api() {
    let (url, dir) = server();
    let mut a = api();
    // Off by default, and says so.
    let s = call(&mut a, "sync_status", Value::Null);
    assert_eq!(s["enabled"], false);
    assert_eq!(s["available"], true);
    call(
        &mut a,
        "init",
        json!({"property_name": "Elm St", "your_name": "Ana", "time_zone": "America/New_York"}),
    );
    // A wrong address is explained, not a stack trace.
    let e = a
        .call(
            "sync_setup",
            json!({"server": "http://example.org", "device_name": "x"}),
        )
        .unwrap_err();
    assert!(e.message.contains("https://"), "{}", e.message);
    let out = call(
        &mut a,
        "sync_setup",
        json!({"server": url, "device_name": "Ana's laptop"}),
    );
    let key = out["recovery_key"].as_str().unwrap().to_string();
    assert_eq!(key.len(), 44);
    assert_eq!(out["status"]["enabled"], true);
    assert_eq!(out["status"]["recovery_confirmed"], false);
    assert_eq!(out["status"]["pending_changes"], 0, "everything was pushed");
    // The key is not kept anywhere on the device.
    let snapshot = serde_json::to_string(&a.store().snapshot().unwrap().entities).unwrap();
    assert!(!snapshot.contains(&key));
    for k in ["config", "device_secret", "chain", "rosters", "status"] {
        assert!(
            !a.store()
                .sync_get(k)
                .unwrap()
                .unwrap_or_default()
                .contains(&key)
        );
    }
    // The sealed volume printed now shows the key; later prints do not,
    // and both count as the same section (no nudge to reprint).
    let mut data = housewright_binder::gather(a.store(), None, a.store().now()).unwrap();
    assert!(matches!(
        data.recovery,
        housewright_binder::Recovery::KeptSeparately { .. }
    ));
    let later =
        housewright_binder::build(&data, housewright_binder::Edition::Full, Default::default())
            .sealed
            .unwrap();
    data.recovery = housewright_binder::Recovery::Key {
        server: url.clone(),
        key: key.clone(),
    };
    let at_setup =
        housewright_binder::build(&data, housewright_binder::Edition::Full, Default::default())
            .sealed
            .unwrap();
    let rec = |d: &housewright_binder::Document| {
        d.sections
            .iter()
            .find(|s| s.key == "sealed:recovery")
            .unwrap()
            .clone()
    };
    assert!(rec(&at_setup).blocks.iter().any(|b| {
        let mut t = vec![];
        b.texts(&mut t);
        t.contains(&key)
    }));
    assert!(!format!("{:?}", rec(&later)).contains(&key));
    assert_eq!(rec(&at_setup).fingerprint(), rec(&later).fingerprint());
    let pdf = a
        .call_download(
            "binder_pdf",
            json!({"edition": "full", "volume": "sealed", "recovery_key": key, "record": false}),
        )
        .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    call(&mut a, "sync_confirm_recovery", Value::Null);
    assert_eq!(
        call(&mut a, "sync_status", Value::Null)["recovery_confirmed"],
        true
    );

    // A big manual, attached on A, is fetched on B only when opened.
    let things = call(
        &mut a,
        "add_from_template",
        json!({"template": "water-heater", "new_room": "Basement"}),
    );
    let item = things["item_id"].as_str().unwrap().to_string();
    let mut pdf = b"%PDF-1.4\n".to_vec();
    pdf.resize(5 * 1024 * 1024, b' ');
    pdf.extend_from_slice(b"\n%%EOF\n");
    let att = a
        .call_upload(
            "attach",
            json!({"about_kind": "item", "about_id": item, "name": "Manual.pdf"}),
            &pdf,
        )
        .unwrap();
    call(&mut a, "sync_now", Value::Null);

    // Pair B as a new member.
    let inv = call(
        &mut a,
        "sync_invite",
        json!({"who": "person", "name": "Ben", "role": "member"}),
    );
    assert!(inv["qr_svg"].as_str().unwrap().starts_with("<svg"));
    let mut b = api();
    let s = call(
        &mut b,
        "sync_join",
        json!({"code": inv["qr_text"], "device_name": "Ben's phone"}),
    );
    assert_eq!(s["state"], "waiting");
    let pa = call(&mut a, "sync_invite_poll", Value::Null);
    let pb = call(&mut b, "sync_join_poll", Value::Null);
    assert_eq!(pa["state"], "compare");
    assert_eq!(pa["safety_number"], pb["safety_number"]);
    assert_eq!(
        call(&mut b, "sync_join_confirm", json!({"matches": true}))["state"],
        "waiting_for_other"
    );
    assert_eq!(
        call(&mut a, "sync_invite_confirm", json!({"matches": true}))["state"],
        "done"
    );
    assert_eq!(call(&mut b, "sync_join_poll", Value::Null)["state"], "done");
    let st = call(&mut b, "status", Value::Null);
    assert_eq!(st["initialized"], true);
    assert_eq!(st["me"]["name"], "Ben");
    assert_eq!(st["property"]["name"], "Elm St");
    // The manual was not downloaded during sync (too big) but opens.
    let hash = b
        .store()
        .list::<housewright_core::model::Attachment>()
        .unwrap()[0]
        .sha256
        .clone();
    assert!(!b.blobs().has(&hash).unwrap());
    let got = b
        .call_download("attachment_file", json!({"id": att["id"]}))
        .unwrap();
    assert_eq!(got, pdf);
    assert!(b.blobs().has(&hash).unwrap(), "kept for next time");

    // Devices, then removal.
    let devices = call(&mut a, "sync_devices", Value::Null);
    assert_eq!(devices.as_array().unwrap().len(), 2);
    let ben = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "Ben's phone")
        .unwrap()
        .clone();
    assert_eq!(ben["member_name"], "Ben");
    assert!(ben["last_seen"].is_string());
    let left = call(&mut a, "sync_remove_device", json!({"id": ben["id"]}));
    assert_eq!(left.as_array().unwrap().len(), 1);
    let e = b.call("sync_now", Value::Null).unwrap_err();
    assert_eq!(e.code, "removed");
    assert_eq!(call(&mut b, "sync_status", Value::Null)["removed"], true);
    // Ben's copy stays usable on his device.
    assert_eq!(call(&mut b, "status", Value::Null)["initialized"], true);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restore_with_the_recovery_key_through_the_api() {
    let (url, dir) = server();
    let mut a = api();
    call(
        &mut a,
        "init",
        json!({"property_name": "Elm St", "your_name": "Ana", "time_zone": "America/New_York"}),
    );
    let key = call(
        &mut a,
        "sync_setup",
        json!({"server": url, "device_name": "Ana's laptop"}),
    )["recovery_key"]
        .as_str()
        .unwrap()
        .to_string();
    let mut fresh = api();
    let e = fresh
        .call(
            "sync_restore",
            json!({"server": url, "recovery_key": "ABCD-EFGH", "device_name": "New"}),
        )
        .unwrap_err();
    assert!(e.message.contains("36"), "{}", e.message);
    let st = call(
        &mut fresh,
        "sync_restore",
        json!({"server": url, "recovery_key": key, "device_name": "New"}),
    );
    assert_eq!(st["initialized"], true);
    assert_eq!(st["me"]["name"], "Ana");
    assert_eq!(
        call(&mut fresh, "sync_status", Value::Null)["role"],
        "owner"
    );
    let _ = std::fs::remove_dir_all(dir);
}
