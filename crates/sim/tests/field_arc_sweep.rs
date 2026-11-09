//! #413: the field engine sweeps XY arcs on the TRUE circle, as the
//! column engine has since #402. Inscribed chords stood a sagitta
//! nearer the arc's centre than the real G2/G3: on a convex corner the
//! displayed stock lost material the tool never touched, and on a
//! concave corner or a bull-nose helix it kept material the tool cut
//! (a rim column the true arc cuts 2.5 mm deep stood uncut). The laws
//! pinned here: both engines read the same top over every column the
//! arc reaches, for every tool, level or helical, either direction;
//! the bull helix's rim columns are cut on the field too; a clean
//! convex corner reads no gouge while a planted one still convicts;
//! and a concave fillet's wall loses exactly the columns the true arc
//! reaches.
//! Provenance: plane geometry and RS-274 helical interpolation; our own
//! code, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::field_arc and the
//! arc arms of camcraft_sim::stock_sim::{field_cut, field_sweep,
//! field_ray}.

use camcraft_sim::cl::{ArcMove, Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{Backend, SimOp, Stock, DEV_TOL, FIELD_BAND};
use std::f64::consts::PI;

const FIELD: Backend = Backend::Field { band: FIELD_BAND };

/// An XY arc about `(ox, oy)` of radius `rad`, from bearing `a0` swept
/// `sweep` radians (positive = CCW), Z from `z0` to `z1`.
fn arc(ox: f64, oy: f64, rad: f64, a0: f64, sweep: f64, z0: f64, z1: f64) -> Move {
    let a1 = a0 + sweep;
    Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: Point3::new(ox + rad * a0.cos(), oy + rad * a0.sin(), z0),
        to: Point3::new(ox + rad * a1.cos(), oy + rad * a1.sin(), z1),
        feed: Some(500.0),
        arc: Some(ArcMove::xy(Point2::new(ox, oy), sweep > 0.0)),
        axis: None,
    }
}

/// XY distance from `(qx, qy)` to the arc's path (bearing inside the
/// sweep -> radial gap, else the nearest end).
fn dist_to_arc(qx: f64, qy: f64, ox: f64, oy: f64, rad: f64, a0: f64, sweep: f64) -> f64 {
    let (wx, wy) = (qx - ox, qy - oy);
    let off = if sweep > 0.0 { wy.atan2(wx) - a0 } else { a0 - wy.atan2(wx) }.rem_euclid(2.0 * PI);
    let ends = [a0, a0 + sweep].map(|a| (qx - ox - rad * a.cos()).hypot(qy - oy - rad * a.sin()));
    let e = ends[0].min(ends[1]);
    if off <= sweep.abs() { (wx.hypot(wy) - rad).abs().min(e) } else { e }
}

#[derive(Clone, Copy, Debug)]
enum Tool {
    Flat,
    Ball,
    Cone(f64),
    Bull(f64),
}

fn op(moves: &[Move], r: f64, t: Tool) -> SimOp<'_> {
    let (ball, cone, corner) = match t {
        Tool::Flat => (false, None, None),
        Tool::Ball => (true, None, None),
        Tool::Cone(cot) => (false, Some(cot), None),
        Tool::Bull(c) => (false, None, Some(c)),
    };
    SimOp { moves, radius: r, ball, cone, corner }
}

/// The column centres of a block from `(x0, y0)`, `n` by `n` cells.
fn centres(x0: f64, y0: f64, cell: f64, n: usize) -> impl Iterator<Item = (f64, f64)> {
    (0..n * n).map(move |k| (x0 + ((k % n) as f64 + 0.5) * cell, y0 + ((k / n) as f64 + 0.5) * cell))
}

