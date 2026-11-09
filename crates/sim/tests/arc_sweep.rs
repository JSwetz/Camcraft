//! #402: the column engine sweeps XY arcs EXACTLY. Inscribed chords
//! stood a sagitta nearer the arc's centre than the real G2/G3: on a
//! convex corner (centre = the part's corner) that clipped a column the
//! tool never touched and the height verdict read it as a full-wall
//! gouge; on a concave corner (the part outside the arc) the same chord
//! stayed short of the wall and hid a real gouge. The laws pinned here:
//! a column is cut iff the true arc's tool reaches it, to the depth a
//! dense scan of the true arc finds, for every atom, level or helical,
//! either direction; the issue's own column stands; and real gouges on
//! convex and concave arcs are still convicted.
//! Provenance: plane geometry and RS-274 helical interpolation; our own
//! code, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim.

use camcraft_sim::cl::{ArcMove, Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{deviate, Dexel, DEV_TOL};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Flat,
    Ball,
    Cone(f64),
    Bull(f64),
}

/// Lowest steel above the tip at horizontal distance `d <= r`.
fn lift(k: Kind, d: f64, r: f64) -> f64 {
    match k {
        Kind::Flat => 0.0,
        Kind::Ball => r - (r * r - d * d).max(0.0).sqrt(),
        Kind::Cone(cot) => cot * d,
        Kind::Bull(c) => {
            let e = (d - (r - c)).max(0.0);
            c - (c * c - e * e).max(0.0).sqrt()
        }
    }
}

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

fn subtract(d: &mut Dexel, m: &Move, r: f64, k: Kind) {
    // A coarse chord budget on purpose: XY arcs no longer read it.
    let tol = 0.5;
    match k {
        Kind::Flat => d.subtract(m, r, tol),
        Kind::Ball => d.subtract_ball(m, r, tol),
        Kind::Cone(cot) => d.subtract_cone(m, r, cot, tol),
        Kind::Bull(c) => d.subtract_bull(m, r, c, tol),
    }
}

/// XY distance from `(qx, qy)` to the arc's path (independent of the
/// kernel: bearing inside the sweep -> radial gap, else nearest end).
fn dist_to_arc(qx: f64, qy: f64, ox: f64, oy: f64, rad: f64, a0: f64, sweep: f64) -> f64 {
    let (wx, wy) = (qx - ox, qy - oy);
    let b = wy.atan2(wx);
    let off = if sweep > 0.0 { b - a0 } else { a0 - b }.rem_euclid(2.0 * PI);
    let ends = [a0, a0 + sweep].map(|a| (qx - ox - rad * a.cos()).hypot(qy - oy - rad * a.sin()));
    let e = ends[0].min(ends[1]);
    if off <= sweep.abs() { (wx.hypot(wy) - rad).abs().min(e) } else { e }
}

#[test]
fn a_level_disc_arc_cuts_exactly_the_columns_within_reach() {
    // CCW and CW, short and long sweeps, one straddling the +-pi seam:
    // a column drops iff its centre is within r of the TRUE arc, with
    // no sagitta band on either side.
    let cases = [(0.3, 1.2), (2.9, 0.8), (-2.0, -1.5), (1.0, 5.5), (-0.4, -4.0)];
    for (a0, sweep) in cases {
        for (rad, r) in [(6.0, 1.5), (3.0, 3.0), (1.2, 2.0)] {
            let mut d = Dexel::block(-10.0, -10.0, 10.0, 10.0, 5.0, 0.13).unwrap();
            d.subtract(&arc(0.07, -0.11, rad, a0, sweep, 1.0, 1.0), r, 0.5);
            for iy in 0..d.ny {
                for ix in 0..d.nx {
                    let q = d.center(ix, iy);
                    let dist = dist_to_arc(q.x, q.y, 0.07, -0.11, rad, a0, sweep);
                    if (dist - r).abs() < 1e-9 {
                        continue;
                    }
                    let got = d.top_at(ix + iy * d.nx);
                    let want = if dist < r { 1.0 } else { 5.0 };
                    assert_eq!(got, want, "a0 {a0} sweep {sweep} R {rad} r {r} at {q:?}: dist {dist}");
                }
            }
        }
    }
}

