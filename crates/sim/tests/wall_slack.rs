//! #299: the gouge verdict grants `tol` sideways at a wall. A column
//! whose centre sits a few microns inside a vertical wall, reached by
//! a tool that finishes that wall within tolerance, used to read as a
//! gouge the full height of the wall; now it is judged against the
//! lowest part top within `tol` of its centre. The laws pinned: the
//! wall-straddling column is clean in both engines (with a floor
//! beside the wall, and with air beside it); a column further into the
//! wall than `tol` that the tool reaches still convicts, at its full
//! depth; a floor gouge away from any wall is untouched; and rest
//! keeps the centre's own top.
//! Provenance: our own tests of the height-field tolerance law in
//! `stock_sim::wall_slack`; nothing from vendor binaries or output.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim (deviate,
//! march_verdicts, wall_slack).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{Backend, Deviation, SimOp, Stock, DEV_TOL, FIELD_BAND};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// The stock: x/y -10..10 on 0.5 mm cells (centres at -9.75 + 0.5 i,
/// so one column line stands at x 0.25), top at z 25.
const CELL: f64 = 0.5;
const COL_X: f64 = 0.25;
const FLOOR: f64 = 5.0;
const WALL_TOP: f64 = 20.0;
const R: f64 = 3.0;

fn stock(b: Backend) -> Stock {
    Stock::block(b, -10.0, -10.0, 10.0, 10.0, 25.0, CELL).expect("test block")
}

/// A riser from `wall_x` to x 10 standing to z 20, on a floor at z 5
/// when `floor` is set (air beside the wall otherwise).
fn part(wall_x: f64, floor: bool) -> Vec<Triangle> {
    let mut f = cube(p(wall_x, -10.0, 0.0), p(10.0, 10.0, WALL_TOP)).facets.to_vec();
    if floor {
        f.extend(cube(p(-10.0, -10.0, 0.0), p(10.0, 10.0, FLOOR)).facets.iter().copied());
    }
    f
}

/// A flat end mill run along Y at the floor, its edge at `edge_x`
/// (centre `edge_x - R`), judged against `facets` by engine `b`.
fn judged(b: Backend, edge_x: f64, facets: &[Triangle]) -> Deviation {
    let x = edge_x - R;
    let moves = [feed(p(x, -12.0, FLOOR), p(x, 12.0, FLOOR))];
    let mut s = stock(b);
    s.simulate(&[SimOp { moves: &moves, radius: R, ball: false, cone: None, corner: None }])
        .expect("program runs");
    s.deviate(facets, DEV_TOL).expect("verdict")
}

fn engines() -> [Backend; 2] {
    [Backend::Column, Backend::Field { band: FIELD_BAND }]
}

#[test]
fn a_column_a_few_microns_inside_a_finished_wall_is_not_a_gouge() {
    // The wall 2 um short of the column line; the tool's edge 3 um past
    // the wall (within the 10 um tolerance) reaches the column centre.
    let wall = COL_X - 0.002;
    for floor in [true, false] {
        let facets = part(wall, floor);
        for b in engines() {
            let d = judged(b, wall + 0.003, &facets);
            assert_eq!(d.gouge_cells, 0, "{b:?} floor {floor}: {d:?}", d = (d.max_gouge, d.max_gouge_at));
            assert!(d.max_gouge <= DEV_TOL, "{b:?}: {}", d.max_gouge);
            // The absolved column reads clean, not rest and not NaN.
            let k = d.dev.iter().enumerate().find(|(i, _)| {
                let ix = i % d.cells_x;
                (-10.0 + (ix as f64 + 0.5) * CELL - COL_X).abs() < 1e-9
            });
            let v = k.map(|(_, v)| *v).unwrap_or(f64::NAN);
            assert!(v.abs() <= DEV_TOL, "{b:?} floor {floor}: the wall column reads {v}");
        }
    }
}

#[test]
fn the_old_law_charged_that_column_the_whole_wall() {
    // The same column, judged with no slack (tol 0): the height alone
    // reads the wall's 15 mm - what the post gate used to refuse.
    let wall = COL_X - 0.002;
    let facets = part(wall, true);
    let x = wall + 0.003 - R;
    let moves = [feed(p(x, -12.0, FLOOR), p(x, 12.0, FLOOR))];
    let mut s = stock(Backend::Column);
    s.simulate(&[SimOp { moves: &moves, radius: R, ball: false, cone: None, corner: None }])
        .expect("program runs");
    let d = s.deviate(&facets, 0.0).expect("verdict");
    assert!((d.max_gouge - (WALL_TOP - FLOOR)).abs() < 1e-9, "{}", d.max_gouge);
}

#[test]
fn a_real_gouge_into_a_wall_still_convicts_at_its_full_depth() {
    // The wall 50 um short of the column line, the tool 0.1 mm into it:
    // that column is past the slack, and it is charged all 15 mm.
    let wall = COL_X - 0.05;
    for floor in [true, false] {
        let facets = part(wall, floor);
        for b in engines() {
            let d = judged(b, wall + 0.1, &facets);
            assert!(d.gouge_cells > 0, "{b:?} floor {floor}: a real wall gouge must convict");
            assert!((d.max_gouge - (WALL_TOP - FLOOR)).abs() < 1e-3, "{b:?}: {}", d.max_gouge);
            let at = d.max_gouge_at.expect("located");
            assert!((at.x - COL_X).abs() < 1e-9, "{b:?}: at {at:?}");
        }
    }
    // Deep into the wall: every reached column past the slack convicts.
    let facets = part(COL_X - 0.002, true);
    let d = judged(Backend::Column, COL_X + 1.0, &facets);
    assert!(d.gouge_cells > 0 && (d.max_gouge - (WALL_TOP - FLOOR)).abs() < 1e-9, "{:?}", d.max_gouge);
    // Even the wall's own column convicts when the cut is just past the
    // slack: wall 12 um short of the line, the tool through it.
    let facets = part(COL_X - 0.012, true);
    let d = judged(Backend::Column, COL_X + 0.01, &facets);
    assert!(d.gouge_cells > 0, "12 um past the wall is past a 10 um slack");
}

#[test]
fn a_floor_gouge_away_from_walls_is_unchanged() {
    // A plain plate whose top is z 6, cut to z 5: every reached column
    // is 1 mm under, slack or not.
    let facets = cube(p(-10.0, -10.0, 0.0), p(10.0, 10.0, 6.0)).facets.to_vec();
    for b in engines() {
        let d = judged(b, 3.0, &facets);
        assert!(d.gouge_cells > 0, "{b:?}");
        assert!((d.max_gouge - 1.0).abs() < 1e-3, "{b:?}: {}", d.max_gouge);
    }
}

#[test]
fn rest_beside_a_wall_keeps_the_centres_own_top() {
    // Nothing cut: the floor column just outside the wall (centre 2 um
    // from it) stands 20 mm over its own floor - rest is not absolved
    // by the wall beside it.
    let facets = part(COL_X + 0.002, true);
    let d = judged(Backend::Column, -9.0, &facets);
    let row: Vec<f64> = (0..d.cells_x).map(|ix| d.dev[ix + d.cells_x * (d.cells_y / 2)]).collect();
    let ix = ((COL_X + 10.0) / CELL - 0.5).round() as usize;
    assert!((row[ix] - (25.0 - FLOOR)).abs() < 1e-9, "rest at the wall's foot: {}", row[ix]);
    assert_eq!(d.gouge_cells, 0);
}
