//! #317: the machined body carried between setups - the lattice
//! re-index through flips over X / Y and quarter turns about Z, the
//! named refusal of every other turn, and the two-sided verdict a
//! flipped body is judged by.
//! Provenance: our own fixtures (blocks with one cut column) over the
//! rigid-body algebra; no vendor material consulted.
//!
//! #404: an odd cell count carried through a translation off the grid
//! pitch keeps every image cell centre on a transformed source centre -
//! the carried origin moves with the part, whole cells, never snapped.
//!
//! Gate map (#317, #404): covers camcraft_sim::stock_sim (carry: Body,
//! LatticeTurn, CARRY_TURN_REFUSAL).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::stock_sim::{Body, CARRY_TURN_REFUSAL, DEV_TOL, Dexel, LatticeTurn, deviate};

const FLIP_X: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];
const TURN_Z90: [[f64; 3]; 3] = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];

/// A 60 x 40 block, Z -2 to 18, on a 1 mm lattice, with one column at
/// (10.5, 5.5) cut down to Z 10 - an asymmetric mark to follow.
fn marked() -> Body {
    let mut d = Dexel::block(-30.0, -20.0, 30.0, 20.0, 18.0, 1.0).expect("block");
    let k = d.col(camcraft_sim::geo2d::Point2::new(10.5, 5.5)).expect("in the grid");
    d.lower(k, 10.0);
    Body::on_floor(d, -2.0)
}

fn column(b: &Body, x: f64, y: f64) -> (f64, f64) {
    let k = b.dex.col(camcraft_sim::geo2d::Point2::new(x, y)).expect("in the grid");
    (b.dex.top_at(k), b.floor_at(k))
}

fn same_body(a: &Body, b: &Body) -> bool {
    a.dex.nx == b.dex.nx
        && a.dex.ny == b.dex.ny
        && (a.dex.x0 - b.dex.x0).abs() < 1e-9
        && (a.dex.y0 - b.dex.y0).abs() < 1e-9
        && (0..a.dex.nx * a.dex.ny).all(|k| {
            (a.dex.top_at(k) - b.dex.top_at(k)).abs() < 1e-9 && (a.floor_at(k) - b.floor_at(k)).abs() < 1e-9
        })
}

#[test]
fn identity_carries_the_body_onto_itself() {
    let b = marked();
    let c = b.carry(&LatticeTurn::IDENTITY).expect("identity");
    assert!(same_body(&b, &c));
    assert!(!c.floor_judged(), "an upright carry leaves the floor unjudged");
    assert!((b.volume() - c.volume()).abs() < 1e-9);
}

#[test]
fn a_flip_over_x_swaps_the_sides_exactly_and_twice_is_home() {
    let b = marked();
    let flip = LatticeTurn::from_matrix(FLIP_X, [0.0, 0.0, 16.0]).expect("flip over X");
    assert!(flip.flips());
    let c = b.carry(&flip).expect("flipped");
    // The mark at (10.5, 5.5), cut to Z 10, now stands at (10.5, -5.5)
    // with its floor at 16 - 10 = 6: what was cut is absent from below.
    assert_eq!(column(&c, 10.5, -5.5), (18.0, 6.0));
    assert_eq!(column(&c, 0.5, 0.5), (18.0, -2.0), "an uncut column: 16 - (-2) up, 16 - 18 down");
    assert!(c.floor_judged());
    assert!((b.volume() - c.volume()).abs() < 1e-9, "nothing re-sampled: {} vs {}", b.volume(), c.volume());
    let (lo, hi) = c.bounds().expect("material");
    assert!((lo.x + 30.0).abs() < 1e-9 && (hi.y - 20.0).abs() < 1e-9 && (lo.z + 2.0).abs() < 1e-9 && (hi.z - 18.0).abs() < 1e-9);
    let home = c.carry(&flip).expect("flipped back");
    assert!(same_body(&b, &home), "a flip is its own inverse on the lattice");
}

#[test]
fn a_quarter_turn_reindexes_the_lattice_and_four_are_home() {
    let b = marked();
    let turn = LatticeTurn::from_matrix(TURN_Z90, [0.0, 0.0, 0.0]).expect("quarter turn");
    let c = b.carry(&turn).expect("turned");
    assert_eq!((c.dex.nx, c.dex.ny), (40, 60), "the 60 x 40 block lies 40 x 60");
    // (x, y) -> (-y, x): the mark lands at (-5.5, 10.5).
    assert_eq!(column(&c, -5.5, 10.5), (10.0, -2.0));
    assert_eq!(column(&c, 5.5, 10.5), (18.0, -2.0));
    assert!(!c.floor_judged());
    let mut back = c;
    for _ in 0..3 {
        back = back.carry(&turn).expect("turn again");
    }
    assert!(same_body(&b, &back), "four quarter turns are the identity");
}

