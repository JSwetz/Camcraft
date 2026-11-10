//! #637 step D (review of #656): `CarriedField::column_body`'s refusal
//! and trim arms - a floor a cut reached, a bad lattice, the clip block
//! and the incoming floor flag.
//!
//! Gate map (#637): covers sim::stock_sim::field_body.
//! Provenance: our own fixtures.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::{CarriedField, Lattice, LatticeTurn, SdfField, SimOp, Stock, FIELD_BAND};

const ID: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// A real 20 x 12 x 6 plate (z -6..0), optionally slotted through.
fn plate(through: bool) -> SdfField {
    let f = SdfField::from_mesh(&cube(Point3::new(0.0, 0.0, -6.0), Point3::new(20.0, 12.0, 0.0)).facets, 0.5, FIELD_BAND)
        .expect("plate");
    if !through {
        return f;
    }
    let mut s = Stock::Field(f);
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let moves = [
        Move::new(MoveKind::Rapid, p(-4.0, 6.0, 5.0), p(-4.0, 6.0, -7.0), None),
        Move::new(MoveKind::Feed, p(-4.0, 6.0, -7.0), p(24.0, 6.0, -7.0), Some(500.0)),
    ];
    s.simulate(&[SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }]).expect("cut");
    s.field().expect("a field").clone()
}

fn lattice() -> Lattice {
    Lattice { x0: 0.0, y0: 0.0, cell: 0.5, nx: 40, ny: 24 }
}

fn id() -> LatticeTurn {
    LatticeTurn::from_matrix(ID, [0.0; 3]).expect("identity")
}

#[test]
fn an_uncut_plate_reads_its_own_top_and_floor() {
    let b = CarriedField::new(plate(false), id()).column_body(lattice(), false).expect("reads");
    let k = 3 + 3 * 40;
    assert!((b.dex.top_at(k) - 0.0).abs() < 1e-9 && (b.floor_at(k) + 6.0).abs() < 1e-9);
    assert!((b.volume() - 20.0 * 12.0 * 6.0).abs() < 1e-6);
    assert!(!b.floor_judged());
}

#[test]
fn a_floor_a_cut_reached_refuses_the_carry_by_name() {
    let e = CarriedField::new(plate(true), id()).column_body(lattice(), false).expect_err("refused");
    assert!(e.contains("cut reached the floor"), "{e}");
}

#[test]
fn a_bad_lattice_is_refused() {
    let c = CarriedField::new(plate(false), id());
    for lat in [
        Lattice { nx: 0, ..lattice() },
        Lattice { ny: 0, ..lattice() },
        Lattice { x0: f64::NAN, ..lattice() },
        Lattice { cell: 0.0, ..lattice() },
        Lattice { cell: f64::INFINITY, ..lattice() },
    ] {
        assert!(c.column_body(lat, false).is_err(), "{lat:?}");
    }
}

#[test]
fn the_clip_trims_tops_floors_and_columns() {
    let c = CarriedField::new(plate(false), id())
        .clipped(Point3::new(5.0, 0.0, -4.0), Point3::new(15.0, 12.0, -1.0))
        .expect("a finite block");
    let b = c.column_body(lattice(), false).expect("reads");
    let inside = 20 + 3 * 40; // x 10.25
    assert!((b.dex.top_at(inside) + 1.0).abs() < 1e-9 && (b.floor_at(inside) + 4.0).abs() < 1e-9);
    assert!(b.empty_at(3 + 3 * 40), "x 1.75 is outside the clip");
    assert!((b.volume() - 10.0 * 12.0 * 3.0).abs() < 1e-6);
}

#[test]
fn the_incoming_floor_flag_is_kept() {
    let b = CarriedField::new(plate(false), id()).column_body(lattice(), true).expect("reads");
    assert!(b.floor_judged(), "judged before stays judged");
}
