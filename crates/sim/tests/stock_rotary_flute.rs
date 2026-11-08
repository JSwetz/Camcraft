//! The flute-rotation agreement suite (#20, M13.1): the rotary chord
//! pre-pass stands each sub-chord's flute on the sub-chord's mid pose,
//! so a tool point at distance `w` from the tool's reference is off
//! its true-pose place by up to `w·2·sin(half/2)`. The pre-pass states
//! that as `RotaryChordStats::flute_shift` (and enforces a flute
//! tolerance when asked). Here a flat end mill turns 30 degrees around
//! a laid bar and the field is read against a brute-force reference
//! written independently: the tool sampled at fine steps of the
//! controller's lerp (tip on its chord, each letter on its own line),
//! a cylinder per sample. Every point farther from the reference's
//! skin than `center_sag + flute_shift` (plus the reference's own
//! sampling step and `FIELD_VERDICT_EPS`) must read the same
//! steel/air on both - the bound is two-sided - and the chord law's
//! own sag alone must NOT be enough (the bound is not vacuous).
//! Provenance: our own engines; the rigid-motion chord `2·w·sin(β/2)`
//! is Craig, "Introduction to Robotics", ch. 2; nothing from vendor
//! binaries.
//!
//! Gate map (M4.4): covers stock_sim::field_rotary (rotary_chords_law,
//! simulate_rotary_bounded, the flute_shift stat).

use camcraft_sim::cl::{angles_to_axis, axis_to_angles, Move, MoveKind};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock::{stock_mesh, StockShape};
use camcraft_sim::stock_sim::{rotary_chords_law, Backend, ChordLaw, SimOp, Stock, FIELD_BAND, FIELD_VERDICT_EPS};
use camcraft_sim::verify::RotaryAxis;

const CELL: f64 = 0.5;
const BAR_R: f64 = 10.0;
const BAR_L: f64 = 30.0;
const R: f64 = 3.0;
const TIP_RHO: f64 = 6.0;
const STATION: f64 = 15.0;
/// One turning move from -15 to +15 degrees off the crown.
const FROM_DEG: f64 = -15.0;
const TO_DEG: f64 = 15.0;
const POSE_TOL: f64 = 1e-3;
/// Reference samples along the move.
const N_REF: usize = 4000;
/// The prefilter's stride over the samples.
const COARSE: usize = 50;
/// The reference flute: past every bar point the tool can reach.
const FLUTE: f64 = 60.0;

fn axis() -> RotaryAxis {
    RotaryAxis { origin: Point3::ZERO, dir: [1.0, 0.0, 0.0] }
}

fn bar() -> Stock {
    let standing = stock_mesh(&StockShape::Cylinder { diameter: 2.0 * BAR_R, height: BAR_L }).expect("the bar meshes");
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    let laid: Vec<Triangle> = standing.iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect();
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &laid, CELL).expect("the laid bar")
}

fn radial(deg: f64) -> [f64; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [0.0, s, c]
}

fn tip(deg: f64) -> Point3 {
    let (s, c) = deg.to_radians().sin_cos();
    Point3::new(STATION, TIP_RHO * s, TIP_RHO * c)
}

fn program() -> Vec<Move> {
    vec![
        Move { kind: MoveKind::Rapid, from: tip(FROM_DEG), to: tip(FROM_DEG), feed: None, arc: None, axis: Some(radial(FROM_DEG)) },
        Move { kind: MoveKind::Feed, from: tip(FROM_DEG), to: tip(TO_DEG), feed: Some(300.0), arc: None, axis: Some(radial(TO_DEG)) },
    ]
}

fn op(moves: &[Move]) -> [SimOp<'_>; 1] {
    [SimOp { moves, radius: R, ball: false, cone: None, corner: None }]
}

/// The brute-force reference: the flat tool at `N_REF + 1` samples of
/// the controller's lerp.
fn samples() -> Vec<(Point3, [f64; 3])> {
    let (la, lb) = (axis_to_angles(radial(FROM_DEG)), axis_to_angles(radial(TO_DEG)));
    let (a, b) = (tip(FROM_DEG), tip(TO_DEG));
    (0..=N_REF)
        .map(|k| {
            let s = k as f64 / N_REF as f64;
            let l = [la[0] + (lb[0] - la[0]) * s, la[1] + (lb[1] - la[1]) * s, 0.0];
            let p = Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
            (p, angles_to_axis(l).expect("a finite pose"))
        })
        .collect()
}

/// Signed distance to the sampled sweep: negative inside (cut).
fn law(cs: &[(Point3, [f64; 3])], p: Point3) -> f64 {
    let mut best = f64::INFINITY;
    for (c, a) in cs {
        let w = [p.x - c.x, p.y - c.y, p.z - c.z];
        let t = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
        let rho = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2] - t * t).max(0.0).sqrt();
        let (dr, dt) = (rho - R, (-t).max(t - FLUTE));
        let d = if dr > 0.0 || dt > 0.0 { dr.max(0.0).hypot(dt.max(0.0)) } else { dr.max(dt) };
        best = best.min(d);
    }
    best
}

