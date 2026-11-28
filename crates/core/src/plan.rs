//! Floor plans (D-25): the store-level glue between the plan entities and
//! the pure geometry in `housewright-plan`.
//!
//! Granularity (how plans merge, D-12, D-130): each wall, opening, pin and
//! label is its own record, and a room's outline is one field of its room.
//! Two people editing different walls, doors or pins at the same time both
//! keep their edits. Two people editing the *same* wall merge field by
//! field (last writer wins per field: `a`, `b`, `thickness`), so one can
//! move its start while the other moves its end. A room's outline is one
//! field on purpose: mixing corners from two different reshapes could make
//! an outline that crosses itself, so the later reshape wins whole.
//!
//! The projection ([`Store::floor_plan`]) is forgiving about what merges
//! can leave behind: an opening whose wall was deleted on another device is
//! not shown; an opening that no longer fits its (shortened) wall is
//! clamped onto it; a pin whose wall is gone stays where it is.

use std::collections::BTreeMap;

use housewright_plan::{self as geo, Point};

use crate::error::{Error, Result};
use crate::ids::*;
use crate::model::*;
use crate::store::Store;

/// Everything on one floor's plan.
#[derive(Clone, Debug, PartialEq)]
pub struct FloorPlan {
    pub floor: Floor,
    /// Rooms on this floor (with or without an outline).
    pub rooms: Vec<Room>,
    pub walls: Vec<Wall>,
    /// Openings whose wall exists, clamped to fit it.
    pub openings: Vec<Opening>,
    pub pins: Vec<PlanPin>,
    pub labels: Vec<PlanLabel>,
}

impl FloorPlan {
    /// The room whose outline contains `p` (the smallest, when outlines
    /// overlap or nest).
    pub fn room_at(&self, p: Point) -> Option<RoomId> {
        self.rooms
            .iter()
            .filter_map(|r| r.outline.as_ref().map(|o| (r, o)))
            .filter(|(_, o)| geo::contains(o, p))
            .min_by(|(ra, a), (rb, b)| {
                geo::area_mm2(a)
                    .total_cmp(&geo::area_mm2(b))
                    .then(ra.id.cmp(&rb.id))
            })
            .map(|(r, _)| r.id)
    }

    /// The total of the room areas on this floor, in square millimetres.
    /// Rooms drawn overlapping are counted twice; the editor warns about
    /// overlaps rather than guessing.
    pub fn total_area_mm2(&self) -> f64 {
        self.rooms
            .iter()
            .filter_map(|r| r.outline.as_deref())
            .map(geo::area_mm2)
            .sum()
    }

    /// Enclosed spaces formed by the walls that no room outline covers
    /// yet (its label point is not inside any room): offered as "name this
    /// room".
    pub fn unnamed_spaces(&self) -> Vec<Vec<Point>> {
        let segs: Vec<(Point, Point)> = self.walls.iter().map(|w| (w.a, w.b)).collect();
        geo::faces(&segs)
            .into_iter()
            .filter(|f| self.room_at(geo::label_point(f)).is_none())
            .collect()
    }

