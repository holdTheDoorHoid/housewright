//! The binder's "Floor plans" sections (D-25): one page per drawn floor,
//! with rooms, their areas, numbered pins and a legend.

use std::collections::BTreeMap;

use housewright_core::ids::PlanPinId;
use housewright_core::model::{OpeningKind, Side, WallEnd};
use housewright_plan::{self as geo, Point, Units};

use crate::data::{FactData, FloorData, HouseData};
use crate::doc::{Block, Drawing, DrawnLabel, DrawnPin, DrawnRoom, Section};

/// Space left around the drawing, in plan millimetres.
const MARGIN: i64 = 500;

/// Pin numbers for a floor: top to bottom, then left to right, so the
/// legend reads like the page.
pub fn pin_numbers(f: &FloorData) -> BTreeMap<PlanPinId, u32> {
    let mut pins: Vec<_> = f
        .pins
        .iter()
        .map(|p| (p.pin.at.y, p.pin.at.x, p.pin.id))
        .collect();
    pins.sort();
    pins.into_iter()
        .enumerate()
        .map(|(i, (_, _, id))| (id, i as u32 + 1))
        .collect()
}

/// Where a fact's pin is: "Ground floor plan, pin 3".
pub fn pin_place(d: &HouseData, pin: PlanPinId) -> Option<String> {
    d.floors.iter().find_map(|f| {
        pin_numbers(f)
            .get(&pin)
            .map(|n| format!("{} plan, pin {n}", f.floor.name))
    })
}

fn bar_for(width: i64, units: Units) -> (i64, String) {
    match units {
        Units::Imperial => {
            let ft = [1, 2, 5, 10, 20, 50, 100]
                .into_iter()
                .rev()
                .find(|ft| (*ft as f64) * 304.8 <= width as f64 / 4.0)
                .unwrap_or(1);
            ((ft as f64 * 304.8).round() as i64, format!("{ft} ft"))
        }
        Units::Metric => {
            let m = [1, 2, 5, 10, 20, 50]
                .into_iter()
                .rev()
                .find(|m| m * 1000 <= width / 4)
                .unwrap_or(1);
            (m * 1000, format!("{m} m"))
        }
    }
}

