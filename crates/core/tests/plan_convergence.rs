//! Floor plans merge without anyone resolving a conflict (D-12, D-130):
//! replicas editing walls, doors, pins and room outlines concurrently, then
//! exchanging operations in any order, end with identical plans; edits to
//! different walls all survive; and whatever the merge, every stored room
//! outline is one that somebody drew (never a mix of two).

use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::store::{ManualClock, Store};
use housewright_plan::{self as geo, Point};
use proptest::prelude::*;
use uuid::Uuid;

const REPLICAS: usize = 3;
const WALLS: usize = 3;

#[derive(Clone, Debug)]
enum Action {
    MoveEnd {
        r: usize,
        wall: usize,
        end_b: bool,
        x: u8,
        y: u8,
    },
    Thickness {
        r: usize,
        wall: usize,
        t: u8,
    },
    DeleteWall {
        r: usize,
        wall: usize,
    },
    AddOpening {
        r: usize,
        wall: usize,
        offset: u8,
        width: u8,
    },
    MovePin {
        r: usize,
        x: u8,
        y: u8,
    },
    Reshape {
        r: usize,
        w: u8,
        h: u8,
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
        4 => (r.clone(), 0..WALLS, any::<bool>(), 0u8..20, 0u8..20)
            .prop_map(|(r, wall, end_b, x, y)| Action::MoveEnd { r, wall, end_b, x, y }),
        1 => (r.clone(), 0..WALLS, 1u8..20).prop_map(|(r, wall, t)| Action::Thickness { r, wall, t }),
        1 => (r.clone(), 0..WALLS).prop_map(|(r, wall)| Action::DeleteWall { r, wall }),
        2 => (r.clone(), 0..WALLS, 0u8..30, 1u8..12)
            .prop_map(|(r, wall, offset, width)| Action::AddOpening { r, wall, offset, width }),
        2 => (r.clone(), 0u8..50, 0u8..50).prop_map(|(r, x, y)| Action::MovePin { r, x, y }),
        2 => (r.clone(), 1u8..10, 1u8..10).prop_map(|(r, w, h)| Action::Reshape { r, w, h }),
        2 => (r.clone(), 0u8..3).prop_map(|(r, ms)| Action::Tick { r, ms }),
        3 => (r.clone(), 0..REPLICAS).prop_map(|(from, to)| Action::Sync { from, to }),
    ]
}

struct World {
    stores: Vec<Store>,
    clocks: Vec<ManualClock>,
    floor: FloorId,
    room: RoomId,
    pin: PlanPinId,
    walls: Vec<WallId>,
    /// Every outline any replica wrote.
    drawn: Vec<Vec<Point>>,
}

fn world() -> World {
    let authors = [
        MemberId(Uuid::from_u128(0xa)),
        MemberId(Uuid::from_u128(0xb)),
        MemberId(Uuid::from_u128(0xb)),
    ];
    let property = PropertyId(Uuid::from_u128(1));
    let floor = FloorId(Uuid::from_u128(2));
    let room = RoomId(Uuid::from_u128(3));
    let pin = PlanPinId(Uuid::from_u128(4));
    let walls: Vec<WallId> = (0..WALLS)
        .map(|i| WallId(Uuid::from_u128(10 + i as u128)))
        .collect();
    let mut stores = Vec::new();
    let mut clocks = Vec::new();
    for a in authors {
        let c = ManualClock::new(1_000);
        let mut s = Store::open_in_memory(Box::new(c.clone())).unwrap();
        s.set_author(a).unwrap();
        stores.push(s);
        clocks.push(c);
    }
    // Replica 0 draws the starting plan; everyone has it before editing.
    let start = geo::rectangle(Point::new(0, 0), Point::new(4000, 3000));
    {
        let s = &mut stores[0];
        s.put(&Floor {
            id: floor,
            property,
            name: "Ground".into(),
            level: 0,
            background: None,
        })
        .unwrap();
        s.put(&Room {
            id: room,
            property,
            floor: Some(floor),
            name: "Kitchen".into(),
            outline: Some(start.clone()),
        })
        .unwrap();
        for (i, w) in walls.iter().enumerate() {
            s.put(&Wall {
                id: *w,
                floor,
                a: start[i],
                b: start[i + 1],
                thickness: None,
            })
            .unwrap();
        }
        s.put(&PlanPin {
            id: pin,
            floor,
            at: Point::new(500, 500),
            item: None,
            label: Some("Water main".into()),
            wall: None,
            sensitivity: Sensitivity::Normal,
        })
        .unwrap();
    }
    let ops = stores[0].ops().unwrap();
    for s in stores.iter_mut().skip(1) {
        s.apply_remote_ops(&ops).unwrap();
    }
    World {
        stores,
        clocks,
        floor,
        room,
        pin,
        walls,
        drawn: vec![start],
    }
}

