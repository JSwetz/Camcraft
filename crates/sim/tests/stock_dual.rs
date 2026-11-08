//! M13.1 (#23): dual contouring keeps a machined pocket's sharp interior
//! corner. A flat endmill slots a box; the slot's floor/wall edge is a
//! true 90-degree concave line. The dual mesh passes within a small
//! twentieth of a cell of that line, where marching tetrahedra chamfers
//! it by about a third of a cell; both stay closed. Degenerate fields
//! (everything cut away, a body smaller than a cell) answer cleanly.
//! Provenance: our own fixtures over the M7.1 field engine and the
//! extraction of Ju, Losasso, Schaefer & Warren (SIGGRAPH 2002); nothing
//! from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::iso, stock_sim::dual, stock_sim::qef.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::iso::{extract, extract_with, open_edges, IsoMethod};
use camcraft_sim::stock_sim::{dist_seg_tri, SdfField, SdfTool, FIELD_BAND};

const CELL: f64 = 1.0;
/// The slot: a flat endmill of radius 2 at tip z 6.2 along y = 5.3, so
/// its walls stand at y = 3.3 and 7.3 - off the lattice's samples and
/// faces both, the generic case.
const R: f64 = 2.0;
const FLOOR: f64 = 6.2;
const AXIS_Y: f64 = 5.3;

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

fn slotted() -> SdfField {
    let m = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0));
    let mut f = SdfField::from_mesh(&m.facets, CELL, FIELD_BAND).expect("box field");
    let tool = SdfTool::Flat { r: R, flank: 20.0 };
    let moves = [
        feed(Point3::new(2.0, AXIS_Y, 14.0), Point3::new(2.0, AXIS_Y, FLOOR)),
        feed(Point3::new(2.0, AXIS_Y, FLOOR), Point3::new(8.0, AXIS_Y, FLOOR)),
    ];
    f.subtract_batch(&moves, &tool, 0.01).expect("the slot cuts");
    f
}

/// The farthest the mesh stands from the slot's straight floor/wall
/// edges, sampled along their middle (clear of the round ends).
fn edge_gap(tris: &[Triangle]) -> f64 {
    let mut worst = 0.0f64;
    for y in [AXIS_Y - R, AXIS_Y + R] {
        for s in 0..=16 {
            let e = Point3::new(3.5 + 3.0 * s as f64 / 16.0, y, FLOOR);
            let d = tris.iter().map(|t| dist_seg_tri(e, e, t)).fold(f64::INFINITY, f64::min);
            worst = worst.max(d);
        }
    }
    worst
}

#[test]
fn dual_contouring_keeps_the_slot_floor_wall_edge_sharp() {
    let f = slotted();
    let dual = extract_with(&f, CELL, IsoMethod::Dual).expect("dual mesh");
    let mt = extract_with(&f, CELL, IsoMethod::Marching).expect("marching mesh");
    assert_eq!(open_edges(&dual), 0, "the dual shell is closed");
    assert_eq!(open_edges(&mt), 0, "the marching shell is closed");
    let (gd, gm) = (edge_gap(&dual), edge_gap(&mt));
    eprintln!("floor/wall edge gap: dual {gd:.4}, marching {gm:.4} (cells of {CELL})");
    assert!(gm > 0.2 * CELL, "marching tetrahedra chamfers the corner: {gm}");
    assert!(gd < 0.05 * CELL, "the dual mesh keeps the corner within a twentieth of a cell: {gd}");
    assert!(gd < 0.5 * gm, "dual {gd} vs marching {gm}");
}

#[test]
fn dual_is_the_default_extractor() {
    let f = slotted();
    let a = extract(&f).expect("default mesh");
    let b = extract_with(&f, CELL, IsoMethod::Dual).expect("dual mesh");
    assert_eq!(a.len(), b.len());
    assert_eq!(IsoMethod::default(), IsoMethod::Dual);
}

#[test]
fn a_fully_cut_field_extracts_nothing_without_error() {
    let m = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 4.0, 4.0));
    let mut f = SdfField::from_mesh(&m.facets, CELL, FIELD_BAND).expect("box field");
    let tool = SdfTool::Flat { r: 10.0, flank: 30.0 };
    f.subtract(&feed(Point3::new(2.0, 2.0, 20.0), Point3::new(2.0, 2.0, -5.0)), &tool, 0.01).expect("cut");
    for method in [IsoMethod::Dual, IsoMethod::Marching] {
        let tris = extract_with(&f, CELL, method).expect("an empty answer, not an error");
        assert!(tris.is_empty(), "{method:?}: nothing is left, {} triangles", tris.len());
    }
}

#[test]
fn a_body_under_one_cell_never_panics_and_never_opens() {
    let m = cube(Point3::new(0.1, 0.1, 0.1), Point3::new(0.6, 0.6, 0.6));
    let Ok(f) = SdfField::from_mesh(&m.facets, CELL, FIELD_BAND) else {
        return; // refused by the field gate: a clean answer too
    };
    for method in [IsoMethod::Dual, IsoMethod::Marching] {
        if let Ok(tris) = extract_with(&f, CELL, method) {
            assert_eq!(open_edges(&tris), 0, "{method:?}: closed or empty");
            assert!(tris.iter().all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite()));
        }
    }
    for s in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(extract_with(&f, s, IsoMethod::Dual).is_err(), "step {s} must refuse");
    }
}


#[test]
fn a_curved_ball_pocket_stays_closed_and_never_bulges() {
    // A ball nose of radius 2.5 plunged 3 deep: a sphere-capped hole,
    // curved everywhere at under three cells of radius. The dual mesh
    // borrows no corner planes here and must agree with the field's
    // volume as closely as marching does.
    let m = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0));
    let mut f = SdfField::from_mesh(&m.facets, CELL, FIELD_BAND).expect("box field");
    let tool = SdfTool::Shanked { r: 2.5, shank: 20.0 };
    f.subtract(&feed(Point3::new(5.1, 4.9, 14.0), Point3::new(5.1, 4.9, 7.0)), &tool, 0.01).expect("cut");
    let vol = |t: &[Triangle]| t.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum::<f64>();
    let dual = extract_with(&f, CELL, IsoMethod::Dual).expect("dual mesh");
    let mt = extract_with(&f, CELL, IsoMethod::Marching).expect("marching mesh");
    assert_eq!(open_edges(&dual), 0);
    let r = 2.5f64;
    // Box minus the cylinder above the ball centre (z 9.5..10) and the
    // lower hemisphere (centre z 9.5).
    let truth = 1000.0 - std::f64::consts::PI * r * r * 0.5 - 2.0 / 3.0 * std::f64::consts::PI * r.powi(3);
    let (vd, vm) = (vol(&dual), vol(&mt));
    eprintln!("ball pocket volume: dual {vd:.3}, marching {vm:.3}, true {truth:.3}");
    assert!((vd - truth).abs() <= (vm - truth).abs().max(0.005 * truth), "dual {vd} vs marching {vm}, truth {truth}");
}
