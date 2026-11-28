//! Rooms from walls: the enclosed spaces of a set of wall segments.
//!
//! Walls are treated as lines (their centre lines). They must meet at their
//! ends, or one wall's end must lie on another wall (a T); the longer wall
//! is split there. Walls that cross in an X are not split, so the spaces
//! either side of a crossing are not found; the editor's snapping makes
//! walls meet at corners, so this is rare and the room can still be drawn
//! directly.

use std::collections::{BTreeMap, BTreeSet};

use crate::geom::{Point, on_segment, orient, twice_signed_area};

/// The enclosed spaces formed by `walls` (pairs of end points), as simple
/// outlines with no straight-through corners, largest first. Dangling walls
/// (a wall that does not close a space) are ignored.
pub fn faces(walls: &[(Point, Point)]) -> Vec<Vec<Point>> {
    // 1. Segments, split where another wall's end lies on them (T joints).
    let ends: BTreeSet<Point> = walls
        .iter()
        .filter(|(a, b)| a != b)
        .flat_map(|(a, b)| [*a, *b])
        .collect();
    let mut edges: BTreeSet<(Point, Point)> = BTreeSet::new();
    for (a, b) in walls.iter().copied().filter(|(a, b)| a != b) {
        let mut cuts: Vec<Point> = ends
            .iter()
            .copied()
            .filter(|p| *p != a && *p != b && on_segment(*p, a, b))
            .collect();
        cuts.push(a);
        cuts.push(b);
        // Order along the segment.
        cuts.sort_by_key(|p| {
            let dx = (p.x - a.x) as i128;
            let dy = (p.y - a.y) as i128;
            dx * dx + dy * dy
        });
        cuts.dedup();
        for w in cuts.windows(2) {
            let (u, v) = (w[0].min(w[1]), w[0].max(w[1]));
            edges.insert((u, v));
        }
    }

    // 2. Adjacency; drop dangling walls (vertices with one neighbour).
    let mut adj: BTreeMap<Point, BTreeSet<Point>> = BTreeMap::new();
    for (u, v) in &edges {
        adj.entry(*u).or_default().insert(*v);
        adj.entry(*v).or_default().insert(*u);
    }
    loop {
        let lonely: Vec<Point> = adj
            .iter()
            .filter(|(_, n)| n.len() < 2)
            .map(|(p, _)| *p)
            .collect();
        if lonely.is_empty() {
            break;
        }
        for p in lonely {
            if let Some(ns) = adj.remove(&p) {
                for n in ns {
                    if let Some(s) = adj.get_mut(&n) {
                        s.remove(&p);
                    }
                }
            }
        }
    }

    // 3. Neighbours of each corner sorted by angle.
    let sorted: BTreeMap<Point, Vec<Point>> = adj
        .iter()
        .map(|(p, ns)| {
            let mut v: Vec<Point> = ns.iter().copied().collect();
            v.sort_by(|a, b| {
                let aa = ((a.y - p.y) as f64).atan2((a.x - p.x) as f64);
                let bb = ((b.y - p.y) as f64).atan2((b.x - p.x) as f64);
                aa.total_cmp(&bb)
            });
            (*p, v)
        })
        .collect();

    // 4. Walk every directed edge once. From u->v, turn to the neighbour of
    //    v that comes just before u in angular order: this traces each
    //    bounded face one way round and the outer boundary the other.
    let mut seen: BTreeSet<(Point, Point)> = BTreeSet::new();
    let mut out: Vec<Vec<Point>> = Vec::new();
    for (u0, ns) in &sorted {
        for v0 in ns {
            if seen.contains(&(*u0, *v0)) {
                continue;
            }
            let mut face = Vec::new();
            let (mut u, mut v) = (*u0, *v0);
            let ok;
            loop {
                if !seen.insert((u, v)) {
                    ok = u == *u0 && v == *v0;
                    break;
                }
                face.push(u);
                let around = &sorted[&v];
                let i = around.iter().position(|p| *p == u).unwrap_or(0);
                let w = around[(i + around.len() - 1) % around.len()];
                u = v;
                v = w;
                if face.len() > edges.len() * 2 + 2 {
                    ok = false;
                    break;
                }
            }
            if ok && face.len() >= 3 && twice_signed_area(&face) > 0 {
                out.push(simplify(&face));
            }
        }
    }
    out.retain(|f| f.len() >= 3);
    out.sort_by(|a, b| {
        twice_signed_area(b)
            .cmp(&twice_signed_area(a))
            .then_with(|| a.cmp(b))
    });
    out
}

