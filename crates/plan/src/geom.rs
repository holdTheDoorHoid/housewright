//! Points, lengths, angles, areas and polygon checks.

use serde::{Deserialize, Serialize};

/// A point on a floor plan, in whole millimetres (y points down).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl Point {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// The largest coordinate accepted anywhere (one kilometre from the origin).
/// Keeps every product below in range and refuses nonsense input.
pub const MAX_COORD: i64 = 1_000_000;

/// Whether a point is inside the accepted range.
pub fn in_range(p: Point) -> bool {
    p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD
}

/// Distance between two points, in millimetres.
pub fn distance(a: Point, b: Point) -> f64 {
    let dx = (b.x - a.x) as f64;
    let dy = (b.y - a.y) as f64;
    dx.hypot(dy)
}

/// Distance rounded to whole millimetres.
pub fn distance_mm(a: Point, b: Point) -> i64 {
    distance(a, b).round() as i64
}

/// Twice the cross product of (b - a) and (c - a), exactly. Positive when
/// a, b, c turn counter-clockwise *in plan coordinates* (y down: that is
/// clockwise on the screen).
pub fn orient(a: Point, b: Point, c: Point) -> i128 {
    let abx = (b.x - a.x) as i128;
    let aby = (b.y - a.y) as i128;
    let acx = (c.x - a.x) as i128;
    let acy = (c.y - a.y) as i128;
    abx * acy - aby * acx
}

/// Twice the signed area of a polygon (shoelace), exactly. The sign depends
/// on the winding; use [`area_mm2`] for the area itself.
pub fn twice_signed_area(poly: &[Point]) -> i128 {
    let n = poly.len();
    if n < 3 {
        return 0;
    }
    let mut s: i128 = 0;
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        s += p.x as i128 * q.y as i128 - q.x as i128 * p.y as i128;
    }
    s
}

/// The area of a polygon in square millimetres (shoelace formula). For a
/// self-intersecting outline this is not a meaningful area; check
/// [`validate_polygon`] first.
pub fn area_mm2(poly: &[Point]) -> f64 {
    twice_signed_area(poly).unsigned_abs() as f64 / 2.0
}

/// The area in square metres.
pub fn area_m2(poly: &[Point]) -> f64 {
    area_mm2(poly) / 1_000_000.0
}

/// The length of a closed outline, in millimetres.
pub fn perimeter(poly: &[Point]) -> f64 {
    let n = poly.len();
    if n < 2 {
        return 0.0;
    }
    (0..n).map(|i| distance(poly[i], poly[(i + 1) % n])).sum()
}

/// The direction from `a` to `b` as people see it on screen: 0° points
/// right, 90° points up, in [0, 360).
pub fn bearing_deg(a: Point, b: Point) -> f64 {
    let dx = (b.x - a.x) as f64;
    let dy = (b.y - a.y) as f64;
    let d = (-dy).atan2(dx).to_degrees();
    let d = if d < 0.0 { d + 360.0 } else { d };
    if d >= 360.0 { 0.0 } else { d }
}

/// The angle at `corner` between the walls to `prev` and to `next`, in
/// degrees in [0, 180]. Two walls meeting square give 90.
pub fn corner_angle(prev: Point, corner: Point, next: Point) -> f64 {
    let ux = (prev.x - corner.x) as f64;
    let uy = (prev.y - corner.y) as f64;
    let vx = (next.x - corner.x) as f64;
    let vy = (next.y - corner.y) as f64;
    let cross = ux * vy - uy * vx;
    let dot = ux * vx + uy * vy;
    if cross == 0.0 && dot == 0.0 {
        return 0.0;
    }
    cross.abs().atan2(dot).to_degrees()
}

/// The angle between two walls given as segments, in [0, 90]: how far they
/// are from parallel. Used to show "90°" between walls that do not touch.
pub fn angle_between(a1: Point, a2: Point, b1: Point, b2: Point) -> f64 {
    let ux = (a2.x - a1.x) as f64;
    let uy = (a2.y - a1.y) as f64;
    let vx = (b2.x - b1.x) as f64;
    let vy = (b2.y - b1.y) as f64;
    let cross = ux * vy - uy * vx;
    let dot = ux * vx + uy * vy;
    let a = cross.abs().atan2(dot.abs()).to_degrees();
    a.clamp(0.0, 90.0)
}

