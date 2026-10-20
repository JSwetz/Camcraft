//! M2.1b: the dexel material model. The laws here are the ones the rest
//! pass and the stock report stand on: the exact reach-interval cut of a
//! chord, rapids touching nothing, chord conservatism for arcs (over-
//! report rest, never invent a gouge), the `above` mask in region
//! clothing, and the part surface read straight off the mesh.
//! Provenance: published CAM simulation practice (Z-map subtraction),
//! our own code, per the Clean-Room Protocol.

use hsmworks_app::cl::{ArcMove, Move, MoveKind};
use hsmworks_app::geo2d::{Loop, Point2, Region};
use hsmworks_app::geometry::{Point3, Triangle};
use hsmworks_app::stock_sim::{self, Dexel, StockError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn feed(from: Point3, to: Point3) -> Move {
    Move {
        kind: MoveKind::Feed,
        from,
        to,
        feed: Some(500.0),
        arc: None,
    }
}

/// Cell `ix, iy` of a grid as a raw read - the tests inspect the field
/// directly because the field IS the law (one number per column).
fn top_of(d: &Dexel, ix: usize, iy: usize) -> f64 {
    d.top[ix + iy * d.nx]
}

#[test]
fn a_plunge_disc_cuts_exactly_its_radius() {
    // The vertical atom: cells whose centers sit within r of the axis
    // drop to the move's lowest Z; every other column is untouched.
    // 8x8 on 2 mm cells: centers 1,3,5,7; plunge at (4,4) to z = 1.
    let mut d = Dexel::block(0.0, 0.0, 8.0, 8.0, 5.0, 2.0).unwrap();
    d.cut_chord(p(4.0, 4.0, 5.0), p(4.0, 4.0, 1.0), 2.5);
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let q = d.center(ix, iy);
            let inside = (q.x - 4.0).hypot(q.y - 4.0) <= 2.5;
            let want = if inside { 1.0 } else { 5.0 };
            assert!(
                near(top_of(&d, ix, iy), want),
                "cell {ix},{iy} at {q:?}: got {}, want {want}",
                top_of(&d, ix, iy)
            );
        }
    }
}

#[test]
fn an_oblique_chord_cuts_the_reach_interval_not_the_endpoints() {
    // (0,0,4) -> (10,0,0), disc r = 3. Over the cell center (5,0) the
    // disc is in reach for t in [0.2, 0.8], so the cut is the LOWEST Z
    // in that interval: 4 - 4*0.8 = 0.8. An endpoint-only sim would
    // say 0 (the whole-segment minimum - a gouge the tool never
    // commits above the reach window) and a closest-point sampling
    // would say 2. The exact interval law says 0.8 and the grid must
    // agree, to the bit the floats allow.
    let mut d = Dexel::block(0.0, -5.0, 12.0, 5.0, 10.0, 2.0).unwrap();
    d.cut_chord(p(0.0, 0.0, 4.0), p(10.0, 0.0, 0.0), 3.0);
    // Centers: x = 1,3,5,7,9,11 (ix 0..5), y = -4,-2,0,2,4 (iy 0..4).
    assert!(near(top_of(&d, 2, 2), 0.8), "reach minimum at (5,0)");
    // (1,0): reach t in [0, 0.4] -> cut 4 - 1.6 = 2.4.
    assert!(near(top_of(&d, 0, 2), 2.4), "reach minimum at (1,0)");
    // Off the line by more than r: untouched.
    assert!(near(top_of(&d, 2, 4), 10.0), "(5,4) never felt the tool");
    // The plunge end of the interval is what counts, not where the
    // segment ends: nothing dips below the segment's own bottom.
    assert!(d.top.iter().all(|&z| z >= 0.0), "never below the chord");
}

#[test]
fn rapids_touch_nothing_and_non_finite_touches_nothing() {
    let mut d = Dexel::block(0.0, 0.0, 4.0, 4.0, 2.0, 1.0).unwrap();
    let before = d.clone();
    let mut r = feed(p(0.5, 0.5, 2.0), p(3.5, 3.5, -9.0));
    r.kind = MoveKind::Rapid;
    d.subtract(&r, 2.0, 0.01);
    assert_eq!(d, before, "a rapid across the whole grid cuts nothing");
    d.subtract(&feed(p(0.5, 0.5, f64::NAN), p(3.5, 3.5, -9.0)), 2.0, 0.01);
    assert_eq!(d, before, "non-finite geometry cuts nothing, silently");
    let mut m = feed(p(0.5, 0.5, 2.0), p(3.5, 3.5, -9.0));
    m.to.x = f64::INFINITY;
    d.subtract(&m, 2.0, 0.01);
    assert_eq!(d, before, "and the refusal lives upstream, not mid-sweep");
}

