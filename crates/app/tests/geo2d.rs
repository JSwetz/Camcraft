//! Geo2D wrapper tests: the millimeter/region contract around the pinned
//! Clipper2 port. Goldens are hand-computable; degenerate inputs must
//! error, never panic (Prime Directive).

use hsmworks_app::geo2d::{GeoError, Loop, Point2, Region};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .expect("test rect")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.max(1.0).abs()
}

#[test]
fn signed_area_and_orientation() {
    assert!(close(rect(0.0, 0.0, 1.0, 1.0).area(), 1.0));
    let cw = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(0.0, 1.0),
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 0.0),
    ])
    .unwrap();
    assert!(close(cw.area(), -1.0));
    assert!(rect(0.0, 0.0, 1.0, 1.0).is_ccw());
    assert!(!cw.is_ccw());
}

#[test]
fn loop_construction_gates() {
    // Fewer than three distinct vertices: refused, including the
    // duplicated-around-the-ring and consecutive-repeat cases.
    assert!(matches!(
        Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)]),
        Err(GeoError::Invalid(_))
    ));
    let degen = Loop::new(vec![
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 1.0),
    ]);
    assert!(matches!(degen, Err(GeoError::Invalid(_))));
    assert!(Loop::new(vec![Point2::new(f64::NAN, 0.0), Point2::new(1.0, 0.0), Point2::new(0.0, 1.0)]).is_err());
    assert!(
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
            Point2::new(0.0, 0.0),
        ])
        .is_ok(),
        "an explicit closing repeat is deduped, not refused"
    );
}

#[test]
fn containment_is_even_odd_parity() {
    let outer = rect(0.0, 0.0, 10.0, 10.0);
    let hole = rect(4.0, 4.0, 6.0, 6.0);
    let region = Region::new(vec![outer, hole]).unwrap();
    assert!(region.contains(Point2::new(1.0, 1.0)));
    assert!(!region.contains(Point2::new(5.0, 5.0)), "inside the hole");
    assert!(!region.contains(Point2::new(-1.0, 5.0)));
    assert!(close(region.area(), 100.0 - 4.0));
}

#[test]
fn booleans_have_golden_areas() {
    let a = Region::new(vec![rect(0.0, 0.0, 1.0, 1.0)]).unwrap();
    let b = Region::new(vec![rect(0.5, 0.5, 1.5, 1.5)]).unwrap();
    assert!(close(a.union(&b).unwrap().area(), 1.75));
    assert!(close(a.intersect(&b).unwrap().area(), 0.25));
    assert!(close(a.difference(&b).unwrap().area(), 0.75));
    assert!(close(b.difference(&a).unwrap().area(), 0.75));
}

#[test]
fn outward_offset_grows_a_square_with_rounded_corners() {
    let r = Region::new(vec![rect(0.0, 0.0, 10.0, 10.0)]).unwrap();
    let o = r.offset(2.0).unwrap();
    // Exact is 14x14 minus four corner fillets of (4 - pi) each.
    let exact = 196.0 - 4.0 * (4.0 - std::f64::consts::PI);
    assert!((o.area() - exact).abs() < 0.5, "{} vs {exact}", o.area());
    assert!(o.contains(Point2::new(-1.9, 5.0)));
    assert!(!o.contains(Point2::new(-2.1, 5.0)));
}

#[test]
fn inward_offset_shrinks_a_square_to_the_exact_inset() {
    // A square has no concave vertices, so a round-join inward offset is
    // the exact 6x6 inset.
    let r = Region::new(vec![rect(0.0, 0.0, 10.0, 10.0)]).unwrap();
    let o = r.offset(-2.0).unwrap();
    assert!(close(o.area(), 36.0));
    assert!(o.contains(Point2::new(2.1, 5.0)));
    assert!(!o.contains(Point2::new(1.9, 5.0)));
}

#[test]
fn inward_offset_grows_the_hole_until_the_ring_collapses() {
    // 20x20 with a 4x4 centered hole, shrunk by 2: outer 16x16 sharp
    // (convex corners mitre exactly), hole grown to 8x8 minus four rounded
    // corner fillets of (4 - pi) each - rounded joins cut material corners,
    // and hole corners are material-concave.
    let r = Region::new(vec![rect(0.0, 0.0, 20.0, 20.0), rect(8.0, 8.0, 12.0, 12.0)]).unwrap();
    assert!(close(r.area(), 400.0 - 16.0));
    let o = r.offset(-2.0).unwrap();
    let exact = 256.0 - (64.0 - 4.0 * (4.0 - std::f64::consts::PI));
    assert!((o.area() - exact).abs() < 1.0, "{} vs {exact}", o.area());
    assert!(o.contains(Point2::new(3.0, 3.0)), "ring material kept");
    assert!(!o.contains(Point2::new(9.9, 10.0)), "grown hole is void");
    // Shrinking past the ring collapses it: empty, not an error, not a panic.
    let gone = r.offset(-12.0).unwrap();
    assert!(gone.is_empty() || gone.area() <= 1e-6, "collapse: {:?}", gone.loops());
}