#[test]
fn every_atom_matches_a_dense_scan_of_the_true_arc() {
    // Level and helical arcs, CW and CCW, an arc wider than the tool
    // and one narrower (the ball's C > B bend): every column's cut is
    // the minimum over the true arc a brute-force scan finds - never
    // above it (no rest invented), never measurably below (no gouge).
    let atoms = [Kind::Flat, Kind::Ball, Kind::Cone(1.0 / 0.6), Kind::Bull(1.0)];
    let arcs = [
        (5.0, 0.4, 2.3, 2.0, 2.0),
        (5.0, 0.4, 2.3, 4.0, 1.0),
        (4.0, 2.5, -3.6, 0.5, 3.5),
        (1.5, -1.0, 5.0, 3.0, 0.0),
    ];
    let r = 2.5;
    for k in atoms {
        for (rad, a0, sweep, z0, z1) in arcs {
            let mut d = Dexel::block(-9.0, -9.0, 9.0, 9.0, 50.0, 0.29).unwrap();
            subtract(&mut d, &arc(0.0, 0.0, rad, a0, sweep, z0, z1), r, k);
            let (mut n, mut tight) = (0, 0);
            for iy in 0..d.ny {
                for ix in 0..d.nx {
                    let q = d.center(ix, iy);
                    let g = |t: f64| {
                        let a = a0 + sweep * t;
                        let dd = (q.x - rad * a.cos()).hypot(q.y - rad * a.sin());
                        if dd <= r { z0 + (z1 - z0) * t + lift(k, dd, r) } else { f64::INFINITY }
                    };
                    let (mut want, mut ts) = (f64::INFINITY, 0.0);
                    for s in 0..=6000 {
                        let t = s as f64 / 6000.0;
                        if g(t) < want {
                            (want, ts) = (g(t), t);
                        }
                    }
                    for s in 0..=4000 {
                        let t = (ts - 1.0 / 6000.0 + s as f64 / 2.0e6).clamp(0.0, 1.0);
                        want = want.min(g(t));
                    }
                    let got = d.top_at(ix + iy * d.nx);
                    if want.is_finite() {
                        assert!(
                            got <= want + 1e-9 && got >= want - 1e-2,
                            "{k:?} arc {rad}/{a0}/{sweep} z {z0}->{z1} at {q:?}: {got} vs scan {want}"
                        );
                        n += 1;
                        // The fine scan steps 5e-7 in t: a helix's
                        // window end is resolved to ~1e-5 in Z.
                        if (got - want).abs() < 1e-5 {
                            tight += 1;
                        }
                    } else if got != 50.0 {
                        // Only a window-edge graze the scan stepped over.
                        let dist = dist_to_arc(q.x, q.y, 0.0, 0.0, rad, a0, sweep);
                        assert!(dist <= r + 1e-9, "{k:?}: phantom cut at {q:?} ({dist} from the arc): {got}");
                    }
                }
            }
            assert!(n > 200, "{k:?}: {n} columns compared");
            assert!(tight * 10 >= n * 9, "{k:?} arc {rad}/{sweep}: {tight} of {n} within 1e-5 of the scan");
        }
    }
}

/// The issue's lattice and part: stock 90x70 from (-45,-35), cell
/// 0.375, top 28; the riser x < 10, y > -30, 25 high. The contour's
/// corner arc about the riser's corner (10,-30), R = r = 3, at `z`.
fn riser() -> Vec<Triangle> {
    cube(Point3::new(-45.0, -30.0, 0.0), Point3::new(10.0, 35.0, 25.0)).facets.to_vec()
}

fn corner_arc(rad: f64, z: f64) -> Move {
    // The issue's move 4 starts at (10.346, -32.980) on the R = 3 circle.
    let a0 = (-2.980f64).atan2(0.346);
    arc(10.0, -30.0, rad, a0, -a0, z, z)
}

