//! Floor plans through real sync (Phase 8, D-130): two people edit one
//! plan offline at the same time, both sync through the server, and both
//! devices end with the same plan holding both people's edits.

mod common;

use common::*;
use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_plan::{Point, rectangle};
use housewright_sync::Invitee;
use uuid::Uuid;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn two_people_editing_one_plan_offline_converge_with_both_edits() {
    let lab = Lab::new("plan");
    let mut a = lab.device("A");
    let mut b = lab.device("B");
    a.init("Ana");
    a.setup();
    a.sync();
    pair(
        &mut a,
        &mut b,
        Invitee::NewPerson {
            name: "Ben".into(),
            role: Role::Member,
        },
    );

    // Ana draws the ground floor: a 4 x 3 m kitchen with four walls.
    let floor = FloorId(id(1));
    let property = PropertyId(id(7));
    a.store
        .put(&Floor {
            id: floor,
            property,
            name: "Ground floor".into(),
            level: 0,
            background: None,
        })
        .unwrap();
    let r = rectangle(Point::new(0, 0), Point::new(4000, 3000));
    let walls: Vec<WallId> = (0..4).map(|i| WallId(id(10 + i))).collect();
    for (i, w) in walls.iter().enumerate() {
        a.store
            .put(&Wall {
                id: *w,
                floor,
                a: r[i],
                b: r[(i + 1) % 4],
                thickness: None,
            })
            .unwrap();
    }
    a.store
        .put(&Room {
            id: RoomId(id(20)),
            property,
            floor: Some(floor),
            name: "Kitchen".into(),
            outline: Some(r.clone()),
        })
        .unwrap();
    a.sync();
    b.sync();
    assert_eq!(a.entities(), b.entities());

    // Both offline. Ana lengthens the kitchen (moves wall 0's end and wall
    // 1) and adds a window; Ben moves wall 0's start, adds a door to wall
    // 2, and drops a pin.
    a.set_online(false);
    b.set_online(false);
    let mut w0: Wall = a.store.fetch(&walls[0].to_string()).unwrap();
    w0.b = Point::new(5000, 0);
    a.store.put(&w0).unwrap();
    let mut w1: Wall = a.store.fetch(&walls[1].to_string()).unwrap();
    w1.a = Point::new(5000, 0);
    w1.b = Point::new(5000, 3000);
    a.store.put(&w1).unwrap();
    a.store
        .put(&Opening {
            id: OpeningId(id(30)),
            floor,
            wall: walls[1],
            kind: OpeningKind::Window,
            offset: 500,
            width: 1200,
            swing: None,
        })
        .unwrap();

    let mut b0: Wall = b.store.fetch(&walls[0].to_string()).unwrap();
    b0.a = Point::new(-200, 0);
    b.store.put(&b0).unwrap();
    b.store
        .put(&Opening {
            id: OpeningId(id(31)),
            floor,
            wall: walls[2],
            kind: OpeningKind::Door,
            offset: 1000,
            width: 900,
            swing: Some(Swing {
                hinge: WallEnd::A,
                side: Side::Right,
            }),
        })
        .unwrap();
    b.store
        .put(&PlanPin {
            id: PlanPinId(id(40)),
            floor,
            at: Point::new(300, 2700),
            item: None,
            label: Some("Water main".into()),
            wall: None,
            sensitivity: Sensitivity::Normal,
        })
        .unwrap();

    a.set_online(true);
    b.set_online(true);
    for _ in 0..2 {
        a.sync();
        b.sync();
    }
    assert_eq!(a.entities(), b.entities());
    for d in [&a, &b] {
        let plan = d.store.floor_plan(floor).unwrap();
        let w0 = plan.walls.iter().find(|w| w.id == walls[0]).unwrap();
        assert_eq!(
            (w0.a, w0.b),
            (Point::new(-200, 0), Point::new(5000, 0)),
            "both ends kept"
        );
        assert_eq!(plan.openings.len(), 2, "the window and the door");
        assert_eq!(plan.pins.len(), 1);
        assert_eq!(plan.room_at(Point::new(300, 2700)), Some(RoomId(id(20))));
    }
}