#[test]
fn arc_chords_under_sweep_so_the_sim_only_over_reports() {
    // A quarter arc R = 5 about the origin at z = -1, cut coarse: a
    // tol of 2.0 exceeds the quarter's sagitta of 1.46, so the arc
    // rides its single sagitta chord (5,0)-(0,5). The cell center at
    // (3.5,3.5) sits ~0.05 under the true arc but 1.41 from the chord:
    // a real tool would have cut it to -1, the chord sim leaves it
    // standing. That is the conservatism law made visible - rest
    // reported that isn't there, never a gouge that isn't.
    let mut d = Dexel::block(0.0, 0.0, 6.0, 6.0, 2.0, 1.0).unwrap();
    let m = Move {
        kind: MoveKind::Feed,
        from: p(5.0, 0.0, -1.0),
        to: p(0.0, 5.0, -1.0),
        feed: Some(500.0),
        arc: Some(ArcMove {
            center: Point2::new(0.0, 0.0),
            ccw: true,
        }),
    };
    d.subtract(&m, 1.0, 2.0);
    assert!(
        near(top_of(&d, 3, 3), 2.0),
        "the chord never reaches the arc's midline - rest over-reported"
    );
    assert!(
        top_of(&d, 4, 0) < 2.0,
        "on the chord line the cell IS cut: {:?}",
        top_of(&d, 4, 0)
    );
    assert!(
        d.top.iter().all(|&z| z >= -1.0),
        "nothing cuts below the arc's own floor - no invented gouge"
    );
}

#[test]
fn above_reports_the_standing_mask_as_a_region() {
    // One plunge disc carved into a full grid: above(z) is exactly the
    // grid minus that disc's cells, for z between the cut floor and the
    // untouched top, and the whole rectangle below the floor.
    let mut d = Dexel::block(0.0, 0.0, 8.0, 8.0, 5.0, 2.0).unwrap();
    d.cut_chord(p(4.0, 4.0, 5.0), p(4.0, 4.0, 1.0), 2.5);
    let mask = d.above(2.0).unwrap();
    assert!(mask.contains(Point2::new(1.0, 1.0)), "untouched corner stands");
    assert!(
        !mask.contains(Point2::new(4.0, 4.0)),
        "the cut disc is below the plane"
    );
    let all = d.above(0.0).unwrap();
    assert!(
        all.contains(Point2::new(1.0, 1.0)) && all.contains(Point2::new(4.0, 4.0)),
        "below every top, the whole block stands"
    );
    let none = d.above(6.0).unwrap();
    assert!(!none.contains(Point2::new(1.0, 1.0)), "above everything: empty");
}

#[test]
fn measure_judges_the_footprint_and_counts_removal_from_the_stock_surface() {
    // 10x2 on 1 mm cells, stock top 5, part top 3 everywhere. Cut two
    // columns: one exactly to the part, one past it.
    let mut d = Dexel::block(0.0, 0.0, 10.0, 2.0, 5.0, 1.0).unwrap();
    d.cut_chord(p(0.5, 0.5, 3.0), p(0.5, 0.5, 3.0), 0.6); // exact
    d.cut_chord(p(1.5, 0.5, 5.0), p(1.5, 0.5, 2.5), 0.6); // a gouge
    let facets = vec![
        Triangle::new(p(0.0, 0.0, 3.0), p(10.0, 0.0, 3.0), p(10.0, 2.0, 3.0)),
        Triangle::new(p(0.0, 0.0, 3.0), p(10.0, 2.0, 3.0), p(0.0, 2.0, 3.0)),
    ];
    let part = stock_sim::part_surface(&facets, d.x0, d.y0, d.cell, d.nx, d.ny);
    assert!(
        part.iter().all(|&z| near(z, 3.0)),
        "a flat facet covers every column at 3"
    );
    let rep = stock_sim::measure(&d, &part, &rect(0.0, 0.0, 10.0, 2.0), 1e-6);
    assert_eq!(rep.rest_cells, 18, "the 18 still-standing columns");
    assert!(near(rep.rest_volume, 36.0), "2 mm deep over 18 cells");
    assert!(near(rep.max_rest_height, 2.0));
    assert_eq!(rep.gouge_cells, 1, "exactly the column cut past the part");
    assert!(near(rep.max_gouge_depth, 0.5));
    assert!(
        near(rep.removed_volume, 4.5),
        "only cut columns count: 2 + 2.5 + eighteen zeros"
    );
    // Outside the footprint the machine's world is not this verdict.
    let half = stock_sim::measure(&d, &part, &rect(0.0, 0.0, 5.0, 2.0), 1e-6);
    assert_eq!(half.rest_cells, 8, "ten columns in range, the two cut ones rest not");
    assert!(near(half.removed_volume, 4.5), "and both cuts lie in it");
}