/// The reference's own sampling error: every continuous instance is
/// within half a step of a sample - the tip's step plus the pose
/// step at the farthest bar point.
fn ref_err() -> f64 {
    let (a, b) = (tip(FROM_DEG), tip(TO_DEG));
    let chord = ((b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt();
    // Every point of the sampled tool, the whole flute included: the
    // distance to a set moves by at most the set's largest displacement.
    let reach = FLUTE + R;
    0.5 * (chord + reach * (TO_DEG - FROM_DEG).to_radians()) / N_REF as f64
}

/// Sign misses of the field against the reference at a lattice of
/// bar cell centers around the band: `(checked, worst |law| over the misses)`
/// for points off the reference's skin by more than `margin`.
fn misses(f: &Stock, cs: &[(Point3, [f64; 3])], margin: f64) -> (usize, f64) {
    // The field's own cell centers: `distance` reads the cell's value
    // there, so a point between centers would charge the cell's
    // quantization to the flute.
    let h = CELL;
    let (mut checked, mut worst) = (0usize, 0.0f64);
    let coarse: Vec<_> = cs.iter().step_by(COARSE).copied().collect();
    let slack = 2.0 * COARSE as f64 * ref_err();
    let n = (BAR_R / h) as i32;
    for k in -n..=n {
        for j in -n..=n {
            for i in ((STATION - R - 1.5) / h) as i32..=((STATION + R + 1.5) / h) as i32 {
                let p = Point3::new((i as f64 + 0.5) * h, (j as f64 + 0.5) * h, (k as f64 + 0.5) * h);
                if p.y.hypot(p.z) >= BAR_R - CELL || p.y.hypot(p.z) < TIP_RHO - 2.0 {
                    continue;
                }
                // Prefilter: the sub-sampled sweep's distance less the
                // stride's displacement bound past the margin is steel
                // past the margin on the full sweep too.
                let lc = law(&coarse, p);
                let l = if lc - slack > margin { lc } else { law(cs, p) };
                if l.abs() <= margin {
                    continue;
                }
                checked += 1;
                if (f.distance(p) > 0.0) != (l < 0.0) {
                    worst = worst.max(l.abs());
                }
            }
        }
    }
    (checked, worst)
}

#[test]
fn the_stated_flute_shift_bounds_the_chorded_sweep_both_ways() {
    let mut f = bar();
    let stats = f.simulate_rotary(&op(&program()), &axis(), POSE_TOL).expect("the field chords the turn");
    let cs = samples();
    let eps = ref_err() + FIELD_VERDICT_EPS;
    let bound = stats.center_sag + stats.flute_shift + eps;
    println!(
        "flute bound (unenforced): {} sub-chords, half-angle {:.3e}, center sag {:.3e}, flute shift {:.3e}, ref err {:.3e}",
        stats.sub_chords, stats.half_angle, stats.center_sag, stats.flute_shift, ref_err()
    );
    assert!(stats.flute_shift > 0.0 && stats.center_sag == 0.0, "a flat tool: exact tip, the flute carries the error");
    let (checked, worst) = misses(&f, &cs, bound);
    println!("  past the stated bound {bound:.3e}: {checked} points, worst miss {worst:.3e}");
    assert!(checked > 10_000, "fixture: the lattice must cover the band");
    assert_eq!(worst, 0.0, "a sign miss past the stated flute bound");
    // Not vacuous: at the chord sag's class alone, the flute is off.
    let (_, worst_sag) = misses(&f, &cs, POSE_TOL + eps);
    println!("  past the sag class {:.3e} alone: worst miss {worst_sag:.3e}", POSE_TOL + eps);
    assert!(worst_sag > POSE_TOL + eps, "the flute rotation must show past the sag class, or the bound is untested");
    assert!(worst_sag <= bound);
}

#[test]
fn an_enforced_flute_tolerance_buys_the_flank_its_class() {
    const FLUTE_TOL: f64 = 0.05;
    let free = bar().simulate_rotary(&op(&program()), &axis(), POSE_TOL).expect("unbounded");
    let mut f = bar();
    let stats = f.simulate_rotary_bounded(&op(&program()), &axis(), POSE_TOL, Some(FLUTE_TOL)).expect("bounded");
    println!(
        "flute bound (enforced {FLUTE_TOL}): {} sub-chords (unbounded {}), flute shift {:.3e} (unbounded {:.3e})",
        stats.sub_chords, free.sub_chords, stats.flute_shift, free.flute_shift
    );
    assert!(stats.flute_shift <= FLUTE_TOL, "the enforced law holds its own tolerance");
    assert!(free.flute_shift > FLUTE_TOL && stats.sub_chords > free.sub_chords, "fixture: the tolerance must bind");
    let cs = samples();
    let bound = FLUTE_TOL + ref_err() + FIELD_VERDICT_EPS;
    let (checked, worst) = misses(&f, &cs, bound);
    println!("  past {bound:.3e}: {checked} points, worst miss {worst:.3e}");
    assert!(checked > 10_000);
    assert_eq!(worst, 0.0, "a sign miss past the enforced flute tolerance");
}

#[test]
fn the_flute_law_refuses_nonsense_and_caps_by_name() {
    let base = ChordLaw { r: R, shank: 50.0, pose_tol: POSE_TOL, ball: false, flute_tol: Some(0.0), reach: None };
    assert!(rotary_chords_law(&program(), &axis(), &base).is_err(), "a zero flute tolerance is refused");
    let nan = ChordLaw { flute_tol: Some(f64::NAN), ..base };
    assert!(rotary_chords_law(&program(), &axis(), &nan).is_err(), "a NaN flute tolerance is refused");
    let fine = ChordLaw { flute_tol: Some(1e-9), ..base };
    let e = rotary_chords_law(&program(), &axis(), &fine).expect_err("past the cap");
    assert!(e.to_string().contains("refused, never coarsened"), "{e}");
    // No reach box: the shank plus the radius is the flute's length.
    let open = ChordLaw { flute_tol: None, ..base };
    let (_, st) = rotary_chords_law(&program(), &axis(), &open).expect("chords");
    let want = (50.0f64 + R) * 2.0 * (0.5 * st.half_angle).sin();
    assert!((st.flute_shift - want).abs() <= 1e-12, "{} vs {want}", st.flute_shift);
}