/// The point `offset` millimetres from `a` towards `b` (rounded).
pub fn point_along(a: Point, b: Point, offset: i64) -> Point {
    let len = distance(a, b);
    if len == 0.0 {
        return a;
    }
    let t = offset as f64 / len;
    Point::new(
        (a.x as f64 + (b.x - a.x) as f64 * t).round() as i64,
        (a.y as f64 + (b.y - a.y) as f64 * t).round() as i64,
    )
}

/// Where `p` falls on segment a-b: the distance along it from `a` (clamped
/// to the segment) and the distance from the segment, both in millimetres.
pub fn project(p: Point, a: Point, b: Point) -> (f64, f64) {
    let len = distance(a, b);
    if len == 0.0 {
        return (0.0, distance(p, a));
    }
    let dx = (b.x - a.x) as f64;
    let dy = (b.y - a.y) as f64;
    let t = (((p.x - a.x) as f64 * dx + (p.y - a.y) as f64 * dy) / (len * len)).clamp(0.0, 1.0);
    let qx = a.x as f64 + dx * t;
    let qy = a.y as f64 + dy * t;
    let d = ((p.x as f64 - qx).powi(2) + (p.y as f64 - qy).powi(2)).sqrt();
    (t * len, d)
}

/// Whether `p` lies on the closed segment a-b, exactly.
pub fn on_segment(p: Point, a: Point, b: Point) -> bool {
    orient(a, b, p) == 0
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

/// Whether closed segments p1-p2 and q1-q2 share any point, exactly.
pub fn segments_touch(p1: Point, p2: Point, q1: Point, q2: Point) -> bool {
    let d1 = orient(q1, q2, p1).signum();
    let d2 = orient(q1, q2, p2).signum();
    let d3 = orient(p1, p2, q1).signum();
    let d4 = orient(p1, p2, q2).signum();
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    (d1 == 0 && on_segment(p1, q1, q2))
        || (d2 == 0 && on_segment(p2, q1, q2))
        || (d3 == 0 && on_segment(q1, p1, p2))
        || (d4 == 0 && on_segment(q2, p1, p2))
}

/// Why an outline cannot be a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolygonError {
    TooFewCorners,
    OutOfRange,
    RepeatedCorner,
    NoArea,
    SelfIntersecting,
}

impl std::fmt::Display for PolygonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PolygonError::TooFewCorners => "a room needs at least three corners",
            PolygonError::OutOfRange => "a corner is too far away (more than a kilometre)",
            PolygonError::RepeatedCorner => "two corners are in the same place",
            PolygonError::NoArea => "the outline has no area (its corners are in a line)",
            PolygonError::SelfIntersecting => "the outline crosses itself",
        })
    }
}

impl std::error::Error for PolygonError {}

/// Check that an outline is a simple polygon: at least three corners, no
/// two consecutive corners equal, some area, and no two edges touching
/// except neighbours at their shared corner.
pub fn validate_polygon(poly: &[Point]) -> Result<(), PolygonError> {
    let n = poly.len();
    if n < 3 {
        return Err(PolygonError::TooFewCorners);
    }
    if poly.iter().any(|p| !in_range(*p)) {
        return Err(PolygonError::OutOfRange);
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if poly[i] == poly[j] {
                return Err(PolygonError::RepeatedCorner);
            }
        }
    }
    if poly.iter().all(|q| orient(poly[0], poly[1], *q) == 0) {
        return Err(PolygonError::NoArea);
    }
    for i in 0..n {
        let a1 = poly[i];
        let a2 = poly[(i + 1) % n];
        for j in (i + 1)..n {
            let b1 = poly[j];
            let b2 = poly[(j + 1) % n];
            let neighbours = j == i + 1 || (i == 0 && j == n - 1);
            if neighbours {
                // Neighbours share one corner; they must not fold back
                // over each other.
                let (shared, other_a, other_b) = if j == i + 1 {
                    (a2, a1, b2)
                } else {
                    (a1, a2, b1)
                };
                if orient(other_a, shared, other_b) == 0 {
                    let ux = other_a.x - shared.x;
                    let uy = other_a.y - shared.y;
                    let vx = other_b.x - shared.x;
                    let vy = other_b.y - shared.y;
                    if (ux as i128 * vx as i128 + uy as i128 * vy as i128) > 0 {
                        return Err(PolygonError::SelfIntersecting);
                    }
                }
                continue;
            }
            if segments_touch(a1, a2, b1, b2) {
                return Err(PolygonError::SelfIntersecting);
            }
        }
    }
    if twice_signed_area(poly) == 0 {
        return Err(PolygonError::NoArea);
    }
    Ok(())
}