/// The drawing of one floor. `strong` pins are drawn dark.
pub fn drawing(f: &FloorData, units: Units, strong: &[PlanPinId]) -> Drawing {
    let mut pts: Vec<Point> = Vec::new();
    for r in &f.rooms {
        pts.extend(r.outline.iter().flatten());
    }
    for w in &f.walls {
        pts.push(w.a);
        pts.push(w.b);
    }
    pts.extend(f.pins.iter().map(|p| p.pin.at));
    pts.extend(f.labels.iter().map(|l| l.at));
    // A door swinging outwards must stay inside the drawing.
    for o in &f.openings {
        if let Some(w) = f.walls.iter().find(|w| w.id == o.wall) {
            let c = geo::point_along(w.a, w.b, o.offset + o.width / 2);
            let r = o.width + 100;
            pts.push(Point::new(c.x - r, c.y - r));
            pts.push(Point::new(c.x + r, c.y + r));
        }
    }
    let (x0, y0, x1, y1) = if pts.is_empty() {
        (0, 0, 1000, 1000)
    } else {
        (
            pts.iter().map(|p| p.x).min().unwrap_or(0),
            pts.iter().map(|p| p.y).min().unwrap_or(0),
            pts.iter().map(|p| p.x).max().unwrap_or(0),
            pts.iter().map(|p| p.y).max().unwrap_or(0),
        )
    };
    let ox = x0 - MARGIN;
    let oy = y0 - MARGIN;
    let at = |p: Point| [p.x - ox, p.y - oy];
    let width = (x1 - x0 + 2 * MARGIN).max(2000);
    let height = (y1 - y0 + 2 * MARGIN).max(1500);

    let mut d = Drawing {
        width,
        height,
        bar: bar_for(width, units),
        ..Drawing::default()
    };
    for r in &f.rooms {
        let Some(o) = &r.outline else { continue };
        d.rooms.push(DrawnRoom {
            points: o.iter().map(|p| at(*p)).collect(),
            name: r.name.clone(),
            area: geo::format_area(geo::area_mm2(o), units),
            at: at(geo::label_point(o)),
        });
    }
    let walls: BTreeMap<_, _> = f.walls.iter().map(|w| (w.id, w)).collect();
    for w in &f.walls {
        let [ax, ay] = at(w.a);
        let [bx, by] = at(w.b);
        d.walls.push([ax, ay, bx, by, w.thickness()]);
    }
    for o in &f.openings {
        let Some(w) = walls.get(&o.wall) else {
            continue;
        };
        let s = geo::point_along(w.a, w.b, o.offset);
        let e = geo::point_along(w.a, w.b, o.offset + o.width);
        let [sx, sy] = at(s);
        let [ex, ey] = at(e);
        d.gaps.push([sx, sy, ex, ey, w.thickness()]);
        match o.kind {
            OpeningKind::Window => d.windows.push([sx, sy, ex, ey]),
            OpeningKind::Gap => {}
            OpeningKind::Door => {
                let swing = o.swing.unwrap_or(housewright_core::model::Swing {
                    hinge: WallEnd::A,
                    side: Side::Left,
                });
                let (hinge, free) = match swing.hinge {
                    WallEnd::A => (s, e),
                    WallEnd::B => (e, s),
                };
                // Unit vector along the wall, and its left normal on screen.
                let len = geo::distance(w.a, w.b).max(1.0);
                let ux = (w.b.x - w.a.x) as f64 / len;
                let uy = (w.b.y - w.a.y) as f64 / len;
                let (nx, ny) = match swing.side {
                    Side::Left => (uy, -ux),
                    Side::Right => (-uy, ux),
                };
                let r = o.width as f64;
                let leaf = Point::new(
                    hinge.x + (nx * r).round() as i64,
                    hinge.y + (ny * r).round() as i64,
                );
                // The arc from the open leaf back to the closed position.
                let a0 = ((leaf.y - hinge.y) as f64).atan2((leaf.x - hinge.x) as f64);
                let a1 = ((free.y - hinge.y) as f64).atan2((free.x - hinge.x) as f64);
                let mut da = a1 - a0;
                while da > std::f64::consts::PI {
                    da -= 2.0 * std::f64::consts::PI;
                }
                while da < -std::f64::consts::PI {
                    da += 2.0 * std::f64::consts::PI;
                }
                let mut arc = vec![at(hinge), at(leaf)];
                for i in 1..=8 {
                    let a = a0 + da * i as f64 / 8.0;
                    arc.push(at(Point::new(
                        hinge.x + (a.cos() * r).round() as i64,
                        hinge.y + (a.sin() * r).round() as i64,
                    )));
                }
                d.swings.push(arc);
            }
        }
    }
    let numbers = pin_numbers(f);
    for p in &f.pins {
        d.pins.push(DrawnPin {
            at: at(p.pin.at),
            number: numbers[&p.pin.id],
            strong: strong.contains(&p.pin.id),
        });
    }
    d.pins.sort_by_key(|p| p.number);
    for l in &f.labels {
        d.labels.push(DrawnLabel {
            at: at(l.at),
            text: l.text.clone(),
        });
    }
    d
}

/// The section key of a floor's plan.
pub fn section_key(f: &FloorData) -> String {
    format!("plan:{}", f.floor.id)
}

