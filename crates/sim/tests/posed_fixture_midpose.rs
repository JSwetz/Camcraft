//! The fixture laws (c2)/(d2) on the pose BETWEEN two stations
//! (M13.8's continuous pose): a feed whose two end poses both clear a
//! clamp leans THROUGH it on the letters' lerp between them - the
//! stamp's step from one station to the next, the swarf's in-air link
//! pivoting at the ball centre, a rotary index. The posed sweep walks
//! the mid-move pose law's intervals ([`camcraft_sim::verify`]'s
//! midpose) and encloses every pose inside each interval, so the flank
//! cannot slip between two samples. The same clamp out of the swept
//! cone is clean; a clamp that turns with the part under a traversing
//! index never meets the tool while a fixed one does, each judged in
//! its own frame.
//!
//! Provenance: our own fixtures; the law is the Prime Directive's
//! ("never a silent collision"), the pose rule RS-274's modal letters.
//! No vendor material consulted.
//!
//! Gate map (M13.8 continuous pose): covers camcraft_sim::verify
//! (posed_steel, midpose, workholding, laws).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{bands_vs_fixtures, bands_vs_workholding, FixturePart, RotaryAxis, ToolGeom, Warning, Workholding};

/// Flute 15, shank 20, holder 30: reach 65.
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

/// The radial pose at azimuth `deg` about world x, in the y-z plane.
fn radial(deg: f64) -> [f64; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [0.0, c, s]
}

/// A feed standing at -40 deg, then one feed that carries the tip 20 mm
/// along x while the pose leans from -40 to -140 deg: the lerp passes
/// straight down at mid-travel, where neither end pose points.
fn leaning_feed() -> Toolpath {
    let a = Point3::new(-10.0, 0.0, -25.0);
    let b = Point3::new(10.0, 0.0, -25.0);
    Toolpath::new(vec![
        Move::new(MoveKind::Feed, a, a, Some(500.0)).with_axis(radial(-40.0)),
        Move::new(MoveKind::Feed, a, b, Some(500.0)).with_axis(radial(-140.0)),
    ])
}

/// A clamp straight under the tip's line at z -95..-80: the holder
/// (35..65 below the tip) reaches it only within a few degrees of
/// straight down - a pose neither end of the feed holds.
fn clamp(dx: f64) -> FixtureBox {
    FixtureBox { min: Point3::new(dx - 5.0, -8.0, -95.0), max: Point3::new(dx + 5.0, 8.0, -80.0) }
}

#[test]
fn a_feed_whose_end_poses_clear_the_clamp_convicts_on_the_lerp_between() {
    let w = bands_vs_fixtures(&leaning_feed(), &tool(), &[clamp(0.0)], None, 0.01);
    assert_eq!(
        w,
        vec![Warning::FixtureHit { at: 1, fixture_part: FixturePart::Holder }],
        "the holder through the clamp at mid-lean must convict the leaning move, once: {w:?}"
    );
}

#[test]
fn the_same_clamp_outside_the_swept_cone_is_clean() {
    let path = leaning_feed();
    assert!(bands_vs_fixtures(&path, &tool(), &[clamp(200.0)], None, 0.01).is_empty());
    // Above the tip's line, where the stack never points.
    let above = FixtureBox { min: Point3::new(-5.0, -8.0, 40.0), max: Point3::new(5.0, 8.0, 55.0) };
    assert!(bands_vs_fixtures(&path, &tool(), &[above], None, 0.01).is_empty());
}

/// The bar's axis: world x through the origin.
fn bar_axis() -> RotaryAxis {
    RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] }
}

/// An index of 90 deg while the tip traverses the bar: upright at
/// (-50, 0, 30), then to (50, -30, 0) pointing -y - on the machine the
/// tool stays upright above the bar and slides 100 mm along x while
/// the rotary turns the part a quarter turn under it.
fn traversing_index() -> Toolpath {
    let a = Point3::new(-50.0, 0.0, 30.0);
    let b = Point3::new(50.0, -30.0, 0.0);
    Toolpath::new(vec![
        Move::new(MoveKind::Feed, a, a, Some(500.0)).with_axis([0.0, 0.0, 1.0]),
        Move::new(MoveKind::Feed, a, b, Some(500.0)).with_axis([0.0, -1.0, 0.0]),
    ])
}

/// A block above the bar's middle at z 68..78, stated at rotary zero:
/// on the machine the upright holder (35..65 above a tip at radius
/// 21..30) passes through it at mid-traverse; in the part frame the
/// stack leans 45 deg outward there and is nowhere near it.
fn block() -> FixtureBox {
    FixtureBox { min: Point3::new(-5.0, -5.0, 68.0), max: Point3::new(5.0, 5.0, 78.0) }
}

#[test]
fn under_a_traversing_index_the_fixed_clamp_meets_the_tool_and_the_turning_one_does_not() {
    let path = traversing_index();
    let b = [block()];
    // Fixed (a tailstock, the table): judged in the machine frame, where
    // the tool stands upright and slides through it - a hit at the
    // index, with the holder.
    let fixed = Workholding { turning: &[], fixed: &b, axis: Some(bar_axis()) };
    let w = bands_vs_workholding(&path, &tool(), &fixed, None, 0.01);
    assert_eq!(
        w,
        vec![Warning::FixtureHit { at: 1, fixture_part: FixturePart::Holder }],
        "the fixed block in the machine frame must convict the index: {w:?}"
    );
    // Turning with the part (a chuck): judged in the part frame, where
    // the leaning stack points outward past it - clean.
    let turning = Workholding { turning: &b, fixed: &[], axis: Some(bar_axis()) };
    let w = bands_vs_workholding(&path, &tool(), &turning, None, 0.01);
    assert!(w.is_empty(), "a clamp that turns with the part never meets the tool: {w:?}");
    assert!(bands_vs_fixtures(&path, &tool(), &b, None, 0.01).is_empty());
}
