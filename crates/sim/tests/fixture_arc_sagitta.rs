//! The fixture laws read an arc's true reach, not its inscribed chords
//! (#491). The box laws sweep an arc along its chords at the chord
//! tolerance; the circle bulges past each chord by up to its sagitta, so
//! a band at its bare radius missed steel standing in that sliver. Each
//! arc chord's band is now padded by what the arc adds to it. Steel
//! beyond the true reach by more than the pad stays clean, straight
//! moves keep their verdicts, and the strict-dip silence still holds on
//! a vertical arc landing tangent on a mounting face.
//!
//! Gate map (M4.4): covers sim::verify::arc_bulge, sim::verify::stack_span,
//! sim::verify::posed_steel, sim::verify::laws.
//! Provenance: our own fixtures; the law is the Prime Directive's.

use std::f64::consts::FRAC_PI_2;

use camcraft_sim::cl::{ArcMove, ArcPlane, Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{bands_vs_fixtures, rapids_vs_fixtures, FixturePart, ToolGeom, Warning};

/// Flute 0..20 r3, shank 20..50 r5, holder 50..90 r20 above the tip.
fn tool() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 5.0,
        shank_len: 30.0,
        holder_radius: 20.0,
        holder_len: 40.0,
    }
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn bx(min: Point3, max: Point3) -> FixtureBox {
    FixtureBox { min, max }
}

fn hit(part: FixturePart) -> Vec<Warning> {
    vec![Warning::FixtureHit { at: 0, fixture_part: part }]
}

const R: f64 = 30.0;
/// Sweep 0.15 rad at R30 and chord tolerance 0.01: three chords of
/// 0.05 rad each, the middle one centred on the arc's apex.
const SWEEP: f64 = 0.15;
const TOL: f64 = 0.01;

/// The sagitta of one 0.05 rad chord at R30: 0.00937 mm.
fn sagitta() -> f64 {
    R * (1.0 - (0.5 * SWEEP / 3.0).cos())
}

/// A ccw XY arc of R30 about the origin, level at `z`, symmetric about
/// its apex at +Y (the tool's true reach there is y = 33).
fn xy_arc(kind: MoveKind, z: f64) -> Move {
    let (a0, a1) = (FRAC_PI_2 - 0.5 * SWEEP, FRAC_PI_2 + 0.5 * SWEEP);
    let mut m = Move::new(kind, p(R * a0.cos(), R * a0.sin(), z), p(R * a1.cos(), R * a1.sin(), z), Some(500.0));
    m.arc = Some(ArcMove { center: Point2::new(0.0, 0.0), ccw: true, plane: ArcPlane::Xy });
    m
}

/// The three chords the laws sweep, middle one at the apex: what the
/// fixtures below are placed against.
#[test]
fn the_apex_sits_mid_chord() {
    let segs = xy_arc(MoveKind::Feed, 0.0).segments3(TOL);
    assert_eq!(segs.len(), 3);
    let (a, b) = segs[1];
    assert!((a.y - b.y).abs() < 1e-12 && (a.x + b.x).abs() < 1e-12);
    assert!((R - a.y - sagitta()).abs() < 1e-12, "chord stands a sagitta in");
    assert!(sagitta() > 0.008 && sagitta() < 0.010);
}

/// The issue's measured repro: a box face half a sagitta inside the
/// tool's true reach at the apex. The chords at the bare radius reach
/// only 33 - sagitta, so the box went unseen; the tool really goes
/// 0.0047 mm into it.
#[test]
fn an_arc_reaching_steel_within_its_sagitta_reports() {
    let face = R + 3.0 - 0.5 * sagitta();
    let jaw = bx(p(-1.0, face, -5.0), p(1.0, face + 10.0, 5.0));
    let path = Toolpath::new(vec![xy_arc(MoveKind::Feed, 0.0)]);
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[jaw], None, TOL), hit(FixturePart::Flute));
    // A rapid on the same arc: law (d2)'s flute column carries the pad.
    let path = Toolpath::new(vec![xy_arc(MoveKind::Rapid, 0.0)]);
    assert_eq!(rapids_vs_fixtures(&path, &tool(), &[jaw]), hit(FixturePart::Rapid));
}

