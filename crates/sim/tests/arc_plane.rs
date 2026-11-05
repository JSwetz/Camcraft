//! M11.2 A: XZ / YZ arcs on the timeline and in the stock. The
//! timeline times a vertical arc by its in-plane length (a helix's
//! over the normal rise) and walks it by angle; the dexel cuts the
//! arc's own 3-D chords - a dip below both endpoints is cut, never
//! read off the endpoints' Z. Provenance: RS-274/NGC G18/G19
//! semantics (public standard); our own engines.
//!
//! Gate map (M4.4): covers camcraft_sim::sim, camcraft_sim::stock_sim, camcraft_sim::cl.

use std::f64::consts::PI;

use camcraft_sim::cl::{ArcPlane, Move, MoveKind, PathBuilder, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::sim::{script, Traverse};
use camcraft_sim::stock_sim::Dexel;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn near(a: Point3, b: Point3, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol && (a.z - b.z).abs() <= tol
}

#[test]
fn a_g18_quarter_is_timed_by_its_arc_and_walked_by_angle() {
    // G2 in XZ about (X0, Z0): (X10, Z0) -> (X0, Z10), feed chosen so
    // the quarter (length 5 pi) takes 0.6 s.
    let feed = 100.0 * 10.0 * PI / 2.0;
    let m = Move::new(MoveKind::Feed, p(10.0, 0.0, 0.0), p(0.0, 0.0, 10.0), Some(feed))
        .with_arc_in(Point2::new(0.0, 0.0), false, ArcPlane::Xz);
    let t = Toolpath::new(vec![m]);
    let sc = script([("A", &t)], &Traverse::default());
    assert!((sc.total - 0.6).abs() < 1e-9, "timed by the arc: {}", sc.total);
    let h = 10.0 / 2f64.sqrt();
    let mid = sc.at(0.3).pos;
    assert!(near(mid, p(h, 0.0, h), 1e-9), "45 degrees in XZ, not a chord midpoint: {mid:?}");
    for k in 0..=10 {
        let q = sc.at(0.6 * k as f64 / 10.0).pos;
        assert!(((q.x * q.x + q.z * q.z).sqrt() - 10.0).abs() < 1e-9, "on the circle");
        assert!(q.y == 0.0, "the plane holds Y");
    }
}

#[test]
fn a_g19_helix_is_timed_over_its_x_rise() {
    // G3 in YZ about (Y0, Z0), quarter radius 10, X rising 5.
    let len = ((10.0 * PI / 2.0).powi(2) + 25.0).sqrt();
    let m = Move::new(MoveKind::Feed, p(0.0, 10.0, 0.0), p(5.0, 0.0, 10.0), Some(60.0 * len))
        .with_arc_in(Point2::new(0.0, 0.0), true, ArcPlane::Yz);
    let t = Toolpath::new(vec![m]);
    let sc = script([("H", &t)], &Traverse::default());
    assert!((sc.total - 1.0).abs() < 1e-9, "60 s/min x length / feed = 1 s: {}", sc.total);
    let h = 10.0 / 2f64.sqrt();
    assert!(near(sc.at(0.5).pos, p(2.5, h, h), 1e-9));
}

#[test]
fn the_dexel_cuts_a_vertical_dip_at_its_bottom() {
    // G2 in XZ about (X5, Z4), r = 3: (2, 5, 4) -> (8, 5, 4) dips to
    // Z1 at X5. The builder splits it at the bottom; the stock must
    // be cut there - endpoint Z (4) everywhere would be the silent lie.
    let mut b = PathBuilder::new(p(2.0, 5.0, 4.0));
    b.push_arc_in(MoveKind::Feed, p(8.0, 5.0, 4.0), Some(500.0), Point2::new(5.0, 4.0), false, ArcPlane::Xz);
    let t = b.finish();
    assert_eq!(t.moves.len(), 2, "split at the bottom");
    assert_eq!(t.moves[0].to, p(5.0, 5.0, 1.0));
    // One row of 2 mm cells along Y = 5: centers X 1, 3, 5, 7, 9.
    let mut d = match Dexel::block(0.0, 4.0, 10.0, 6.0, 6.0, 2.0) {
        Ok(d) => d,
        Err(e) => panic!("block: {e:?}"),
    };
    for m in &t.moves {
        d.subtract(m, 1.0, 0.001);
    }
    let top = |ix: usize| d.top_at(ix);
    // X5: reach [4, 6] holds the bottom Z1.
    assert!((top(2) - 1.0).abs() < 2e-3, "bottom cut: {}", top(2));
    // X3 / X7: reach ends 1 from the bottom: Z = 4 - sqrt(9 - 1).
    let side = 4.0 - 8f64.sqrt();
    assert!((top(1) - side).abs() < 2e-3, "X3: {}", top(1));
    assert!((top(3) - side).abs() < 2e-3, "X7: {}", top(3));
    // X1 / X9: reach only the endpoints at Z4.
    assert!((top(0) - 4.0).abs() < 1e-9 && (top(4) - 4.0).abs() < 1e-9);
}