fn run(w: &mut World, a: &Action) {
    match *a {
        Action::MoveEnd {
            r,
            wall,
            end_b,
            x,
            y,
        } => {
            if let Some(mut wl) = w.stores[r].get::<Wall>(&w.walls[wall].to_string()).unwrap() {
                let p = Point::new(i64::from(x) * 250, i64::from(y) * 250);
                if end_b {
                    wl.b = p
                } else {
                    wl.a = p
                }
                if wl.a != wl.b {
                    w.stores[r].put(&wl).unwrap();
                }
            }
        }
        Action::Thickness { r, wall, t } => {
            if let Some(mut wl) = w.stores[r].get::<Wall>(&w.walls[wall].to_string()).unwrap() {
                wl.thickness = Some(i64::from(t) * 20);
                w.stores[r].put(&wl).unwrap();
            }
        }
        Action::DeleteWall { r, wall } => {
            let _ = w.stores[r].delete_wall(w.walls[wall]);
        }
        Action::AddOpening {
            r,
            wall,
            offset,
            width,
        } => {
            let o = Opening {
                id: OpeningId::new(),
                floor: w.floor,
                wall: w.walls[wall],
                kind: OpeningKind::Door,
                offset: i64::from(offset) * 100,
                width: i64::from(width) * 100,
                swing: None,
            };
            w.stores[r].put(&o).unwrap();
        }
        Action::MovePin { r, x, y } => {
            let mut p: PlanPin = w.stores[r].fetch(&w.pin.to_string()).unwrap();
            p.at = Point::new(i64::from(x) * 100, i64::from(y) * 100);
            w.stores[r].place_pin(&p).unwrap();
        }
        Action::Reshape { r, w: ww, h } => {
            let mut rm: Room = w.stores[r].fetch(&w.room.to_string()).unwrap();
            let o = geo::rectangle(
                Point::new(0, 0),
                Point::new(i64::from(ww) * 500, i64::from(h) * 500),
            );
            w.drawn.push(o.clone());
            rm.outline = Some(o);
            w.stores[r].put(&rm).unwrap();
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn plans_converge(actions in proptest::collection::vec(action(), 1..50)) {
        let mut w = world();
        for a in &actions {
            run(&mut w, a);
        }
        let mut all = Vec::new();
        for s in &w.stores {
            for op in s.ops().unwrap() {
                if !all.iter().any(|o: &housewright_core::oplog::Op| o.id == op.id) {
                    all.push(op);
                }
            }
        }
        for s in &mut w.stores {
            s.apply_remote_ops(&all).unwrap();
        }
        let first = w.stores[0].snapshot().unwrap();
        let plan = w.stores[0].floor_plan(w.floor).unwrap();
        for s in &w.stores[1..] {
            prop_assert_eq!(&s.snapshot().unwrap(), &first);
            prop_assert_eq!(&s.floor_plan(w.floor).unwrap(), &plan);
        }
        // The room's outline is one somebody drew, whole.
        let rm: Room = w.stores[0].fetch(&w.room.to_string()).unwrap();
        let outline = rm.outline.unwrap();
        prop_assert!(w.drawn.contains(&outline));
        prop_assert!(geo::validate_polygon(&outline).is_ok());
        // Every opening shown fits its wall.
        for o in &plan.openings {
            let wall = plan.walls.iter().find(|x| x.id == o.wall);
            prop_assert!(wall.is_some(), "an opening is shown only with its wall");
            prop_assert!(geo::opening_fits(wall.unwrap().length(), o.offset, o.width));
        }
    }
}

#[test]
fn edits_to_different_walls_and_ends_both_survive() {
    let mut w = world();
    // Two people offline at once: Ana moves wall 0's start and wall 1;
    // Ben moves wall 0's end and adds a door to wall 2.
    let mut w0: Wall = w.stores[0].fetch(&w.walls[0].to_string()).unwrap();
    w0.a = Point::new(-500, 0);
    w.stores[0].put(&w0).unwrap();
    let mut w1: Wall = w.stores[0].fetch(&w.walls[1].to_string()).unwrap();
    w1.thickness = Some(200);
    w.stores[0].put(&w1).unwrap();

    w.clocks[1].advance(5);
    let mut b0: Wall = w.stores[1].fetch(&w.walls[0].to_string()).unwrap();
    b0.b = Point::new(4500, 0);
    w.stores[1].put(&b0).unwrap();
    let door = Opening {
        id: OpeningId::new(),
        floor: w.floor,
        wall: w.walls[2],
        kind: OpeningKind::Door,
        offset: 1000,
        width: 900,
        swing: Some(Swing {
            hinge: WallEnd::A,
            side: Side::Left,
        }),
    };
    w.stores[1].put(&door).unwrap();

    let a_ops = w.stores[0].ops().unwrap();
    let b_ops = w.stores[1].ops().unwrap();
    w.stores[0].apply_remote_ops(&b_ops).unwrap();
    w.stores[1].apply_remote_ops(&a_ops).unwrap();
    for s in &w.stores[..2] {
        let wall0: Wall = s.fetch(&w.walls[0].to_string()).unwrap();
        assert_eq!(
            (wall0.a, wall0.b),
            (Point::new(-500, 0), Point::new(4500, 0))
        );
        let wall1: Wall = s.fetch(&w.walls[1].to_string()).unwrap();
        assert_eq!(wall1.thickness, Some(200));
        assert_eq!(s.floor_plan(w.floor).unwrap().openings.len(), 1);
    }
    assert_eq!(
        w.stores[0].snapshot().unwrap(),
        w.stores[1].snapshot().unwrap()
    );
}

#[test]
fn a_door_added_to_a_wall_deleted_elsewhere_disappears_with_it() {
    let mut w = world();
    w.stores[0].delete_wall(w.walls[1]).unwrap();
    let door = Opening {
        id: OpeningId::new(),
        floor: w.floor,
        wall: w.walls[1],
        kind: OpeningKind::Window,
        offset: 0,
        width: 600,
        swing: None,
    };
    w.stores[1].put(&door).unwrap();
    let a = w.stores[0].ops().unwrap();
    let b = w.stores[1].ops().unwrap();
    w.stores[0].apply_remote_ops(&b).unwrap();
    w.stores[1].apply_remote_ops(&a).unwrap();
    for s in &w.stores[..2] {
        let plan = s.floor_plan(w.floor).unwrap();
        assert_eq!(plan.walls.len(), WALLS - 1);
        assert!(plan.openings.is_empty(), "the orphaned window is not shown");
    }
}
