//! #637 step B (option 2): a field column's floor is read exactly from
//! the shell where no cut reaches it, and refused by name where one does.
//!
//! Gate map (#637): covers sim::stock_sim::field_floor (SdfField::floor_at).
//! Provenance: our own fixtures.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::{FloorRead, SdfField, Stock, SimOp, FIELD_BAND};

/// A real 20 x 12 x 6 block (z -6..0) as a closed field.
fn plate() -> SdfField {
    match SdfField::from_mesh(&cube(Point3::new(0.0, 0.0, -6.0), Point3::new(20.0, 12.0, 0.0)).facets, 0.5, FIELD_BAND) {
        Ok(f) => f,
        Err(e) => panic!("plate: {e:?}"),
    }
}

fn cut(f: SdfField, z: f64) -> SdfField {
    let mut s = Stock::Field(f);
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let moves = [
        Move::new(MoveKind::Rapid, p(-4.0, 6.0, 5.0), p(-4.0, 6.0, z), None),
        Move::new(MoveKind::Feed, p(-4.0, 6.0, z), p(24.0, 6.0, z), Some(500.0)),
    ];
    if let Err(e) = s.simulate(&[SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }]) {
        panic!("cut: {e:?}");
    }
    s.field().expect("a field").clone()
}

#[test]
fn an_uncut_floor_reads_the_shell_exactly() {
    let f = plate();
    assert_eq!(f.floor_at(10.25, 6.25), FloorRead::Floor(-6.0));
    // A shallow slot from the top leaves the floor untouched.
    let g = cut(f, -2.0);
    assert_eq!(g.floor_at(10.25, 6.25), FloorRead::Floor(-6.0), "a top cut does not reach the floor");
    assert_eq!(g.floor_at(40.0, 6.25), FloorRead::NoMaterial, "beside the stock");
}

#[test]
fn a_floor_a_cut_reached_is_refused_by_name() {
    // A slot cut through the plate: its floor cannot be read upward exactly.
    let g = cut(plate(), -7.0);
    assert_eq!(g.floor_at(10.25, 6.25), FloorRead::CutFromBelow);
    // Away from the slot the floor is still exact.
    assert_eq!(g.floor_at(10.25, 0.75), FloorRead::Floor(-6.0));
}
