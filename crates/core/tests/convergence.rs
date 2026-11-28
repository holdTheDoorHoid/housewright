//! Property tests: replicas that exchange operations in any order, with
//! duplicates and partial syncs along the way, converge to identical state.

use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::oplog::Op;
use housewright_core::store::{ManualClock, Store};
use jiff::Timestamp;
use proptest::prelude::*;
use uuid::Uuid;

const REPLICAS: usize = 3;
const ROOMS: usize = 3;
const TASKS: usize = 2;

#[derive(Clone, Debug)]
enum Action {
    Rename {
        r: usize,
        room: usize,
        name: u8,
    },
    SetFloor {
        r: usize,
        room: usize,
        floor: Option<u8>,
    },
    Delete {
        r: usize,
        room: usize,
    },
    Complete {
        r: usize,
        task: usize,
        minute: u8,
    },
    Fact {
        r: usize,
        value: u8,
        sensitive: bool,
    },
    Tick {
        r: usize,
        ms: u8,
    },
    Sync {
        from: usize,
        to: usize,
    },
}

fn action() -> impl Strategy<Value = Action> {
    let r = 0..REPLICAS;
    prop_oneof![
        3 => (r.clone(), 0..ROOMS, 0u8..4).prop_map(|(r, room, name)| Action::Rename { r, room, name }),
        2 => (r.clone(), 0..ROOMS, proptest::option::of(0u8..3))
            .prop_map(|(r, room, floor)| Action::SetFloor { r, room, floor }),
        1 => (r.clone(), 0..ROOMS).prop_map(|(r, room)| Action::Delete { r, room }),
        2 => (r.clone(), 0..TASKS, any::<u8>()).prop_map(|(r, task, minute)| Action::Complete { r, task, minute }),
        1 => (r.clone(), 0u8..3, any::<bool>()).prop_map(|(r, value, sensitive)| Action::Fact { r, value, sensitive }),
        2 => (r.clone(), 0u8..3).prop_map(|(r, ms)| Action::Tick { r, ms }),
        2 => (r.clone(), 0..REPLICAS).prop_map(|(from, to)| Action::Sync { from, to }),
    ]
}

struct World {
    stores: Vec<Store>,
    clocks: Vec<ManualClock>,
    rooms: Vec<RoomId>,
    tasks: Vec<TaskId>,
    fact: FactId,
    property: PropertyId,
}

fn world() -> World {
    // Replicas 1 and 2 are the same member on two devices: ties then fall
    // through to the op id.
    let a = MemberId(Uuid::from_u128(0xa));
    let b = MemberId(Uuid::from_u128(0xb));
    let authors = [a, b, b];
    let mut stores = Vec::new();
    let mut clocks = Vec::new();
    for author in authors {
        // All clocks start equal, so HLC collisions are common.
        let clock = ManualClock::new(1_000);
        let mut s = Store::open_in_memory(Box::new(clock.clone())).unwrap();
        s.set_author(author).unwrap();
        stores.push(s);
        clocks.push(clock);
    }
    World {
        stores,
        clocks,
        rooms: (0..ROOMS)
            .map(|i| RoomId(Uuid::from_u128(100 + i as u128)))
            .collect(),
        tasks: (0..TASKS)
            .map(|i| TaskId(Uuid::from_u128(200 + i as u128)))
            .collect(),
        fact: FactId(Uuid::from_u128(300)),
        property: PropertyId(Uuid::from_u128(400)),
    }
}

fn room(w: &World, r: usize, i: usize) -> Room {
    w.stores[r]
        .get::<Room>(&w.rooms[i].to_string())
        .unwrap()
        .unwrap_or(Room {
            id: w.rooms[i],
            property: w.property,
            floor: None,
            name: "Room".into(),
        })
}

