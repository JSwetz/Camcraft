//! #633: a block stock on the field engine builds in closed form. The
//! band fill along a floorless block's ten-span walls made a 120 mm
//! block take ~58 s in release (its cuts took milliseconds); the box's
//! exact distance now answers the mesh layer, so the build stores no
//! band at all - and reads the same stock as the mesh-seeded field.
//!
//! Gate map (#633): covers sim::stock_sim::field_box (AnalyticBox,
//! SdfField::from_box), field::mag_at, backend_build::block_field.
//! Provenance: our own fixtures; the closed-form box distance.

use std::time::Instant;

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::{auto_cell, Backend, SdfField, SimOp, Stock, FIELD_BAND};

fn field_block(x0: f64, y0: f64, x1: f64, y1: f64, top: f64, cell: f64) -> Stock {
    match Stock::block(Backend::Field { band: FIELD_BAND }, x0, y0, x1, y1, top, cell) {
        Ok(s) => s,
        Err(e) => panic!("block: {e:?}"),
    }
}

#[test]
fn a_block_stores_no_band_at_build() {
    // The case that took ~58 s: 120 mm square at a 0.375 mm cell.
    let s = field_block(-60.0, -60.0, 60.0, 60.0, 0.0, 0.375);
    let f = s.field().expect("a field stock");
    assert_eq!(f.brick_count(), 0, "no band is filled at build - the box answers in closed form");
}

#[test]
fn the_closed_form_reads_the_stock_the_band_fill_read() {
    // A small block both ways: band-filled from its shell, and closed-form.
    let (lo, hi, cell) = (Point3::new(0.0, 0.0, -6.0), Point3::new(10.0, 8.0, 0.0), 0.5);
    let meshed = match SdfField::from_mesh(&cube(lo, hi).facets, cell, FIELD_BAND) {
        Ok(f) => f,
        Err(e) => panic!("mesh field: {e:?}"),
    };
    let boxed = match SdfField::from_box(lo, hi, cell, FIELD_BAND) {
        Ok(f) => f,
        Err(e) => panic!("box field: {e:?}"),
    };
    let mut worst = 0.0f64;
    for i in -6..=26 {
        for j in -6..=22 {
            for k in -18..=6 {
                let p = Point3::new(f64::from(i) * 0.5 + 0.25, f64::from(j) * 0.5 + 0.25, f64::from(k) * 0.5 + 0.25);
                let (a, b) = (meshed.mesh_distance(p), boxed.mesh_distance(p));
                // Same side of the surface everywhere (the verdicts' sign).
                assert!(a.signum() == b.signum() || a == 0.0 || b == 0.0, "{p:?}: mesh {a} vs box {b}");
                worst = worst.max((a - b).abs());
            }
        }
    }
    // The band fill is a graph-distance bound within a few cells of the
    // truth; the closed form IS the truth, so they agree to that bound.
    assert!(worst <= 2.0 * cell, "worst |mesh - box| = {worst}");
}

#[test]
#[ignore = "heavy lane (#633 cost): a 120 mm field block, drill program, release ~ms - run with --release -- --ignored"]
fn a_large_field_block_runs_a_drill_program_quickly() {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let mut moves = Vec::new();
    for (x, y) in [(-30.0, -30.0), (30.0, -30.0), (30.0, 30.0), (-30.0, 30.0)] {
        moves.push(Move::new(MoveKind::Rapid, p(x, y, 5.0), p(x, y, 1.0), None));
        moves.push(Move::new(MoveKind::Feed, p(x, y, 1.0), p(x, y, -5.0), Some(200.0)));
        moves.push(Move::new(MoveKind::Rapid, p(x, y, -5.0), p(x, y, 5.0), None));
    }
    let cell = auto_cell(3.0, 120.0, 120.0);
    let t0 = Instant::now();
    let mut s = field_block(-60.0, -60.0, 60.0, 60.0, 0.0, cell);
    if let Err(e) = s.simulate(&[SimOp { moves: &moves, radius: 3.0, ball: false, cone: None, corner: None }]) {
        panic!("simulate: {e:?}");
    }
    let dt = t0.elapsed();
    // Budget: the build plus four plunges, on a 320 x 320 lattice. Was
    // ~58 s; a regression to a band fill blows through this at once.
    assert!(dt.as_secs_f64() < 2.0, "field block + drill took {dt:?}");
}
