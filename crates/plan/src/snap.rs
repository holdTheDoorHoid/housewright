//! Snapping a pointer position while drawing: to existing corners, to
//! square (90°) or 45° directions from the last point, and to the grid.

use serde::{Deserialize, Serialize};

use crate::geom::{Point, distance};

/// How to snap. Distances are plan millimetres (the interface converts its
/// on-screen tolerance into millimetres at the current zoom).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapOptions {
    /// Grid spacing in millimetres; `None` or 0 for no grid. Fractional,
    /// so a 6-inch grid (152.4 mm) does not drift: grid points are the
    /// nearest whole millimetre to each multiple.
    pub grid: Option<f64>,
    /// Corners within this distance are snapped onto.
    pub endpoint_tolerance: i64,
    /// Snap the direction from the anchor to multiples of this many degrees
    /// (90 or 45); `None` for free directions.
    pub angle_step: Option<f64>,
    /// How close (degrees) a direction must be to a multiple to snap.
    pub angle_tolerance: f64,
}

impl Default for SnapOptions {
    fn default() -> Self {
        Self {
            grid: Some(100.0),
            endpoint_tolerance: 150,
            angle_step: Some(45.0),
            angle_tolerance: 6.0,
        }
    }
}

/// What a snapped point snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapKind {
    /// Nothing: the point is as given.
    Free,
    /// An existing corner.
    Endpoint,
    /// A 90° or 45° direction from the anchor (with the length on the grid).
    Angle,
    /// The nearest grid point.
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapped {
    pub point: Point,
    pub kind: SnapKind,
}

/// The whole millimetre nearest to the multiple of `step` nearest to `v`
/// (ties away from zero).
pub fn round_to(v: i64, step: f64) -> i64 {
    if step <= 0.0 || !step.is_finite() {
        return v;
    }
    ((v as f64 / step).round() * step).round() as i64
}

/// Snap `p`. `anchor` is the previous point of the wall being drawn, if any;
/// `corners` are the existing corners on this floor. Order of preference:
/// an existing corner, then a 90°/45° direction from the anchor, then the
/// grid.
pub fn snap(p: Point, anchor: Option<Point>, corners: &[Point], o: &SnapOptions) -> Snapped {
    if o.endpoint_tolerance > 0 {
        let best = corners
            .iter()
            .copied()
            .filter(|c| Some(*c) != anchor)
            .map(|c| (distance(c, p), c))
            .filter(|(d, _)| *d <= o.endpoint_tolerance as f64)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((_, c)) = best {
            return Snapped {
                point: c,
                kind: SnapKind::Endpoint,
            };
        }
    }
    let grid = o.grid.filter(|g| *g > 0.0);
    if let (Some(a), Some(step)) = (anchor, o.angle_step.filter(|s| *s > 0.0)) {
        let dx = (p.x - a.x) as f64;
        let dy = (p.y - a.y) as f64;
        let len = dx.hypot(dy);
        if len > 0.0 {
            let ang = dy.atan2(dx).to_degrees();
            let snapped = (ang / step).round() * step;
            if (ang - snapped).abs() <= o.angle_tolerance {
                let rad = snapped.to_radians();
                let (ux, uy) = (rad.cos(), rad.sin());
                // Length along the snapped direction (projection), on the
                // grid when there is one.
                let mut l = dx * ux + dy * uy;
                if let Some(g) = grid {
                    l = round_to(l.round() as i64, g) as f64;
                }
                let q = Point::new(a.x + (ux * l).round() as i64, a.y + (uy * l).round() as i64);
                return Snapped {
                    point: q,
                    kind: SnapKind::Angle,
                };
            }
        }
    }
    if let Some(g) = grid {
        return Snapped {
            point: Point::new(round_to(p.x, g), round_to(p.y, g)),
            kind: SnapKind::Grid,
        };
    }
    Snapped {
        point: p,
        kind: SnapKind::Free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::bearing_deg;

    #[test]
    fn rounding() {
        assert_eq!(round_to(149, 100.0), 100);
        assert_eq!(round_to(150, 100.0), 200);
        assert_eq!(round_to(-149, 100.0), -100);
        assert_eq!(round_to(-150, 100.0), -200);
        assert_eq!(round_to(7, 0.0), 7);
        // A 6-inch grid: 2 ft is 610 mm, 10 ft is 3048 mm, never drifting.
        assert_eq!(round_to(600, 152.4), 610);
        assert_eq!(round_to(3050, 152.4), 3048);
    }

    #[test]
    fn endpoints_win() {
        let o = SnapOptions::default();
        let s = snap(
            Point::new(1040, 990),
            Some(Point::new(0, 0)),
            &[Point::new(1000, 1000)],
            &o,
        );
        assert_eq!(
            s,
            Snapped {
                point: Point::new(1000, 1000),
                kind: SnapKind::Endpoint
            }
        );
    }

    #[test]
    fn square_and_diagonal_directions() {
        let o = SnapOptions {
            grid: Some(10.0),
            ..SnapOptions::default()
        };
        let a = Point::new(0, 0);
        // Nearly straight right: snaps flat, length on the grid.
        let s = snap(Point::new(3004, 80), Some(a), &[], &o);
        assert_eq!(s.kind, SnapKind::Angle);
        assert_eq!(s.point, Point::new(3000, 0));
        // Nearly 45°.
        let s = snap(Point::new(2000, 2100), Some(a), &[], &o);
        assert_eq!(s.kind, SnapKind::Angle);
        let b = bearing_deg(a, s.point);
        assert!((b - 315.0).abs() < 0.05, "{b}");
        // Clearly in between: the grid.
        let s = snap(Point::new(2003, 1001), Some(a), &[], &o);
        assert_eq!(
            s,
            Snapped {
                point: Point::new(2000, 1000),
                kind: SnapKind::Grid
            }
        );
    }

    #[test]
    fn nothing_to_snap_to() {
        let o = SnapOptions {
            grid: None,
            endpoint_tolerance: 0,
            angle_step: None,
            angle_tolerance: 0.0,
        };
        let s = snap(
            Point::new(13, 17),
            Some(Point::new(0, 0)),
            &[Point::new(13, 17)],
            &o,
        );
        assert_eq!(
            s,
            Snapped {
                point: Point::new(13, 17),
                kind: SnapKind::Free
            }
        );
    }
}