fn run(w: &mut World, a: &Action) {
    match *a {
        Action::Rename { r, room: i, name } => {
            let mut rm = room(w, r, i);
            rm.name = format!("Room {name}");
            // Fails harmlessly if this replica has seen a delete.
            let _ = w.stores[r].put(&rm);
        }
        Action::SetFloor { r, room: i, floor } => {
            let mut rm = room(w, r, i);
            rm.floor = floor.map(|f| FloorId(Uuid::from_u128(500 + f as u128)));
            let _ = w.stores[r].put(&rm);
        }
        Action::Delete { r, room: i } => {
            let _ = w.stores[r].delete(EntityKind::Room, &w.rooms[i].to_string());
        }
        Action::Complete { r, task, minute } => {
            let author = w.stores[r].author().unwrap();
            let c = Completion {
                id: CompletionId::new(),
                task: w.tasks[task],
                by: author,
                at: Timestamp::from_second(1_800_000_000 + i64::from(minute) * 60).unwrap(),
                notes: None,
                cost: None,
                values: Default::default(),
                contractor: None,
                attachments: vec![],
                voids: None,
                done_by: None,
            };
            w.stores[r].append(&c).unwrap();
        }
        Action::Fact {
            r,
            value,
            sensitive,
        } => {
            let f = Fact {
                id: w.fact,
                property: w.property,
                category: "codes".into(),
                label: "Alarm code".into(),
                value: format!("{value}"),
                sensitivity: if sensitive {
                    Sensitivity::Sensitive
                } else {
                    Sensitivity::Normal
                },
                key: None,
                room: None,
            };
            let _ = w.stores[r].put(&f);
        }
        Action::Tick { r, ms } => w.clocks[r].advance(u64::from(ms)),
        Action::Sync { from, to } => {
            if from != to {
                let ops = w.stores[from].ops().unwrap();
                w.stores[to].apply_remote_ops(&ops).unwrap();
            }
        }
    }
}

/// Deterministic shuffle from a seed (xorshift), so failures replay.
fn shuffle<T>(v: &mut [T], mut seed: u64) {
    seed |= 1;
    for i in (1..v.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = (seed % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

fn fresh() -> Store {
    let mut s = Store::open_in_memory(Box::new(ManualClock::new(0))).unwrap();
    s.set_author(MemberId(Uuid::from_u128(0xf))).unwrap();
    s
}

/// Feed ops in chunks, re-sending some chunks to create duplicates.
fn feed(store: &mut Store, ops: &[Op], chunk: usize) {
    for (i, c) in ops.chunks(chunk.max(1)).enumerate() {
        store.apply_remote_ops(c).unwrap();
        if i % 2 == 0 {
            let r = store.apply_remote_ops(c).unwrap();
            assert_eq!(r.applied, 0);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn replicas_converge(actions in proptest::collection::vec(action(), 1..60),
                         seed_a in any::<u64>(), seed_b in any::<u64>(),
                         chunk_a in 1usize..7, chunk_b in 1usize..7) {
        let mut w = world();
        for a in &actions {
            run(&mut w, a);
        }
        let mut all: Vec<Op> = Vec::new();
        for s in &w.stores {
            for op in s.ops().unwrap() {
                if !all.iter().any(|o| o.id == op.id) {
                    all.push(op);
                }
            }
        }

        let mut order_a = all.clone();
        shuffle(&mut order_a, seed_a);
        let mut order_b = all.clone();
        shuffle(&mut order_b, seed_b);
        let mut x = fresh();
        let mut y = fresh();
        feed(&mut x, &order_a, chunk_a);
        feed(&mut y, &order_b, chunk_b);
        let sx = x.snapshot().unwrap();
        prop_assert_eq!(&sx, &y.snapshot().unwrap());

        // Every original replica, once it has everything, agrees too.
        for (i, s) in w.stores.iter_mut().enumerate() {
            let mut o = all.clone();
            shuffle(&mut o, seed_a ^ (i as u64 + 1));
            s.apply_remote_ops(&o).unwrap();
            prop_assert_eq!(&s.snapshot().unwrap(), &sx);
            prop_assert_eq!(s.ops().unwrap().len(), all.len());
        }
    }

    #[test]
    fn apply_is_idempotent(actions in proptest::collection::vec(action(), 1..30)) {
        let mut w = world();
        for a in &actions {
            run(&mut w, a);
        }
        let ops = w.stores[0].ops().unwrap();
        let mut x = fresh();
        x.apply_remote_ops(&ops).unwrap();
        let once = x.snapshot().unwrap();
        let r = x.apply_remote_ops(&ops).unwrap();
        prop_assert_eq!(r.applied, 0);
        prop_assert_eq!(x.snapshot().unwrap(), once);
    }
}