#[test]
fn part_surface_reads_the_mesh_and_names_the_air() {
    // A sloped facet interpolates; uncovered columns are -inf (air),
    // and vertical walls carry no surface at all - the caps do.
    let facets = vec![Triangle::new(p(0.0, 0.0, 0.0), p(10.0, 0.0, 10.0), p(0.0, 10.0, 0.0))];
    let part = stock_sim::part_surface(&facets, 0.0, 0.0, 2.0, 5, 5);
    assert!(near(top_of_dims(&part, 5, 1, 1), 3.0), "the plane is z = x");
    assert!(
        part[3 + 3 * 5] == f64::NEG_INFINITY,
        "past the hypotenuse the column is air"
    );
    let walls = box_facets([-4.0, -4.0, 0.0], [4.0, 4.0, 3.0]);
    let part = stock_sim::part_surface(&walls, -6.0, -6.0, 2.0, 6, 6);
    // Centers -5,-3,-1,1,3,5: inside footprint at z <= 3 only where the
    // top cap covers (|x|,|y| < 4 -> cells 1..=4); -5/5 are air.
    assert!(near(top_of_dims(&part, 6, 2, 2), 3.0), "the top cap covers");
    assert!(part[0] == f64::NEG_INFINITY, "outside the box: air, not wall");
    assert!(
        part.iter().all(|&z| z == f64::NEG_INFINITY || near(z, 3.0)),
        "a wall facet contributed no surface height anywhere"
    );
}

fn top_of_dims(v: &[f64], nx: usize, ix: usize, iy: usize) -> f64 {
    v[ix + iy * nx]
}