    /// Every wall corner on the floor (for snapping).
    pub fn corners(&self) -> Vec<Point> {
        let mut v: Vec<Point> = self
            .walls
            .iter()
            .flat_map(|w| [w.a, w.b])
            .chain(
                self.rooms
                    .iter()
                    .flat_map(|r| r.outline.iter().flatten().copied()),
            )
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Check an outline before it is stored.
pub fn check_outline(outline: &[Point]) -> Result<()> {
    geo::validate_polygon(outline).map_err(|e| Error::Invalid(e.to_string()))
}

fn check_point(p: Point) -> Result<()> {
    if geo::in_range(p) {
        Ok(())
    } else {
        Err(Error::Invalid(
            "that point is too far away (more than a kilometre)".into(),
        ))
    }
}

/// Validate a wall before it is stored.
pub fn check_wall(w: &Wall) -> Result<()> {
    check_point(w.a)?;
    check_point(w.b)?;
    if w.a == w.b {
        return Err(Error::Invalid("a wall needs two different ends".into()));
    }
    if let Some(t) = w.thickness
        && !(10..=2000).contains(&t)
    {
        return Err(Error::Invalid(
            "a wall's thickness must be between 1 cm and 2 m".into(),
        ));
    }
    Ok(())
}

/// Clamp an opening onto a wall of `len` millimetres (after a merge made
/// the wall shorter). Returns `None` when nothing of it would be left.
pub fn fit_opening(o: &Opening, len: i64) -> Option<Opening> {
    if len <= 0 {
        return None;
    }
    let width = o.width.clamp(1, len);
    let offset = o.offset.clamp(0, len - width);
    Some(Opening {
        width,
        offset,
        ..o.clone()
    })
}

impl Store {
    /// The plan of one floor.
    pub fn floor_plan(&self, floor: FloorId) -> Result<FloorPlan> {
        let f: Floor = self.fetch(&floor.to_string())?;
        let id = floor.to_string();
        let rooms = self.list_by::<Room>("floor", &id)?;
        let walls = self.list_by::<Wall>("floor", &id)?;
        let lens: BTreeMap<WallId, i64> = walls.iter().map(|w| (w.id, w.length())).collect();
        let openings = self
            .list_by::<Opening>("floor", &id)?
            .into_iter()
            .filter_map(|o| lens.get(&o.wall).and_then(|l| fit_opening(&o, *l)))
            .collect();
        Ok(FloorPlan {
            floor: f,
            rooms,
            walls,
            openings,
            pins: self.list_by::<PlanPin>("floor", &id)?,
            labels: self.list_by::<PlanLabel>("floor", &id)?,
        })
    }

    /// The floors of a property, lowest first.
    pub fn floors(&self, property: PropertyId) -> Result<Vec<Floor>> {
        let mut v = self.list_by::<Floor>("property", &property.to_string())?;
        v.sort_by(|a, b| a.level.cmp(&b.level).then(a.id.cmp(&b.id)));
        Ok(v)
    }

    /// Delete a wall and the doors and windows on it. Pins that sat on it
    /// stay where they are.
    pub fn delete_wall(&mut self, id: WallId) -> Result<()> {
        let w: Wall = self.fetch(&id.to_string())?;
        self.batch(|s| {
            for o in s.list_by::<Opening>("wall", &id.to_string())? {
                s.delete(EntityKind::Opening, &o.id.to_string())?;
            }
            for mut p in s.list_by::<PlanPin>("floor", &w.floor.to_string())? {
                if p.wall == Some(id) {
                    p.wall = None;
                    s.put(&p)?;
                }
            }
            s.delete(EntityKind::Wall, &id.to_string())
        })
    }

    /// Delete a floor and everything drawn on it. Its rooms are kept (with
    /// their things and tasks) but lose their outline and floor, so nothing
    /// but the drawing is lost. Facts pointing at its pins lose the link.
    pub fn delete_floor(&mut self, id: FloorId) -> Result<()> {
        let _: Floor = self.fetch(&id.to_string())?;
        let key = id.to_string();
        self.batch(|s| {
            for kind in EntityKind::PLAN {
                let ids: Vec<String> = s
                    .raw_all(kind)?
                    .into_iter()
                    .filter(|r| {
                        !r.deleted && r.data.get("floor").and_then(|v| v.as_str()) == Some(&key)
                    })
                    .map(|r| r.id)
                    .collect();
                for e in ids {
                    s.delete(kind, &e)?;
                }
            }
            for mut f in s.list::<Fact>()? {
                if let Some(p) = f.pin
                    && s.get::<PlanPin>(&p.to_string())?.is_none()
                {
                    f.pin = None;
                    s.put(&f)?;
                }
            }
            for mut r in s.list_by::<Room>("floor", &key)? {
                r.floor = None;
                r.outline = None;
                s.put(&r)?;
            }
            s.delete(EntityKind::Floor, &key)
        })
    }

    /// Delete a pin; facts linked to it lose the link.
    pub fn delete_pin(&mut self, id: PlanPinId) -> Result<()> {
        let _: PlanPin = self.fetch(&id.to_string())?;
        self.batch(|s| {
            for mut f in s.list::<Fact>()? {
                if f.pin == Some(id) {
                    f.pin = None;
                    s.put(&f)?;
                }
            }
            s.delete(EntityKind::PlanPin, &id.to_string())
        })
    }

    /// Store a pin; when it holds a thing and lands inside a room, the
    /// thing is put in that room (P1: no separate step). Returns the room
    /// the thing was moved to, if it moved.
    pub fn place_pin(&mut self, pin: &PlanPin) -> Result<Option<RoomId>> {
        check_point(pin.at)?;
        let plan = self.floor_plan(pin.floor)?;
        self.batch(|s| {
            s.put(pin)?;
            let Some(item) = pin.item else {
                return Ok(None);
            };
            let mut it: Item = s.fetch(&item.to_string())?;
            match plan.room_at(pin.at) {
                Some(room) if it.room != Some(room) => {
                    it.room = Some(room);
                    s.put(&it)?;
                    Ok(Some(room))
                }
                _ => Ok(None),
            }
        })
    }

    /// The pin of a thing, if it has one (the first, by id).
    pub fn pin_of(&self, item: ItemId) -> Result<Option<PlanPin>> {
        Ok(self
            .list_by::<PlanPin>("item", &item.to_string())?
            .into_iter()
            .next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ManualClock;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    fn store() -> (Store, PropertyId) {
        let mut s = Store::open_in_memory(Box::new(ManualClock::new(1_700_000_000_000))).unwrap();
        s.init_household("H", "Ana", "America/New_York").unwrap();
        let prop = Property {
            id: PropertyId::new(),
            name: "House".into(),
            address: None,
            time_zone: "America/New_York".into(),
            climate: Default::default(),
        };
        s.put(&prop).unwrap();
        (s, prop.id)
    }

    fn floor(s: &mut Store, prop: PropertyId) -> FloorId {
        let f = Floor {
            id: FloorId::new(),
            property: prop,
            name: "Ground floor".into(),
            level: 0,
            background: None,
        };
        s.put(&f).unwrap();
        f.id
    }

    #[test]
    fn pins_put_things_in_rooms_and_walls_take_their_doors() {
        let (mut s, prop) = store();
        let f = floor(&mut s, prop);
        let kitchen = Room {
            id: RoomId::new(),
            property: prop,
            floor: Some(f),
            name: "Kitchen".into(),
            outline: Some(geo::rectangle(p(0, 0), p(4000, 3000))),
        };
        s.put(&kitchen).unwrap();
        let item = Item {
            id: ItemId::new(),
            property: prop,
            room: None,
            parent: None,
            name: "Fridge".into(),
            category: None,
            make: None,
            model: None,
            serial: None,
            purchased: None,
            installed: None,
            warranty: None,
            location_note: None,
            template: None,
            extra_fields: vec![],
            custom: Default::default(),
            sensitivity: Sensitivity::Normal,
        };
        s.put(&item).unwrap();
        let pin = PlanPin {
            id: PlanPinId::new(),
            floor: f,
            at: p(1000, 1000),
            item: Some(item.id),
            label: None,
            wall: None,
            sensitivity: Sensitivity::Normal,
        };
        assert_eq!(s.place_pin(&pin).unwrap(), Some(kitchen.id));
        assert_eq!(
            s.fetch::<Item>(&item.id.to_string()).unwrap().room,
            Some(kitchen.id)
        );
        assert_eq!(s.pin_of(item.id).unwrap().unwrap().id, pin.id);
        // Outside every room: the thing stays where it was.
        let moved = PlanPin {
            at: p(9000, 9000),
            ..pin.clone()
        };
        assert_eq!(s.place_pin(&moved).unwrap(), None);

        let wall = Wall {
            id: WallId::new(),
            floor: f,
            a: p(0, 0),
            b: p(4000, 0),
            thickness: None,
        };
        check_wall(&wall).unwrap();
        s.put(&wall).unwrap();
        let door = Opening {
            id: OpeningId::new(),
            floor: f,
            wall: wall.id,
            kind: OpeningKind::Door,
            offset: 500,
            width: 900,
            swing: None,
        };
        s.put(&door).unwrap();
        let plan = s.floor_plan(f).unwrap();
        assert_eq!(plan.openings.len(), 1);
        assert_eq!(plan.total_area_mm2(), 12_000_000.0);
        assert_eq!(plan.room_at(p(10, 10)), Some(kitchen.id));
        s.delete_wall(wall.id).unwrap();
        assert!(s.get::<Opening>(&door.id.to_string()).unwrap().is_none());
    }

    #[test]
    fn deleting_a_floor_keeps_its_rooms() {
        let (mut s, prop) = store();
        let f = floor(&mut s, prop);
        let r = Room {
            id: RoomId::new(),
            property: prop,
            floor: Some(f),
            name: "Den".into(),
            outline: Some(geo::rectangle(p(0, 0), p(1000, 1000))),
        };
        s.put(&r).unwrap();
        let w = Wall {
            id: WallId::new(),
            floor: f,
            a: p(0, 0),
            b: p(1, 0),
            thickness: Some(100),
        };
        s.put(&w).unwrap();
        s.delete_floor(f).unwrap();
        let r2: Room = s.fetch(&r.id.to_string()).unwrap();
        assert_eq!((r2.floor, r2.outline), (None, None));
        assert!(s.get::<Wall>(&w.id.to_string()).unwrap().is_none());
        assert!(s.floors(prop).unwrap().is_empty());
    }

    #[test]
    fn openings_are_clamped_and_orphans_hidden() {
        let o = Opening {
            id: OpeningId::new(),
            floor: FloorId::new(),
            wall: WallId::new(),
            kind: OpeningKind::Window,
            offset: 2500,
            width: 1000,
            swing: None,
        };
        let f = fit_opening(&o, 3000).unwrap();
        assert_eq!((f.offset, f.width), (2000, 1000));
        let f = fit_opening(&o, 800).unwrap();
        assert_eq!((f.offset, f.width), (0, 800));
        assert!(fit_opening(&o, 0).is_none());
    }

    #[test]
    fn checks() {
        assert!(check_outline(&[p(0, 0), p(10, 10), p(10, 0), p(0, 10)]).is_err());
        assert!(check_outline(&geo::rectangle(p(0, 0), p(10, 10))).is_ok());
        let w = Wall {
            id: WallId::new(),
            floor: FloorId::new(),
            a: p(0, 0),
            b: p(0, 0),
            thickness: None,
        };
        assert!(check_wall(&w).is_err());
        let w = Wall {
            b: p(5, 0),
            thickness: Some(5),
            ..w
        };
        assert!(check_wall(&w).is_err());
    }
}