#[test]
fn the_issue_402_convex_corner_column_stands() {
    let mut d = Dexel::block(-45.0, -35.0, 45.0, 35.0, 28.0, 0.375).unwrap();
    for z in [25.0, 20.0, 15.0] {
        d.subtract(&corner_arc(3.0, z), 3.0, d.cell * 0.25);
    }
    // Column (146, 13), centre (9.9375, -29.9375): 0.088 mm inside the
    // corner, 3.06 mm from the nearest true tool centre.
    let q = d.center(146, 13);
    assert!((q.x - 9.9375).abs() < 1e-12 && (q.y + 29.9375).abs() < 1e-12);
    assert_eq!(d.top_at(146 + 13 * d.nx), 28.0, "the real arc never reaches this column");
    let dev = deviate(&d, &riser(), DEV_TOL).unwrap();
    assert_eq!(dev.gouge_cells, 0, "a clean corner arc reads a gouge: {:?}", dev.max_gouge_at);
}

#[test]
fn a_convex_corner_arc_cut_too_tight_is_still_a_gouge() {
    // The planted twin: the same corner swept at R = 2.5 with r = 3,
    // the tool 0.5 mm into the riser's corner. Convicted, located at
    // the corner, the full 10 mm the riser stands above the cut.
    let mut d = Dexel::block(-45.0, -35.0, 45.0, 35.0, 28.0, 0.375).unwrap();
    d.subtract(&corner_arc(2.5, 15.0), 3.0, d.cell * 0.25);
    let dev = deviate(&d, &riser(), DEV_TOL).unwrap();
    assert!(dev.gouge_cells > 0, "a corner cut into the part must convict");
    assert!((dev.max_gouge - 10.0).abs() < 1e-9, "{}", dev.max_gouge);
    let at = dev.max_gouge_at.unwrap();
    assert!((at.x - 10.0).hypot(at.y + 30.0) < 3.0, "located at the corner: {at:?}");
}

/// A pocket corner's wall: material outside the radius `rw` about the
/// origin in the first quadrant, as the count of columns the cut put
/// below `wall_z` there (columns hugging `rw` within 1e-9 are skipped).
fn wall_hits(d: &Dexel, rw: f64, wall_z: f64) -> usize {
    let mut n = 0;
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let q = d.center(ix, iy);
            let rho = q.x.hypot(q.y);
            if q.x > 0.0 && q.y > 0.0 && rho > rw + 1e-9 && d.top_at(ix + iy * d.nx) < wall_z {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn a_concave_corner_arc_is_exact_on_the_wall_side() {
    // A 6 mm fillet wall, a 4 mm-radius cutter: the clean path is the
    // R = 2 quarter arc, which touches the wall and never crosses it.
    // Over-cut by 0.06 mm - under the 0.094 mm sagitta budget the
    // chords had at this cell, and on the side the inscribed chords
    // fell short of - the columns in that band are convicted, every
    // one the true arc reaches and no other.
    let (rw, r, cell) = (6.0, 4.0, 0.375);
    let clean = {
        let mut d = Dexel::block(-8.0, -8.0, 10.0, 10.0, 20.0, cell).unwrap();
        d.subtract(&arc(0.0, 0.0, rw - r, 0.0, PI / 2.0, 5.0, 5.0), r, cell * 0.25);
        d
    };
    assert_eq!(wall_hits(&clean, rw, 20.0), 0, "the clean fillet path crosses the wall");
    let over = 0.06;
    let mut d = Dexel::block(-8.0, -8.0, 10.0, 10.0, 20.0, cell).unwrap();
    d.subtract(&arc(0.0, 0.0, rw - r + over, 0.0, PI / 2.0, 5.0, 5.0), r, cell * 0.25);
    let mut want = 0;
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let q = d.center(ix, iy);
            let rho = q.x.hypot(q.y);
            let dist = dist_to_arc(q.x, q.y, 0.0, 0.0, rw - r + over, 0.0, PI / 2.0);
            if q.x > 0.0 && q.y > 0.0 && rho > rw + 1e-9 && dist <= r {
                want += 1;
            }
        }
    }
    assert!(want > 0, "the lattice must put a column in the over-cut band");
    assert_eq!(wall_hits(&d, rw, 20.0), want, "every wall column the true arc reaches, and no other");
}
