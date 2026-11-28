//! Over a real socket: the server's own `serve` on 127.0.0.1 and the
//! native HTTP client (`native-http`).

#![cfg(feature = "native-http")]

use housewright_core::blobs::MemoryBlobs;
use housewright_core::store::{Store, SystemClock};
use housewright_server::{AppState, Config, Listen, serve};
use housewright_sync::native::UreqTransport;
use housewright_sync::{Invitee, PairState, SyncClient};

#[test]
fn two_devices_over_http() {
    let dir = std::env::temp_dir().join(format!("hw-http-{}", uuid::Uuid::now_v7()));
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let state = AppState::open(Config::new(&dir)).unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(serve(state, "", Listen::Plain(addr)))
            .unwrap();
    });
    let url = format!("http://127.0.0.1:{port}");
    let mut a = Store::open_in_memory(Box::new(SystemClock)).unwrap();
    a.init_household("Home", "Ana", "UTC").unwrap();
    let mut ab = MemoryBlobs::default();
    let mut ac = SyncClient::new(Box::new(UreqTransport::default()));
    for _ in 0..50 {
        if ac.health(&a, &url).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    ac.setup(&mut a, &url, "Ana's laptop", None).unwrap();
    ac.sync(&mut a, &mut ab).unwrap();
    let inv = ac.invite_start(&mut a, Invitee::MyDevice).unwrap();
    let mut b = Store::open_in_memory(Box::new(SystemClock)).unwrap();
    let mut bb = MemoryBlobs::default();
    let mut bc = SyncClient::new(Box::new(UreqTransport::default()));
    bc.join_start(&b, &url, &inv.code, "Ana's phone").unwrap();
    let PairState::Compare {
        safety_number: x, ..
    } = ac.invite_poll(&a).unwrap()
    else {
        panic!()
    };
    let PairState::Compare {
        safety_number: y, ..
    } = bc.join_poll(&mut b, &mut bb).unwrap()
    else {
        panic!()
    };
    assert_eq!(x, y);
    bc.join_confirm(&mut b, &mut bb, true).unwrap();
    ac.invite_confirm(&mut a, &mut ab, true).unwrap();
    assert_eq!(bc.join_poll(&mut b, &mut bb).unwrap(), PairState::Done);
    assert_eq!(b.household().unwrap().name, "Home");
    let _ = std::fs::remove_dir_all(dir);
}