/// Whether `p` is inside the polygon or on its outline (exact).
pub fn contains(poly: &[Point], p: Point) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        if on_segment(p, a, b) {
            return true;
        }
        // Crossing test on the half-open rule, with exact arithmetic.
        if (a.y > p.y) != (b.y > p.y) {
            // x of the edge at p.y, compared to p.x without division.
            let o = orient(a, b, p);
            // Edge going down (b.y > a.y): point is left of the crossing
            // when o > 0 in these coordinates.
            if (b.y > a.y && o > 0) || (b.y < a.y && o < 0) {
                inside = !inside;
            }
        }
    }
    inside
}

/// The centre of mass of a polygon (for an outline with no area: the
/// average of its corners).
pub fn centroid(poly: &[Point]) -> (f64, f64) {
    let a2 = twice_signed_area(poly) as f64;
    let n = poly.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    if a2 == 0.0 {
        let sx: f64 = poly.iter().map(|p| p.x as f64).sum();
        let sy: f64 = poly.iter().map(|p| p.y as f64).sum();
        return (sx / n as f64, sy / n as f64);
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        let c = p.x as f64 * q.y as f64 - q.x as f64 * p.y as f64;
        cx += (p.x + q.x) as f64 * c;
        cy += (p.y + q.y) as f64 * c;
    }
    (cx / (3.0 * a2), cy / (3.0 * a2))
}

/// A good place to write a room's name: its centroid when that is inside
/// (rectangles, most L shapes), otherwise the middle of the widest stretch
/// of the room along the horizontal line through the centroid.
pub fn label_point(poly: &[Point]) -> Point {
    let (cx, cy) = centroid(poly);
    let c = Point::new(cx.round() as i64, cy.round() as i64);
    if contains(poly, c) || poly.len() < 3 {
        return c;
    }
    let y = cy;
    let mut xs: Vec<f64> = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let (ay, by) = (a.y as f64, b.y as f64);
        if (ay > y) != (by > y) {
            let t = (y - ay) / (by - ay);
            xs.push(a.x as f64 + t * (b.x - a.x) as f64);
        }
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let mut best: Option<(f64, f64)> = None;
    for pair in xs.chunks(2) {
        if let [l, r] = pair
            && best.is_none_or(|(bl, br)| r - l > br - bl)
        {
            best = Some((*l, *r));
        }
    }
    match best {
        Some((l, r)) => Point::new(((l + r) / 2.0).round() as i64, y.round() as i64),
        None => c,
    }
}

/// Whether an opening `offset`..`offset + width` (millimetres from the
/// wall's start) fits on a wall of length `wall_len`.
pub fn opening_fits(wall_len: i64, offset: i64, width: i64) -> bool {
    width > 0 && offset >= 0 && offset.saturating_add(width) <= wall_len
}

