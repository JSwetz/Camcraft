//! M13.1: the bull nose cuts as itself. The field's swept bull solid
//! is held against a brute-force reference written independently of
//! `field_bull` (a dense scan over the move parameter of the exact
//! distance to the profile's boundary as a polyline - top, flank,
//! floor and a finely chorded corner arc), and the two stock engines
//! are held against each other on bull-nose programs the way the
//! M7.1 C parity suite holds them on the ball, the cone and the disc.
//! The corner's own claim is pinned too: beside a fillet wall the
//! bull leaves material a flat end mill removes.
//! Provenance: the rounded-rectangle profile of the published 2-D
//! distance catalogue revolved (I. Quilez, distfunctions /
//! distfunctions2d), the swept-volume law (distance to a sweep = min
//! over the trajectory of the distance to the moved solid), and this
//! codebase's own parity law - nothing from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::field_bull,
//! camcraft_sim::stock_sim::dexel_bull, and the bull arms of
//! camcraft_sim::stock_sim::{field_cut, backend, cut, display}.

use camcraft_sim::cl::{ArcMove, ArcPlane, Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{Backend, Deviation, SdfField, SdfTool, SimOp, Stock, DEV_TOL, FIELD_BAND};

type V3 = [f64; 3];

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn mv(from: Point3, to: Point3, axis: Option<V3>) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis }
}

fn seg2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((px - ax) * dx + (py - ay) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (px - (ax + dx * t)).hypot(py - (ay + dy * t))
}

/// The reference pose distance: the profile's boundary as a polyline
/// (the arc in 128 chords, sagitta ~8e-5 of the corner), the nearest
/// piece by exact point-segment distance, the sign by membership.
fn bull_pose_ref(y: V3, ax: V3, r: f64, c: f64, h: f64) -> f64 {
    let zl = y[0] * ax[0] + y[1] * ax[1] + y[2] * ax[2];
    let rho = (y[0] * y[0] + y[1] * y[1] + y[2] * y[2] - zl * zl).max(0.0).sqrt();
    let k = r - c;
    let mut best = seg2(rho, zl, 0.0, h, r, h) // top
        .min(seg2(rho, zl, r, c, r, h)) // flank
        .min(seg2(rho, zl, 0.0, 0.0, k, 0.0)); // floor
    let n = 128;
    for i in 0..n {
        let th = |j: usize| -std::f64::consts::FRAC_PI_2 * (1.0 - j as f64 / n as f64);
        let (a, b) = (th(i), th(i + 1));
        best = best.min(seg2(rho, zl, k + c * a.cos(), c + c * a.sin(), k + c * b.cos(), c + c * b.sin()));
    }
    let member = (0.0..=h).contains(&zl) && rho <= r && (rho <= k || zl >= c || (rho - k).hypot(zl - c) <= c);
    if member {
        -best
    } else {
        best
    }
}

/// The sweep's distance by dense parameter scan, refined beside the
/// coarse best (the map is convex in the parameter).
fn sweep_ref(q: Point3, a: Point3, b: Point3, ax: V3, r: f64, c: f64, h: f64) -> f64 {
    let at = |t: f64| {
        let y = [q.x - (a.x + (b.x - a.x) * t), q.y - (a.y + (b.y - a.y) * t), q.z - (a.z + (b.z - a.z) * t)];
        bull_pose_ref(y, ax, r, c, h)
    };
    let n = 100;
    let (mut best, mut tb) = (f64::INFINITY, 0.0);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let v = at(t);
        if v < best {
            (best, tb) = (v, t);
        }
    }
    for i in 0..=100 {
        let t = (tb - 1.0 / n as f64 + i as f64 * 2.0 / (n as f64 * 100.0)).clamp(0.0, 1.0);
        best = best.min(at(t));
    }
    best
}

/// The sweep's interior depth from its support function: the sweep
/// is the Minkowski sum of the (convex) bull and the segment, so its
/// support is `h_bull(n) + max(n.a, n.b)` and the depth of an inside
/// point is the least slab gap `h(n) - n.q` over directions - here
/// over a Fibonacci sphere of 20000, a minimum that can only read a
/// hair DEEPER than the truth.
fn support_depth(q: Point3, a: Point3, b: Point3, ax: V3, r: f64, c: f64, h: f64) -> f64 {
    let k = r - c;
    let hb = |n: V3| {
        let na = n[0] * ax[0] + n[1] * ax[1] + n[2] * ax[2];
        let s = (1.0 - na * na).max(0.0).sqrt(); // the radial share
        let lin = |rho: f64, zl: f64| s * rho + na * zl;
        let mut v = lin(0.0, h).max(lin(r, h)).max(lin(r, c)).max(lin(k, 0.0)).max(lin(0.0, 0.0));
        if na <= 0.0 {
            v = v.max(s * k + na * c + c * s.hypot(na)); // the arc's own tangent point
        }
        v
    };
    let m = 20_000;
    let golden = std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
    let mut best = f64::INFINITY;
    for i in 0..m {
        let z = 1.0 - 2.0 * (i as f64 + 0.5) / m as f64;
        let rr = (1.0 - z * z).sqrt();
        let th = golden * i as f64;
        let n = [rr * th.cos(), rr * th.sin(), z];
        let dot = |p: Point3| n[0] * p.x + n[1] * p.y + n[2] * p.z;
        best = best.min(hb(n) + dot(a).max(dot(b)) - dot(q));
    }
    best
}

