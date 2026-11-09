//! #413: the envelope scan measures XY arcs on the TRUE circle. Its
//! ball-centre path drew a G2 / G3 as chords a sagitta inside the arc,
//! so a ball rolled round a convex corner read phantom gouge, and one
//! that over-cut a concave wall by less than a sagitta read clean. The
//! chords now only index; every distance is read off the arc. Pinned:
//! a tangent ride round a box's outside corner reads no gouge at all,
//! and a wall the true arc dips into by half a sagitta - where the old
//! chord's midpoint stood a full sagitta short of it - reads exactly
//! that dip.
//! Provenance: plane geometry and RS-274 circular interpolation; our
//! own fixtures, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim (envelope scan:
//! scan_arc, gouge, ridge_scan, scan_hidden, scan_near).

use camcraft_sim::cl::{ArcMove, Move, MoveKind, arc_chords};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::envelope_scan;

const R: f64 = 3.0;
const TOL: f64 = 0.04;

/// A level XY arc of the ball TIP about `(ox, oy)`, radius `rad`, from
/// bearing `a0` through `sweep` (CCW positive), tip at `z`.
fn arc(ox: f64, oy: f64, rad: f64, a0: f64, sweep: f64, z: f64) -> Move {
    let a1 = a0 + sweep;
    Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: Point3::new(ox + rad * a0.cos(), oy + rad * a0.sin(), z),
        to: Point3::new(ox + rad * a1.cos(), oy + rad * a1.sin(), z),
        feed: Some(500.0),
        arc: Some(ArcMove::xy(Point2::new(ox, oy), sweep > 0.0)),
        axis: None,
    }
}

#[test]
fn a_ball_rolled_round_an_outside_corner_reads_no_gouge() {
    // The box's vertical edge at (10, 10) IS the arc's centre: every
    // ball of the quarter turn stands exactly `R` from it. The chords'
    // midpoints stood a sagitta (up to TOL/4) nearer - a phantom gouge.
    let model = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0)).facets.to_vec();
    let m = arc(10.0, 10.0, R, 0.0, std::f64::consts::FRAC_PI_2, 5.0 - R);
    let rep = envelope_scan(&model, &[m], R, TOL).expect("scan");
    assert!(rep.gouge_mm < 1e-9, "a tangent corner ride reads {} mm of gouge", rep.gouge_mm);
}

#[test]
fn a_wall_the_true_arc_dips_into_reads_the_dip() {
    // One chord spans the arc (its sweep under the TOL/4 budget), its
    // midpoint at bearing 0, where the true arc reaches farthest in +X.
    // The wall stands half a sagitta inside that reach: the arc's ball
    // cuts it by exactly that, the chord's ball stood clear of it.
    let (ox, oy, rad) = (0.0, 0.0, 6.0);
    let ct = TOL * 0.25;
    let max_step = 2.0 * (1.0 - ct / rad).acos();
    let s = 0.95 * max_step;
    let m = arc(ox, oy, rad, -0.5 * s, s, 5.0 - R);
    let one = arc_chords(Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y), m.arc.expect("arc"), ct);
    assert_eq!(one.len(), 1, "the fixture spans one chord");
    let sag = rad * (1.0 - (0.5 * s).cos());
    let dip = 0.5 * sag;
    let wall = ox + rad + R - dip;
    let model = cube(Point3::new(wall, -20.0, 0.0), Point3::new(wall + 10.0, 20.0, 10.0)).facets.to_vec();
    let rep = envelope_scan(&model, &[m], R, TOL).expect("scan");
    assert!((rep.gouge_mm - dip).abs() < 1e-7, "the dip reads {} mm, want {dip}", rep.gouge_mm);
}
