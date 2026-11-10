//! M2.1b: the dexel material model. The laws here are the ones the rest
//! pass and the stock report stand on: the exact reach-interval cut of a
//! chord, rapids touching nothing, XY arcs swept exactly (no chord
//! sagitta on either side, #402), the `above` mask in region
//! clothing, and the part surface read straight off the mesh.
//! Provenance: published CAM simulation practice (Z-map subtraction),
//! our own code, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::cl, camcraft_sim::geo2d, camcraft_sim::geometry, camcraft_sim::stock, camcraft_sim::stock_sim.

use camcraft_sim::cl::{ArcMove, Move, MoveKind};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim::{self, Dexel, StockError};

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
        rotary: None,
        kind: MoveKind::Feed,
        from,
        to,
        feed: Some(500.0),
        arc: None,
        axis: None,
    }
}

/// Cell `ix, iy` of a grid as a raw read - the tests inspect the field
/// directly because the field IS the law (one number per column; the
/// tiles that store it are an economy, never a different answer).
fn top_of(d: &Dexel, ix: usize, iy: usize) -> f64 {
    d.top_at(ix + iy * d.nx)
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
    assert!(d.tops().all(|z| z >= 0.0), "never below the chord");
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
fn xy_arcs_sweep_the_true_arc_whatever_the_chord_budget() {
    // A quarter arc R = 5 about the origin at z = -1, handed a chord
    // budget of 2.0 - more than the quarter's whole sagitta of 1.46.
    // The chord sim rode the one chord (5,0)-(0,5) and left the cell
    // at (3.5,3.5) standing, ~0.05 under the true arc. That was never
    // conservative: the same sagitta band cut toward the arc's centre
    // where the tool never went (#402's false gouge on a convex
    // corner) and stopped short of a concave wall (a real gouge
    // hidden). XY arcs now sweep the true arc: the budget is unread.
    let mut d = Dexel::block(0.0, 0.0, 6.0, 6.0, 2.0, 1.0).unwrap();
    let m = Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: p(5.0, 0.0, -1.0),
        to: p(0.0, 5.0, -1.0),
        feed: Some(500.0),
        arc: Some(ArcMove::xy(Point2::new(0.0, 0.0), true)),
        axis: None,
    };
    d.subtract(&m, 1.0, 2.0);
    assert!(near(top_of(&d, 3, 3), -1.0), "the true arc cuts its midline");
    assert!(near(top_of(&d, 4, 0), -1.0), "and the cell beside its start");
    assert!(
        near(top_of(&d, 0, 0), 2.0) && near(top_of(&d, 1, 1), 2.0),
        "the centre side beyond r stands: no sagitta band inward"
    );
    assert!(
        d.tops().all(|z| z >= -1.0),
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
fn hidden_rest_over_budget_says_not_judged() {
    // M13.1: the same ceiling fixture under a starved budget. The old
    // law answered "the stated zero" - a rest that read like a clean
    // shadow; now the report says the hidden half was NOT judged, the
    // rest number keeps only what the ridge law measured, and the
    // verify warning names the state.
    let mut f = closed_slab(0.0, 2.0);
    f.extend(closed_slab(9.0, 10.0));
    let full = stock_sim::envelope_scan(&f, &raster(10.0), 3.0, 0.1).expect("scan");
    assert_eq!(full.hidden, stock_sim::HiddenRest::Judged, "the shipped budget judges this fixture");
    for (budget, what) in [
        // M13.1 (#24): past the holding budget the scan streams in row
        // bands, so these starve what ONE ROW needs (a row's ~128
        // claims, a row's ~50 bin headers) or the walk itself.
        (stock_sim::ScanBudget { cells: usize::MAX, claims: 10, work: usize::MAX }, "claims"),
        (stock_sim::ScanBudget { cells: 10, claims: usize::MAX, work: usize::MAX }, "cells"),
        (stock_sim::ScanBudget { cells: usize::MAX, claims: usize::MAX, work: 100 }, "work"),
    ] {
        let rep = stock_sim::envelope_scan_within(&f, &raster(10.0), 3.0, 0.1, budget).expect("scan");
        let stock_sim::HiddenRest::OverBudget { cells, claims } = rep.hidden else {
            panic!("{what}: over budget must say unjudged, got {:?}", rep.hidden);
        };
        assert!(cells > 0, "{what}: the lattice is stated");
        if what == "claims" {
            assert!(claims > 10, "{what}: the claim count where it stopped: {claims}");
        }
        assert!(rep.rest_mm < 7.0, "{what}: the hidden 8 mm is NOT folded in: {}", rep.rest_mm);
        assert_eq!(rep.gouge_mm, full.gouge_mm, "{what}: the gouge half is untouched");
        let w = camcraft_sim::verify::Warning::hidden_unjudged(&rep, 3).expect("the warning");
        assert!(w.to_string().contains("NOT JUDGED"), "{what}: {w}");
    }
    assert_eq!(camcraft_sim::verify::Warning::hidden_unjudged(&full, 3), None, "judged says nothing");
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

// ---- M2.3 stage A: the program-level sim ------------------------------

fn flat_top(z: f64, hw: f64) -> Vec<Triangle> {
    vec![
        Triangle::new(p(-hw, -hw, z), p(hw, -hw, z), p(hw, hw, z)),
        Triangle::new(p(-hw, -hw, z), p(hw, hw, z), p(-hw, hw, z)),
    ]
}

fn plunge(x: f64, y: f64, to_z: f64, top_z: f64) -> Move {
    feed(p(x, y, top_z), p(x, y, to_z))
}

#[test]
fn program_chains_ops_in_order() {
    // A face traverse that stops short of its own right margin, and
    // a plunge that goes one millimeter past the model on the left:
    // rest is what the program never reached (subtraction is
    // monotonic - nothing can leave a plateau by cutting HIGH, only
    // by not arriving), and a gouge is what it reached too deep. A
    // third op that finishes the margin erases the rest, and no op
    // ever erases a gouge. Rapids touch nothing anywhere.
    let cell = stock_sim::auto_cell(2.0, 10.0, 10.0);
    let box_stock = camcraft_sim::stock::StockShape::Box { dx: 10.0, dy: 10.0, dz: 5.0 };
    let mut d = Dexel::from_shape(&box_stock, cell).expect("box stock grid");
    let o1 = [feed(p(-3.0, 0.0, 2.0), p(2.0, 0.0, 2.0))];
    let o2 = [
        Move {
            rotary: None,
            kind: MoveKind::Rapid,
            from: p(4.5, 4.5, 10.0),
            to: p(4.5, 4.5, 0.5),
            feed: None,
            arc: None,
            axis: None,
        },
        plunge(-2.5, -2.5, 1.0, 5.0),
    ];
    let ops = [
        stock_sim::SimOp { moves: &o1, radius: 2.5, ball: false, cone: None, corner: None },
        stock_sim::SimOp { moves: &o2, radius: 2.0, ball: false, cone: None, corner: None },
    ];
    stock_sim::simulate(&mut d, &ops).expect("chain");
    let model = flat_top(2.0, 3.2);
    let dev = stock_sim::deviate(&d, &model, 0.01).expect("deviate");
    assert!(!dev.is_clean());
    assert!(dev.rest_cells > 0 && dev.gouge_cells > 0, "both truths missing");
    // The locations ARE the point. Rest stands at a column of full
    // height - the sim's Z of 5.0 says "never reached" in numbers,
    // which is the whole content of a short traverse; the gouge
    // stands at the XY the plunge occupied, at its own Z of 1.0.
    let r_at = dev.max_rest_at.expect("rest located");
    let g_at = dev.max_gouge_at.expect("gouge located");
    // Any column of the too-deep disc is an honest location - name it
    // by what the viewer will ask: the grid there reads the plunge's
    // bottom, and it is inside the swept circle.
    let gc = d.col(Point2::new(g_at.x, g_at.y)).expect("gouge in grid");
    assert!(
        near(d.top_at(gc), 1.0),
        "gouge located at a column reading {}",
        d.top_at(gc)
    );
    assert!(
        (g_at.x + 2.5).hypot(g_at.y + 2.5) <= 2.0 + d.cell,
        "gouge located outside the plunge: {g_at:?}"
    );
    assert!(near(r_at.z, 5.0) && near(g_at.z, 1.0), "located at the wrong depth");
    // And the rapid's column: the path crossed it all the way down,
    // and the grid still reads full height.
    let kc = d.col(Point2::new(4.5, 4.5)).expect("in grid");
    assert!(near(d.top_at(kc), 5.0), "a rapid cut stock");
    // Op 3 is the pass that covers the plate end to end: rest gone,
    // the gouge still stands.
    let o3 = [feed(p(-3.2, 0.0, 2.0), p(3.2, 0.0, 2.0))];
    let ops3 = [
        stock_sim::SimOp { moves: &o1, radius: 2.5, ball: false, cone: None, corner: None },
        stock_sim::SimOp { moves: &o2, radius: 2.0, ball: false, cone: None, corner: None },
        stock_sim::SimOp { moves: &o3, radius: 3.2, ball: false, cone: None, corner: None },
    ];
    let mut d2 = Dexel::from_shape(&box_stock, cell).unwrap();
    stock_sim::simulate(&mut d2, &ops3).expect("chain 3");
    let dev2 = stock_sim::deviate(&d2, &model, 0.01).expect("deviate 2");
    assert_eq!(dev2.rest_cells, 0, "the fixer op left rest");
    assert!(dev2.gouge_cells > 0 && !dev2.is_clean(), "a gouge never un-happens");
    // A program that meets the model everywhere the model EXISTS is
    // clean: a traverse wide enough to sweep the whole part face, and
    // columns of pure air outside the model carry no verdict - the
    // 3 mm shelf of stock beside a small part is the machine's
    // world, not this program's debt.
    let mut d3 = Dexel::from_shape(&box_stock, cell).unwrap();
    let band = [feed(p(-3.0, 0.0, 2.0), p(3.0, 0.0, 2.0))];
    stock_sim::simulate(
        &mut d3,
        &[stock_sim::SimOp { moves: &band, radius: 3.5, ball: false, cone: None, corner: None }],
    )
    .expect("sweep");
    let dev3 = stock_sim::deviate(&d3, &flat_top(2.0, 3.2), 0.01).expect("deviate 3");
    assert!(dev3.is_clean(), "{} {}", dev3.max_rest, dev3.max_gouge);
}

#[test]
fn the_cylinders_world_is_air_beside_the_round() {
    // A sweep that leaves the circle cuts the round's flank and finds
    // phantom material nowhere: outside columns never existed, and
    // deviation over them is NaN - no verdict, never a zero.
    let mut d = Dexel::from_shape(
        &camcraft_sim::stock::StockShape::Cylinder { diameter: 10.0, height: 5.0 },
        0.5,
    )
    .unwrap();
    let moves = [feed(p(0.0, 0.0, 1.0), p(8.0, 0.0, 1.0))];
    stock_sim::simulate(
        &mut d,
        &[stock_sim::SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }],
    )
    .expect("sweep");
    // The witness column the first drafts missed: it must stand in
    // the MODEL's material while outside the stock's round - air in
    // the stock, real in the part, and the only honest answer there
    // is no answer.
    let far = d.col(Point2::new(4.2, 4.2)).expect("in bbox"); // center (4.25,4.25): 6.01 off-axis
    assert!(d.top_at(far) == f64::NEG_INFINITY, "air beside the round was cut");
    let on = d.col(Point2::new(3.0, 0.0)).expect("in grid");
    assert!(near(d.top_at(on), 1.0), "in reach and in material: cut");
    let dev = stock_sim::deviate(&d, &flat_top(2.0, 4.7), 0.01).expect("deviate");
    assert!(dev.dev[far].is_nan(), "a verdict where no stock stood");
}

#[test]
fn model_stock_stocks_the_model_itself() {
    // FromModel is facets, not the enum's numbers: the grid starts at
    // the mesh's own top, and degenerate meshes refuse at build.
    let mut d = Dexel::from_facets(&flat_top(5.0, 3.0), 0.5).unwrap();
    let inside = d.col(Point2::new(0.0, 0.0)).unwrap();
    assert!(near(d.top_at(inside), 5.0));
    let outside = d.col(Point2::new(4.0, 0.0));
    assert!(
        outside.is_none() || d.top_at(outside.unwrap()) == f64::NEG_INFINITY,
        "the bbox corner was material"
    );
    let moves = [plunge(0.0, 0.0, 1.0, 5.0)];
    stock_sim::simulate(
        &mut d,
        &[stock_sim::SimOp { moves: &moves, radius: 1.0, ball: false, cone: None, corner: None }],
    )
    .expect("cut");
    assert!(near(d.top_at(inside), 1.0));
    // Refusals, before any subtraction: empty, flat in XY, non-finite.
    assert!(Dexel::from_facets(&[], 0.5).is_err());
    let pin = vec![Triangle::new(p(1.0, 1.0, 0.0), p(1.0, 1.0, 5.0), p(1.0, 1.0, 2.0))];
    assert!(Dexel::from_facets(&pin, 0.5).is_err());
    let bad = vec![Triangle::new(p(0.0, 0.0, 0.0), p(f64::NAN, 1.0, 0.0), p(0.0, 1.0, 0.0))];
    assert!(Dexel::from_facets(&bad, 0.5).is_err());
    assert!(matches!(
        Dexel::from_shape(&camcraft_sim::stock::StockShape::FromModel { source: "m".into() }, 0.5),
        Err(StockError::Invalid(_))
    ));
    // And the shape's own degeneracies refuse like the block's do.
    assert!(Dexel::from_shape(
        &camcraft_sim::stock::StockShape::Box { dx: 0.0, dy: 4.0, dz: 4.0 },
        0.5
    )
    .is_err());
}

#[test]
fn simulate_refuses_before_touching_anything() {
    // A half-run grid that looks like a sim is the silent wrong
    // answer wearing a lab coat: validation happens op by op, first.
    let box_stock = camcraft_sim::stock::StockShape::Box { dx: 10.0, dy: 10.0, dz: 5.0 };
    let mut d = Dexel::from_shape(&box_stock, 0.5).unwrap();
    let before = d.clone();
    let bad_moves = [plunge(0.0, 0.0, f64::NAN, 5.0)];
    let ops = [stock_sim::SimOp { moves: &bad_moves, radius: 2.0, ball: false, cone: None, corner: None }];
    assert!(stock_sim::simulate(&mut d, &ops).is_err());
    assert_eq!(d, before, "the refusal cut anyway");
    let good = [plunge(0.0, 0.0, 1.0, 5.0)];
    let ops = [
        stock_sim::SimOp { moves: &good, radius: 2.0, ball: false, cone: None, corner: None },
        stock_sim::SimOp { moves: &bad_moves, radius: 2.0, ball: false, cone: None, corner: None },
    ];
    assert!(stock_sim::simulate(&mut d, &ops).is_err(), "op 2's poison ran the program");
    assert_eq!(d, before, "a refused program left its first op cut into the stock");
    let ops = [stock_sim::SimOp { moves: &good, radius: 0.0, ball: false, cone: None, corner: None }];
    assert!(stock_sim::simulate(&mut d, &ops).is_err());
}

#[test]
fn the_lattice_law_is_one_law() {
    // auto_cell IS envelope_scan's lattice: r/8 of the smallest tool,
    // a 100 um floor, grown to keep any span under 1024 columns.
    assert!(near(stock_sim::auto_cell(2.0, 10.0, 10.0), 0.25));
    assert!(near(stock_sim::auto_cell(8192.0, 1.0, 1.0), 1024.0));
    assert!(stock_sim::auto_cell(1e-9, 1e-6, 1e-6) >= 1e-4, "dust sank below the floor");
    assert!(
        stock_sim::auto_cell(2.0, 2046.0, 1.0) >= 2.0,
        "a stadium kept a tool-sized grid"
    );
}

#[test]
fn the_sim_is_deterministic_including_its_nans() {
    // The color map redraws this field every frame; two runs of one
    // program must agree to the bit, NaN patterns included.
    let run = || {
        let mut d = Dexel::from_shape(
            &camcraft_sim::stock::StockShape::Cylinder { diameter: 10.0, height: 5.0 },
            0.4,
        )
        .unwrap();
        let m1 = [feed(p(-4.0, 0.0, 2.0), p(4.0, 0.0, 2.0))];
        let m2 = [plunge(0.0, 2.0, 1.0, 2.0)];
        stock_sim::simulate(
            &mut d,
            &[
                stock_sim::SimOp { moves: &m1, radius: 1.5, ball: false, cone: None, corner: None },
                stock_sim::SimOp { moves: &m2, radius: 1.0, ball: true, cone: None, corner: None },
            ],
        )
        .unwrap();
        stock_sim::deviate(&d, &flat_top(2.0, 4.5), 0.02).unwrap()
    };
    let (a, b) = (run(), run());
    assert_eq!(a.dev.len(), b.dev.len());
    for (x, y) in a.dev.iter().zip(&b.dev) {
        assert_eq!(x.to_bits(), y.to_bits(), "a NaN differed from a NaN");
    }
    let loc = |p: Option<Point3>| p.map(|q| (q.x.to_bits(), q.y.to_bits()));
    assert_eq!(loc(a.max_rest_at), loc(b.max_rest_at));
    assert_eq!(loc(a.max_gouge_at), loc(b.max_gouge_at));
}

// ---- M5.3 E2: the tiled field's laws -------------------------------
// The tiles are an economy, not a new law: what the dense Vec
// answered, `top_at` answers, and what the dense Vec cost, a grid
// that was never cut must not.

#[test]
fn an_untouched_grid_costs_zero_tiles() {
    let d = Dexel::block(-100.0, -100.0, 100.0, 100.0, 5.0, 0.5).unwrap();
    assert_eq!(d.cells(), 400 * 400);
    assert_eq!(d.tiles_allocated(), 0, "untouched lattice is a fact about stock_z, not 4 MiB");
    assert!(d.tops().all(|z| near(z, 5.0)), "and every cell still answers it");
    // Reading past the end answers NaN - never the old index panic.
    assert!(d.top_at(d.cells() + 7).is_nan());
}

#[test]
fn a_cut_pays_only_the_tiles_it_reaches() {
    let mut d = Dexel::block(-100.0, -100.0, 100.0, 100.0, 5.0, 0.5).unwrap();
    // Plunge on a corner FOUR tiles share (cell 192,192 stands at
    // (-4,-4)): the footprint straddles all of them and no fifth.
    let m = [plunge(-4.0, -4.0, 1.0, 2.0)];
    stock_sim::simulate(
        &mut d,
        &[stock_sim::SimOp { moves: &m, radius: 3.0, ball: false, cone: None, corner: None }],
    )
    .expect("sweep");
    assert!(
        d.tiles_allocated() > 0 && d.tiles_allocated() <= 4,
        "{} tiles paid for one disc",
        d.tiles_allocated()
    );
    // The values are the old ones exactly: in reach the plunge's
    // bottom, out of reach the untouched face.
    let at = d.col(Point2::new(-4.0, -4.0)).unwrap();
    assert!(near(d.top_at(at), 1.0), "the plunge's own column");
    let far = d.col(Point2::new(99.75, 99.75)).unwrap();
    assert!(near(d.top_at(far), 5.0), "the corner of the world");
    assert!(d.tops().all(|z| near(z, 1.0) || near(z, 5.0)), "no third answer");
}

#[test]
fn equality_reads_values_not_bytes() {
    let fresh = Dexel::block(-10.0, -10.0, 10.0, 10.0, 5.0, 0.5).unwrap();
    let mut d = Dexel::block(-10.0, -10.0, 10.0, 10.0, 5.0, 0.5).unwrap();
    // The cut law at the face: lowering to the height that already
    // stands touches nothing - the tile stays the fact it was.
    let m = [plunge(0.0, 0.0, 5.0, 6.0)];
    stock_sim::simulate(
        &mut d,
        &[stock_sim::SimOp { moves: &m, radius: 3.0, ball: false, cone: None, corner: None }],
    )
    .expect("sweep");
    assert_eq!(d.tiles_allocated(), 0, "a cut to the standing height is not a cut");
    assert_eq!(d, fresh, "equal grids answer equal numbers");
    // And the fill law un-cuts everything back into nothing:
    let m = [plunge(0.0, 0.0, 1.0, 2.0)];
    stock_sim::simulate(
        &mut d,
        &[stock_sim::SimOp { moves: &m, radius: 3.0, ball: false, cone: None, corner: None }],
    )
    .expect("cut");
    assert!(d.tiles_allocated() > 0, "a real cut paid a real tile");
    d.fill(5.0);
    assert_eq!(d.tiles_allocated(), 0, "fill to the stock face is representable as nothing");
    assert_eq!(d, fresh);
}

#[test]
fn the_cylinder_masks_air_by_tiles() {
    let d = Dexel::from_shape(
        &camcraft_sim::stock::StockShape::Cylinder { diameter: 100.0, height: 5.0 },
        0.5,
    )
    .unwrap();
    // 200x200 cells = 16 tiles: the round's core answers height, the
    // world beside it answers air, and the tile wholly inside the
    // circle proves the wholly-material law costs nothing.
    let core = d.col(Point2::new(0.0, 0.0)).unwrap();
    assert!(near(d.top_at(core), 5.0));
    let corner = d.col(Point2::new(49.9, 49.9)).unwrap(); // 70.6 off-axis
    assert!(d.top_at(corner) == f64::NEG_INFINITY, "the bbox corner stands as material");
    assert!(d.tiles_allocated() < 16, "the middle tile was never even written");
    // The ring keeps the old hypot law: first air out, last material
    // in, cell centers strictly past the radius are air.
    let just_in = d.col(Point2::new(49.75, 0.0)).unwrap(); // center 49.75
    assert!(near(d.top_at(just_in), 5.0), "49.75 stands inside the 50.0 round");
}

#[test]
fn model_stock_collapses_uniform_runs() {
    let d = Dexel::from_facets(&flat_top(5.0, 40.0), 0.5).unwrap();
    // 162x162 cells = 9 tiles over a flat square: the mesh's own top
    // everywhere the facets cover, air in the margin ring - and the
    // wholly-material middle collapses to nothing at all.
    let core = d.col(Point2::new(0.0, 0.0)).unwrap();
    assert!(near(d.top_at(core), 5.0), "the mesh's top stands");
    let air = d.col(Point2::new(-40.4, -40.4)).unwrap(); // margin cell
    assert!(d.top_at(air) == f64::NEG_INFINITY, "the margin is air");
    assert!(d.tiles_allocated() < 9, "the mesh's own face costs no per-cell memory");
}

#[test]
fn the_cone_atom_leaves_the_face_the_chamfer_promised() {
    // M2.4 B's verdict: the CL convention says a chamfer path's moves
    // carry the virtual sharp ON the wall at the corner depth, and the
    // face law says the surviving surface is 45 degrees from there.
    // This runs the cone atom - the chamfer mill, tip-first, `cone`
    // the cotangent of the half-angle from the axis (cot(45) = 1 for
    // the shop's 90-degree tool) - over a wall and reads the answer
    // off the field: a column at distance d from the axis stands at
    // `corner + |d|` while that is under the top plane, and nothing
    // anywhere stands below the corner. The kernel promised the face;
    // the sim is what keeps it honest.
    let mut d = Dexel::block(0.0, 0.0, 10.0, 10.0, 5.0, 1.0).unwrap();
    let wall_x = 9.0; // the corner depth the kernel posts for top 5,
    let corner_z = 3.0; // width 2 at 45 degrees: 5 - 2 * tan(45).
    let o = [feed(p(wall_x, 1.0, corner_z), p(wall_x, 9.0, corner_z))];
    let ops = [stock_sim::SimOp { moves: &o, radius: 3.0, ball: false, cone: Some(1.0), corner: None }];
    stock_sim::simulate(&mut d, &ops).expect("cone op");
    // Row whose centers stand on the traverse: iy = 5 reads y = 5.5.
    for ix in 0..d.nx {
        let dist = (ix as f64 + 0.5 - wall_x).abs();
        let want = (corner_z + dist).min(5.0);
        assert!(
            (top_of(&d, ix, 5) - want).abs() <= 0.15,
            "column {ix} (|d| = {dist}) reads {}, the face says {want}",
            top_of(&d, ix, 5)
        );
        assert!(top_of(&d, ix, 5) >= corner_z - 1e-9, "a gouge below the corner at column {ix}");
    }
    // The slope in numbers: inside the reach, every cell steps one
    // cell's height - tan(45) read across the grid.
    for ix in 0..d.nx - 1 {
        let a = top_of(&d, ix, 5);
        let b = top_of(&d, ix + 1, 5);
        if a < 4.9 && b < 4.9 {
            assert!((a - b).abs() <= 1.0 + 0.3, "the face steepened or flattened between {ix} and {}", ix + 1);
        }
    }
}
