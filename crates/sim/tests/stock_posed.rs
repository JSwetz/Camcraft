//! Posed (tilted-tool) moves on the two stock engines: the column arm's
//! atoms stand upright on +Z, so a move whose `axis` leans is REFUSED
//! there by name - never cut as if vertical - while the field arm cuts
//! the leaning solid itself, checked here against a hand case whose
//! answer differs from the upright cut at every probe point.
//! Provenance: our own hand geometry (a flat end mill laid along +X,
//! a cylinder about the x axis); nothing from vendor material.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::cut (the posed
//! refusal) and camcraft_sim::stock_sim::field_cut (the posed cut).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{first_posed, simulate, Backend, Dexel, SimOp, Stock, FIELD_BAND, POSED_REFUSAL};

fn mv(kind: MoveKind, from: Point3, to: Point3, axis: Option<[f64; 3]>) -> Move {
    Move { kind, from, to, feed: Some(500.0), arc: None, axis }
}

/// A 2 mm-radius flat tool lying along +X (tip toward -X), its tip
/// sliding y 8 -> 12 at x 5, z 5: the cylinder about the x axis.
fn laid_slot(axis: Option<[f64; 3]>) -> Vec<Move> {
    vec![mv(MoveKind::Feed, Point3::new(5.0, 8.0, 5.0), Point3::new(5.0, 12.0, 5.0), axis)]
}

fn op(moves: &[Move]) -> [SimOp<'_>; 1] {
    [SimOp { moves, radius: 2.0, ball: false, cone: None, corner: None }]
}

#[test]
fn the_column_engine_refuses_a_posed_move_by_name() {
    let moves = laid_slot(Some([1.0, 0.0, 0.0]));
    let mut dex = Dexel::block(0.0, 0.0, 20.0, 20.0, 10.0, 0.5).expect("block");
    let before = dex.clone();
    let err = simulate(&mut dex, &op(&moves)).expect_err("a leaning tool is not an upright one");
    let msg = err.to_string();
    assert!(msg.contains(POSED_REFUSAL) && msg.contains("op 0 move 0"), "{msg}");
    // Refused before a single column moved.
    for k in 0..dex.nx * dex.ny {
        assert_eq!(dex.top_at(k).to_bits(), before.top_at(k).to_bits(), "column {k} was cut");
    }
    // The seam says the same through its column arm.
    let mut s = Stock::block(Backend::Column, 0.0, 0.0, 20.0, 20.0, 10.0, 0.5).expect("block");
    let msg = s.simulate(&op(&moves)).expect_err("seam refuses too").to_string();
    assert!(msg.contains(POSED_REFUSAL), "{msg}");
}

#[test]
fn upright_stamps_and_posed_rapids_are_not_posed() {
    let up = laid_slot(Some([0.0, 0.0, 1.0]));
    let up2 = laid_slot(Some([0.0, 0.0, 2.5]));
    let none = laid_slot(None);
    assert_eq!(first_posed(&up), None);
    assert_eq!(first_posed(&up2), None);
    assert_eq!(first_posed(&none), None);
    let rapid = vec![mv(MoveKind::Rapid, Point3::new(0.0, 0.0, 9.0), Point3::new(5.0, 5.0, 9.0), Some([1.0, 0.0, 0.0]))];
    assert_eq!(first_posed(&rapid), None, "a rapid touches nothing on either arm");
    let nan = laid_slot(Some([f64::NAN, 0.0, 1.0]));
    assert_eq!(first_posed(&nan), Some(0), "a non-finite axis states no upright pose");
    // An explicit +Z stamp cuts exactly what an unstamped move cuts.
    let mut a = Dexel::block(0.0, 0.0, 20.0, 20.0, 10.0, 0.5).expect("block");
    let mut b = a.clone();
    simulate(&mut a, &op(&up)).expect("upright stamp");
    simulate(&mut b, &op(&none)).expect("unstamped");
    for k in 0..a.nx * a.ny {
        assert_eq!(a.top_at(k).to_bits(), b.top_at(k).to_bits(), "column {k}");
    }
}

#[test]
fn the_field_engine_cuts_the_leaning_solid_not_the_upright_one() {
    let moves = laid_slot(Some([1.0, 0.0, 0.0]));
    let mut f = Stock::block(Backend::Field { band: FIELD_BAND }, 0.0, 0.0, 20.0, 20.0, 10.0, 0.5).expect("block");
    f.simulate(&op(&moves)).expect("the field reads the pose");
    let air = |p: Point3| f.distance(p) > 0.0;
    // The hand case: removed = { x >= 5, y in [8, 12] widened by the
    // radius' circle, (y', z) within 2 of the axis line z = 5 }. Each
    // probe below is answered the OTHER way by an upright cut at the
    // same tip path (which removes the column x in [3, 7] above z 5).
    for (p, want_air, why) in [
        (Point3::new(12.0, 10.0, 5.0), true, "on the laid axis, 7 mm up the shank: cut (upright: x 12 is out of reach)"),
        (Point3::new(16.0, 10.0, 6.5), true, "inside the lying cylinder far along it: cut"),
        (Point3::new(5.5, 10.0, 8.5), false, "3.5 mm above the axis: steel (upright: cut, it is over the tip)"),
        (Point3::new(6.0, 10.0, 9.5), false, "the stock's top over the tip: steel (upright: cut)"),
        (Point3::new(3.0, 10.0, 5.0), false, "behind the tip face: steel"),
        (Point3::new(10.0, 10.0, 2.5), false, "below the cylinder: steel"),
    ] {
        assert_eq!(air(p), want_air, "{why} at {p:?}: distance {}", f.distance(p));
    }
    // The upright cut of the same tip path, for contrast: it takes the
    // steel over the tip and leaves the shank's - the wrong answer the
    // column engine would have given before it refused.
    let mut up = Stock::block(Backend::Field { band: FIELD_BAND }, 0.0, 0.0, 20.0, 20.0, 10.0, 0.5).expect("block");
    up.simulate(&op(&laid_slot(None))).expect("upright");
    assert!(up.distance(Point3::new(5.5, 10.0, 8.5)) > 0.0, "upright: cut over the tip");
    assert!(up.distance(Point3::new(12.0, 10.0, 5.0)) < 0.0, "upright: x 12 untouched");
}