/// The closed 12-facet box `[lo, hi]^3`.
fn box_mesh(lo: f64, hi: f64) -> Vec<Triangle> {
    let v = |i: usize| p([lo, hi][i & 1], [lo, hi][(i >> 1) & 1], [lo, hi][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut t = Vec::with_capacity(12);
    for q in [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)] {
        t.extend(q);
    }
    t
}

/// Cut one bull move into a fresh 10 mm box field on half-mm cells
/// and hold every cell center against the law `after == max(before,
/// min(-phi_ref, cap))`.
fn field_law(m: &Move, r: f64, c: f64, h: f64, label: &str) {
    let mut f = SdfField::from_mesh(&box_mesh(0.0, 10.0), 0.5, 4).expect("box field");
    let mut pts = Vec::new();
    for i in -1..=11i32 {
        for j in -1..=11 {
            for k in -1..=11 {
                pts.push(p(i as f64 + 0.25, j as f64 + 0.75, k as f64 + 0.25));
            }
        }
    }
    let before: Vec<f64> = pts.iter().map(|q| f.distance(*q)).collect();
    f.subtract(m, &SdfTool::BullNose { r, corner: c, flank: h }, 0.01).expect("the bull cuts");
    let cap = f.cap();
    let ax = m.axis.map_or([0.0, 0.0, 1.0], |a| {
        let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / n, a[1] / n, a[2] / n]
    });
    let reach = r.hypot(h) + cap + 1.0;
    let (mut judged, mut moved) = (0, 0);
    for (q, b) in pts.iter().zip(&before) {
        // a cheap lower bound first: far points claim nothing
        if seg3(*q, m.from, m.to) > reach {
            assert_eq!(f.distance(*q), *b, "{label}: a far point moved at {q:?}");
            continue;
        }
        let phi = sweep_ref(*q, m.from, m.to, ax, r, c, h);
        if (phi - cap).abs() < 1e-2 {
            continue; // the store-skip's border: either answer is the law
        }
        let got = f.distance(*q);
        if phi >= 0.0 {
            // Outside the sweep the min over the trajectory IS the
            // distance (distance to a union): exact agreement.
            let want = b.max((-phi).min(cap));
            assert!((got - want).abs() <= 1e-3, "{label}: at {q:?} field {got}, reference law {want} (before {b}, phi {phi})");
        } else {
            // Inside, the deepest single pose under-reads the sweep's
            // depth (the field may answer it, the conservative read)
            // and the support-function depth over-reads it a hair (a
            // sampled minimum): the exact depth lies between.
            let deep = support_depth(*q, m.from, m.to, ax, r, c, h);
            let (lo, hi) = (b.max((-phi).min(cap)), b.max(deep.min(cap)));
            assert!(got >= lo - 1e-3 && got <= hi + 5e-3, "{label}: at {q:?} field {got}, reference bracket [{lo}, {hi}] (before {b})");
        }
        judged += 1;
        if got != *b {
            moved += 1;
        }
    }
    assert!(judged > 500 && moved > 50, "{label}: judged {judged}, moved {moved}");
}

