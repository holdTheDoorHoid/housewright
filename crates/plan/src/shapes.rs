//! Ready-made room outlines for drawing by dragging (Stage A).

use crate::geom::Point;

/// The rectangle with opposite corners `a` and `b`, clockwise on screen
/// from the top-left corner. Empty when it would have no area.
pub fn rectangle(a: Point, b: Point) -> Vec<Point> {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    if x0 == x1 || y0 == y1 {
        return Vec::new();
    }
    vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ]
}

/// Which corner of the bounding box an L shape leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notch {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

/// An L-shaped outline: the rectangle `a`-`b` with a `notch_w` by `notch_h`
/// rectangle cut from one corner. Empty when the notch does not leave an L
/// (zero, or as big as the whole rectangle in either direction).
pub fn l_shape(a: Point, b: Point, notch: Notch, notch_w: i64, notch_h: i64) -> Vec<Point> {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0 || h <= 0 || notch_w <= 0 || notch_h <= 0 || notch_w >= w || notch_h >= h {
        return Vec::new();
    }
    let p = Point::new;
    match notch {
        Notch::TopRight => vec![
            p(x0, y0),
            p(x1 - notch_w, y0),
            p(x1 - notch_w, y0 + notch_h),
            p(x1, y0 + notch_h),
            p(x1, y1),
            p(x0, y1),
        ],
        Notch::BottomRight => vec![
            p(x0, y0),
            p(x1, y0),
            p(x1, y1 - notch_h),
            p(x1 - notch_w, y1 - notch_h),
            p(x1 - notch_w, y1),
            p(x0, y1),
        ],
        Notch::BottomLeft => vec![
            p(x0, y0),
            p(x1, y0),
            p(x1, y1),
            p(x0 + notch_w, y1),
            p(x0 + notch_w, y1 - notch_h),
            p(x0, y1 - notch_h),
        ],
        Notch::TopLeft => vec![
            p(x0 + notch_w, y0),
            p(x1, y0),
            p(x1, y1),
            p(x0, y1),
            p(x0, y0 + notch_h),
            p(x0 + notch_w, y0 + notch_h),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{area_mm2, validate_polygon};

    #[test]
    fn rectangles_from_any_two_corners() {
        let r1 = rectangle(Point::new(0, 0), Point::new(4000, 3000));
        let r2 = rectangle(Point::new(4000, 3000), Point::new(0, 0));
        let r3 = rectangle(Point::new(0, 3000), Point::new(4000, 0));
        assert_eq!(r1, r2);
        assert_eq!(r1, r3);
        assert_eq!(area_mm2(&r1), 12_000_000.0);
        assert!(rectangle(Point::new(0, 0), Point::new(0, 10)).is_empty());
    }

    #[test]
    fn l_shapes_have_the_right_area_and_are_valid() {
        for notch in [
            Notch::TopLeft,
            Notch::TopRight,
            Notch::BottomRight,
            Notch::BottomLeft,
        ] {
            let l = l_shape(Point::new(0, 0), Point::new(6000, 4000), notch, 2000, 1500);
            assert_eq!(l.len(), 6);
            assert_eq!(area_mm2(&l), 24_000_000.0 - 3_000_000.0, "{notch:?}");
            assert!(validate_polygon(&l).is_ok(), "{notch:?}");
        }
        assert!(
            l_shape(
                Point::new(0, 0),
                Point::new(6000, 4000),
                Notch::TopLeft,
                6000,
                10
            )
            .is_empty()
        );
    }
}
