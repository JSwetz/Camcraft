//! The helix entry as the viewport draws it (user hand-test 2026-11-05:
//! "just arcs on different planes, not a continuous helix"). The CL's
//! helix was always continuous - consecutive half-turn arcs, each with
//! its Z change; the overlay flattened every arc chord to its start Z.
//! These pin the drawn chords to the move: continuous, Z monotone,
//! landing on the move's end. Provenance: our own tests.

use camcraft_app::cl::{MoveKind, PathBuilder};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;

use crate::interact::arc_segments;

#[test]
fn a_helical_arc_draws_as_a_continuous_descent() {
    // Two half turns of a 1 mm radius helix dropping 2 mm, the planar
    // pocket's helix entry in miniature.
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, 0.0));
    b.push_arc(MoveKind::Feed, Point3::new(2.0, 0.0, -1.0), Some(500.0), Point2::new(1.0, 0.0), true);
    b.push_arc(MoveKind::Feed, Point3::new(0.0, 0.0, -2.0), Some(500.0), Point2::new(1.0, 0.0), true);
    let t = b.finish();
    let mut prev_end: Option<Point3> = None;
    for m in &t.moves {
        assert!(m.arc.is_some(), "a helix turn is an arc move");
        assert!(m.to.z < m.from.z, "every half turn carries its own Z change: {m:?}");
        let segs = arc_segments(m);
        assert!(segs.len() > 4, "the arc is chorded");
        let first = segs.first().map(|s| s.0);
        let last = segs.last().map(|s| s.1);
        assert_eq!(first.map(|p| p.z), Some(m.from.z), "the drawing starts at the move's start Z");
        assert!(last.is_some_and(|p| (p.z - m.to.z).abs() < 1e-12), "and lands on its end Z");
        for w in segs.windows(2) {
            assert_eq!(w[0].1, w[1].0, "consecutive chords join");
            assert!(w[1].1.z <= w[1].0.z, "Z never rises along a descending helix");
        }
        if let (Some(p), Some(f)) = (prev_end, first) {
            assert!((p.x - f.x).abs() < 1e-9 && (p.y - f.y).abs() < 1e-9 && (p.z - f.z).abs() < 1e-12,
                "each turn starts where the last one ended");
        }
        prev_end = last;
    }
}

#[test]
fn a_planar_arc_still_draws_on_its_plane() {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, -3.0));
    b.push_arc(MoveKind::Feed, Point3::new(2.0, 0.0, -3.0), Some(500.0), Point2::new(1.0, 0.0), true);
    let t = b.finish();
    for m in &t.moves {
        for (a, c) in arc_segments(m) {
            assert_eq!((a.z, c.z), (-3.0, -3.0));
        }
    }
}