fn seg3(q: Point3, a: Point3, b: Point3) -> f64 {
    let d = [b.x - a.x, b.y - a.y, b.z - a.z];
    let w = [q.x - a.x, q.y - a.y, q.z - a.z];
    let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let t = if dd > 0.0 { ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (w[0] - d[0] * t).hypot(w[1] - d[1] * t).hypot(w[2] - d[2] * t)
}

#[test]
fn an_oblique_bull_sweep_matches_the_brute_force_reference() {
    // a descending diagonal: the golden-section arm
    let m = mv(p(2.0, 3.0, 9.0), p(8.0, 6.5, 6.5), None);
    field_law(&m, 2.0, 0.75, 4.0, "oblique ramp");
}

#[test]
fn level_and_plunge_bull_sweeps_match_the_reference() {
    // the two closed-form arms: the level foot and the taller profile
    field_law(&mv(p(1.5, 4.0, 8.0), p(8.5, 6.0, 8.0), None), 2.0, 1.0, 3.0, "level slot");
    field_law(&mv(p(5.0, 5.0, 10.5), p(5.0, 5.0, 7.0), None), 2.5, 1.0, 3.0, "plunge");
    field_law(&mv(p(5.0, 5.0, 7.0), p(5.0, 5.0, 9.5), None), 2.5, 2.5, 3.0, "rising, full corner");
}

#[test]
fn a_tilted_bull_rides_its_stated_axis() {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let m = mv(p(3.0, 5.0, 7.0), p(7.0, 5.5, 7.5), Some([s, 0.0, s]));
    field_law(&m, 1.5, 0.5, 3.0, "tilted");
}

// ---- the two engines on bull programs -------------------------------

const TOP: f64 = 5.0;

/// The test block, built once per engine and cloned (the field's
/// ten-span-deep shell is the costly part of every fixture here).
fn block(b: Backend) -> Stock {
    use std::sync::OnceLock;
    static COLUMN: OnceLock<Stock> = OnceLock::new();
    static FIELD: OnceLock<Stock> = OnceLock::new();
    let build = || Stock::block(b, 0.0, 0.0, 12.0, 12.0, TOP, 0.5).expect("test block");
    match b {
        Backend::Column => COLUMN.get_or_init(build).clone(),
        Backend::Field { band } => {
            assert_eq!(band, FIELD_BAND, "the cache holds the shipped band only");
            FIELD.get_or_init(build).clone()
        }
    }
}

fn plate(z: f64) -> Vec<Triangle> {
    cube(p(0.0, 0.0, z - 7.0), p(12.0, 12.0, z)).facets.to_vec()
}

fn both(moves: &[Move], r: f64, corner: Option<f64>) -> (Stock, Stock) {
    let (mut c, mut f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    let ops = [SimOp { moves, radius: r, ball: false, cone: None, corner }];
    c.simulate(&ops).expect("column program must run");
    f.simulate(&ops).expect("field program must run");
    (c, f)
}

fn parity(a: &Deviation, b: &Deviation, h: f64, label: &str) {
    assert_eq!((a.cells_x, a.cells_y, a.cell), (b.cells_x, b.cells_y, b.cell), "{label}: lattice");
    assert_eq!((a.rest_cells, a.gouge_cells), (b.rest_cells, b.gouge_cells), "{label}: counts");
    assert_eq!(a.is_clean(), b.is_clean(), "{label}: verdict word");
    assert!((a.max_rest - b.max_rest).abs() <= h && (a.max_gouge - b.max_gouge).abs() <= h, "{label}: maxima");
    for (k, (x, y)) in a.dev.iter().zip(&b.dev).enumerate() {
        assert_eq!(x.is_nan(), y.is_nan(), "{label}: column {k} abstains on one engine only");
        if x.is_finite() {
            assert!((x - y).abs() <= h, "{label}: column {k}: {x} vs {y}");
        }
    }
}

fn programs() -> Vec<(&'static str, Vec<Move>, f64)> {
    let arc = Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: p(9.0, 6.0, 3.0),
        to: p(3.0, 6.0, 2.0),
        feed: Some(500.0),
        arc: Some(ArcMove { center: Point2::new(6.0, 6.0), ccw: true, plane: ArcPlane::Xy }),
        axis: None,
    };
    vec![
        ("level slot", vec![mv(p(2.0, 6.0, 2.5), p(10.0, 6.0, 2.5), None)], 2.5),
        ("plunge", vec![mv(p(6.0, 6.0, 7.0), p(6.0, 6.0, 2.0), None)], 2.5),
        ("ramp", vec![mv(p(2.0, 3.0, 4.5), p(10.0, 9.0, 1.5), None)], 2.5),
        ("helix arc", vec![arc], 2.5),
    ]
}

#[test]
fn both_engines_cut_a_bull_nose_alike() {
    for (label, moves, plate_z) in programs() {
        let (c, f) = both(&moves, 2.0, Some(0.75));
        let dc = c.deviate(&plate(plate_z), DEV_TOL).expect("column deviate");
        let df = f.deviate(&plate(plate_z), DEV_TOL).expect("field deviate");
        assert!(dc.gouge_cells + dc.rest_cells > 0, "{label}: the fixture judges something");
        if moves.iter().any(|m| m.arc.is_some()) {
            arc_parity(&dc, &df, label);
        } else {
            parity(&dc, &df, 1e-5, label);
        }
    }
}

/// #402: the column engine sweeps an XY arc exactly; the field engine
/// still rides inscribed chords at a quarter cell, so on the helix arc
/// (centre (6,6), R 3, upper half) the two agree only to that chord's
/// error. Columns whose true distance to the arc keeps a sagitta clear
/// of the rim differ by at most the bull's profile change across one
/// sagitta (its slope at `r - sag` times `sag`); the rim band itself is
/// where one engine reaches and the other does not - the column
/// engine's answer there is pinned against a dense scan of the true arc
/// in `arc_sweep.rs`.
fn arc_parity(a: &Deviation, b: &Deviation, label: &str) {
    let (r, c, sag) = (2.0f64, 0.75f64, a.cell * 0.25);
    let e = c - sag; // the corner offset at d = r - sag
    let bound = sag * e / (c * c - e * e).sqrt() + 1e-5;
    for (k, (x, y)) in a.dev.iter().zip(&b.dev).enumerate() {
        let (qx, qy) = ((k % a.cells_x) as f64 * a.cell + 0.5 * a.cell, (k / a.cells_x) as f64 * a.cell + 0.5 * a.cell);
        let ends = (qx - 9.0).hypot(qy - 6.0).min((qx - 3.0).hypot(qy - 6.0));
        let d = if qy >= 6.0 { ((qx - 6.0).hypot(qy - 6.0) - 3.0).abs().min(ends) } else { ends };
        if x.is_finite() && y.is_finite() && d < r - sag {
            assert!((x - y).abs() <= bound, "{label}: column {k} ({qx}, {qy}): {x} vs {y}, bound {bound}");
        }
    }
}

#[test]
fn the_display_cut_agrees_with_the_verdict_cut_in_sign() {
    // simulate_display shortens the flank to the stock: inside the
    // stock's box the two bull solids must say the same thing.
    for (label, moves, _) in programs() {
        let mk = || block(Backend::Field { band: FIELD_BAND });
        let (mut verdict, mut picture) = (mk(), mk());
        let ops = [SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: Some(0.75) }];
        verdict.simulate(&ops).expect("verdict");
        picture.simulate_display(&ops).expect("picture");
        for i in 0..24 {
            for j in 0..24 {
                for k in 0..10 {
                    let q = p(i as f64 * 0.5 + 0.13, j as f64 * 0.5 + 0.07, k as f64 * 0.5 + 0.11);
                    let (a, b) = (verdict.distance(q), picture.distance(q));
                    assert_eq!(a < 0.0, b < 0.0, "{label}: sign differs at {q:?}: {a} vs {b}");
                }
            }
        }
    }
}

