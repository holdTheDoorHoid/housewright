//! The bridge joins a household through the real server (in this
//! process, on a local socket), and what it does reaches the app's device
//! and back, labelled "via Home Assistant".

use std::net::SocketAddr;

use housewright_api::Api;
use housewright_bridge::*;
use housewright_core::store::{Store, SystemClock};
use housewright_server::{AppState, Config, Listen, serve};
use housewright_sync::native::UreqTransport;
use serde_json::{Value, json};

fn server() -> (String, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("hw-bridge-{}", uuid::Uuid::now_v7()));
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
    for _ in 0..50 {
        if std::net::TcpStream::connect(addr).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    (format!("http://127.0.0.1:{port}"), dir)
}

fn call(api: &mut Api, cmd: &str, args: Value) -> Value {
    api.call(cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
}

fn owner(url: &str) -> Api {
    let mut a = Api::new(Store::open_in_memory(Box::new(SystemClock)).unwrap()).unwrap();
    a.set_transport(Box::new(UreqTransport::default()));
    call(
        &mut a,
        "init",
        json!({"property_name": "Elm St", "your_name": "Ana", "time_zone": "America/New_York"}),
    );
    call(
        &mut a,
        "sync_setup",
        json!({"server": url, "device_name": "Ana's phone"}),
    );
    let added = call(
        &mut a,
        "add_from_template",
        json!({"template": "washing-machine", "new_room": "Laundry"}),
    );
    for t in added["tasks"].as_array().unwrap() {
        call(&mut a, "set_active", json!({"id": t["id"], "active": true}));
    }
    call(&mut a, "sync_now", Value::Null);
    a
}

fn pair(a: &mut Api, b: &mut Bridge, who: &str) -> Result<(), housewright_api::ApiError> {
    let inv = call(a, "sync_invite", json!({"who": who}));
    let s = b.pair_start(PairIn {
        server: None,
        code: inv["qr_text"].as_str().unwrap().into(),
        device_name: None,
    })?;
    assert_eq!(s["state"], "waiting");
    let pa = call(a, "sync_invite_poll", Value::Null);
    let pb = b.pair_poll()?;
    assert_eq!(pa["state"], "compare");
    // The same six digits on both sides (D-94).
    assert_eq!(pa["safety_number"], pb["safety_number"]);
    assert_eq!(b.pair_confirm(true)?["state"], "waiting_for_other");
    assert_eq!(
        call(a, "sync_invite_confirm", json!({"matches": true}))["state"],
        "done"
    );
    assert_eq!(b.pair_poll()?["state"], "done");
    Ok(())
}

fn find<'a>(v: &'a [TaskOut], title: &str) -> &'a TaskOut {
    v.iter().find(|t| t.title == title).unwrap()
}

#[test]
fn bridge_pairs_syncs_and_converges_with_the_app() {
    let (url, dir) = server();
    let mut a = owner(&url);

    // An owner's "another device of mine" code is refused: Home Assistant
    // must not get an owner's powers.
    let mut wrong = Bridge::in_memory(None).unwrap();
    let inv = call(&mut a, "sync_invite", json!({"who": "device"}));
    let e = wrong
        .pair_start(PairIn {
            server: None,
            code: inv["qr_text"].as_str().unwrap().into(),
            device_name: None,
        })
        .unwrap_err();
    assert!(e.message.contains("owner"), "{}", e.message);
    assert!(!wrong.is_paired());
    let _ = a.call("sync_invite_cancel", Value::Null);

    let mut b = Bridge::in_memory(None).unwrap();
    assert!(!b.status().unwrap().paired);
    pair(&mut a, &mut b, "home_assistant").unwrap();
    let st = b.status().unwrap();
    assert!(st.paired);
    assert_eq!(st.household.as_deref(), Some("Ana's household"));
    let me = st.member.unwrap();
    assert_eq!(me.name, "Home Assistant");
    assert!(me.home_assistant);
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);

    // The app lists the bridge as a Home Assistant device.
    let devices = call(&mut a, "sync_devices", Value::Null);
    let ha = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "Home Assistant")
        .unwrap();
    assert_eq!(ha["home_assistant"], true);
    assert_eq!(ha["role"], "member");

    // The bridge sees the washer's tasks and marks one done for Ana.
    let tasks = b.tasks().unwrap();
    assert_eq!(find(&tasks, "Inspect the washer supply hoses").state, "due");
    b.complete(CompleteIn {
        task: "Inspect the washer supply hoses".into(),
        member: Some("Ana".into()),
        at: None,
    })
    .unwrap();
    // A meter baseline, then 45 loads later; and a leak.
    b.reading(ReadingIn {
        item: "Washing machine".into(),
        meter: "loads".into(),
        value: 10.0,
        delta: false,
        throttle: false,
    })
    .unwrap();
    let r = b
        .reading(ReadingIn {
            item: "Washing machine".into(),
            meter: "loads".into(),
            value: 55.0,
            delta: false,
            throttle: false,
        })
        .unwrap();
    assert_eq!(r.tasks[0].state, "due");
    let e = b
        .event(EventIn {
            item: "Washing machine".into(),
            event: "leak".into(),
        })
        .unwrap();
    assert_eq!(e.tasks[0].state, "due");
    b.sync_quietly();
    assert_eq!(b.status().unwrap().sync.pending_changes, 0);

    // The app gets all of it, labelled.
    call(&mut a, "sync_now", Value::Null);
    let h = call(&mut a, "history", Value::Null).to_string();
    assert!(
        h.contains("Ana marked 'Inspect the washer supply hoses' done on"),
        "{h}"
    );
    assert!(h.contains(", via Home Assistant"), "{h}");
    assert!(
        h.contains("Recorded a reading of 55 on meter 'Loads washed', via Home Assistant"),
        "{h}"
    );
    let today = call(&mut a, "today", Value::Null);
    let titles: Vec<&str> = today["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"Run a washer cleaning cycle"),
        "{titles:?}"
    );
    assert!(
        titles.contains(&"Check the washing machine after a leak alert"),
        "{titles:?}"
    );
    assert!(
        !titles.contains(&"Inspect the washer supply hoses"),
        "{titles:?}"
    );
    let item = call(&mut a, "items", Value::Null)[0]["id"].clone();
    let detail = call(&mut a, "item", json!({"id": item}));
    assert_eq!(detail["history"][0]["by_name"], "Ana, via Home Assistant");

    // And back: the app snoozes the cleaning cycle; the bridge sees it.
    let id = find(&b.tasks().unwrap(), "Run a washer cleaning cycle")
        .id
        .clone();
    call(&mut a, "snooze", json!({"id": id, "until": "2099-01-01"}));
    call(&mut a, "sync_now", Value::Null);
    b.sync_quietly();
    assert_eq!(
        find(&b.tasks().unwrap(), "Run a washer cleaning cycle").state,
        "snoozed"
    );

    // The server holds none of it readable.
    let bytes: Vec<u8> = housewright_server::data_files(&dir)
        .iter()
        .flat_map(|f| std::fs::read(f).unwrap_or_default())
        .collect();
    for s in ["Washing machine", "Home Assistant", "Loads washed"] {
        assert!(
            !bytes.windows(s.len()).any(|w| w == s.as_bytes()),
            "server files contain {s}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}
