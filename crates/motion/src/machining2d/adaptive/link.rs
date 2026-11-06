//! Smooth stay-down links between cuts (M11.4 C): curvature-bounded and
//! tangent at both ends, so a cut is left and entered on an arc. A link
//! is the shortest of Dubins' circle-straight-circle words (Dubins 1957,
//! "On curves of minimal length with a constraint on average curvature",
//! Amer. J. Math. 79; the CSC family as classified by Shkel & Lumelsky
//! 2001) that stays inside the legal center area and loads the tool no
//! more than the bound - the M11.2 D keep-down law's legality (region
//! containment), with the planner's in-process stock as the engagement
//! witness. Tangent at both ends, so no direction change in cut.

use crate::geo2d::{Point2, Region};

use super::front::dist;
use super::raster::Raster;

/// Inside the legal center area, its boundary included: links start
/// and end on wall runs, exactly on the boundary the half-open
/// containment rule may call outside.
pub(super) fn inside(center: &Region, q: Point2) -> bool {
    center.contains(q) || super::loops::boundary_dist(center, q) < 1e-5
}

/// A link starts on the tail of the cut it leaves: its first samples
/// read that cut's own load, so the stay-down test grants it a sliver
/// over the bound (the walk that follows measures it for real).
const LINK_SLACK: f64 = 1.08;

/// Arc sampling: at most this many degrees per chord.
const ARC_STEP_DEG: f64 = 5.0;

fn left(h: Point2) -> Point2 {
    Point2::new(-h.y, h.x)
}

fn add(a: Point2, b: Point2, s: f64) -> Point2 {
    Point2::new(a.x + b.x * s, a.y + b.y * s)
}

/// Sample the circle about `c` from point `a` sweeping `sweep` radians
/// (positive = counter-clockwise); `a` itself excluded.
fn arc_pts(c: Point2, a: Point2, sweep: f64, seg: f64) -> Vec<Point2> {
    if sweep.abs() < 1e-9 {
        return Vec::new();
    }
    let rr = dist(c, a);
    let a0 = (a.y - c.y).atan2(a.x - c.x);
    let by_angle = (sweep.abs() / ARC_STEP_DEG.to_radians()).ceil();
    let by_len = (sweep.abs() * rr / seg.max(1e-6)).ceil();
    let n = by_angle.max(by_len).clamp(1.0, 2048.0) as usize;
    (1..=n)
        .map(|k| {
            let t = a0 + sweep * k as f64 / n as f64;
            Point2::new(c.x + rr * t.cos(), c.y + rr * t.sin())
        })
        .collect()
}

/// Counter-clockwise angle from `a` to `b` about `c`, in [0, 2pi).
fn ccw_sweep(c: Point2, a: Point2, b: Point2) -> f64 {
    let a0 = (a.y - c.y).atan2(a.x - c.x);
    let a1 = (b.y - c.y).atan2(b.x - c.x);
    let mut s = a1 - a0;
    while s < 0.0 {
        s += std::f64::consts::TAU;
    }
    while s >= std::f64::consts::TAU {
        s -= std::f64::consts::TAU;
    }
    s
}

/// One Dubins CSC word from pose (p0, h0) to (p1, h1) at radius `rho`:
/// `l0`/`l1` pick a left (CCW) or right turn at each end. Sampled
/// polyline from p0 to p1 inclusive, or `None` when the word has no
/// tangent (inner tangents need the circles 2 rho apart).
#[allow(clippy::too_many_arguments)]
fn csc(p0: Point2, h0: Point2, p1: Point2, h1: Point2, rho: f64, l0: bool, l1: bool, seg: f64) -> Option<Vec<Point2>> {
    let side = |h: Point2, l: bool| if l { left(h) } else { Point2::new(h.y, -h.x) };
    let c0 = add(p0, side(h0, l0), rho);
    let c1 = add(p1, side(h1, l1), rho);
    let d = dist(c0, c1);
    let alpha = (c1.y - c0.y).atan2(c1.x - c0.x);
    let theta = if l0 == l1 {
        alpha
    } else {
        if d < 2.0 * rho {
            return None;
        }
        let ls = (d * d - 4.0 * rho * rho).sqrt();
        let beta = (2.0 * rho).atan2(ls);
        if l0 { alpha + beta } else { alpha - beta }
    };
    let h = Point2::new(theta.cos(), theta.sin());
    // Tangent points: a left circle carries its point on the right of
    // the heading (the center is to the left), and vice versa.
    let t0 = add(c0, side(h, !l0), rho);
    let t1 = add(c1, side(h, !l1), rho);
    let s0 = if l0 { ccw_sweep(c0, p0, t0) } else { -ccw_sweep(c0, t0, p0) };
    let s1 = if l1 { ccw_sweep(c1, t1, p1) } else { -ccw_sweep(c1, p1, t1) };
    let mut out = vec![p0];
    out.extend(arc_pts(c0, p0, s0, seg));
    let straight = dist(t0, t1);
    let k = ((straight / seg.max(1e-6)).ceil() as usize).clamp(1, 4096);
    let base = out.last().copied().unwrap_or(t0);
    for j in 1..=k {
        let t = j as f64 / k as f64;
        out.push(Point2::new(base.x + (t1.x - base.x) * t, base.y + (t1.y - base.y) * t));
    }
    out.extend(arc_pts(c1, t1, s1, seg));
    if let Some(l) = out.last_mut() {
        *l = p1;
    }
    Some(out)
}

/// The link from pose (p0, h0) to (p1, h1): the shortest legal CSC word
/// over the radius ladder `rho, rho/2, rho/4` - every sample inside
/// `center`, the worst contact angle on the stock at most `bound`.
/// `None` when no word is legal (the caller says what it does instead).
pub(super) fn dubins(
    ras: &Raster,
    center: &Region,
    (p0, h0): (Point2, Point2),
    (p1, h1): (Point2, Point2),
    rho: f64,
    (rim, step, bound): (f64, f64, f64),
) -> Option<Vec<Point2>> {
    if dist(p0, p1) <= 1e-9 && (h0.x * h1.x + h0.y * h1.y) > 0.9999 {
        return Some(vec![p0]);
    }
    let seg = step.max(1e-3);
    // Every word at every radius of the ladder, shortest first: the
    // least air wins, a tighter turn only where it is what fits.
    let mut words: Vec<(f64, Vec<Point2>)> = Vec::new();
    for rr in [rho, rho * 0.5, rho * 0.25] {
        for (l0, l1) in [(true, true), (false, false), (true, false), (false, true)] {
            if let Some(w) = csc(p0, h0, p1, h1, rr, l0, l1, seg) {
                words.push((super::front::len(&w), w));
            }
        }
    }
    words.sort_by(|a, b| a.0.total_cmp(&b.0));
    words
        .into_iter()
        .map(|(_, w)| w)
        .find(|w| w.iter().all(|&q| inside(center, q)) && ras.peek(w, rim, step) <= bound * LINK_SLACK)
}
