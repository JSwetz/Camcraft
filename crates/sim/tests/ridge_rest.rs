//! M13.8: the ridge rest law fires. A raster whose spacing leaves a
//! scallop past tolerance is convicted at the cusp, the same raster
//! at a sound spacing is not, and a crease no ball can touch (a 90°
//! V the tangent ball bridges) is excused rather than read as scallop.
//! Before M13.8 the law's band capped its own answer at `tol`, so the
//! wide raster read clean.
//! Provenance: scallop height of adjacent ball passes,
//! `h = r - sqrt(r^2 - (s/2)^2)` (machining handbooks); our own
//! fixtures, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim (envelope scan, ridge law).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim;

const R: f64 = 3.0;
const TOL: f64 = 0.1;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move {
        kind: MoveKind::Feed,
        from,
        to,
        feed: Some(500.0),
        arc: None,
        axis: None,
    }
}

/// The verifier's gate for this scan: tolerance plus the lattice floor.
fn gate(cell: f64) -> f64 {
    TOL + cell * cell / (2.0 * R) + 1e-9
}

/// A flat sheet, top at z = 0, over [-10, 10]^2.
fn plate() -> Vec<Triangle> {
    vec![
        Triangle::new(p(-10., -10., 0.), p(10., -10., 0.), p(10., 10., 0.)),
        Triangle::new(p(-10., -10., 0.), p(10., 10., 0.), p(-10., 10., 0.)),
    ]
}

/// Tip-height raster rows along x across the plate, `s` apart, ends
/// joined serpentine so the path is one chain.
fn raster(s: f64) -> Vec<Move> {
    let n = (20.0 / s).round() as i64;
    let mut out = Vec::new();
    for k in 0..=n {
        let y = -10.0 + s * k as f64;
        let (a, b) = if k % 2 == 0 { (-12.0, 12.0) } else { (12.0, -12.0) };
        out.push(feed(p(a, y, 0.0), p(b, y, 0.0)));
        if k < n {
            out.push(feed(p(b, y, 0.0), p(b, y + s, 0.0)));
        }
    }
    out
}

#[test]
fn a_wide_stepover_leaves_its_scallop_on_the_record() {
    // s = 2.8 on an r3 ball: the cusp stands r - sqrt(r^2 - 1.96) = 0.35 mm
    // tall, and the floor point under it sqrt(r^2 + 1.96) - r = 0.311 mm
    // outside both balls - the distance the law reports. The flank
    // reach is `band + 3·tol` (M13.9's narrower variant): both feet
    // must stand within it, so the law reads scallops up to 4·tol
    // past the ball and needs a lattice column within reach of both
    // passes - at the r/8 cell that is s <= 2.9 here; a spacing wider
    // than that (s = 4, 0.606 mm) is a gap outside this law's reach,
    // the missing-pass law's word, not this one's.
    let rep = stock_sim::envelope_scan(&plate(), &raster(2.8), R, TOL).expect("scan");
    let cusp = (R * R + 1.96f64).sqrt() - R;
    assert!(rep.gouge_mm <= gate(rep.cell), "a flat ride bites nothing: {}", rep.gouge_mm);
    assert!(rep.rest_mm <= gate(rep.cell), "the scallop is its own reading, never envelope rest: {}", rep.rest_mm);
    assert!(rep.scallop_mm > gate(rep.cell), "the scallop must convict: scallop {} vs gate {}", rep.scallop_mm, gate(rep.cell));
    // The lattice reads the cusp from its nearest column, up to half
    // a cell off it, minus the floor the sagitta of one cell hides.
    let floor = rep.cell * rep.cell / (2.0 * R);
    let lo = ((1.4 - rep.cell / 2.0).powi(2) + R * R).sqrt() - R - floor - 1e-6;
    assert!(rep.scallop_mm <= cusp + 1e-6 && rep.scallop_mm >= lo, "scallop {} reads the cusp {cusp} (lattice floor {lo})", rep.scallop_mm);
    // At the cusp: midway between two rows, inside the plate.
    let row = (rep.scallop_at.y + 10.0) / 2.8;
    assert!((row - row.floor() - 0.5).abs() < 0.1, "convicted off the cusp at {:?}", rep.scallop_at);
}

#[test]
fn a_sound_stepover_stays_quiet() {
    // s = 1: the cusp is 0.042 mm, under the 0.1 promise.
    let rep = stock_sim::envelope_scan(&plate(), &raster(1.0), R, TOL).expect("scan");
    assert!(rep.gouge_mm <= gate(rep.cell), "gouge {}", rep.gouge_mm);
    assert!(rep.rest_mm <= gate(rep.cell), "a sound raster convicted: rest {} at {:?}", rep.rest_mm, rep.at);
    assert!(rep.scallop_mm <= gate(rep.cell), "a sound raster read scallop {} at {:?}", rep.scallop_mm, rep.scallop_at);
}

/// A 90° V channel z = |x| over x in [-8, 8], y in [-10, 10].
fn vee() -> Vec<Triangle> {
    let q = |x: f64, y: f64| p(x, y, x.abs());
    vec![
        Triangle::new(q(-8., -10.), q(0., -10.), q(0., 10.)),
        Triangle::new(q(-8., -10.), q(0., 10.), q(-8., 10.)),
        Triangle::new(q(0., -10.), q(8., -10.), q(8., 10.)),
        Triangle::new(q(0., -10.), q(8., 10.), q(0., 10.)),
    ]
}

#[test]
fn a_crease_no_ball_reaches_is_excused_not_scallop() {
    // Rows along the valley 0.5 apart, every centre tangent to the
    // V (z_c = |x| + r·sqrt2): the ball bridges the crease, leaving
    // r(sqrt2 - 1)·sqrt2 = 1.24 mm standing at the bottom, flanked on
    // both sides - and no spacing could ever reach it.
    let mut moves = Vec::new();
    for k in -12..=12 {
        let x = 0.5 * k as f64;
        let z = x.abs() + R * 2f64.sqrt() - R;
        moves.push(feed(p(x, -12.0, z), p(x, 12.0, z)));
    }
    let rep = stock_sim::envelope_scan(&vee(), &moves, R, TOL).expect("scan");
    assert!(rep.gouge_mm <= gate(rep.cell), "tangent rows bite nothing: {}", rep.gouge_mm);
    assert!(rep.rest_mm <= gate(rep.cell), "the crease read as rest: {} at {:?}", rep.rest_mm, rep.at);
    assert!(rep.scallop_mm <= gate(rep.cell), "the crease read as scallop: {} at {:?}", rep.scallop_mm, rep.scallop_at);
    // The same rows with every other one dropped (s = 1.0 along x,
    // 1.41 along the flank, a 0.085 cusp) stay quiet; with three in
    // four dropped (s = 2.0, a 2.83 flank spacing, a 0.35 cusp) the
    // reachable flank convicts.
    let wide: Vec<Move> = moves.iter().step_by(4).copied().collect();
    let rep = stock_sim::envelope_scan(&vee(), &wide, R, TOL).expect("scan");
    assert!(rep.scallop_mm > gate(rep.cell), "the flank's own scallop must convict: {}", rep.scallop_mm);
    assert!(rep.scallop_at.x.abs() > R / 2f64.sqrt(), "convicted inside the crease at {:?}", rep.scallop_at);
}