/// Remove corners where the outline goes straight on.
pub fn simplify(poly: &[Point]) -> Vec<Point> {
    let n = poly.len();
    if n < 4 {
        return poly.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = poly[(i + n - 1) % n];
        let next = poly[(i + 1) % n];
        if orient(prev, poly[i], next) != 0 {
            out.push(poly[i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::area_m2;

    fn p(x: i64, y: i64) -> Point {
        Point::new(x, y)
    }

    fn loop_of(pts: &[Point]) -> Vec<(Point, Point)> {
        (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    }

    #[test]
    fn one_room() {
        let w = loop_of(&[p(0, 0), p(4000, 0), p(4000, 3000), p(0, 3000)]);
        let f = faces(&w);
        assert_eq!(f.len(), 1);
        assert_eq!(area_m2(&f[0]), 12.0);
    }

    #[test]
    fn two_rooms_sharing_a_wall_drawn_as_a_t() {
        // A 6 x 3 m box with a dividing wall at x = 4 m whose ends lie on
        // the long walls (which are not split by whoever drew them).
        let mut w = loop_of(&[p(0, 0), p(6000, 0), p(6000, 3000), p(0, 3000)]);
        w.push((p(4000, 0), p(4000, 3000)));
        let f = faces(&w);
        assert_eq!(f.len(), 2);
        assert_eq!(area_m2(&f[0]), 12.0);
        assert_eq!(area_m2(&f[1]), 6.0);
    }

    #[test]
    fn dangling_walls_and_open_shapes_make_no_room() {
        let w = vec![(p(0, 0), p(1000, 0)), (p(1000, 0), p(1000, 1000))];
        assert!(faces(&w).is_empty());
        let mut w = loop_of(&[p(0, 0), p(1000, 0), p(1000, 1000), p(0, 1000)]);
        w.push((p(1000, 1000), p(2000, 2000)));
        let f = faces(&w);
        assert_eq!(f.len(), 1);
        assert_eq!(area_m2(&f[0]), 1.0);
    }

    #[test]
    fn walls_drawn_in_pieces_and_duplicates() {
        // The top wall drawn as two pieces, and the left wall twice.
        let w = vec![
            (p(0, 0), p(2000, 0)),
            (p(2000, 0), p(4000, 0)),
            (p(4000, 0), p(4000, 3000)),
            (p(4000, 3000), p(0, 3000)),
            (p(0, 3000), p(0, 0)),
            (p(0, 0), p(0, 3000)),
        ];
        let f = faces(&w);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].len(), 4, "the straight-through corner is dropped");
    }

    #[test]
    fn l_shaped_room_and_a_grid_of_four() {
        let l = loop_of(&[
            p(0, 0),
            p(6000, 0),
            p(6000, 2000),
            p(4000, 2000),
            p(4000, 4000),
            p(0, 4000),
        ]);
        let f = faces(&l);
        assert_eq!(f.len(), 1);
        assert_eq!(area_m2(&f[0]), 20.0);

        let mut w = loop_of(&[p(0, 0), p(2000, 0), p(2000, 2000), p(0, 2000)]);
        w.push((p(1000, 0), p(1000, 2000)));
        w.push((p(0, 1000), p(2000, 1000)));
        // The cross in the middle is an X: the two inner walls are not
        // split there, so only the halves either side are found... unless
        // each is drawn in two pieces, which the editor does.
        let mut w2 = loop_of(&[p(0, 0), p(2000, 0), p(2000, 2000), p(0, 2000)]);
        w2.extend([
            (p(1000, 0), p(1000, 1000)),
            (p(1000, 1000), p(1000, 2000)),
            (p(0, 1000), p(1000, 1000)),
            (p(1000, 1000), p(2000, 1000)),
        ]);
        assert_eq!(faces(&w2).len(), 4);
        assert!(faces(&w).len() < 4);
    }
}