/// Steel past the true reach by more than the pad (the lattice's 1 um
/// plus the chord's along-offset) stays clean.
#[test]
fn steel_beyond_the_true_reach_stays_clean() {
    let face = R + 3.0 + 0.002;
    let jaw = bx(p(-1.0, face, -5.0), p(1.0, face + 10.0, 5.0));
    let path = Toolpath::new(vec![xy_arc(MoveKind::Feed, 0.0)]);
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[jaw], None, TOL), Vec::new());
    let path = Toolpath::new(vec![xy_arc(MoveKind::Rapid, 0.0)]);
    assert_eq!(rapids_vs_fixtures(&path, &tool(), &[jaw]), Vec::new());
}

/// The upper bands ride the same pad: a holder (r20) passing a post
/// within the sagitta of its true reach.
#[test]
fn the_holder_band_is_padded_too() {
    let face = R + 20.0 - 0.5 * sagitta();
    let post = bx(p(-1.0, face, 60.0), p(1.0, face + 10.0, 80.0));
    let path = Toolpath::new(vec![xy_arc(MoveKind::Feed, 0.0)]);
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[post], None, TOL), hit(FixturePart::Holder));
    let clear = bx(p(-1.0, R + 20.002, 60.0), p(1.0, R + 30.0, 80.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[clear], None, TOL), Vec::new());
}

/// An XZ arc bulging toward +X past a jaw: the chord deviation there is
/// horizontal, and pads the band's radius.
#[test]
fn an_xz_arc_bulging_sideways_reports() {
    // In XZ, (u, v) = (z, x): the apex x = 30 is at angle pi/2. Centre
    // (X0, Z10), ccw from pi/2 - 0.075 to pi/2 + 0.075 (z falls).
    let (a0, a1) = (FRAC_PI_2 - 0.5 * SWEEP, FRAC_PI_2 + 0.5 * SWEEP);
    let at = |a: f64| p(R * a.sin(), 0.0, 10.0 + R * a.cos());
    let mut m = Move::new(MoveKind::Feed, at(a0), at(a1), Some(500.0));
    m.arc = Some(ArcMove { center: Point2::new(0.0, 10.0), ccw: true, plane: ArcPlane::Xz });
    assert_eq!(m.segments3(TOL).len(), 3);
    let path = Toolpath::new(vec![m]);
    let face = R + 3.0 - 0.5 * sagitta();
    let jaw = bx(p(face, -1.0, 9.0), p(face + 10.0, 1.0, 11.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[jaw], None, TOL), hit(FixturePart::Flute));
    let clear = bx(p(R + 3.002, -1.0, 9.0), p(R + 13.0, 1.0, 11.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[clear], None, TOL), Vec::new());
}

/// The same in YZ, (u, v) = (y, z): the apex y = 30 at angle 0.
#[test]
fn a_yz_arc_bulging_sideways_reports() {
    let at = |a: f64| p(0.0, R * a.cos(), 10.0 + R * a.sin());
    let mut m = Move::new(MoveKind::Feed, at(-0.5 * SWEEP), at(0.5 * SWEEP), Some(500.0));
    m.arc = Some(ArcMove { center: Point2::new(0.0, 10.0), ccw: true, plane: ArcPlane::Yz });
    assert_eq!(m.segments3(TOL).len(), 3);
    let path = Toolpath::new(vec![m]);
    let face = R + 3.0 - 0.5 * sagitta();
    let jaw = bx(p(-1.0, face, 9.0), p(1.0, face + 10.0, 11.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[jaw], None, TOL), hit(FixturePart::Flute));
    let clear = bx(p(-1.0, R + 3.002, 9.0), p(1.0, R + 13.0, 11.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[clear], None, TOL), Vec::new());
}