#[test]
fn the_field_cuts_every_tool_on_the_true_arc_like_the_column() {
    // Level and helical, CW and CCW, a sweep past half a turn and one
    // whose radius is under the tool's: away from the reach rim (where
    // the engines' tie laws, not the arc, decide), the two engines read
    // the same top to the straight moves' 1e-5. The rim band is a half
    // cell wide, and a skim under a cell deep is skipped too: a sliver
    // of cut thinner than that is under the field's own sampling (a
    // straight move's reads the same way).
    let (cell, n, top) = (0.4, 30usize, 5.0);
    let base = |b| Stock::block(b, -6.0, -6.0, 6.0, 6.0, top, cell).expect("test block");
    let (col0, fld0) = (base(Backend::Column), base(FIELD));
    // The cone is steep enough to stand over the stock top from every
    // tip (the field's cone has no shank above its widest radius).
    let tools = [Tool::Flat, Tool::Ball, Tool::Cone(2.5), Tool::Bull(1.0)];
    let arcs = [(3.5, 0.4, 2.3, 2.0, 2.0), (3.5, 0.4, 2.3, 2.5, 1.0), (3.0, 2.5, -3.6, 0.5, 2.5), (1.5, -1.0, 5.0, 2.0, 0.5)];
    let r = 2.0;
    for t in tools {
        for (rad, a0, sweep, z0, z1) in arcs {
            let m = [arc(0.0, 0.0, rad, a0, sweep, z0, z1)];
            let (mut c, mut f) = (col0.clone(), fld0.clone());
            c.simulate(&[op(&m, r, t)]).expect("column program");
            f.simulate(&[op(&m, r, t)]).expect("field program");
            let mut cut = 0;
            for (qx, qy) in centres(-6.0, -6.0, cell, n) {
                let d = dist_to_arc(qx, qy, 0.0, 0.0, rad, a0, sweep);
                if (d - r).abs() < 0.5 * cell {
                    continue; // the reach rim: sampling, not arc law
                }
                // An uncut column reads each engine's own block top (the
                // field's sits on its lattice).
                let (zc, zf) = (c.top(qx, qy), f.top(qx, qy));
                let zc = if zc >= top { fld0.top(qx, qy) } else { zc };
                if zc < top && zc > top - cell {
                    continue; // a skim thinner than a cell: field sampling
                }
                assert!(
                    (zc - zf).abs() <= 1e-5,
                    "{t:?} arc {rad}/{a0}/{sweep} z {z0}->{z1} at ({qx}, {qy}), {d} from the arc: column {zc} vs field {zf}"
                );
                if zf < fld0.top(qx, qy) {
                    cut += 1;
                }
            }
            assert!(cut > 50, "{t:?} arc {rad}/{sweep}: only {cut} columns cut");
        }
    }
}

#[test]
fn the_bull_helix_rim_is_cut_on_the_field_too() {
    // `stock_bull_nose`'s helix: centre (6,6), R 3, (9,6,3) -> (3,6,2)
    // CCW, bull r 2 corner 0.75. The chorded field stood at the stock
    // top over rim columns the true arc cuts ~2.5 mm deep; now every
    // column the true arc reaches is cut, as deep as the column says.
    let (cell, top) = (0.5, 5.0);
    let m = [arc(6.0, 6.0, 3.0, 0.0, PI, 3.0, 2.0)];
    let mut c = Stock::block(Backend::Column, 0.0, 0.0, 12.0, 12.0, top, cell).expect("block");
    let uncut = Stock::block(FIELD, 0.0, 0.0, 12.0, 12.0, top, cell).expect("block");
    let mut f = uncut.clone();
    c.simulate(&[op(&m, 2.0, Tool::Bull(0.75))]).expect("column program");
    f.simulate(&[op(&m, 2.0, Tool::Bull(0.75))]).expect("field program");
    let (mut rim, mut deep) = (0, 0.0f64);
    for (qx, qy) in centres(0.0, 0.0, cell, 24) {
        let d = dist_to_arc(qx, qy, 6.0, 6.0, 3.0, 0.0, PI);
        let (zc, zf) = (c.top(qx, qy), f.top(qx, qy));
        let zc = if zc >= top { uncut.top(qx, qy) } else { zc };
        assert!((zc - zf).abs() <= 1e-5, "({qx}, {qy}), {d} from the arc: column {zc} vs field {zf}");
        // The outer band of the reach, where the chords fell short.
        if d < 2.0 && d > 1.75 && zc < top {
            rim += 1;
            deep = deep.max(top - zf);
        }
    }
    assert!(rim > 0, "the rim band must hold a cut column");
    assert!(deep > 2.0, "the rim is cut deep on the field: {deep}");
}

/// The riser's corner: x < CX, y > CY, 25 high. Placed (as #402's
/// was, by accident) so a lattice column stands just inside it, 0.06 mm
/// from the corner, opposite the middle chord of the corner arc below:
/// the inscribed chords' sagitta (0.087 mm) reached it, the true arc,
/// standing exactly `r` from the corner, never does.
const CX: f64 = 9.9375 + 0.045;
const CY: f64 = -29.8125 - 0.040;
const PROBE: (f64, f64) = (9.9375, -29.8125);

