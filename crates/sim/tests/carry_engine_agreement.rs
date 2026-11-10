//! #637 step D: a carried stock reads the same on both engines. The same
//! block, cut by the same slots, carried through every lattice turn: the
//! column carry (`Body::carry`, re-indexed) and the field carry
//! (`CarriedField::column_body`, read back) agree column for column on
//! the tops, the floors and which columns are empty.
//!
//! Gate map (#637): covers sim::stock_sim::field_body (CarriedField::
//! column_body), carry::Body (from_parts) against carry::Body::carry.
//! Provenance: our own fixtures.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Backend, Body, CarriedField, Lattice, LatticeTurn, SimOp, Stock, FIELD_BAND};

const TOP: f64 = 5.0;
const CELL: f64 = 0.5;
/// The two engines read a top at a cell centre each: equal to rounding
/// on the mesh planes and the tool's flat floor.
const EPS: f64 = 1e-6;

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

/// A flat-end slot across the 20 x 12 block at depth `z`.
fn cut(b: Backend, z: f64) -> Stock {
    let mut s = Stock::block(b, 0.0, 0.0, 20.0, 12.0, TOP, CELL).expect("block");
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let moves = [
        Move::new(MoveKind::Rapid, p(-4.0, 6.0, 8.0), p(-4.0, 6.0, z), None),
        Move::new(MoveKind::Feed, p(-4.0, 6.0, z), p(24.0, 6.0, z), Some(500.0)),
    ];
    s.simulate(&[SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }]).expect("cut");
    s
}

fn agree(slot_z: f64) {
    let col = match cut(Backend::Column, slot_z) {
        Stock::Column(d) => d,
        Stock::Field(_) => panic!("a column stock"),
    };
    let field = cut(Backend::Field { band: FIELD_BAND }, slot_z).field().expect("a field").clone();
    // The block stock's floor as the field built it (an uncut corner).
    let floor = match field.floor_at(0.25, 0.25) {
        camcraft_sim::stock_sim::FloorRead::Floor(z) => z,
        r => panic!("the block's floor: {r:?}"),
    };
    let body = Body::on_floor(col, floor);
    for t in turns() {
        let a = body.carry(&t).unwrap_or_else(|e| panic!("{t:?}: column carry: {e}"));
        let d = &a.dex;
        let lat = Lattice { x0: d.x0, y0: d.y0, cell: d.cell, nx: d.nx, ny: d.ny };
        let b = CarriedField::new(field.clone(), t).column_body(lat, false).unwrap_or_else(|e| panic!("{t:?}: field carry: {e}"));
        assert_eq!(a.floor_judged(), b.floor_judged(), "{t:?}");
        for k in 0..d.nx * d.ny {
            assert_eq!(a.empty_at(k), b.empty_at(k), "{t:?} column {k}: empty");
            if a.empty_at(k) {
                continue;
            }
            let (ta, tb) = (a.dex.top_at(k), b.dex.top_at(k));
            let (fa, fb) = (a.floor_at(k), b.floor_at(k));
            assert!((ta - tb).abs() <= EPS, "{t:?} column {k}: top {ta} vs {tb}");
            assert!((fa - fb).abs() <= EPS, "{t:?} column {k}: floor {fa} vs {fb}");
        }
        assert!((a.volume() - b.volume()).abs() <= 1e-6, "{t:?}: volume {} vs {}", a.volume(), b.volume());
    }
}

#[test]
fn a_slot_from_the_top_carries_the_same_on_both_engines() {
    agree(3.0);
}

#[test]
fn a_slot_cut_through_carries_as_air_on_both_engines() {
    // A slot deeper than the top: on the column engine the cut columns
    // stand on the block's floor; the field reads the same top and floor.
    agree(1.0);
}
