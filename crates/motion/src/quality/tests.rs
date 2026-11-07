//! The yardstick, calibrated on cuts whose engagement is known in
//! closed form (Gate map (M11.4 C): covers motion::quality).

use super::*;
use crate::cl::{Move, MoveKind, Toolpath};

fn feed(a: (f64, f64), b: (f64, f64), z: f64) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a.0, a.1, z), Point3::new(b.0, b.1, z), Some(1000.0))
}

/// Plunge at `a` to `z`, then feed to `b`: a pass that starts where the
/// tool has already cleared its own disc.
fn pass(a: (f64, f64), b: (f64, f64), z: f64) -> Toolpath {
    Toolpath::new(vec![
        Move::new(MoveKind::Plunge, Point3::new(a.0, a.1, 1.0), Point3::new(a.0, a.1, z), Some(100.0)),
        feed(a, b, z),
    ])
}

fn block(spec: &QualitySpec) -> Dexel {
    let Ok(dex) = Dexel::block(-10.0, -20.0, 70.0, 20.0, 0.0, spec.cell) else { panic!("stock") };
    dex
}

#[test]
fn a_straight_side_cut_reads_its_width() {
    // A pre-cut channel, then a parallel pass ae = 0.2 D beside it: the
    // rim reads acos((r - ae) / rim) and the load law gives 0.2 back.
    let spec = QualitySpec::new(10.0);
    let mut dex = block(&spec);
    let _ = measure(&pass((0.0, 0.0), (60.0, 0.0), -1.0), &mut dex, &spec);
    let q = measure(&pass((0.0, 2.0), (50.0, 2.0), -1.0), &mut dex, &spec);
    let want = ((5.0 - 2.0) / q.rim).acos().to_degrees();
    assert!((q.max_deg - want).abs() < 4.0, "read {} want {want}", q.max_deg);
    assert!((q.max_load() - 0.2).abs() < 0.02, "load {}", q.max_load());
}

#[test]
fn a_slot_reads_half_the_rim_and_counts_as_slotting() {
    let spec = QualitySpec::new(10.0);
    let mut dex = block(&spec);
    let q = measure(&pass((0.0, 0.0), (60.0, 0.0), -1.0), &mut dex, &spec);
    assert!((q.max_deg - 180.0).abs() < 8.0, "a slot is 180: {}", q.max_deg);
    assert!(q.slot_len > 50.0, "slot length {}", q.slot_len);
    assert!(q.entry_max_deg > 300.0, "the plunge is buried: {}", q.entry_max_deg);
}

#[test]
fn a_sharp_corner_in_cut_is_a_break_and_air_is_air() {
    let spec = QualitySpec::new(10.0);
    let mut dex = block(&spec);
    let mut moves = pass((0.0, 0.0), (20.0, 0.0), -1.0).moves;
    moves.push(feed((20.0, 0.0), (20.0, 15.0), -1.0));
    // back over cleared floor: air - but stopping in the slot and
    // reversing is a break in cut too (the tool stands in stock).
    moves.push(feed((20.0, 15.0), (20.0, 5.0), -1.0));
    moves.push(Move::new(MoveKind::Rapid, Point3::new(20.0, 5.0, -1.0), Point3::new(20.0, 5.0, 5.0), None));
    let q = measure(&Toolpath::new(moves), &mut dex, &spec);
    assert_eq!(q.breaks, 2, "the 90-degree turn and the reversal: {:?}", q.break_list);
    assert!((q.break_list[0].1 - 90.0).abs() < 1e-6 && (q.worst_break_deg - 180.0).abs() < 1e-6);
    assert!(q.air_len > 5.0, "the return is air: {}", q.air_len);
    assert_eq!(q.retracts, 1);
}