/// An XZ lead arc landing tangent on a mounting face keeps the
/// strict-dip silence: its z window is the joints', so the flush
/// landing is contact, not a crash. A thousandth deeper reports.
#[test]
fn an_xz_arc_landing_tangent_on_a_face_stays_silent() {
    // Quarter circle r5 about (X0, Z5): from (-5, 0, 5) cw down to the
    // circle's bottom (0, 0, 0).
    let mut m = Move::new(MoveKind::Feed, p(-5.0, 0.0, 5.0), p(0.0, 0.0, 0.0), Some(500.0));
    m.arc = Some(ArcMove { center: Point2::new(0.0, 5.0), ccw: false, plane: ArcPlane::Xz });
    let path = Toolpath::new(vec![m]);
    let face = bx(p(-10.0, -10.0, -10.0), p(10.0, 10.0, 0.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[face], None, TOL), Vec::new());
    let dip = bx(p(-10.0, -10.0, -10.0), p(10.0, 10.0, 0.001));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[dip], None, TOL), hit(FixturePart::Flute));
}

/// An XZ arc built straight through its circle's bottom (the builder
/// splits these; a hand-built move does not): its bottom chord stands a
/// sagitta above the true low point, and steel in between is struck.
#[test]
fn an_unsplit_xz_arc_dipping_past_its_chords_reports() {
    // Half circle r5 about (X0, Z5), cw from (-5, 5) through the bottom
    // to (5, 5): 25 chords, the middle one centred on the bottom.
    let mut m = Move::new(MoveKind::Feed, p(-5.0, 0.0, 5.0), p(5.0, 0.0, 5.0), Some(500.0));
    m.arc = Some(ArcMove { center: Point2::new(0.0, 5.0), ccw: false, plane: ArcPlane::Xz });
    let segs = m.segments3(TOL);
    let low = segs.iter().map(|(a, b)| a.z.min(b.z)).fold(f64::INFINITY, f64::min);
    assert!(low > 0.008, "the bottom chord stands {low} above the true low point");
    let path = Toolpath::new(vec![m]);
    let face = bx(p(-10.0, -10.0, -10.0), p(10.0, 10.0, 0.5 * low));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[face], None, TOL), hit(FixturePart::Flute));
}

/// A posed arc (constant lean) reads the same pad on its capsules. The
/// posed walk's interval travel (up to a quarter millimetre) already
/// covered most of this sliver; the bulge makes the enclosure hold
/// for intervals shorter than the sagitta too. Pinned, not a repro.
#[test]
fn a_posed_arc_reaching_steel_within_its_sagitta_reports() {
    let lean = [0.0, -0.2, 1.0];
    let m = xy_arc(MoveKind::Feed, 0.0).with_axis(lean);
    let path = Toolpath::new(vec![m]);
    // The flute tip's true reach at the apex, along +Y: 33. A box
    // standing just below the tip, half a sagitta in.
    let face = R + 3.0 - 0.5 * sagitta();
    let jaw = bx(p(-1.0, face, -5.0), p(1.0, face + 10.0, 0.5));
    assert!(!bands_vs_fixtures(&path, &tool(), &[jaw], None, TOL).is_empty());
}

/// Straight moves keep their verdicts sum for sum: a level feed's
/// flute grazing a face exactly at its radius still counts (touching in
/// XY is contact), one a micron clear does not.
#[test]
fn straight_moves_keep_their_verdicts() {
    let path = Toolpath::new(vec![Move::new(MoveKind::Feed, p(-10.0, 30.0, 0.0), p(10.0, 30.0, 0.0), Some(500.0))]);
    let touch = bx(p(-1.0, 33.0, -5.0), p(1.0, 40.0, 5.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[touch], None, TOL), hit(FixturePart::Flute));
    let clear = bx(p(-1.0, 33.000_001, -5.0), p(1.0, 40.0, 5.0));
    assert_eq!(bands_vs_fixtures(&path, &tool(), &[clear], None, TOL), Vec::new());
    let rapid = Toolpath::new(vec![Move::new(MoveKind::Rapid, p(-10.0, 30.0, 0.0), p(10.0, 30.0, 0.0), None)]);
    assert_eq!(rapids_vs_fixtures(&rapid, &tool(), &[clear]), Vec::new());
}