/// Whether two openings on the same wall overlap (touching ends is fine).
pub fn openings_overlap(a: (i64, i64), b: (i64, i64)) -> bool {
    let (ao, aw) = a;
    let (bo, bw) = b;
    ao < bo + bw && bo < ao + aw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    fn rect(w: i64, h: i64) -> Vec<Point> {
        vec![p(0, 0), p(w, 0), p(w, h), p(0, h)]
    }

    #[test]
    fn rectangle_area_and_perimeter() {
        let r = rect(4000, 3000);
        assert_eq!(area_mm2(&r), 12_000_000.0);
        assert_eq!(area_m2(&r), 12.0);
        assert_eq!(perimeter(&r), 14_000.0);
    }

    #[test]
    fn l_shape_area() {
        // 6 x 4 m with a 2 x 2 m corner cut out: 20 m².
        let l = vec![
            p(0, 0),
            p(6000, 0),
            p(6000, 2000),
            p(4000, 2000),
            p(4000, 4000),
            p(0, 4000),
        ];
        assert_eq!(area_m2(&l), 20.0);
        assert!(validate_polygon(&l).is_ok());
        assert!(contains(&l, p(1000, 3000)));
        assert!(!contains(&l, p(5000, 3000)));
        assert!(contains(&l, label_point(&l)));
    }

    #[test]
    fn half_millimetre_areas_are_kept() {
        let t = vec![p(0, 0), p(1, 0), p(0, 1)];
        assert_eq!(area_mm2(&t), 0.5);
    }

    #[test]
    fn angles() {
        assert_eq!(corner_angle(p(1000, 0), p(0, 0), p(0, 1000)), 90.0);
        assert!((corner_angle(p(1000, 0), p(0, 0), p(1000, 1000)) - 45.0).abs() < 1e-9);
        assert_eq!(corner_angle(p(1000, 0), p(0, 0), p(-1000, 0)), 180.0);
        assert_eq!(bearing_deg(p(0, 0), p(1000, 0)), 0.0);
        // Up on the screen is negative y.
        assert_eq!(bearing_deg(p(0, 0), p(0, -1000)), 90.0);
        assert_eq!(bearing_deg(p(0, 0), p(-1000, 0)), 180.0);
        assert_eq!(bearing_deg(p(0, 0), p(0, 1000)), 270.0);
        assert_eq!(angle_between(p(0, 0), p(10, 0), p(5, 5), p(5, 50)), 90.0);
        assert_eq!(angle_between(p(0, 0), p(10, 0), p(0, 5), p(-30, 5)), 0.0);
    }

    #[test]
    fn polygon_validity() {
        assert_eq!(
            validate_polygon(&[p(0, 0), p(1, 0)]),
            Err(PolygonError::TooFewCorners)
        );
        assert_eq!(
            validate_polygon(&[p(0, 0), p(5, 0), p(10, 0)]),
            Err(PolygonError::NoArea)
        );
        assert_eq!(
            validate_polygon(&[p(0, 0), p(10, 0), p(10, 10), p(0, 0), p(-5, 5)]),
            Err(PolygonError::RepeatedCorner)
        );
        // A bow tie crosses itself.
        assert_eq!(
            validate_polygon(&[p(0, 0), p(10, 10), p(10, 0), p(0, 10)]),
            Err(PolygonError::SelfIntersecting)
        );
        // A spike that folds back along itself.
        assert_eq!(
            validate_polygon(&[p(0, 0), p(10, 0), p(5, 0), p(5, 10)]),
            Err(PolygonError::SelfIntersecting)
        );
        // A corner touching another edge.
        assert_eq!(
            validate_polygon(&[p(0, 0), p(10, 0), p(10, 10), p(5, 0), p(0, 10)]),
            Err(PolygonError::SelfIntersecting)
        );
        assert_eq!(
            validate_polygon(&[p(0, 0), p(MAX_COORD + 1, 0), p(0, 5)]),
            Err(PolygonError::OutOfRange)
        );
        assert!(validate_polygon(&rect(10, 10)).is_ok());
        // A straight-through corner (collinear neighbours) is allowed.
        assert!(validate_polygon(&[p(0, 0), p(5, 0), p(10, 0), p(10, 10), p(0, 10)]).is_ok());
    }

    #[test]
    fn point_in_room() {
        let r = rect(100, 100);
        assert!(contains(&r, p(50, 50)));
        assert!(contains(&r, p(0, 50)), "the outline counts as inside");
        assert!(contains(&r, p(100, 100)));
        assert!(!contains(&r, p(101, 50)));
        assert!(!contains(&r, p(-1, -1)));
        // Same answer with the opposite winding.
        let mut rev = r.clone();
        rev.reverse();
        for q in [p(50, 50), p(150, 50), p(0, 0), p(50, -10)] {
            assert_eq!(contains(&r, q), contains(&rev, q));
        }
    }

    #[test]
    fn projection_and_points_along() {
        let (t, d) = project(p(500, 300), p(0, 0), p(1000, 0));
        assert_eq!((t, d), (500.0, 300.0));
        let (t, _) = project(p(-500, 0), p(0, 0), p(1000, 0));
        assert_eq!(t, 0.0);
        assert_eq!(point_along(p(0, 0), p(0, 2000), 500), p(0, 500));
        assert_eq!(point_along(p(0, 0), p(3000, 4000), 2500), p(1500, 2000));
    }

    #[test]
    fn openings() {
        assert!(opening_fits(3000, 0, 900));
        assert!(opening_fits(3000, 2100, 900));
        assert!(!opening_fits(3000, 2101, 900));
        assert!(!opening_fits(3000, -1, 900));
        assert!(!opening_fits(3000, 0, 0));
        assert!(openings_overlap((0, 900), (800, 900)));
        assert!(!openings_overlap((0, 900), (900, 900)));
    }
}
