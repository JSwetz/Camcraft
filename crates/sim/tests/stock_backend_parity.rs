//! M7.1 C: the two stock engines answer the same thing. Every law the
//! column arm has judged with since M2.1 - the deviation verdict, the
//! one-op measure, the viewer's sheet, and every refusal word - runs
//! again through the field arm's ray march, and this suite holds the
//! two outputs against each other value for value. The divergences
//! the march cannot close (a polygon cylinder against an analytic
//! circle; the truncated cone's chimney above its own height) are
//! asserted in the blessed direction: the field never hides material
//! the column finds, and where it differs it shows more rest, not
//! less. The march's own error is the solver floor, not the cell:
//! `field_ray`'s entry law answers each column where the vertical
//! ray MEETS the swept solid, or at the cell boundary the straddled
//! plane proves - what is left is what a 40-round bisection leaves,
//! stated below as tolerances, never as a silent agreement.
//! Provenance: the parity law is this project's own design (Phase 7's
//! verification-by-second-engine), built on the dexel and SDF engines
//! their stages A/B specs describe - nothing from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::backend and
//! camcraft_sim::stock_sim::march (the verdict march), end to end.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock::{StockShape, CYLINDER_SEGMENTS};
use camcraft_sim::stock_sim::{
    color_classes, Backend, Deviation, SimOp, Stock, StockError, DEV_TOL, FIELD_BAND,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move {
        kind: MoveKind::Feed,
        from,
        to,
        feed: Some(500.0),
        arc: None,
        axis: None,
    }
}

/// The test block: a 10 x 10 x 5 box on unit cells, whose field
/// lattice lands exactly on the dexel's (centers 0.5 .. 9.5 both).
const TOP: f64 = 5.0;
fn block(b: Backend) -> Stock {
    Stock::block(b, 0.0, 0.0, 10.0, 10.0, TOP, 1.0).expect("test block")
}

