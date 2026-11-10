//! #637 step A: a field carried through a setup change's lattice turn is
//! the same field seen through the turn - the carried distance at T(p) is
//! the original distance at p, exactly, for every one of the eight turns
//! and their compositions, cut layers included.
//!
//! Gate map (#637): covers sim::stock_sim::field_carry (CarriedField),
//! carry::LatticeTurn (unapply, then).
//! Provenance: rigid-motion invariance of the Euclidean distance; our own
//! fixtures.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Backend, CarriedField, LatticeTurn, SimOp, Stock, FIELD_BAND};

/// The eight lattice turns (Z on +-Z, no mirror), each with a shift.
fn turns() -> Vec<LatticeTurn> {
    let mut out = Vec::new();
    for (r, t) in [
        ([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [3.0, -2.0, 1.0]),
        ([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], [10.0, 0.0, 0.0]),
        ([[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]], [20.0, 12.0, 0.0]),
        ([[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], [0.0, 20.0, 0.0]),
        ([[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]], [0.0, 12.0, 5.0]),
        ([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]], [20.0, 0.0, 5.0]),
        ([[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]], [0.0, 0.0, 5.0]),
        ([[0.0, -1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0]], [12.0, 20.0, 5.0]),
    ] {
        out.push(LatticeTurn::from_matrix(r, t).unwrap_or_else(|e| panic!("turn {r:?}: {e}")));
    }
    out
}

/// A 20 x 12 x 5 block with a slot cut across it - a field with a cut layer.
fn cut_field() -> camcraft_sim::stock_sim::SdfField {
    let mut s = Stock::block(Backend::Field { band: FIELD_BAND }, 0.0, 0.0, 20.0, 12.0, 5.0, 0.5).expect("block");
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let moves = [
        Move::new(MoveKind::Rapid, p(-4.0, 6.0, 8.0), p(-4.0, 6.0, 3.0), None),
        Move::new(MoveKind::Feed, p(-4.0, 6.0, 3.0), p(24.0, 6.0, 3.0), Some(500.0)),
    ];
    s.simulate(&[SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }]).expect("cut");
    s.field().expect("a field").clone()
}

fn samples() -> Vec<Point3> {
    let mut v = Vec::new();
    for i in -2..=22 {
        for j in -2..=14 {
            for k in -2..=7 {
                v.push(Point3::new(f64::from(i) * 1.0 + 0.3, f64::from(j) * 1.0 + 0.2, f64::from(k) * 1.0 + 0.1));
            }
        }
    }
    v
}

#[test]
fn unapply_inverts_apply_and_then_composes() {
    let pts = samples();
    for a in turns() {
        for p in &pts {
            let q = a.unapply(a.apply(*p));
            assert!((q.x - p.x).abs() < 1e-12 && (q.y - p.y).abs() < 1e-12 && (q.z - p.z).abs() < 1e-12, "{a:?}: {p:?} -> {q:?}");
        }
        for b in turns() {
            let ab = a.then(b);
            for p in pts.iter().step_by(37) {
                let (x, y) = (ab.apply(*p), b.apply(a.apply(*p)));
                assert!((x.x - y.x).abs() < 1e-12 && (x.y - y.y).abs() < 1e-12 && (x.z - y.z).abs() < 1e-12, "{a:?} then {b:?}");
            }
        }
    }
}

#[test]
fn a_carried_field_reads_the_same_solid_through_every_turn() {
    let f = cut_field();
    let pts = samples();
    for t in turns() {
        let c = CarriedField::new(f.clone(), t);
        for p in &pts {
            let q = t.apply(*p);
            assert_eq!(c.distance(q).to_bits(), f.distance(*p).to_bits(), "{t:?} at {p:?}");
            assert_eq!(c.mesh_distance(q).to_bits(), f.mesh_distance(*p).to_bits(), "{t:?} mesh at {p:?}");
            assert_eq!(c.inside(q), f.inside(*p), "{t:?} parity at {p:?}");
        }
    }
}

#[test]
fn carrying_twice_is_carrying_through_the_composed_turn() {
    let f = cut_field();
    let ts = turns();
    let (a, b) = (ts[1], ts[5]);
    let twice = CarriedField::new(f.clone(), a).then(b);
    let once = CarriedField::new(f, a.then(b));
    for p in samples().iter().step_by(11) {
        assert_eq!(twice.distance(*p).to_bits(), once.distance(*p).to_bits(), "{p:?}");
    }
}

