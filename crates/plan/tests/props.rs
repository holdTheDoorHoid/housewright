//! Property tests for the plan geometry, and the shared test vectors.

use housewright_plan::*;
use proptest::prelude::*;
use serde_json::Value;

fn pt() -> impl Strategy<Value = Point> {
    (-50_000i64..50_000, -50_000i64..50_000).prop_map(|(x, y)| Point::new(x, y))
}

fn rect_strategy() -> impl Strategy<Value = (Point, i64, i64)> {
    (pt(), 1i64..20_000, 1i64..20_000)
}

proptest! {
    #[test]
    fn rectangle_area_is_width_times_height((a, w, h) in rect_strategy()) {
        let r = rectangle(a, Point::new(a.x + w, a.y + h));
        prop_assert_eq!(area_mm2(&r), (w * h) as f64);
        prop_assert_eq!(perimeter(&r), (2 * (w + h)) as f64);
        prop_assert!(validate_polygon(&r).is_ok());
    }

    #[test]
    fn area_and_perimeter_do_not_depend_on_position_or_order(
        (a, w, h) in rect_strategy(),
        nw in 1i64..10_000, nh in 1i64..10_000,
        shift in pt(), start in 0usize..6, rev in any::<bool>(), turns in 0u8..4,
    ) {
        let b = Point::new(a.x + w + nw, a.y + h + nh);
        let mut l = l_shape(a, b, shapes::Notch::TopRight, nw, nh);
        prop_assume!(!l.is_empty());
        let area = area_mm2(&l);
        let per = perimeter(&l);
        // Translate, rotate by quarter turns, start anywhere, either winding.
        for p in &mut l {
            *p = Point::new(p.x + shift.x, p.y + shift.y);
            for _ in 0..turns {
                *p = Point::new(-p.y, p.x);
            }
        }
        let n = l.len();
        l.rotate_left(start % n);
        if rev { l.reverse(); }
        prop_assert_eq!(area_mm2(&l), area);
        prop_assert!((perimeter(&l) - per).abs() < 1e-6);
        prop_assert!(validate_polygon(&l).is_ok());
        prop_assert!(contains(&l, label_point(&l)));
    }

    #[test]
    fn area_of_a_triangle_never_exceeds_its_bounding_box(a in pt(), b in pt(), c in pt()) {
        let t = [a, b, c];
        let w = (t.iter().map(|p| p.x).max().unwrap() - t.iter().map(|p| p.x).min().unwrap()) as f64;
        let h = (t.iter().map(|p| p.y).max().unwrap() - t.iter().map(|p| p.y).min().unwrap()) as f64;
        prop_assert!(area_mm2(&t) <= w * h / 2.0 + 0.5);
        // The perimeter is at least twice the longest side.
        let longest = distance(a, b).max(distance(b, c)).max(distance(c, a));
        prop_assert!(perimeter(&t) + 1e-6 >= 2.0 * longest);
    }

    #[test]
    fn corners_and_centre_of_a_rectangle_are_inside((a, w, h) in rect_strategy(), outside in 1i64..1000) {
        let b = Point::new(a.x + w, a.y + h);
        let r = rectangle(a, b);
        for c in &r { prop_assert!(contains(&r, *c)); }
        prop_assert!(contains(&r, Point::new(a.x + w / 2, a.y + h / 2)));
        prop_assert!(!contains(&r, Point::new(b.x + outside, a.y)));
        prop_assert!(!contains(&r, Point::new(a.x, a.y - outside)));
    }

    #[test]
    fn grid_snapping_is_idempotent_and_close(p in pt(), g in 1i64..2000) {
        let o = SnapOptions { grid: Some(g), endpoint_tolerance: 0, angle_step: None, angle_tolerance: 0.0 };
        let s = snap(p, None, &[], &o);
        prop_assert_eq!(s.kind, SnapKind::Grid);
        prop_assert_eq!(s.point.x % g, 0);
        prop_assert_eq!(s.point.y % g, 0);
        prop_assert!((s.point.x - p.x).abs() * 2 <= g);
        prop_assert!((s.point.y - p.y).abs() * 2 <= g);
        prop_assert_eq!(snap(s.point, None, &[], &o), s);
    }

    #[test]
    fn angle_snapping_gives_square_or_diagonal_walls(a in pt(), p in pt(), step in prop::sample::select(vec![45.0f64, 90.0])) {
        prop_assume!(a != p);
        let o = SnapOptions { grid: Some(10), endpoint_tolerance: 0, angle_step: Some(step), angle_tolerance: 7.0 };
        let s = snap(p, Some(a), &[], &o);
        if s.kind == SnapKind::Angle && s.point != a {
            let b = bearing_deg(a, s.point);
            let off = (b / step - (b / step).round()).abs() * step;
            // Rounding to whole millimetres bends a long diagonal slightly.
            prop_assert!(off < 0.05, "bearing {} not a multiple of {}", b, step);
        }
        // Snapping never moves a point further than it was from the anchor
        // by more than the grid allows.
        prop_assert!(distance(a, s.point) <= distance(a, p) + 10.0);
    }

    #[test]
    fn endpoint_snapping_lands_exactly_on_a_corner(p in pt(), c in pt()) {
        let o = SnapOptions { grid: Some(100), endpoint_tolerance: 200, angle_step: Some(45.0), angle_tolerance: 6.0 };
        let s = snap(p, None, &[c], &o);
        if distance(p, c) <= 200.0 {
            prop_assert_eq!(s, Snapped { point: c, kind: SnapKind::Endpoint });
        } else {
            prop_assert_ne!(s.kind, SnapKind::Endpoint);
        }
    }

    #[test]
    fn lengths_survive_formatting_and_reading(mm in 0i64..100_000) {
        let metric = parse_length(&format_length(mm, Units::Metric), Units::Metric).unwrap();
        prop_assert!((metric - mm).abs() <= 5, "{} -> {}", mm, metric);
        let imp = parse_length(&format_length(mm, Units::Imperial), Units::Imperial).unwrap();
        prop_assert!((imp - mm).abs() as f64 <= 12.7 + 0.5, "{} -> {}", mm, imp);
    }

    #[test]
    fn rooms_from_walls_match_drawn_rectangles((a, w, h) in rect_strategy()) {
        let r = rectangle(a, Point::new(a.x + w, a.y + h));
        let walls: Vec<_> = (0..4).map(|i| (r[i], r[(i + 1) % 4])).collect();
        let f = faces(&walls);
        prop_assert_eq!(f.len(), 1);
        prop_assert_eq!(area_mm2(&f[0]), area_mm2(&r));
    }
}