fn sim(moves: &[Move], r: f64, ball: bool, cone: Option<f64>) -> (Stock, Stock) {
    let (mut c, mut f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    let ops = [SimOp { moves, radius: r, ball, cone }];
    c.simulate(&ops).expect("column program must run");
    f.simulate(&ops).expect("field program must run");
    (c, f)
}

/// A finished part whose top is a plane at `z` over the whole block.
fn plate(z: f64) -> Vec<Triangle> {
    cube(p(0.0, 0.0, z - 7.0), p(10.0, 10.0, z)).facets.to_vec()
}

/// The parity law, stated once: same lattice, same counts, same
/// aggregates, and every column's deviation the same number - within
/// the march's ulp floor, never more.
fn dev_parity(a: &Deviation, b: &Deviation, h: f64) {
    assert_eq!((a.cells_x, a.cells_y), (b.cells_x, b.cells_y), "lattice must match");
    assert_eq!(a.cell, b.cell);
    assert!((a.max_rest - b.max_rest).abs() <= h, "max_rest {:?} vs {:?}", a.max_rest, b.max_rest);
    assert!((a.max_gouge - b.max_gouge).abs() <= h, "max_gouge {:?} vs {:?}", a.max_gouge, b.max_gouge);
    assert_eq!(a.is_clean(), b.is_clean(), "the verdict word must match");
    // On a shared lattice (the block-built fixtures here) the
    // abstentions must line up column for column - a verdict one
    // engine can make and the other cannot is a law difference, not
    // noise. (The cylinder fixture meshing differs by construction and
    // carries its own looser word; it does not call this.)
    assert_eq!(a.rest_cells, b.rest_cells, "rest count");
    assert_eq!(a.gouge_cells, b.gouge_cells, "gouge count");
    assert!((a.rest_volume - b.rest_volume).abs() <= 1e-3, "rest volume");
    assert!((a.gouge_volume - b.gouge_volume).abs() <= 1e-3, "gouge volume");
    for (k, (x, y)) in a.dev.iter().zip(&b.dev).enumerate() {
        assert_eq!(x.is_nan(), y.is_nan(), "column {k}: one engine abstains, the other judges");
        if x.is_finite() {
            assert!((x - y).abs() <= h, "column {k}: dev {x} vs {y}");
        }
    }
}

#[test]
fn pocket_verdict_is_the_same_verdict() {
    // A r=2 flat plunge to z=2: 12 lattice columns are cut to the
    // floor (four corners sit past the disc), the other 88 stand 3 mm
    // of rest above the plate. Both engines must count the SAME 12.
    let moves = [feed(p(5.0, 5.0, 6.0), p(5.0, 5.0, 2.0))];
    let (c, f) = sim(&moves, 2.0, false, None);
    let dc = c.deviate(&plate(2.0), DEV_TOL).expect("column deviate");
    let df = f.deviate(&plate(2.0), DEV_TOL).expect("field deviate");
    dev_parity(&dc, &df, 1e-6);
    assert_eq!(dc.rest_cells, 88, "fixture sanity");
    assert!((dc.max_rest - 3.0).abs() <= 1e-6);
    assert_eq!(dc.gouge_cells, 0);
    assert!(!df.is_clean());
}

#[test]
fn measure_reports_the_same_removal() {
    use camcraft_sim::geo2d::{Loop, Point2, Region};
    let rect = |x0, y0, x1, y1| {
        Region::new(vec![Loop::new(vec![
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ])
        .unwrap()])
        .unwrap()
    };
    let model = plate(2.0);
    let moves = [feed(p(5.0, 5.0, 6.0), p(5.0, 5.0, 2.0))];
    let (c, f) = sim(&moves, 2.0, false, None);
    let foot = rect(-1.0, -1.0, 11.0, 11.0);
    let rc = c.measure(&model, &foot, DEV_TOL);
    let rf = f.measure(&model, &foot, DEV_TOL);
    // 12 columns x 3 mm x 1 mm^2, and the same rest against the plate.
    assert!((rc.removed_volume - 36.0).abs() <= 1e-6, "fixture sanity");
    for (x, y, h) in [
        // The field's floor is the solver's, not the lattice's: the
        // golden section's residual (~1e-7 per column, ghost-rest
        // side) SUMS over the twelve, where one column's dev_parity
        // check sees it alone.
        (rc.removed_volume, rf.removed_volume, 1e-5),
        (rc.rest_volume, rf.rest_volume, 1e-3),
        (rc.max_rest_height, rf.max_rest_height, 1e-6),
        (rc.gouge_volume, rf.gouge_volume, 1e-6),
        (rc.max_gouge_depth, rf.max_gouge_depth, 1e-6),
    ] {
        assert!((x - y).abs() <= h, "measure field mismatch {x} vs {y}");
    }
    assert_eq!(rc.rest_cells, rf.rest_cells);
    assert_eq!(rc.gouge_cells, rf.gouge_cells);
    // A footprint is a filter, not a re-plan: half the grid sees half
    // (the same summed solver floor as the whole-grid line above).
    let half = rect(-1.0, -1.0, 5.0, 11.0);
    let hc = c.measure(&model, &half, DEV_TOL);
    let hf = f.measure(&model, &half, DEV_TOL);
    assert!((hc.removed_volume - hf.removed_volume).abs() <= 1e-5);
    assert!(hc.removed_volume < rf.removed_volume, "the fixture must actually clip");
}

#[test]
fn heights_and_signs_agree_before_any_cut() {
    let (c, f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    assert_eq!(c.cell(), f.cell());
    for i in 0..10usize {
        for j in 0..10usize {
            let (x, y) = (i as f64 + 0.5, j as f64 + 0.5);
            let (tc, tf) = (c.top(x, y), f.top(x, y));
            assert!((tc - tf).abs() <= 1e-9, "untouched height {tc} vs {tf}");
            // Above the top: air by both; 0.5 down: material by both.
            assert!(c.distance(p(x, y, 5.5)) > 0.0 && f.distance(p(x, y, 5.5)) > 0.0);
            assert!(c.distance(p(x, y, 4.5)) < 0.0 && f.distance(p(x, y, 4.5)) < 0.0);
        }
    }
}

#[test]
fn ball_groove_is_the_same_surface() {
    // The exact sphere atom against the exact capsule solid: the same
    // closed-form surface, evaluated twice - once per column, once
    // per ray march. The march reads the groove where its ray enters
    // the capsule, so the difference is the solver floor, not the
    // lattice's cell - stated, never silent.
    let moves = [feed(p(3.0, 3.0, 3.0), p(7.0, 3.0, 3.0))];
    let (c, f) = sim(&moves, 2.0, true, None);
    let (dc, df) = (
        c.deviate(&plate(4.0), DEV_TOL).unwrap(),
        f.deviate(&plate(4.0), DEV_TOL).unwrap(),
    );
    dev_parity(&dc, &df, 1e-5);
    assert!(dc.gouge_cells > 0 && df.gouge_cells > 0, "the groove must bite");
}

#[test]
fn a_deep_ball_pass_clears_the_stock_above_it() {
    // M7.3 (the waterline-cone golden's finding): a ball running more
    // than its own diameter under the stock top. The column's atom
    // clears everything above the ball's bottom; a bare sphere left
    // the top 3 mm floating over the groove. The shank closes it.
    let moves = [feed(p(3.0, 5.0, 0.5), p(7.0, 5.0, 0.5))];
    let (c, f) = sim(&moves, 1.0, true, None);
    let (dc, df) = (c.deviate(&plate(1.0), DEV_TOL).unwrap(), f.deviate(&plate(1.0), DEV_TOL).unwrap());
    dev_parity(&dc, &df, 1e-5);
    assert!(dc.gouge_cells > 0, "fixture: the groove bottoms at 0.5, under the plate");
}

#[test]
fn deep_rest_is_heights_not_magnitudes() {
    // 7 mm of rest standing over the model - past the field's band cap
    // - reads the same number as it does on the column, because a
    // verdict marches heights and the capped magnitude only steers it.
    let (mut c, mut f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    let hover = [feed(p(5.0, 5.0, 6.0), p(5.0, 5.0, 6.0))];
    let ops = [SimOp { moves: &hover[..], radius: 2.0, ball: false, cone: None }];
    c.simulate(&ops).unwrap();
    f.simulate(&ops).unwrap(); // a tool hovering 1 mm up takes nothing
    let (dc, df) = (
        c.deviate(&plate(-2.0), DEV_TOL).unwrap(),
        f.deviate(&plate(-2.0), DEV_TOL).unwrap(),
    );
    dev_parity(&dc, &df, 1e-6);
    assert!((dc.max_rest - 7.0).abs() <= 1e-6, "fixture: 5 - (-2)");
    assert_eq!(dc.rest_cells, 100);
}

#[test]
fn rising_cut_clears_the_column_the_same_way() {
    // Up through the material to z=4: the column atom says "everything
    // above the move's lowest reach is gone"; the spanning flank gives
    // the field solid exactly that reach, so the verdicts match.
    let moves = [feed(p(5.0, 5.0, 1.0), p(5.0, 5.0, 4.0))];
    let (c, f) = sim(&moves, 2.0, false, None);
    let (dc, df) = (
        c.deviate(&plate(2.0), DEV_TOL).unwrap(),
        f.deviate(&plate(2.0), DEV_TOL).unwrap(),
    );
    dev_parity(&dc, &df, 1e-6);
    assert!((dc.max_gouge - 1.0).abs() <= 1e-6, "floor at 1 against plate 2");
    assert_eq!(dc.gouge_cells, 12);
}

#[test]
fn a_stationary_cut_is_a_plunge() {
    // The zero-length move: the reach window collapses to the tip
    // height on both engines; nothing degenerates.
    let moves = [feed(p(5.0, 5.0, 2.0), p(5.0, 5.0, 2.0))];
    let (c, f) = sim(&moves, 2.0, false, None);
    let (dc, df) = (
        c.deviate(&plate(2.0), DEV_TOL).unwrap(),
        f.deviate(&plate(2.0), DEV_TOL).unwrap(),
    );
    dev_parity(&dc, &df, 1e-6);
    assert_eq!(dc.gouge_cells, 0);
    assert_eq!(dc.rest_cells, 88);
}

#[test]
fn cone_atom_parity_below_its_own_truncation() {
    // A chamfer station: 45 deg, r=2 - the cone's flank is z = 3 + d,
    // and its truncation height (r * cot = 2, to the stock top) is
    // exactly where the box's own material ends, so this fit sees no
    // chimney divergence; the law above's blessed direction is for a
    // wall that stands over the cone's rim, which this block has none
    // of. The flank runs tilted through its cells at 45 deg, and the
    // march reads it at the ray-entry `field_ray` proves, not at the
    // lattice's convenience: the residue is the solver floor, and the
    // counts - twelve bitten columns, the d = 2.121 corners standing
    // untouched past the cone's own reach - are exact on both sides.
    let moves = [feed(p(5.0, 5.0, 3.0), p(5.0, 5.0, 3.0))];
    let (c, f) = sim(&moves, 2.0, false, Some(1.0));
    let (dc, df) = (
        c.deviate(&plate(5.0), DEV_TOL).unwrap(),
        f.deviate(&plate(5.0), DEV_TOL).unwrap(),
    );
    dev_parity(&dc, &df, 1e-5);
    assert_eq!(dc.gouge_cells, 12, "fixture sanity");
}

#[test]
fn the_viewer_sheet_is_the_same_sheet() {
    let moves = [feed(p(5.0, 5.0, 6.0), p(5.0, 5.0, 2.0))];
    let (c, f) = sim(&moves, 2.0, false, None);
    let (dc, df) = (
        c.deviate(&plate(2.0), DEV_TOL).unwrap(),
        f.deviate(&plate(2.0), DEV_TOL).unwrap(),
    );
    let sc = c.surface(&color_classes(&dc));
    let sf = f.surface(&color_classes(&df));
    assert_eq!(sc.tris.len(), sf.tris.len(), "same walls, same quads");
    assert_eq!(sc.verts.len(), sf.verts.len());
    for (a, b) in sc.verts.iter().zip(&sf.verts) {
        assert!((a.x - b.x).abs() <= 1e-9 && (a.y - b.y).abs() <= 1e-9);
        assert!((a.z - b.z).abs() <= 1e-6, "sheet height {a:?} vs {b:?}");
    }
    assert_eq!(sc.vcls, sf.vcls, "the map colors the same columns");
}

#[test]
fn cylinder_stock_agrees_where_both_own_the_column() {
    // 32 segments against an analytic hypot: the rim columns differ by
    // the polygon's sagitta and the corners abstain on both sides. The
    // interior verdicts, and the words on the extremes, must still match.
    let sh = StockShape::Cylinder { diameter: 10.0, height: 5.0 };
    let mut c = Stock::from_shape(Backend::Column, &sh, 1.0).unwrap();
    let mut f = Stock::from_shape(Backend::Field { band: FIELD_BAND }, &sh, 1.0).unwrap();
    assert_eq!(CYLINDER_SEGMENTS, 32, "fixture ties to the mesher");
    let moves = [feed(p(0.0, 0.0, 6.0), p(0.0, 0.0, 2.0))];
    let part = cube(p(-5.0, -5.0, -5.0), p(5.0, 5.0, 2.0)).facets.to_vec();
    // The plate spans the block; inside the round the untouched top
    // stands 3 over it, and the pocket floor sits on it clean.
    let ops = [SimOp { moves: &moves[..], radius: 2.0, ball: false, cone: None }];
    c.simulate(&ops).unwrap();
    f.simulate(&ops).unwrap();
    let (dc, df) = (c.deviate(&part, DEV_TOL).unwrap(), f.deviate(&part, DEV_TOL).unwrap());
    assert!((dc.max_rest - df.max_rest).abs() <= 1e-6, "untouched tops must agree");
    assert_eq!(dc.gouge_cells, df.gouge_cells);
    assert_eq!(dc.is_clean(), df.is_clean());
    // The far corner: no material on either engine - NaN, not zero.
    let (ck, fk) = (dc.dev.iter().filter(|v| v.is_nan()).count(), df.dev.iter().filter(|v| v.is_nan()).count());
    assert!(ck > 0, "column must abstain outside the round");
    assert!((ck as isize - fk as isize).abs() <= 8, "abstentions within the sagitta ring: {ck} vs {fk}");
}

#[test]
fn refusals_say_the_same_words_either_way() {
    fn sim_err(b: Backend, moves: &[Move], r: f64, ball: bool, cone: Option<f64>) -> String {
        let mut s = block(b);
        let ops = [SimOp { moves, radius: r, ball, cone }];
        s.simulate(&ops).err().map(|e| e.to_string()).expect("must refuse")
    }
    let m = [feed(p(5.0, 5.0, 2.0), p(5.0, 5.0, 2.0))];
    for (moves, r, ball, cone, what) in [
        (&m[..], 0.0, false, None, "radius"),
        (&m[..], -2.0, false, None, "radius"),
        (&m[..], f64::NAN, false, None, "radius"),
        (&m[..], 2.0, false, Some(f64::NAN), "cone slope"),
        (&m[..], 2.0, false, Some(0.0), "cone slope"),
    ] {
        let (a, b) = (sim_err(Backend::Column, moves, r, ball, cone), sim_err(Backend::Field { band: FIELD_BAND }, moves, r, ball, cone));
        assert_eq!(a, b, "refusal words differ for {what}");
    }
    // A non-finite move, and the contradictory tool, and the tol gate.
    let bad = [feed(p(5.0, 5.0, f64::NAN), p(5.0, 5.0, 2.0))];
    assert_eq!(sim_err(Backend::Column, &bad, 2.0, false, None), sim_err(Backend::Field { band: FIELD_BAND }, &bad, 2.0, false, None));
    assert!(sim_err(Backend::Column, &m, 2.0, true, Some(1.0)).contains("no tool is both"));
    let c = block(Backend::Column);
    let fd = block(Backend::Field { band: FIELD_BAND });
    let e1 = c.deviate(&plate(2.0), f64::NAN).err().unwrap().to_string();
    let e2 = fd.deviate(&plate(2.0), f64::NAN).err().unwrap().to_string();
    assert_eq!(e1, e2, "the tolerance gate is backend-independent");

    // Construction refusals, gate for gate.
    fn ctor<F>(f: F) -> (String, String)
    where
        F: Fn(Backend) -> Result<Stock, StockError>,
    {
        (
            f(Backend::Column).expect_err("must refuse").to_string(),
            f(Backend::Field { band: FIELD_BAND })
                .expect_err("must refuse")
                .to_string(),
        )
    }
    let (a, b) = ctor(|x| Stock::block(x, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0));
    assert_eq!(a, b);
    let (a, b) = ctor(|x| Stock::block(x, f64::NAN, 0.0, 10.0, 10.0, 5.0, 1.0));
    assert_eq!(a, b);
    let (a, b) = ctor(|x| Stock::block(x, 0.0, 0.0, 10.0, 10.0, 5.0, 0.001));
    assert_eq!(a, b);
    assert!(a.contains("4096"), "the grid ceiling is named: {a}");
    let (a, b) = ctor(|x| Stock::block(x, 0.0, 0.0, 10.0, 10.0, 5.0, 0.0));
    assert_eq!(a, b);
    let (a, b) = ctor(|x| {
        Stock::from_shape(x, &StockShape::Box { dx: 0.0, dy: 5.0, dz: 5.0 }, 1.0)
    });
    assert_eq!(a, b);
    let (a, b) = ctor(|x| {
        Stock::from_shape(x, &StockShape::Cylinder { diameter: -1.0, height: 5.0 }, 1.0)
    });
    assert_eq!(a, b);
    let (a, b) = ctor(|x| {
        Stock::from_shape(x, &StockShape::FromModel { source: "part".into() }, 1.0)
    });
    assert_eq!(a, b);
    assert!(a.contains("from_facets"), "and says what to do instead: {a}");
    let flat: Vec<Triangle> = Vec::new();
    let (a, b) = ctor(|x| Stock::from_facets(x, &flat, 1.0));
    assert_eq!(a, b);
    let (a, b) = ctor(|x| Stock::from_facets(x, &plate(2.0), f64::NAN));
    assert_eq!(a, b);
}

#[test]
fn a_field_only_truth_refuses_by_name_never_silently() {
    // The march's world has a floor the semi-infinite column lacks; a
    // watertightness demand is the parity's price. These refusals exist
    // on ONE arm - so the law is they must be loud where they fire. An
    // open shell (a single facet) is the field's to refuse; the column
    // arm has no opinion (a top it can read is a top) and the field
    // must not answer a verdict on parity it cannot establish.
    let open = vec![Triangle::new(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), p(10.0, 10.0, 0.0))];
    let col = Stock::from_facets(Backend::Column, &open, 1.0);
    let fld = Stock::from_facets(Backend::Field { band: FIELD_BAND }, &open, 1.0);
    match fld {
        Err(e) => assert!(!e.to_string().is_empty(), "a refusal must say something"),
        // If a future watertight-law relaxation lets it build, it must
        // still never panic and still answer NaN for air columns.
        Ok(s) => assert!(s.top(-50.0, -50.0).is_nan()),
    }
    assert!(col.is_ok(), "the column arm's opinion is unchanged");
}