/// One section per drawn floor. `shutoffs` are the facts the emergency
/// page lists; their pins are drawn dark and named in the legend.
pub fn plan_sections(d: &HouseData, shutoffs: &[&FactData]) -> Vec<Section> {
    let strong: Vec<PlanPinId> = shutoffs.iter().filter_map(|f| f.fact.pin).collect();
    let fact_of_pin: BTreeMap<PlanPinId, &str> = d
        .facts
        .iter()
        .filter_map(|f| f.fact.pin.map(|p| (p, f.fact.label.as_str())))
        .collect();
    let mut out = Vec::new();
    for (i, f) in d.floors.iter().enumerate() {
        let short: String = format!("Plan {}", f.floor.name).chars().take(28).collect();
        let mut s = Section::new(
            section_key(f),
            short,
            format!("Floor plan: {}", f.floor.name),
        );
        s.push(Block::Plan {
            drawing: drawing(f, d.units, &strong),
        });
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut total = 0.0;
        let mut rooms: Vec<_> = f.rooms.iter().filter(|r| r.outline.is_some()).collect();
        rooms.sort_by_key(|r| r.name.to_lowercase());
        for r in rooms {
            let o = r.outline.as_deref().unwrap_or(&[]);
            let a = geo::area_mm2(o);
            total += a;
            rows.push(vec![r.name.clone(), geo::format_area(a, d.units)]);
        }
        if !rows.is_empty() {
            rows.push(vec!["Total".into(), geo::format_area(total, d.units)]);
            s.push(Block::Table {
                columns: vec!["Room".into(), "Area".into()],
                rows,
            });
        }
        let numbers = pin_numbers(f);
        let mut pins: Vec<_> = f.pins.iter().collect();
        pins.sort_by_key(|p| numbers[&p.pin.id]);
        let plan = housewright_core::plan::FloorPlan {
            floor: f.floor.clone(),
            rooms: f.rooms.clone(),
            walls: vec![],
            openings: vec![],
            pins: vec![],
            labels: vec![],
        };
        let room_names: BTreeMap<_, _> = f.rooms.iter().map(|r| (r.id, r.name.clone())).collect();
        let legend: Vec<Vec<String>> = pins
            .iter()
            .map(|p| {
                let what = match (&p.item_name, &p.pin.label) {
                    (Some(n), _) => n.clone(),
                    (None, Some(l)) => l.clone(),
                    (None, None) => "Marked spot".into(),
                };
                let what = match fact_of_pin.get(&p.pin.id) {
                    // "Water main" and "Water main shutoff": say it once.
                    Some(fact) if fact.to_lowercase().starts_with(&what.to_lowercase()) => {
                        fact.to_string()
                    }
                    Some(fact) if !what.to_lowercase().contains(&fact.to_lowercase()) => {
                        format!("{what} ({fact})")
                    }
                    _ => what,
                };
                let room = plan
                    .room_at(p.pin.at)
                    .and_then(|r| room_names.get(&r).cloned())
                    .unwrap_or_default();
                vec![numbers[&p.pin.id].to_string(), what, room]
            })
            .collect();
        if !legend.is_empty() {
            // No room column when no pin is in a drawn room.
            let (columns, rows) = if legend.iter().all(|r| r[2].is_empty()) {
                (
                    vec!["Pin".into(), "What".into()],
                    legend
                        .into_iter()
                        .map(|mut r| {
                            r.truncate(2);
                            r
                        })
                        .collect(),
                )
            } else {
                (vec!["Pin".into(), "What".into(), "Room".into()], legend)
            };
            s.push(Block::Table { columns, rows });
            if !strong.is_empty() && f.pins.iter().any(|p| strong.contains(&p.pin.id)) {
                s.push(Block::Note {
                    text: "Dark pins are shutoffs listed on the emergency page.".into(),
                });
            }
        }
        if i == 0 && d.withheld.pins > 0 {
            s.push(Block::Note {
                text: format!(
                    "{} marked private {} not shown on these plans.",
                    d.withheld.pins,
                    if d.withheld.pins == 1 {
                        "pin or label is"
                    } else {
                        "pins or labels are"
                    }
                ),
            });
        }
        out.push(s);
    }
    out
}
