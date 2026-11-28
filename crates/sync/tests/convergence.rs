//! Three devices and a server, random interleavings of edits, syncs,
//! offline periods and server restarts: every device ends up with the
//! same state (D-12: merging never needs a person).

mod common;

use common::*;
use housewright_core::model::Role;
use housewright_sync::Invitee;
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Step {
    Edit { dev: usize, room: u128, name: u8 },
    Delete { dev: usize, room: u128 },
    Sync { dev: usize },
    Offline { dev: usize },
    Online { dev: usize },
    Restart,
    Tick(u64),
    Drop { dev: usize, after: usize },
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0..3usize, 0..4u128, any::<u8>()).prop_map(|(dev, room, name)| Step::Edit { dev, room, name }),
        1 => (0..3usize, 0..4u128).prop_map(|(dev, room)| Step::Delete { dev, room }),
        4 => (0..3usize).prop_map(|dev| Step::Sync { dev }),
        1 => (0..3usize).prop_map(|dev| Step::Offline { dev }),
        1 => (0..3usize).prop_map(|dev| Step::Online { dev }),
        1 => Just(Step::Restart),
        2 => (0..5000u64).prop_map(Step::Tick),
        1 => (0..3usize, 0..4usize).prop_map(|(dev, after)| Step::Drop { dev, after }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

    #[test]
    fn three_devices_converge(steps in prop::collection::vec(step(), 1..40)) {
        let lab = Lab::new("prop");
        let mut d = [lab.device("A"), lab.device("B"), lab.device("C")];
        d[0].init("A");
        d[0].setup();
        d[0].sync();
        {
            let (first, rest) = d.split_at_mut(1);
            pair(&mut first[0], &mut rest[0], Invitee::NewPerson { name: "B".into(), role: Role::Member });
            pair(&mut first[0], &mut rest[1], Invitee::MyDevice);
        }
        for s in steps {
            match s {
                Step::Edit { dev, room, name } => d[dev].try_room(room, &format!("room {name}")),
                Step::Delete { dev, room } => {
                    let id = housewright_core::ids::RoomId(uuid::Uuid::from_u128(room)).to_string();
                    let _ = d[dev].store.delete(housewright_core::model::EntityKind::Room, &id);
                }
                Step::Sync { dev } => { let _ = d[dev].try_sync(); }
                Step::Offline { dev } => d[dev].set_online(false),
                Step::Online { dev } => d[dev].set_online(true),
                Step::Restart => lab.restart(),
                Step::Tick(ms) => lab.clock.advance(ms),
                Step::Drop { dev, after } => {
                    d[dev].fail_after.store(after, std::sync::atomic::Ordering::SeqCst);
                    let _ = d[dev].try_sync();
                    d[dev].fail_after.store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }
        for x in d.iter() {
            x.set_online(true);
        }
        for _ in 0..2 {
            for x in d.iter_mut() {
                x.sync();
            }
        }
        prop_assert_eq!(d[0].entities(), d[1].entities());
        prop_assert_eq!(d[1].entities(), d[2].entities());
        for x in d.iter() {
            prop_assert_eq!(housewright_sync::SyncClient::pending_count(&x.store).unwrap(), 0);
        }
    }
}