#[test]
fn a_clip_intersects_the_carried_solid_with_the_next_block() {
    // #637 step C: the next setup's block clips the carried body -
    // max(d_body, d_block); inside only where both hold material.
    use camcraft_sim::stock_sim::box_signed;
    let f = cut_field();
    let t = turns()[0];
    let (lo, hi) = (Point3::new(5.0, 0.0, 0.0), Point3::new(15.0, 14.0, 9.0));
    let c = CarriedField::new(f.clone(), t).clipped(lo, hi).expect("a finite block");
    for p in samples() {
        let body = f.distance(t.unapply(p));
        let block = box_signed(lo, hi, p);
        assert_eq!(c.distance(p).to_bits(), body.max(block).to_bits(), "{p:?}");
        assert_eq!(c.inside(p), f.inside(t.unapply(p)) && block < 0.0, "{p:?}");
    }
    // A second clip narrows, never widens.
    let narrow = c.clone().clipped(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 14.0, 9.0)).expect("a finite block");
    let p = Point3::new(12.3, 4.2, 2.1);
    assert!(narrow.distance(p) >= c.distance(p) && narrow.distance(p) > 0.0, "x 12.3 is outside the narrowed block");
}

#[test]
fn box_signed_is_negative_inside_and_euclidean_outside() {
    use camcraft_sim::stock_sim::box_signed;
    let (lo, hi) = (Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 2.0, 2.0));
    assert_eq!(box_signed(lo, hi, Point3::new(1.0, 1.0, 1.0)), -1.0, "1 from the nearest face (y and z)");
    assert_eq!(box_signed(lo, hi, Point3::new(7.0, 1.0, 1.0)), 3.0);
    assert!((box_signed(lo, hi, Point3::new(7.0, 6.0, 1.0)) - 5.0).abs() < 1e-12, "a 3-4-5 corner");
    assert_eq!(box_signed(lo, hi, Point3::new(4.0, 1.0, 1.0)), 0.0, "on a face");
}

#[test]
fn a_nan_point_stays_nan_through_the_clip() {
    // Review of #651: f64::max dropped the NaN, so a NaN point read deep
    // inside (-inf) once a clip was set.
    use camcraft_sim::stock_sim::box_signed;
    let (lo, hi) = (Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 2.0, 2.0));
    assert!(box_signed(lo, hi, Point3::new(f64::NAN, 1.0, 1.0)).is_nan());
    assert!(box_signed(lo, hi, Point3::new(f64::NAN, f64::NAN, f64::NAN)).is_nan());
    let c = CarriedField::new(cut_field(), turns()[0]).clipped(lo, hi).expect("a finite block");
    for p in [Point3::new(f64::NAN, 1.0, 1.0), Point3::new(f64::NAN, f64::NAN, f64::NAN)] {
        assert!(c.distance(p).is_nan(), "{p:?} -> {}", c.distance(p));
        assert!(!c.inside(p));
    }
}

#[test]
fn two_clips_that_miss_stay_empty_through_a_turn() {
    // Review of #651: an empty intersection re-sorted by `then` became a
    // real box, so material reappeared after a turn.
    use camcraft_sim::stock_sim::Clip;
    let f = cut_field();
    let c = CarriedField::new(f, turns()[0])
        .clipped(Point3::new(0.0, 0.0, 0.0), Point3::new(5.0, 14.0, 9.0))
        .and_then(|c| c.clipped(Point3::new(10.0, 0.0, 0.0), Point3::new(20.0, 14.0, 9.0)))
        .expect("finite blocks");
    assert_eq!(c.clip, Some(Clip::Empty));
    for t in turns() {
        let d = c.clone().then(t);
        assert_eq!(d.clip, Some(Clip::Empty), "{t:?}");
        for p in samples() {
            assert!(!d.inside(p) && d.distance(p) > 0.0, "{t:?} {p:?}");
        }
    }
}

#[test]
fn a_non_finite_or_inverted_clip_is_refused_by_name() {
    use camcraft_sim::stock_sim::ClipRefusal;
    let c = || CarriedField::new(cut_field(), turns()[0]);
    let ok = Point3::new(20.0, 14.0, 9.0);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(c().clipped(Point3::new(bad, 0.0, 0.0), ok).err(), Some(ClipRefusal::NonFinite));
        assert_eq!(c().clipped(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, bad, 9.0)).err(), Some(ClipRefusal::NonFinite));
    }
    assert_eq!(c().clipped(ok, Point3::new(0.0, 0.0, 0.0)).err(), Some(ClipRefusal::Inverted));
}
