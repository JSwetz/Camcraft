//! The fixture laws (c2)/(d2) on posed links (M13.8): a radial index
//! under a bar's equator carries the shank and holder through steel the
//! tip never comes near. The prismatic and lean-enclosure laws read a
//! rapid's flute only; the posed sweep judges the whole stack along the
//! real pose. Upright programs keep their verdicts.
//! Provenance: our own fixtures; the law is the Prime Directive's.

use camcraft_sim::cl::{angles_to_axis, axis_to_angles, Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{
    bands_vs_fixtures, bands_vs_workholding, rapids_vs_fixtures, rapids_vs_fixtures_spindle, rapids_vs_workholding,
    FixturePart, RotaryAxis, ToolGeom, Warning, Workholding,
};

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 15.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 10.0,
        holder_len: 30.0,
    }
}

/// The tip and the radial pose at azimuth `deg` (about world x, in the
/// y-z plane) and tip radius `rho`.
fn at(x: f64, rho: f64, deg: f64) -> (Point3, [f64; 3]) {
    let (s, c) = deg.to_radians().sin_cos();
    (Point3::new(x, rho * c, rho * s), [0.0, c, s])
}

/// Pass at -40 deg, radial retract to r25, index the long way under
/// the bar to -140 deg in 2-deg chords, radial plunge back to r6.
fn link_under_the_bar() -> Toolpath {
    let mut v = Vec::new();
    let (a, pa) = at(0.0, 6.0, -40.0);
    let (b, _) = at(0.0, 25.0, -40.0);
    v.push(Move::new(MoveKind::Feed, a, b, Some(500.0)).with_axis(pa));
    let mut prev = b;
    for k in 1..=50 {
        let (p, q) = at(0.0, 25.0, -40.0 - 2.0 * k as f64);
        v.push(Move::new(MoveKind::Rapid, prev, p, None).with_axis(q));
        prev = p;
    }
    let (c, pc) = at(0.0, 6.0, -140.0);
    v.push(Move::new(MoveKind::Plunge, prev, c, Some(100.0)).with_axis(pc));
    Toolpath::new(v)
}

/// A chuck block straight under the bar, where the holder passes at
/// -90 deg (radius 60..90 from the axis) - the tip (r25) and the flute
/// (r25..40) never reach it, nor do the retract and plunge at +-50 deg
/// off the bottom.
fn chuck(dx: f64) -> FixtureBox {
    FixtureBox { min: Point3::new(dx - 5.0, -8.0, -95.0), max: Point3::new(dx + 5.0, 8.0, -55.0) }
}

#[test]
fn a_downward_index_sweeps_its_holder_through_the_chuck() {
    let path = link_under_the_bar();
    let w = rapids_vs_fixtures(&path, &tool(), &[chuck(0.0)]);
    assert!(
        w.iter().any(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Rapid, .. })),
        "the index's holder through the chuck must convict: {w:?}"
    );
    // The hits sit on the index chords near the bottom, not the ends.
    for x in &w {
        if let Warning::FixtureHit { at, .. } = x {
            assert!((10..=41).contains(at), "hit at move {at} - not under the bar");
        }
    }
    let b = bands_vs_fixtures(&path, &tool(), &[chuck(0.0)], None, 0.01);
    assert!(b.is_empty(), "the radial retract and plunge stay clear of the block: {b:?}");
}

#[test]
fn the_same_link_with_the_chuck_moved_clear_is_silent() {
    let path = link_under_the_bar();
    assert!(rapids_vs_fixtures(&path, &tool(), &[chuck(200.0)]).is_empty());
    assert!(bands_vs_fixtures(&path, &tool(), &[chuck(200.0)], None, 0.01).is_empty());
}

#[test]
fn a_turning_move_is_swept_through_its_pose_change_not_its_ends() {
    // One rapid turning the pose from -40 to -140 deg about a fixed tip
    // far from the bar: neither end's stack meets the block, the pose
    // lerped through -90 sweeps the holder through it.
    let tip = Point3::new(0.0, 0.0, -25.0);
    let (_, p0) = at(0.0, 1.0, -40.0);
    let (_, p1) = at(0.0, 1.0, -140.0);
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, tip, tip, None).with_axis(p0),
        Move::new(MoveKind::Rapid, tip, tip, None).with_axis(p1),
    ]);
    // The letters' lerp passes the straight-down pose on the way.
    let mid = angles_to_axis({
        let (a, b) = (axis_to_angles(p0), axis_to_angles(p1));
        [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
    })
    .expect("a pose");
    assert!(mid[2] < -0.9, "the lerp turns through the bottom: {mid:?}");
    let block = FixtureBox { min: Point3::new(-5.0, -8.0, -95.0), max: Point3::new(5.0, 8.0, -80.0) };
    let w = rapids_vs_fixtures(&path, &tool(), &[block]);
    assert_eq!(w.len(), 1, "the turn convicts once, the standing first record not: {w:?}");
}

#[test]
fn upright_programs_keep_their_verdicts() {
    // A rapid hop at z 10 over a block topped at 0, and a feed at z 1
    // over it, both upright: silent, as before the posed sweep.
    let block = FixtureBox { min: Point3::new(-5.0, -5.0, -20.0), max: Point3::new(5.0, 5.0, 0.0) };
    let path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, Point3::new(-20.0, 0.0, 10.0), Point3::new(20.0, 0.0, 10.0), None),
        Move::new(MoveKind::Feed, Point3::new(20.0, 0.0, 1.0), Point3::new(-20.0, 0.0, 1.0), Some(500.0))
            .with_axis([0.0, 0.0, 1.0]),
    ]);
    assert!(rapids_vs_fixtures(&path, &tool(), &[block]).is_empty());
    assert!(bands_vs_fixtures(&path, &tool(), &[block], None, 0.01).is_empty());
    // The same rapid dipping through the block still convicts.
    let dip = Toolpath::new(vec![Move::new(MoveKind::Rapid, Point3::new(-20.0, 0.0, -5.0), Point3::new(20.0, 0.0, -5.0), None)]);
    assert_eq!(rapids_vs_fixtures(&dip, &tool(), &[block]).len(), 1);
}

