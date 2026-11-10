//! #585 (M7.3): law (d)'s prior-stock readers (`verify::prior_surface`,
//! `verify::acquit_by_prior`) read through the `Columns` seam, so a
//! rest grid and a field stock answer alike - the readers no longer
//! name the column engine, and the field can replace the grid behind
//! them without touching the law.
//!
//! Gate map (#585): covers sim::verify::prior, sim::stock_sim::columns
//! (`Columns::of_grid`, `cell`, `center`, `origin`).
//! Provenance: our own fixtures; the parity law of the M7.1 C suite.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Backend, Columns, SimOp, Stock, FIELD_BAND};
use camcraft_sim::verify::{acquit_by_prior, prior_surface, Warning};

const TOP: f64 = 25.0;

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind, from, to, feed: (kind != MoveKind::Rapid).then_some(300.0), arc: None, axis: None }
}

/// A 40 x 40 block, top Z 25, with a 6 mm end mill's plunge to Z 15 at
/// the origin cut by "the program ahead" - on the given engine.
fn prior(b: Backend) -> Stock {
    let mut s = match Stock::block(b, -20.0, -20.0, 20.0, 20.0, TOP, 0.5) {
        Ok(s) => s,
        Err(e) => panic!("block: {e:?}"),
    };
    let moves = [
        mv(MoveKind::Feed, Point3::new(0.0, 0.0, 30.0), Point3::new(0.0, 0.0, 15.0)),
        mv(MoveKind::Feed, Point3::new(0.0, 0.0, 15.0), Point3::new(0.0, 0.0, 30.0)),
    ];
    if let Err(e) = s.simulate(&[SimOp { moves: &moves, radius: 3.0, ball: false, cone: None, corner: None }]) {
        panic!("simulate: {e:?}");
    }
    s
}

fn dive(to_z: f64) -> Toolpath {
    let mut t = Toolpath::new(vec![mv(MoveKind::Rapid, Point3::new(0.0, 0.0, 35.0), Point3::new(0.0, 0.0, to_z))]);
    t.probes.clear();
    t
}

#[test]
fn the_prior_surface_reads_alike_through_a_grid_and_a_field() {
    let (c, f) = (prior(Backend::Column), prior(Backend::Field { band: FIELD_BAND }));
    let (cc, fc) = match (c.columns(), f.columns()) {
        (Ok(a), Ok(b)) => (a, b),
        (a, b) => panic!("columns: {:?} / {:?}", a.err(), b.err()),
    };
    // Inside the hole, across its rim, and on the raw top.
    for (q, r) in [(Point2::new(0.0, 0.0), 1.0), (Point2::new(0.0, 0.0), 2.0), (Point2::new(0.0, 0.0), 5.0), (Point2::new(10.0, 10.0), 2.0)] {
        let (a, b) = (prior_surface(&cc, q, r), prior_surface(&fc, q, r));
        match (a, b) {
            (Some(a), Some(b)) => assert!((a - b).abs() < 1e-6, "{q:?} r {r}: grid {a} vs field {b}"),
            other => panic!("{q:?} r {r}: both engines must read a surface, got {other:?}"),
        }
    }
    assert!(prior_surface(&cc, Point2::new(0.0, 0.0), 1.0).is_some_and(|z| (z - 15.0).abs() < 1e-6), "the hole floor");
    assert!(prior_surface(&cc, Point2::new(10.0, 10.0), 2.0).is_some_and(|z| (z - TOP).abs() < 1e-6), "the raw top");
}

#[test]
fn a_dive_is_acquitted_alike_on_either_engine_and_through_a_bare_grid() {
    let (c, f) = (prior(Backend::Column), prior(Backend::Field { band: FIELD_BAND }));
    let grid = match &c {
        Stock::Column(d) => d.clone(),
        Stock::Field(_) => panic!("a column stock"),
    };
    let readers = [
        ("column stock", c.columns().ok()),
        ("field stock", f.columns().ok()),
        ("bare grid", Some(Columns::of_grid(&grid))),
    ];
    for (name, cols) in readers {
        let Some(cols) = cols else { panic!("{name}: no reader") };
        let mut into_hole = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(16.0), &cols, 1.0, 1e-3, &mut into_hole);
        assert!(into_hole.is_empty(), "{name}: a dive into the cleared hole is acquitted: {into_hole:?}");
        let mut past_floor = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(14.0), &cols, 1.0, 1e-3, &mut past_floor);
        assert_eq!(past_floor, vec![Warning::RapidInStock { at: 0 }], "{name}: past the floor stands");
    }
}