fn units(s: &str) -> Units {
    if s == "metric" {
        Units::Metric
    } else {
        Units::Imperial
    }
}

fn point(v: &Value) -> Point {
    Point::new(v[0].as_i64().unwrap(), v[1].as_i64().unwrap())
}

#[test]
fn shared_vectors() {
    let v: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for c in v["parse"].as_array().unwrap() {
        let got = parse_length(c[0].as_str().unwrap(), units(c[1].as_str().unwrap()));
        assert_eq!(got, Ok(c[2].as_i64().unwrap()), "{c}");
    }
    for c in v["parse_errors"].as_array().unwrap() {
        assert!(
            parse_length(c[0].as_str().unwrap(), units(c[1].as_str().unwrap())).is_err(),
            "{c}"
        );
    }
    for c in v["format"].as_array().unwrap() {
        assert_eq!(
            format_length(c[0].as_i64().unwrap(), units(c[1].as_str().unwrap())),
            c[2].as_str().unwrap()
        );
    }
    for c in v["area"].as_array().unwrap() {
        let poly: Vec<Point> = c[0].as_array().unwrap().iter().map(point).collect();
        let a = area_mm2(&poly);
        assert_eq!(a, c[1].as_f64().unwrap());
        assert_eq!(format_area(a, Units::Imperial), c[2].as_str().unwrap());
        assert_eq!(format_area(a, Units::Metric), c[3].as_str().unwrap());
    }
    for c in v["angle"].as_array().unwrap() {
        let a = corner_angle(point(&c[0]), point(&c[1]), point(&c[2]));
        assert!((a - c[3].as_f64().unwrap()).abs() < 1e-9, "{c}");
    }
}