#[test]
fn turns_the_lattice_cannot_carry_are_refused_by_name() {
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let skew = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
    let side_on = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
    for r in [skew, side_on] {
        let err = LatticeTurn::from_matrix(r, [0.0; 3]).expect_err("not a lattice turn");
        assert_eq!(err, CARRY_TURN_REFUSAL);
    }
    let mirror = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]];
    let err = LatticeTurn::from_matrix(mirror, [0.0; 3]).expect_err("a mirror");
    assert!(err.contains("mirrors"), "{err}");
    let err = LatticeTurn::from_matrix(FLIP_X, [f64::NAN, 0.0, 0.0]).expect_err("non-finite");
    assert!(err.contains("not finite"), "{err}");
}

#[test]
fn a_flipped_body_is_judged_on_both_sides() {
    // A 20 x 20 part from Z 2 to 18 in a block from 0 to 20 (2 mm cell
    // margin all round is outside the part: unjudged).
    let part: Vec<Triangle> = cube(Point3::new(-10.0, -10.0, 2.0), Point3::new(10.0, 10.0, 18.0)).facets.to_vec();
    let d = Dexel::block(-15.0, -15.0, 15.0, 15.0, 20.0, 1.0).expect("block");
    let upright = Body::on_floor(d.clone(), 0.0);
    let plain = deviate(&d, &part, DEV_TOL).expect("plain");
    let same = upright.deviate(&part, DEV_TOL).expect("upright");
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&same.dev), bits(&plain.dev), "never flipped: the column verdict, column for column");
    assert_eq!((same.rest_cells, same.gouge_cells, same.max_rest_at), (plain.rest_cells, plain.gouge_cells, plain.max_rest_at));

    // Face the top to the part's top, flip about Z 10 (the part maps onto
    // itself): the faced side is now the floor and reads clean; the
    // untouched side stands 2 mm proud.
    let mut faced = upright.clone();
    for k in 0..faced.dex.nx * faced.dex.ny {
        faced.dex.lower(k, 18.0);
    }
    let flip = LatticeTurn::from_matrix(FLIP_X, [0.0, 0.0, 20.0]).expect("flip");
    let c = faced.carry(&flip).expect("flipped");
    let v = c.deviate(&part, DEV_TOL).expect("two-sided");
    assert!((v.max_rest - 2.0).abs() < 1e-9 && v.max_gouge <= DEV_TOL, "{v:?}");

    // A gouge the first setup cut into the (former) top survives the flip.
    let mut gouged = upright;
    let k = gouged.dex.col(camcraft_sim::geo2d::Point2::new(0.5, 0.5)).expect("centre");
    for j in 0..gouged.dex.nx * gouged.dex.ny {
        gouged.dex.lower(j, 18.0);
    }
    gouged.dex.lower(k, 17.0);
    let v = gouged.carry(&flip).expect("flipped").deviate(&part, DEV_TOL).expect("two-sided");
    assert!((v.max_gouge - 1.0).abs() < 1e-9, "the 1 mm gouge reads on the floor side: {v:?}");
    let at = v.max_gouge_at.expect("located");
    assert!((at.z - 3.0).abs() < 1e-9 && (at.y + 0.5).abs() < 1e-9, "{at:?}");
}

#[test]
fn a_column_cut_through_is_air_to_the_eye_and_a_gouge_to_the_verdict() {
    let part: Vec<Triangle> = cube(Point3::new(-10.0, -10.0, 2.0), Point3::new(10.0, 10.0, 18.0)).facets.to_vec();
    let d = Dexel::block(-15.0, -15.0, 15.0, 15.0, 20.0, 1.0).expect("block");
    let mut b = Body::on_floor(d, 0.0);
    let k = b.dex.col(camcraft_sim::geo2d::Point2::new(0.5, 0.5)).expect("centre");
    b.dex.lower(k, -1.0);
    assert!(b.empty_at(k));
    let c = b.carry(&LatticeTurn::from_matrix(FLIP_X, [0.0, 0.0, 20.0]).expect("flip")).expect("flipped");
    let j = c.dex.col(camcraft_sim::geo2d::Point2::new(0.5, -0.5)).expect("centre, flipped");
    assert!(c.empty_at(j), "cut through stays cut through");
    assert_eq!(c.display().top_at(j), f64::NEG_INFINITY, "the eye shows air");
    let v = c.deviate(&part, DEV_TOL).expect("two-sided");
    assert!(v.max_gouge >= 16.0 - 1e-9, "the hole is a gouge on the floor side: {v:?}");
}