fn riser() -> Vec<Triangle> {
    cube(Point3::new(-20.0, CY, 0.0), Point3::new(CX, -10.0, 25.0)).facets.to_vec()
}

/// The #402 contour's corner arc about the riser's corner, R `rad`.
fn corner_arc(rad: f64, z: f64) -> Move {
    let a0 = (-2.980f64).atan2(0.346);
    arc(CX, CY, rad, a0, -a0, z, z)
}

/// The #402 cell size (0.375) over the corner.
fn corner_stock() -> Stock {
    Stock::block(FIELD, -4.5, -35.0, 16.5, -23.0, 28.0, 0.375).expect("corner block")
}

#[test]
fn a_clean_convex_corner_arc_reads_no_gouge_on_the_field() {
    let uncut = corner_stock();
    let mut f = uncut.clone();
    let moves: Vec<Move> = [25.0, 20.0, 15.0].map(|z| corner_arc(3.0, z)).to_vec();
    f.simulate(&[op(&moves, 3.0, Tool::Flat)]).expect("field program");
    let (px, py) = PROBE;
    assert!((px - CX).hypot(py - CY) < 0.087 && px < CX && py > CY, "the probe sits inside the corner");
    assert_eq!(f.top(px, py), uncut.top(px, py), "the real arc never reaches this column");
    let dev = f.deviate(&riser(), DEV_TOL).expect("field deviate");
    assert_eq!(dev.gouge_cells, 0, "a clean corner arc reads a gouge: {:?}", dev.max_gouge_at);
}

#[test]
fn a_convex_corner_arc_cut_too_tight_is_still_a_gouge_on_the_field() {
    let mut f = corner_stock();
    let m = [corner_arc(2.5, 15.0)];
    f.simulate(&[op(&m, 3.0, Tool::Flat)]).expect("field program");
    let dev = f.deviate(&riser(), DEV_TOL).expect("field deviate");
    assert!(dev.gouge_cells > 0, "a corner cut into the part must convict");
    assert!((dev.max_gouge - 10.0).abs() < 1e-3, "{}", dev.max_gouge);
    let at = dev.max_gouge_at.expect("located");
    assert!((at.x - CX).hypot(at.y - CY) < 3.0, "located at the corner: {at:?}");
}

#[test]
fn a_concave_fillet_wall_loses_exactly_the_columns_the_true_arc_reaches() {
    // A 6 mm fillet wall, a 4 mm flat cutter: the clean R 2 quarter arc
    // touches the wall and never crosses it; over-cut by 0.06 mm (under
    // the 0.094 mm sagitta the chords had, on the side they fell
    // short of) the field cuts every wall column the true arc reaches
    // and no other.
    let (rw, r, cell) = (6.0, 4.0, 0.375);
    let base = Stock::block(FIELD, -7.5, -7.5, 9.75, 9.75, 20.0, cell).expect("fillet block");
    let n = 46usize;
    let wall_hits = |s: &Stock, path_r: f64| {
        let (mut got, mut want) = (0, 0);
        for (qx, qy) in centres(-7.5, -7.5, cell, n) {
            let rho = qx.hypot(qy);
            if !(qx > 0.0 && qy > 0.0 && rho > rw + 1e-9) {
                continue;
            }
            let d = dist_to_arc(qx, qy, 0.0, 0.0, path_r, 0.0, PI / 2.0);
            if (d - r).abs() < 1e-6 {
                continue; // on the reach rim itself
            }
            if s.top(qx, qy) < 20.0 {
                got += 1;
            }
            if d < r {
                want += 1;
            }
        }
        (got, want)
    };
    let mut clean = base.clone();
    let m = [arc(0.0, 0.0, rw - r, 0.0, PI / 2.0, 5.0, 5.0)];
    clean.simulate(&[op(&m, r, Tool::Flat)]).expect("clean program");
    assert_eq!(wall_hits(&clean, rw - r), (0, 0), "the clean fillet path crosses the wall");
    let over = 0.06;
    let mut cut = base.clone();
    let m = [arc(0.0, 0.0, rw - r + over, 0.0, PI / 2.0, 5.0, 5.0)];
    cut.simulate(&[op(&m, r, Tool::Flat)]).expect("over-cut program");
    let (got, want) = wall_hits(&cut, rw - r + over);
    assert!(want > 0, "the lattice must put a column in the over-cut band");
    assert_eq!(got, want, "every wall column the true arc reaches, and no other");
}