#[test]
fn self_crossing_loops_are_refused_not_area_averaged() {
    // A bowtie is not the boundary of a region; its enclosed "area" depends
    // on convention, which is exactly the silent-wrong-answer failure mode.
    let bowtie = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 2.0),
        Point2::new(2.0, 0.0),
        Point2::new(0.0, 2.0),
    ])
    .unwrap();
    assert!(matches!(
        Region::new(vec![bowtie]),
        Err(GeoError::Invalid(_))
    ));
}

#[test]
fn overlapping_solids_normalize_to_one_region_not_a_phantom_hole() {
    // Two overlapping boxes sliced into one loop set: parity alone would
    // call the overlap a hole. Region::new must normalize.
    let r = Region::new(vec![rect(0.0, 0.0, 1.0, 1.0), rect(0.5, 0.5, 1.5, 1.5)]).unwrap();
    assert!(close(r.area(), 1.75));
    assert!(r.contains(Point2::new(0.75, 0.75)), "overlap is material");
    assert_eq!(r.loops().len(), 1, "the union is one outer boundary");
}

#[test]
fn region_round_trips_keep_membership() {
    let r = Region::new(vec![rect(0.0, 0.0, 10.0, 10.0), rect(4.0, 4.0, 6.0, 6.0)]).unwrap();
    let again = Region::new(r.loops().to_vec()).unwrap();
    for (x, y) in [(1.0, 1.0), (5.0, 5.0), (9.999, 5.0), (10.5, 5.0)] {
        assert_eq!(
            again.contains(Point2::new(x, y)),
            r.contains(Point2::new(x, y)),
            "membership at ({x}, {y})"
        );
    }
    assert!(close(again.area(), r.area()));
}

#[test]
fn out_of_range_and_non_finite_are_refused_not_wrapped() {
    assert_eq!(
        Loop::new(vec![
            Point2::new(1e13, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 0.0),
        ]),
        Err(GeoError::OutOfRange)
    );
    let r = Region::new(vec![rect(0.0, 0.0, 1.0, 1.0)]).unwrap();
    assert!(r.offset(f64::NAN).is_err());
    assert!(r.offset(f64::INFINITY).is_err());
}

#[test]
fn pseudo_random_degenerate_loops_never_panic() {
    // Deterministic LCG: collinear runs, repeated vertices, self-crossing
    // stars - the shapes real geometry hands a wrapper. Any Err is fine;
    // a panic fails the test by crashing it, which is the point.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for case in 0..300 {
        let n = 3 + (case % 9) as usize;
        let span = match case % 3 {
            0 => 4.0,
            1 => 1e-7, // near-degenerate micron-scale dust
            _ => 1e5,  // large machine-envelope coordinates
        };
        let pts: Vec<Point2> = (0..n)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let u = ((state >> 33) as f64 / (1u32 << 31) as f64) - 0.5;
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let v = ((state >> 33) as f64 / (1u32 << 31) as f64) - 0.5;
                Point2::new(u * span, v * span)
            })
            .collect();
        if let Ok(l) = Loop::new(pts.clone()) {
            let _ = l.area();
            let _ = l.is_ccw();
            let _ = l.points().first().is_some_and(|p| !p.x.is_nan());
        }
        if let Ok(r) = Region::new(vec![Loop::new(pts.clone()).unwrap_or_else(|_| rect(0.0, 0.0, 1.0, 1.0))]) {
            let _ = r.area();
            let _ = r.contains(Point2::new(0.0, 0.0));
            let _ = r.offset(-1e-3);
            let _ = r.offset(1e-3);
            let _ = r.union(&r.clone());
            let _ = r.intersect(&r.clone());
            let _ = r.difference(&r.clone());
        }
        // A second loop from the same run, collinear by construction:
        let line = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0), Point2::new(2.0, 2.0)];
        if let Ok(l) = Loop::new(line) {
            let _ = Region::new(vec![l, Loop::new(pts).unwrap_or_else(|_| rect(0.0, 0.0, 1.0, 1.0))]);
        }
    }
}