/// #404: a 7 x 5 lattice at a 0.5 mm pitch, every column a distinct
/// height, carried through a flip over X and through a quarter turn,
/// each with a translation off the grid pitch (0.37, -1.13). Every
/// source column lands on exactly one image column whose centre IS the
/// transformed source centre (whole cells, the origin moved with the
/// part, not snapped back onto the old grid), the heights ride with
/// it, and the bounds are the transformed bounds - no material shifted.
#[test]
fn an_odd_lattice_carried_off_the_grid_keeps_whole_cells() {
    use camcraft_sim::geo2d::Point2;
    let mut d = Dexel::block(-1.75, -1.25, 1.75, 1.25, 18.0, 0.5).expect("block");
    assert_eq!((d.nx, d.ny), (7, 5), "an odd cell count on both axes");
    for k in 0..d.nx * d.ny {
        d.lower(k, 10.0 + 0.01 * k as f64);
    }
    let b = Body::on_floor(d, -2.0);
    let (blo, bhi) = b.bounds().expect("material");
    let t = [0.37, -1.13, 16.0];
    // (turn, expected image origin, expected image cell counts)
    let cases = [
        (FLIP_X, (-1.75 + 0.37, -1.25 - 1.13), (7, 5)),
        (TURN_Z90, (-1.25 + 0.37, -1.75 - 1.13), (5, 7)),
    ];
    for (r, (ox, oy), (nx, ny)) in cases {
        let turn = LatticeTurn::from_matrix(r, t).expect("a lattice turn");
        let c = b.carry(&turn).expect("carried");
        assert_eq!((c.dex.nx, c.dex.ny), (nx, ny));
        assert!((c.dex.x0 - ox).abs() < 1e-12 && (c.dex.y0 - oy).abs() < 1e-12, "origin moved with the part: ({}, {})", c.dex.x0, c.dex.y0);
        let mut hit = vec![false; nx * ny];
        for iy in 0..b.dex.ny {
            for ix in 0..b.dex.nx {
                let k = ix + iy * b.dex.nx;
                let src = b.dex.center(ix, iy);
                let p = turn.apply(Point3::new(src.x, src.y, 0.0));
                let j = c.dex.col(Point2::new(p.x, p.y)).expect("the image column exists");
                let (jx, jy) = (j % nx, j / nx);
                let q = c.dex.center(jx, jy);
                assert!((q.x - p.x).abs() < 1e-9 && (q.y - p.y).abs() < 1e-9, "column ({ix}, {iy}): image centre {q:?} vs transformed {p:?}");
                assert!(!hit[j], "two source columns on image column {j}");
                hit[j] = true;
                let (top, floor) = (b.dex.top_at(k), b.floor_at(k));
                let want = if turn.flips() { (t[2] - floor, t[2] - top) } else { (top + t[2], floor + t[2]) };
                assert_eq!((c.dex.top_at(j), c.floor_at(j)), want, "column ({ix}, {iy}) heights");
            }
        }
        assert!(hit.iter().all(|h| *h), "every image column is carried from one source column");
        assert!((b.volume() - c.volume()).abs() < 1e-9, "{} vs {}", b.volume(), c.volume());
        let (lo, hi) = c.bounds().expect("material");
        let corners = [turn.apply(blo), turn.apply(bhi)];
        let (ex0, ex1) = (corners[0].x.min(corners[1].x), corners[0].x.max(corners[1].x));
        let (ey0, ey1) = (corners[0].y.min(corners[1].y), corners[0].y.max(corners[1].y));
        assert!((lo.x - ex0).abs() < 1e-9 && (hi.x - ex1).abs() < 1e-9, "x {lo:?} {hi:?} vs {ex0} {ex1}");
        assert!((lo.y - ey0).abs() < 1e-9 && (hi.y - ey1).abs() < 1e-9, "y {lo:?} {hi:?} vs {ey0} {ey1}");
    }
}
