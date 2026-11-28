//! Phase 8 commands: floor plans (D-25).
//!
//! Everything here is optional (P3): rooms, things and tasks work without a
//! plan, and a plan only ever adds an outline to a room or a pin to a thing.

use std::collections::BTreeMap;

use housewright_core::ids::*;
use housewright_core::model::*;
use housewright_core::plan::{FloorPlan, check_outline, check_wall};
use housewright_plan::{self as geo, Point, Units};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

// -- arguments ----------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FloorArgs {
    floor_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveFloorArgs {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    level: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomShapeArgs {
    floor_id: String,
    #[serde(default)]
    room_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    outline: Option<Vec<Point>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WallIn {
    #[serde(default)]
    id: Option<String>,
    a: Point,
    b: Point,
    #[serde(default)]
    thickness: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveWallsArgs {
    floor_id: String,
    walls: Vec<WallIn>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpeningArgs {
    #[serde(default)]
    id: Option<String>,
    wall_id: String,
    kind: OpeningKind,
    offset: i64,
    width: i64,
    #[serde(default)]
    swing: Option<Swing>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinArgs {
    #[serde(default)]
    id: Option<String>,
    floor_id: String,
    at: Point,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    wall_id: Option<String>,
    #[serde(default)]
    sensitive: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelArgs {
    #[serde(default)]
    id: Option<String>,
    floor_id: String,
    at: Point,
    text: String,
    #[serde(default)]
    sensitive: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BackgroundArgs {
    floor_id: String,
    /// Absent or null: remove the background.
    #[serde(default)]
    attachment_id: Option<String>,
    #[serde(default)]
    at: Option<Point>,
    #[serde(default)]
    width: Option<i64>,
    #[serde(default)]
    opacity: Option<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitsArgs {
    units: Units,
}

/// A room's part of a plan state: its name and outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomState {
    pub id: RoomId,
    pub name: String,
    #[serde(default)]
    pub outline: Option<Vec<Point>>,
}

/// Everything drawn on one floor, as stored. The interface keeps these
/// for undo and redo (D-131).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanState {
    #[serde(default)]
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub openings: Vec<Opening>,
    #[serde(default)]
    pub pins: Vec<PlanPin>,
    #[serde(default)]
    pub labels: Vec<PlanLabel>,
    #[serde(default)]
    pub rooms: Vec<RoomState>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreArgs {
    floor_id: String,
    /// The state to go back to.
    to: PlanState,
    /// The state now (what the undone step made).
    from: PlanState,
}

// -- results -------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct FloorOut {
    pub id: String,
    pub name: String,
    pub level: i32,
    /// Something is drawn on it.
    pub drawn: bool,
    pub rooms: usize,
    pub area_text: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanRoomOut {
    pub id: String,
    pub name: String,
    pub outline: Option<Vec<Point>>,
    pub area_mm2: Option<f64>,
    pub area_text: Option<String>,
    pub perimeter_text: Option<String>,
    pub label_at: Option<Point>,
    pub things: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanWallOut {
    pub id: String,
    pub a: Point,
    pub b: Point,
    pub thickness: i64,
    pub length_mm: i64,
    pub length_text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanOpeningOut {
    pub id: String,
    pub wall_id: String,
    pub kind: OpeningKind,
    pub offset: i64,
    pub width: i64,
    pub width_text: String,
    pub swing: Option<Swing>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanPinOut {
    pub id: String,
    pub number: u32,
    pub at: Point,
    pub item_id: Option<String>,
    pub item_name: Option<String>,
    /// Null when private and the viewer may not see private details.
    pub label: Option<String>,
    pub wall_id: Option<String>,
    pub sensitive: bool,
    pub room_name: Option<String>,
    /// The home fact linked to this pin ("Water main").
    pub fact_id: Option<String>,
    pub fact_label: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanLabelOut {
    pub id: String,
    pub at: Point,
    pub text: Option<String>,
    pub sensitive: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SpaceOut {
    pub outline: Vec<Point>,
    pub area_text: String,
    pub label_at: Point,
}

#[derive(Clone, Debug, Serialize)]
pub struct BackgroundOut {
    pub attachment_id: String,
    pub at: Point,
    pub width: i64,
    pub opacity: u8,
    /// Pixel size of the picture (to keep its shape).
    pub pixel_width: Option<u32>,
    pub pixel_height: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanOut {
    pub floor: FloorOut,
    pub floors: Vec<FloorOut>,
    pub units: Units,
    pub rooms: Vec<PlanRoomOut>,
    pub walls: Vec<PlanWallOut>,
    pub openings: Vec<PlanOpeningOut>,
    pub pins: Vec<PlanPinOut>,
    pub labels: Vec<PlanLabelOut>,
    /// Spaces enclosed by walls that are not a room yet.
    pub spaces: Vec<SpaceOut>,
    pub total_area_text: Option<String>,
    pub background: Option<BackgroundOut>,
    /// Corners to snap to.
    pub corners: Vec<Point>,
    pub can_edit: bool,
    /// The stored state, for undo.
    pub state: PlanState,
}

#[derive(Clone, Debug, Serialize)]
struct SavedOut {
    plan: PlanOut,
    /// Ids of what was created or saved, in order.
    ids: Vec<String>,
    /// The room a thing was put in by its pin.
    moved_to: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct RestoredOut {
    plan: PlanOut,
    /// Old id to new id, for things that had to be made again.
    renamed: BTreeMap<String, String>,
}

fn plan_error(e: housewright_core::Error) -> ApiError {
    match e {
        housewright_core::Error::Invalid(m) => ApiError::invalid(m),
        e => e.into(),
    }
}

fn pid<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, ApiError> {
    parse_id(s, what)
}

impl Api {
    pub(crate) fn plan_call(&mut self, cmd: &str, a: Value) -> Option<Result<Value, ApiError>> {
        Some(match cmd {
            "floors" => self.floors_out().and_then(ok),
            "save_floor" => args(a).and_then(|a: SaveFloorArgs| ok(self.save_floor(a)?)),
            "delete_floor" => args(a).and_then(|a: IdArgs| {
                self.need_edit()?;
                self.store
                    .delete_floor(pid(&a.id, "floor")?)
                    .map_err(plan_error)?;
                ok(self.floors_out()?)
            }),
            "floor_plan" => {
                args(a).and_then(|a: FloorArgs| ok(self.plan_out(pid(&a.floor_id, "floor")?)?))
            }
            "save_room_shape" => args(a).and_then(|a: RoomShapeArgs| ok(self.save_room_shape(a)?)),
            "remove_room_shape" => args(a).and_then(|a: IdArgs| {
                self.need_edit()?;
                let mut r: Room = self
                    .store
                    .fetch(&pid::<RoomId>(&a.id, "room")?.to_string())?;
                let floor = r.floor;
                r.outline = None;
                self.store.put(&r)?;
                match floor {
                    Some(f) => ok(self.plan_out(f)?),
                    None => ok(Value::Null),
                }
            }),
            "save_walls" => args(a).and_then(|a: SaveWallsArgs| ok(self.save_walls(a)?)),
            "delete_wall" => args(a).and_then(|a: IdArgs| {
                self.need_edit()?;
                let id: WallId = pid(&a.id, "wall")?;
                let w: Wall = self.store.fetch(&id.to_string())?;
                self.store.delete_wall(id).map_err(plan_error)?;
                ok(self.plan_out(w.floor)?)
            }),
            "save_opening" => args(a).and_then(|a: OpeningArgs| ok(self.save_opening(a)?)),
            "delete_opening" => args(a).and_then(|a: IdArgs| {
                self.need_edit()?;
                let id: OpeningId = pid(&a.id, "opening")?;
                let o: Opening = self.store.fetch(&id.to_string())?;
                self.store.delete(EntityKind::Opening, &id.to_string())?;
                ok(self.plan_out(o.floor)?)
            }),
            "save_pin" => args(a).and_then(|a: PinArgs| ok(self.save_pin(a)?)),
            "delete_pin" => args(a).and_then(|a: IdArgs| {
                self.need_edit()?;
                let id: PlanPinId = pid(&a.id, "pin")?;
                let p: PlanPin = self.store.fetch(&id.to_string())?;
                self.store.delete_pin(id).map_err(plan_error)?;
                ok(self.plan_out(p.floor)?)
            }),
            "save_label" => args(a).and_then(|a: LabelArgs| ok(self.save_label(a)?)),
            "delete_label" => args(a).and_then(|a: IdArgs| {
                self.need_edit()?;
                let id: PlanLabelId = pid(&a.id, "label")?;
                let l: PlanLabel = self.store.fetch(&id.to_string())?;
                self.store.delete(EntityKind::PlanLabel, &id.to_string())?;
                ok(self.plan_out(l.floor)?)
            }),
            "set_background" => args(a).and_then(|a: BackgroundArgs| ok(self.set_background(a)?)),
            "set_units" => args(a).and_then(|a: UnitsArgs| {
                self.need_edit()?;
                let mut h = self.household()?;
                h.units = Some(a.units);
                self.store.put(&h)?;
                ok(self.status()?)
            }),
            "plan_restore" => args(a).and_then(|a: RestoreArgs| ok(self.plan_restore(a)?)),
            _ => return None,
        })
    }

    fn need_edit(&self) -> Result<(), ApiError> {
        if self.me()?.role.can_edit_plan() {
            Ok(())
        } else {
            Err(ApiError::invalid(
                "only the household's owner and members can change the floor plan".to_string(),
            ))
        }
    }

    fn units(&self) -> Result<Units, ApiError> {
        Ok(self.household()?.units())
    }

    fn floor_summary(&self, f: &Floor, units: Units) -> Result<FloorOut, ApiError> {
        let plan = self.store.floor_plan(f.id)?;
        let drawn = !plan.walls.is_empty()
            || !plan.pins.is_empty()
            || plan.rooms.iter().any(|r| r.outline.is_some());
        let area = plan.total_area_mm2();
        Ok(FloorOut {
            id: f.id.to_string(),
            name: f.name.clone(),
            level: f.level,
            drawn,
            rooms: plan.rooms.iter().filter(|r| r.outline.is_some()).count(),
            area_text: (area > 0.0).then(|| geo::format_area(area, units)),
        })
    }

    fn floors_out(&self) -> Result<Vec<FloorOut>, ApiError> {
        let p = self.first_property()?;
        let units = self.units()?;
        self.store
            .floors(p.id)?
            .iter()
            .map(|f| self.floor_summary(f, units))
            .collect()
    }

    fn save_floor(&mut self, a: SaveFloorArgs) -> Result<Vec<FloorOut>, ApiError> {
        self.need_edit()?;
        let name = a.name.trim();
        if name.is_empty() {
            return Err(ApiError::invalid("give the floor a name".to_string()));
        }
        let p = self.first_property()?;
        let mut f = match &a.id {
            Some(id) => self
                .store
                .fetch::<Floor>(&pid::<FloorId>(id, "floor")?.to_string())?,
            None => {
                let next = self
                    .store
                    .floors(p.id)?
                    .iter()
                    .map(|f| f.level + 1)
                    .max()
                    .unwrap_or(0);
                Floor {
                    id: FloorId::new(),
                    property: p.id,
                    name: String::new(),
                    level: next,
                    background: None,
                }
            }
        };
        f.name = name.chars().take(60).collect();
        if let Some(l) = a.level {
            f.level = l.clamp(-5, 50);
        }
        self.store.put(&f)?;
        self.floors_out()
    }

    pub(crate) fn plan_out(&self, floor: FloorId) -> Result<PlanOut, ApiError> {
        let plan: FloorPlan = self.store.floor_plan(floor)?;
        let units = self.units()?;
        let me = self.me()?;
        let see = me.role.can_see_sensitive();
        let items: BTreeMap<ItemId, Item> = self
            .store
            .list::<Item>()?
            .into_iter()
            .map(|i| (i.id, i))
            .collect();
        let facts: Vec<Fact> = self.store.list()?;
        let room_name: BTreeMap<RoomId, String> =
            plan.rooms.iter().map(|r| (r.id, r.name.clone())).collect();

        let mut rooms: Vec<PlanRoomOut> = plan
            .rooms
            .iter()
            .map(|r| {
                let o = r.outline.as_deref();
                PlanRoomOut {
                    id: r.id.to_string(),
                    name: r.name.clone(),
                    outline: r.outline.clone(),
                    area_mm2: o.map(geo::area_mm2),
                    area_text: o.map(|o| geo::format_area(geo::area_mm2(o), units)),
                    perimeter_text: o
                        .map(|o| geo::format_length(geo::perimeter(o).round() as i64, units)),
                    label_at: o.map(geo::label_point),
                    things: items.values().filter(|i| i.room == Some(r.id)).count(),
                }
            })
            .collect();
        rooms.sort_by_key(|r| r.name.to_lowercase());

        let mut pins_sorted: Vec<&PlanPin> = plan.pins.iter().collect();
        pins_sorted.sort_by_key(|p| (p.at.y, p.at.x, p.id));
        let pins = pins_sorted
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let item = p.item.and_then(|i| items.get(&i));
                let sensitive = p.sensitivity == Sensitivity::Sensitive
                    || item.is_some_and(|i| i.sensitivity == Sensitivity::Sensitive);
                let fact = facts.iter().find(|f| f.pin == Some(p.id));
                PlanPinOut {
                    id: p.id.to_string(),
                    number: i as u32 + 1,
                    at: p.at,
                    item_id: p.item.map(|i| i.to_string()),
                    item_name: item.map(|i| i.name.clone()),
                    label: if sensitive && !see {
                        None
                    } else {
                        p.label.clone()
                    },
                    wall_id: p.wall.map(|w| w.to_string()),
                    sensitive,
                    room_name: plan.room_at(p.at).and_then(|r| room_name.get(&r).cloned()),
                    fact_id: fact.map(|f| f.id.to_string()),
                    fact_label: fact.map(|f| f.label.clone()),
                }
            })
            .collect();

        let walls = plan
            .walls
            .iter()
            .map(|w| PlanWallOut {
                id: w.id.to_string(),
                a: w.a,
                b: w.b,
                thickness: w.thickness(),
                length_mm: w.length(),
                length_text: geo::format_length(w.length(), units),
            })
            .collect();
        let openings = plan
            .openings
            .iter()
            .map(|o| PlanOpeningOut {
                id: o.id.to_string(),
                wall_id: o.wall.to_string(),
                kind: o.kind,
                offset: o.offset,
                width: o.width,
                width_text: geo::format_length(o.width, units),
                swing: o.swing,
            })
            .collect();
        let labels = plan
            .labels
            .iter()
            .map(|l| {
                let sensitive = l.sensitivity == Sensitivity::Sensitive;
                PlanLabelOut {
                    id: l.id.to_string(),
                    at: l.at,
                    text: if sensitive && !see {
                        None
                    } else {
                        Some(l.text.clone())
                    },
                    sensitive,
                }
            })
            .collect();
        let spaces = plan
            .unnamed_spaces()
            .into_iter()
            .map(|o| SpaceOut {
                area_text: geo::format_area(geo::area_mm2(&o), units),
                label_at: geo::label_point(&o),
                outline: o,
            })
            .collect();
        let background = match &plan.floor.background {
            Some(b) => {
                let att = self.store.get::<Attachment>(&b.attachment.to_string())?;
                att.map(|att| BackgroundOut {
                    attachment_id: b.attachment.to_string(),
                    at: b.at,
                    width: b.width,
                    opacity: b.opacity,
                    pixel_width: att.width,
                    pixel_height: att.height,
                })
            }
            None => None,
        };
        let total = plan.total_area_mm2();
        let mut state = PlanState {
            walls: plan.walls.clone(),
            openings: plan.openings.clone(),
            pins: plan.pins.clone(),
            labels: plan.labels.clone(),
            rooms: plan
                .rooms
                .iter()
                .map(|r| RoomState {
                    id: r.id,
                    name: r.name.clone(),
                    outline: r.outline.clone(),
                })
                .collect(),
        };
        if !see {
            // Undo must not carry private text to someone who may not see it.
            for p in &mut state.pins {
                if p.sensitivity == Sensitivity::Sensitive {
                    p.label = None;
                }
            }
            for l in &mut state.labels {
                if l.sensitivity == Sensitivity::Sensitive {
                    l.text = String::new();
                }
            }
        }
        let floors = self.floors_out()?;
        Ok(PlanOut {
            floor: self.floor_summary(&plan.floor, units)?,
            floors,
            units,
            corners: plan.corners(),
            rooms,
            walls,
            openings,
            pins,
            labels,
            spaces,
            total_area_text: (total > 0.0).then(|| geo::format_area(total, units)),
            background,
            can_edit: me.role.can_edit_plan(),
            state,
        })
    }

    fn save_room_shape(&mut self, a: RoomShapeArgs) -> Result<SavedOut, ApiError> {
        self.need_edit()?;
        let floor: FloorId = pid(&a.floor_id, "floor")?;
        let f: Floor = self.store.fetch(&floor.to_string())?;
        if let Some(o) = &a.outline {
            check_outline(o).map_err(plan_error)?;
        }
        let name = a.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        let room = match &a.room_id {
            Some(id) => {
                let mut r: Room = self.store.fetch(&pid::<RoomId>(id, "room")?.to_string())?;
                if let Some(n) = name {
                    r.name = n.chars().take(60).collect();
                }
                if a.outline.is_some() {
                    r.outline = a.outline.clone();
                    r.floor = Some(floor);
                }
                r
            }
            None => {
                let n =
                    name.ok_or_else(|| ApiError::invalid("give the room a name".to_string()))?;
                // A room of that name that is not on any plan yet is reused,
                // so things already in "Kitchen" stay in the kitchen.
                let existing = self
                    .store
                    .list_by::<Room>("property", &f.property.to_string())?
                    .into_iter()
                    .find(|r| r.name.eq_ignore_ascii_case(n) && r.outline.is_none());
                match existing {
                    Some(mut r) => {
                        r.floor = Some(floor);
                        r.outline = a.outline.clone();
                        r
                    }
                    None => Room {
                        id: RoomId::new(),
                        property: f.property,
                        floor: Some(floor),
                        name: n.chars().take(60).collect(),
                        outline: a.outline.clone(),
                    },
                }
            }
        };
        self.store.put(&room)?;
        Ok(SavedOut {
            plan: self.plan_out(floor)?,
            ids: vec![room.id.to_string()],
            moved_to: None,
        })
    }

    fn save_walls(&mut self, a: SaveWallsArgs) -> Result<SavedOut, ApiError> {
        self.need_edit()?;
        let floor: FloorId = pid(&a.floor_id, "floor")?;
        let _: Floor = self.store.fetch(&floor.to_string())?;
        if a.walls.len() > 500 {
            return Err(ApiError::invalid("too many walls at once".to_string()));
        }
        let mut walls = Vec::new();
        for w in a.walls {
            let id = match &w.id {
                Some(id) => {
                    let id: WallId = pid(id, "wall")?;
                    let old: Wall = self.store.fetch(&id.to_string())?;
                    if old.floor != floor {
                        return Err(ApiError::invalid(
                            "that wall is on another floor".to_string(),
                        ));
                    }
                    id
                }
                None => WallId::new(),
            };
            let wall = Wall {
                id,
                floor,
                a: w.a,
                b: w.b,
                thickness: w.thickness,
            };
            check_wall(&wall).map_err(plan_error)?;
            walls.push(wall);
        }
        let ids = walls.iter().map(|w| w.id.to_string()).collect();
        self.store.batch(|s| {
            for w in &walls {
                s.put(w)?;
            }
            Ok(())
        })?;
        Ok(SavedOut {
            plan: self.plan_out(floor)?,
            ids,
            moved_to: None,
        })
    }

    fn save_opening(&mut self, a: OpeningArgs) -> Result<SavedOut, ApiError> {
        self.need_edit()?;
        let wall_id: WallId = pid(&a.wall_id, "wall")?;
        let wall: Wall = self.store.fetch(&wall_id.to_string())?;
        let units = self.units()?;
        if !geo::opening_fits(wall.length(), a.offset, a.width) {
            return Err(ApiError::invalid(format!(
                "that does not fit: the wall is {} long",
                geo::format_length(wall.length(), units)
            )));
        }
        let id = match &a.id {
            Some(id) => pid::<OpeningId>(id, "opening")?,
            None => OpeningId::new(),
        };
        let others = self
            .store
            .list_by::<Opening>("wall", &wall_id.to_string())?;
        if let Some(o) = others
            .iter()
            .find(|o| o.id != id && geo::openings_overlap((o.offset, o.width), (a.offset, a.width)))
        {
            let what = match o.kind {
                OpeningKind::Door => "door",
                OpeningKind::Window => "window",
                OpeningKind::Gap => "opening",
            };
            return Err(ApiError::invalid(format!(
                "that overlaps the {what} already on this wall"
            )));
        }
        let o = Opening {
            id,
            floor: wall.floor,
            wall: wall_id,
            kind: a.kind,
            offset: a.offset,
            width: a.width,
            swing: if a.kind == OpeningKind::Door {
                a.swing
            } else {
                None
            },
        };
        self.store.put(&o)?;
        Ok(SavedOut {
            plan: self.plan_out(wall.floor)?,
            ids: vec![o.id.to_string()],
            moved_to: None,
        })
    }

    fn save_pin(&mut self, a: PinArgs) -> Result<SavedOut, ApiError> {
        self.need_edit()?;
        let floor: FloorId = pid(&a.floor_id, "floor")?;
        let _: Floor = self.store.fetch(&floor.to_string())?;
        let item = match &a.item_id {
            Some(i) => {
                let i: ItemId = pid(i, "thing")?;
                let _: Item = self.store.fetch(&i.to_string())?;
                Some(i)
            }
            None => None,
        };
        let existing = match (&a.id, item) {
            (Some(id), _) => Some(
                self.store
                    .fetch::<PlanPin>(&pid::<PlanPinId>(id, "pin")?.to_string())?,
            ),
            // One pin per thing: placing it again moves it.
            (None, Some(i)) => self.store.pin_of(i)?,
            (None, None) => None,
        };
        let label = a
            .label
            .map(|l| l.trim().chars().take(80).collect::<String>())
            .filter(|l| !l.is_empty());
        if item.is_none() && label.is_none() && existing.as_ref().is_none_or(|e| e.label.is_none())
        {
            return Err(ApiError::invalid(
                "a pin needs a thing or a name".to_string(),
            ));
        }
        let wall = match &a.wall_id {
            Some(w) if !w.is_empty() => {
                let w: WallId = pid(w, "wall")?;
                let _: Wall = self.store.fetch(&w.to_string())?;
                Some(w)
            }
            _ => None,
        };
        let pin = PlanPin {
            id: existing.as_ref().map(|e| e.id).unwrap_or_default(),
            floor,
            at: a.at,
            item: item.or(existing.as_ref().and_then(|e| e.item)),
            label: label.or(existing.as_ref().and_then(|e| e.label.clone())),
            wall,
            sensitivity: match a.sensitive {
                Some(true) => Sensitivity::Sensitive,
                Some(false) => Sensitivity::Normal,
                None => existing.as_ref().map(|e| e.sensitivity).unwrap_or_default(),
            },
        };
        let moved = self.store.place_pin(&pin).map_err(plan_error)?;
        let moved_to = match moved {
            Some(r) => self.store.get::<Room>(&r.to_string())?.map(|r| r.name),
            None => None,
        };
        Ok(SavedOut {
            plan: self.plan_out(floor)?,
            ids: vec![pin.id.to_string()],
            moved_to,
        })
    }

    fn save_label(&mut self, a: LabelArgs) -> Result<SavedOut, ApiError> {
        self.need_edit()?;
        let floor: FloorId = pid(&a.floor_id, "floor")?;
        let _: Floor = self.store.fetch(&floor.to_string())?;
        let text: String = a.text.trim().chars().take(80).collect();
        if text.is_empty() {
            return Err(ApiError::invalid("write the label".to_string()));
        }
        if !geo::in_range(a.at) {
            return Err(ApiError::invalid("that point is too far away".to_string()));
        }
        let existing = match &a.id {
            Some(id) => Some(
                self.store
                    .fetch::<PlanLabel>(&pid::<PlanLabelId>(id, "label")?.to_string())?,
            ),
            None => None,
        };
        let l = PlanLabel {
            id: existing.as_ref().map(|e| e.id).unwrap_or_default(),
            floor,
            at: a.at,
            text,
            sensitivity: match a.sensitive {
                Some(true) => Sensitivity::Sensitive,
                Some(false) => Sensitivity::Normal,
                None => existing.map(|e| e.sensitivity).unwrap_or_else(|| {
                    housewright_core::facts::default_sensitivity("other", &a.text)
                }),
            },
        };
        self.store.put(&l)?;
        Ok(SavedOut {
            plan: self.plan_out(floor)?,
            ids: vec![l.id.to_string()],
            moved_to: None,
        })
    }

    fn set_background(&mut self, a: BackgroundArgs) -> Result<PlanOut, ApiError> {
        self.need_edit()?;
        let floor: FloorId = pid(&a.floor_id, "floor")?;
        let mut f: Floor = self.store.fetch(&floor.to_string())?;
        f.background = match a.attachment_id.filter(|s| !s.is_empty()) {
            None => None,
            Some(id) => {
                let id: AttachmentId = pid(&id, "file")?;
                let att: Attachment = self.store.fetch(&id.to_string())?;
                if !att.media_type.starts_with("image/") {
                    return Err(ApiError::invalid(
                        "use a photo or screenshot of the plan (a PDF cannot be shown behind the plan yet)".to_string(),
                    ));
                }
                let width = a.width.unwrap_or(15_000).clamp(500, 200_000);
                let at = a.at.unwrap_or_default();
                if !geo::in_range(at) {
                    return Err(ApiError::invalid("that point is too far away".to_string()));
                }
                Some(PlanBackground {
                    attachment: id,
                    at,
                    width,
                    opacity: a.opacity.unwrap_or(35).min(100),
                })
            }
        };
        self.store.put(&f)?;
        self.plan_out(floor)
    }

    /// Undo or redo (D-131): put back what one step changed, and only
    /// that, so edits made meanwhile on other devices are kept. Anything
    /// that was deleted is made again under a new id (deletes are final,
    /// D-52); `renamed` says which.
    fn plan_restore(&mut self, a: RestoreArgs) -> Result<RestoredOut, ApiError> {
        self.need_edit()?;
        let floor: FloorId = pid(&a.floor_id, "floor")?;
        let _: Floor = self.store.fetch(&floor.to_string())?;
        let see = self.me()?.role.can_see_sensitive();
        let mut renamed: BTreeMap<String, String> = BTreeMap::new();
        let from = a.from;
        let to = a.to;
        let check_floor = |f: FloorId| {
            if f == floor {
                Ok(())
            } else {
                Err(ApiError::invalid("that is on another floor".to_string()))
            }
        };
        for w in &to.walls {
            check_floor(w.floor)?;
            check_wall(w).map_err(plan_error)?;
        }
        for o in &to.openings {
            check_floor(o.floor)?;
        }
        for p in &to.pins {
            check_floor(p.floor)?;
        }
        for l in &to.labels {
            check_floor(l.floor)?;
        }
        for r in &to.rooms {
            if let Some(o) = &r.outline {
                check_outline(o).map_err(plan_error)?;
            }
        }
        self.store.batch(|s| {
            // Walls first, so openings and pins can follow a renamed wall.
            let mut wall_map: BTreeMap<WallId, WallId> = BTreeMap::new();
            for w in &to.walls {
                if from.walls.iter().any(|x| x == w) {
                    continue;
                }
                let mut w = w.clone();
                if s.get::<Wall>(&w.id.to_string())?.is_none() {
                    let new = WallId::new();
                    renamed.insert(w.id.to_string(), new.to_string());
                    wall_map.insert(w.id, new);
                    w.id = new;
                }
                s.put(&w)?;
            }
            let wall_of = |id: WallId| wall_map.get(&id).copied().unwrap_or(id);
            for o in &to.openings {
                if from.openings.iter().any(|x| x == o) {
                    continue;
                }
                let mut o = o.clone();
                o.wall = wall_of(o.wall);
                if s.get::<Opening>(&o.id.to_string())?.is_none() {
                    let new = OpeningId::new();
                    renamed.insert(o.id.to_string(), new.to_string());
                    o.id = new;
                }
                if s.get::<Wall>(&o.wall.to_string())?.is_some() {
                    s.put(&o)?;
                }
            }
            for p in &to.pins {
                if from.pins.iter().any(|x| x == p) {
                    continue;
                }
                let mut p = p.clone();
                p.wall = p.wall.map(wall_of);
                let current = s.get::<PlanPin>(&p.id.to_string())?;
                if !see && p.sensitivity == Sensitivity::Sensitive {
                    p.label = current.as_ref().and_then(|c| c.label.clone());
                }
                if current.is_none() {
                    let new = PlanPinId::new();
                    renamed.insert(p.id.to_string(), new.to_string());
                    p.id = new;
                }
                if p.item.is_some() || p.label.is_some() {
                    s.put(&p)?;
                }
            }
            for l in &to.labels {
                if from.labels.iter().any(|x| x == l) {
                    continue;
                }
                let mut l = l.clone();
                let current = s.get::<PlanLabel>(&l.id.to_string())?;
                if !see && l.sensitivity == Sensitivity::Sensitive {
                    match &current {
                        Some(c) => l.text = c.text.clone(),
                        None => continue,
                    }
                }
                if current.is_none() {
                    let new = PlanLabelId::new();
                    renamed.insert(l.id.to_string(), new.to_string());
                    l.id = new;
                }
                s.put(&l)?;
            }
            for r in &to.rooms {
                if from.rooms.iter().any(|x| x == r) {
                    continue;
                }
                if let Some(mut room) = s.get::<Room>(&r.id.to_string())? {
                    room.name = r.name.clone();
                    room.outline = r.outline.clone();
                    if r.outline.is_some() {
                        room.floor = Some(floor);
                    }
                    s.put(&room)?;
                }
            }
            // What the step added and the target does not have: remove.
            for w in &from.walls {
                if !to.walls.iter().any(|x| x.id == w.id)
                    && s.get::<Wall>(&w.id.to_string())?.is_some()
                {
                    s.delete_wall(w.id)?;
                }
            }
            for o in &from.openings {
                if !to.openings.iter().any(|x| x.id == o.id)
                    && s.get::<Opening>(&o.id.to_string())?.is_some()
                {
                    s.delete(EntityKind::Opening, &o.id.to_string())?;
                }
            }
            for p in &from.pins {
                if !to.pins.iter().any(|x| x.id == p.id)
                    && s.get::<PlanPin>(&p.id.to_string())?.is_some()
                {
                    s.delete_pin(p.id)?;
                }
            }
            for l in &from.labels {
                if !to.labels.iter().any(|x| x.id == l.id)
                    && s.get::<PlanLabel>(&l.id.to_string())?.is_some()
                {
                    s.delete(EntityKind::PlanLabel, &l.id.to_string())?;
                }
            }
            for r in &from.rooms {
                // A room the step created keeps existing (it may hold
                // things by now); undo takes it off the plan.
                if !to.rooms.iter().any(|x| x.id == r.id)
                    && let Some(mut room) = s.get::<Room>(&r.id.to_string())?
                    && room.outline.is_some()
                {
                    room.outline = None;
                    s.put(&room)?;
                }
            }
            Ok(())
        })?;
        Ok(RestoredOut {
            plan: self.plan_out(floor)?,
            renamed,
        })
    }
}