/// A block straight under the bar at radius 110..130: past the holder's
/// reach on the index (r60..90 plus its cap), inside the spindle
/// housing's (r90..140) - only the nose sweeps it.
fn deep_block(dx: f64) -> FixtureBox {
    FixtureBox { min: Point3::new(dx - 5.0, -8.0, -130.0), max: Point3::new(dx + 5.0, 8.0, -110.0) }
}

/// The housing: radius 40, length 50, riding on the holder's top.
const SPINDLE: Option<(f64, f64)> = Some((40.0, 50.0));

#[test]
fn a_posed_rapid_sweeps_its_spindle_nose_through_the_block() {
    let path = link_under_the_bar();
    let w = rapids_vs_fixtures_spindle(&path, &tool(), &[deep_block(0.0)], SPINDLE);
    assert!(
        w.iter().any(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Rapid, .. })),
        "the index's spindle nose through the block must convict: {w:?}"
    );
    // The same block with no housing stated: flute, shank and holder
    // never reach it.
    assert!(rapids_vs_fixtures_spindle(&path, &tool(), &[deep_block(0.0)], None).is_empty());
    assert!(rapids_vs_fixtures(&path, &tool(), &[deep_block(0.0)]).is_empty());
    // The housing stated, the block moved clear: silent.
    assert!(rapids_vs_fixtures_spindle(&path, &tool(), &[deep_block(200.0)], SPINDLE).is_empty());
    // The split law carries the housing the same way.
    let wh = Workholding { turning: &[deep_block(0.0)], fixed: &[], axis: None };
    assert!(!rapids_vs_workholding(&path, &tool(), &wh, SPINDLE).is_empty());
}

/// The bar's axis: world x through the origin (the link's own).
fn bar_axis() -> RotaryAxis {
    RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] }
}

/// A table under the bar, topped at z -40: in the PATH frame the link
/// under the bar drives the whole stack through it; on the machine the
/// rotary turns the part so the tool always stands upright above.
fn table() -> FixtureBox {
    FixtureBox { min: Point3::new(-50.0, -100.0, -200.0), max: Point3::new(50.0, 100.0, -40.0) }
}

#[test]
fn a_fixed_table_is_judged_in_the_machine_frame() {
    let path = link_under_the_bar();
    let t = [table()];
    // Fixed: the rotary stands every pose upright above the bar - clean.
    let fixed = Workholding { turning: &[], fixed: &t, axis: Some(bar_axis()) };
    let r = rapids_vs_workholding(&path, &tool(), &fixed, SPINDLE);
    let b = bands_vs_workholding(&path, &tool(), &fixed, SPINDLE, 0.01);
    assert!(r.is_empty(), "a fixed table is clear in the machine frame: {r:?}");
    assert!(b.is_empty(), "a fixed table is clear in the machine frame: {b:?}");
    // Turning with the part (a chuck-mounted plate): the path frame
    // convicts, exactly as the old law does.
    let turning = Workholding { turning: &t, fixed: &[], axis: Some(bar_axis()) };
    let r = rapids_vs_workholding(&path, &tool(), &turning, SPINDLE);
    assert!(!r.is_empty(), "a turning box under the bar convicts the index");
    assert_eq!(r, rapids_vs_fixtures_spindle(&path, &tool(), &t, SPINDLE));
    assert_eq!(
        bands_vs_workholding(&path, &tool(), &turning, SPINDLE, 0.01),
        bands_vs_fixtures(&path, &tool(), &t, SPINDLE, 0.01)
    );
    // Fixed but no axis stated: no machine frame - the part-frame verdict.
    let blind = Workholding { turning: &[], fixed: &t, axis: None };
    assert_eq!(rapids_vs_workholding(&path, &tool(), &blind, SPINDLE), r);
}

#[test]
fn a_fixed_box_the_machine_frame_tool_meets_still_convicts() {
    // A tailstock-like box above the bar where the upright holder stands
    // on the machine (tip z 25, holder z 60..90): fixed, it convicts on
    // the index; in the path frame the swinging stack is under the bar,
    // nowhere near the box's own rapids.
    let path = link_under_the_bar();
    let above = [FixtureBox { min: Point3::new(-5.0, -8.0, 70.0), max: Point3::new(5.0, 8.0, 80.0) }];
    let fixed = Workholding { turning: &[], fixed: &above, axis: Some(bar_axis()) };
    let r = rapids_vs_workholding(&path, &tool(), &fixed, None);
    assert!(
        r.iter().any(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Rapid, .. })),
        "the machine-frame holder through the fixed box must convict: {r:?}"
    );
    // An upright move keeps the part-frame law (rotary at zero): a
    // rapid dipping through a fixed block convicts.
    let block = [FixtureBox { min: Point3::new(-5.0, -5.0, -20.0), max: Point3::new(5.0, 5.0, 0.0) }];
    let dip = Toolpath::new(vec![Move::new(MoveKind::Rapid, Point3::new(-20.0, 0.0, -5.0), Point3::new(20.0, 0.0, -5.0), None)]);
    let wh = Workholding { turning: &[], fixed: &block, axis: Some(bar_axis()) };
    assert_eq!(rapids_vs_workholding(&dip, &tool(), &wh, None).len(), 1);
}