#[test]
fn a_strip_on_the_wall_reads_its_width_and_the_bare_wall_reads_air() {
    // A 40 x 20 pocket (walls outside it): a pass 0.3 mm off the wall's
    // center line... the tool rides the wall with a 0.3 mm strip on it.
    let mut spec = QualitySpec::new(10.0);
    spec.cell = 10.0 / 120.0;
    spec.wall_below = Some(-1.5);
    let Ok(reg) = Region::new(vec![crate::geo2d::Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(40.0, 0.0),
        Point2::new(40.0, 20.0),
        Point2::new(0.0, 20.0),
    ])
    .unwrap_or_else(|_| panic!("loop"))]) else {
        panic!("region")
    };
    let Ok(mut dex) = stock_for(&reg, 0.0, -1.0, 1.0, &spec) else { panic!("stock") };
    // Clear everything but a 0.3 mm strip along the bottom wall.
    let _ = measure(&pass((5.0, 5.3), (35.0, 5.3), -0.5), &mut dex, &spec);
    let _ = measure(&pass((5.0, 12.0), (35.0, 12.0), -0.5), &mut dex, &spec);
    let q = measure(&pass((8.0, 5.0), (32.0, 5.0), -0.5), &mut dex, &spec);
    assert!((q.max_load() - 0.03).abs() < 0.01, "a 0.3 mm strip on the wall reads 3% of D: {}", q.max_load());
    assert!(q.air_len < 1.0, "the strip is cut, not air: {}", q.air_len);
    // The same ride again: the wall alone, nothing to cut - air.
    let q = measure(&pass((8.0, 5.0), (32.0, 5.0), -0.5), &mut dex, &spec);
    assert!(q.cut_len < 1.0 && q.air_len > 20.0, "a bare wall is air: cut {} air {}", q.cut_len, q.air_len);
}

#[test]
fn rest_area_counts_what_stands_inside() {
    let spec = QualitySpec::new(10.0);
    let Ok(reg) = Region::square(10.0) else { panic!("square") };
    let Ok(dex) = stock_for(&reg, 0.0, -1.0, 0.0, &spec) else { panic!("stock") };
    let a = rest_area(&dex, &reg, -1.0);
    assert!((a - 100.0).abs() < 3.0, "uncut square stands whole: {a}");
}

#[test]
fn a_lifted_return_over_the_cut_floor_is_lift_not_floor_air() {
    // A slot cut at z = -1, then back over it 0.3 above the floor (a
    // microlift return), then the same return on the floor itself.
    let spec = QualitySpec::new(10.0);
    let mut dex = block(&spec);
    // A 14 mm channel first: the returns below run 6 mm off its walls.
    let _ = measure(&pass((0.0, -2.0), (50.0, -2.0), -1.0), &mut dex, &spec);
    let mut moves = pass((0.0, 2.0), (40.0, 2.0), -1.0).moves;
    moves.push(Move::new(MoveKind::Feed, Point3::new(40.0, 2.0, -1.0), Point3::new(40.0, 0.0, -0.7), Some(1000.0)));
    moves.push(feed((40.0, 0.0), (10.0, 0.0), -0.7));
    moves.push(Move::new(MoveKind::Feed, Point3::new(10.0, 0.0, -0.7), Point3::new(10.0, 0.0, -1.0), Some(1000.0)));
    moves.push(feed((10.0, 0.0), (30.0, 0.0), -1.0));
    let q = measure(&Toolpath::new(moves), &mut dex, &spec);
    assert!(q.cut_len > 30.0, "the second pass bites: {}", q.cut_len);
    assert!((q.lift_len - 30.0).abs() < 1.0, "the lifted return: {}", q.lift_len);
    assert!((q.air_len - 20.0).abs() < 1.0, "the floor return is floor air: {}", q.air_len);
}

#[test]
fn a_chord_vertex_faces_the_curve_and_a_corner_keeps_its_chord() {
    // Issue #10: the vertex between two chords of a curve faces their
    // bisector (the tangent there), not the arriving chord, which lags
    // it by half the turn; a corner past the break limit and the end of
    // a pass keep the arriving chord.
    let out = |deg: f64| Some((deg.to_radians().cos(), deg.to_radians().sin()));
    let Some((x, y)) = vertex_heading((2.0, 0.0), out(4.0), 5.0) else { panic!("a 4 deg chord vertex is a curve") };
    assert!((y.atan2(x).to_degrees() - 2.0).abs() < 1e-9, "heading {}", y.atan2(x).to_degrees());
    assert!(vertex_heading((1.0, 0.0), out(30.0), 5.0).is_none(), "a 30 deg corner keeps its chord");
    assert!(vertex_heading((1.0, 0.0), None, 5.0).is_none(), "a pass end keeps its chord");
    assert!(vertex_heading((0.0, 0.0), out(1.0), 5.0).is_none(), "no arriving direction");
}