#[test]
fn the_corner_leaves_rest_a_flat_end_mill_would_remove() {
    // A level slot at the plate's own height: the flat clears its
    // whole disc to the plate (clean), the bull leaves the fillet's
    // ring standing as rest - the silent approximation M13.1 closes.
    let moves = [mv(p(2.0, 6.0, 2.5), p(10.0, 6.0, 2.5), None)];
    for backend in [Backend::Column, Backend::Field { band: FIELD_BAND }] {
        let ops_flat = [SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: None }];
        let ops_bull = [SimOp { moves: &moves, radius: 2.0, ball: false, cone: None, corner: Some(1.0) }];
        let (mut sf, mut sb) = (block(backend), block(backend));
        sf.simulate(&ops_flat).expect("flat");
        sb.simulate(&ops_bull).expect("bull");
        let at = |s: &Stock, y: f64| s.top(6.25, y);
        // y = 7.75: 1.75 off the axis, 0.75 into the 1 mm corner
        let fillet = 1.0 - (1.0f64 - 0.75 * 0.75).sqrt();
        assert!((at(&sf, 7.75) - 2.5).abs() < 1e-5, "{backend:?}: the flat cuts to its floor");
        assert!((at(&sb, 7.75) - (2.5 + fillet)).abs() < 1e-5, "{backend:?}: the bull leaves its fillet: {}", at(&sb, 7.75));
        assert!((at(&sb, 6.25) - 2.5).abs() < 1e-5, "{backend:?}: inside the flat disc the bull is a flat");
    }
}

#[test]
fn bull_refusals_say_the_same_words_either_way() {
    let m = [mv(p(6.0, 6.0, 4.0), p(6.0, 6.0, 4.0), None)];
    let refuse = |b: Backend, ball: bool, cone: Option<f64>, corner: Option<f64>| {
        let mut s = block(b);
        let ops = [SimOp { moves: &m, radius: 2.0, ball, cone, corner }];
        s.simulate(&ops).err().map(|e| e.to_string()).expect("must refuse")
    };
    for (ball, cone, corner, what) in [
        (false, None, Some(0.0), "zero corner"),
        (false, None, Some(2.5), "corner past the radius"),
        (false, None, Some(f64::NAN), "NaN corner"),
        (true, None, Some(1.0), "ball and bull"),
        (false, Some(1.0), Some(1.0), "cone and bull"),
    ] {
        let (a, b) = (refuse(Backend::Column, ball, cone, corner), refuse(Backend::Field { band: FIELD_BAND }, ball, cone, corner));
        assert_eq!(a, b, "{what}: refusal words differ");
        assert!(a.contains("corner") || a.contains("no tool is both"), "{what}: {a}");
    }
}