/// A solid box as facet soup, outward CCW - same construction the
/// machining3d tests use, duplicated here so the file stands alone.
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        p(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [
            Triangle::new(v(a), v(b), v(c)),
            Triangle::new(v(a), v(c), v(d)),
        ]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

#[test]
fn degenerate_grids_and_cuts_refuse_or_ignore_cleanly() {
    // Box math errors refuse the block with the numbers named.
    assert!(matches!(
        Dexel::block(0.0, 0.0, 0.0, 1.0, 1.0, 0.5),
        Err(StockError::Invalid(_))
    ));
    assert!(matches!(
        Dexel::block(0.0, 0.0, 1.0, 1.0, 1.0, 0.0),
        Err(StockError::Invalid(_))
    ));
    assert!(matches!(
        Dexel::block(f64::NAN, 0.0, 1.0, 1.0, 1.0, 0.5),
        Err(StockError::Invalid(_))
    ));
    let err = Dexel::block(0.0, 0.0, 4097.0 * 2.0, 1.0, 1.0, 2.0)
        .expect_err("the cap must refuse a stadium at 2 mm");
    let msg = err.to_string();
    assert!(
        msg.contains("4097x1") && msg.contains("4096"),
        "the refusal names the numbers: {msg}"
    );
    // Empty facets: air everywhere, no panic, no surface.
    let part = stock_sim::part_surface(&[], 0.0, 0.0, 1.0, 3, 3);
    assert!(part.iter().all(|&z| z == f64::NEG_INFINITY));
    // A zero-radius disc still cuts the centers it exactly covers.
    let mut d = Dexel::block(0.0, 0.0, 2.0, 2.0, 1.0, 1.0).unwrap();
    d.cut_chord(p(0.5, 0.5, 0.0), p(0.5, 0.5, 0.0), 0.0);
    assert!(near(top_of(&d, 0, 0), 0.0), "r = 0 is a needle, not an error");
    assert!(near(top_of(&d, 1, 0), 1.0), "and reaches no further than a point");
    // The measure's tol law: a hair of rest under tol is not rest.
    let part = vec![0.9999995; d.nx * d.ny];
    let rep = stock_sim::measure(&d, &part, &rect(0.0, 0.0, 2.0, 2.0), 1e-6);
    assert_eq!(rep.rest_cells, 0, "sub-tol columns are cut, per the lattice law");
}

// ---- M2.2d: hidden rest, the solid truth behind the height field ----

/// A closed square slab [-6,6]^2 x [z0,z1]: two cap triangles per
/// face, two per wall. The walls project edge-on - no surface, no
/// claims - exactly how the kernel already treats them.
fn closed_slab(z0: f64, z1: f64) -> Vec<Triangle> {
    let cap = |z: f64| {
        vec![
            Triangle::new(p(-6., -6., z), p(6., -6., z), p(6., 6., z)),
            Triangle::new(p(-6., -6., z), p(6., 6., z), p(-6., 6., z)),
        ]
    };
    let mut f = cap(z0);
    f.extend(cap(z1));
    let wall = |q: [Point3; 4]| vec![Triangle::new(q[0], q[1], q[2]), Triangle::new(q[0], q[2], q[3])];
    f.extend(wall([p(-6., -6., z0), p(6., -6., z0), p(6., -6., z1), p(-6., -6., z1)]));
    f.extend(wall([p(6., -6., z0), p(6., 6., z0), p(6., 6., z1), p(6., -6., z1)]));
    f.extend(wall([p(6., 6., z0), p(-6., 6., z0), p(-6., 6., z1), p(6., 6., z1)]));
    f.extend(wall([p(-6., 6., z0), p(-6., -6., z0), p(-6., -6., z1), p(-6., 6., z1)]));
    f
}

/// Nine raster rows at tip height `z`, stepover 1.4 on the r = 3 ball -
/// fine enough that the height field itself promises near-zero rest.
fn raster(z: f64) -> Vec<Move> {
    (0..9)
        .map(|k| feed(p(-8.0, -5.3 + 1.4 * k as f64, z), p(8.0, -5.3 + 1.4 * k as f64, z)))
        .collect()
}

#[test]
fn hidden_rest_surfaces_under_a_ceiling() {
    // The M2.2b debt, paid: a floor slab 0..2 under a ceiling plate
    // 9..10. The path rides the plate top at z = 10, so the height
    // field sees rest ~ 0 - and the solid truth says 8 mm of slab is
    // standing untouched in the ceiling's shadow.
    let mut f = closed_slab(0.0, 2.0);
    f.extend(closed_slab(9.0, 10.0));
    let rep = stock_sim::envelope_scan(&f, &raster(10.0), 3.0, 0.1).expect("scan");
    assert!(rep.gouge_mm < 1e-9, "riding the ceiling cuts nothing deep: {}", rep.gouge_mm);
    assert!(
        rep.rest_mm > 7.0 && rep.rest_mm < 8.5,
        "hidden slab top reads ~8 out of reach: {}",
        rep.rest_mm
    );
    assert!(rep.at.x.abs() <= 6.001 && rep.at.y.abs() <= 6.001, "at the shadow, not thin air");
}

#[test]
fn a_flat_model_hides_nothing() {
    // The no-false-alarm half of the law: the same slab with no
    // ceiling over it is ordinary milled ground - the height field's
    // answer stands, the solid scan adds zero.
    let f = closed_slab(0.0, 2.0);
    let rep = stock_sim::envelope_scan(&f, &raster(2.0), 3.0, 0.1).expect("scan");
    assert!(rep.gouge_mm < 1e-9, "flat ride is flush: {}", rep.gouge_mm);
    assert!(rep.rest_mm <= 0.2, "nothing hides without a ceiling: {}", rep.rest_mm);
}

#[test]
fn an_open_shell_reports_unknown_not_material() {
    // solid_rest law (b) inside the kernel: a lone sheet's columns are
    // odd crossings - unknown - and the hidden scan must contribute
    // nothing, not guess a shadow.
    let f = closed_slab(0.0, 0.0001); // a plate so thin its caps coincide
    let rep = stock_sim::envelope_scan(&f, &raster(0.0), 3.0, 0.1).expect("scan");
    assert!(rep.rest_mm <= 0.2, "thin closed plate is flat ground: {}", rep.rest_mm);
    let sheet = vec![
        Triangle::new(p(-6., -6., 10.), p(6., -6., 10.), p(6., 6., 10.)),
        Triangle::new(p(-6., -6., 10.), p(6., 6., 10.), p(-6., 6., 10.)),
    ];
    let rep = stock_sim::envelope_scan(&sheet, &raster(10.0), 3.0, 0.1).expect("scan");
    assert!(rep.rest_mm <= 0.2, "open shells hide nothing: {}", rep.rest_mm);
}
