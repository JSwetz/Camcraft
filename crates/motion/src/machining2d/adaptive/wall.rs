//! The wall pass (M11.4 C): once no engagement front fits inside the
//! center area, what stands is a strip narrower than one step along the
//! walls; the cutter takes it in one climb lap on the center area's own
//! boundary, its sharp corners rounded by a tangent arc (the classical
//! circle tangent to two lines - corner rounding of pocket paths in the
//! sense of Pateloup, Duc & Ray 2004, "Corner optimization for pocket
//! machining", Int. J. Mach. Tools Manuf. 44), so the lap stays
//! curvature-continuous. The rounding leaves corner rest by design; its
//! size is stated in the notes for the finishing pass.

use crate::geo2d::{Point2, Region};

use super::front::{densify, dist, unit};

/// One lap per loop of `center`, oriented for `climb` (clipper keeps
/// the area on the left of every loop; the wall strip is then on the
/// right - the climb side), each convex corner rounded at `rho` (leg
/// room permitting). Rings come back closed (first point repeated).
/// Also returns, per rounded corner, (corner, how far the center was
/// pulled off it).
pub(super) fn laps(center: &Region, climb: bool, rho: f64, seg: f64) -> (Vec<Vec<Point2>>, Vec<(Point2, f64)>) {
    laps_at(center, climb, rho, seg, 5.0)
}

/// [`laps`] with the corner arcs tessellated at `deg` degrees a joint
/// (the trochoid's laps: under the 5-degree break law with a margin).
pub(super) fn laps_at(center: &Region, climb: bool, rho: f64, seg: f64, deg: f64) -> (Vec<Vec<Point2>>, Vec<(Point2, f64)>) {
    let mut out = Vec::new();
    let mut pulled = Vec::new();
    for l in center.loops() {
        let mut pts: Vec<Point2> = l.points().to_vec();
        if !climb {
            pts.reverse();
        }
        let n = pts.len();
        if n < 3 {
            continue;
        }
        let mut ring: Vec<Point2> = Vec::with_capacity(n * 4);
        for i in 0..n {
            let (p, v, q) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let (u, w) = (unit(p, v), unit(v, q));
            let cross = u.x * w.y - u.y * w.x;
            let cos = (u.x * w.x + u.y * w.y).clamp(-1.0, 1.0);
            // Convex for the area: a left turn when the area is on the
            // left (climb), a right turn otherwise.
            let convex = if climb { cross > 0.0 } else { cross < 0.0 };
            if !convex || cos > 0.996 || rho <= 0.0 {
                ring.push(v);
                continue;
            }
            // Tangent length t = rho tan(theta/2) = rho sin / (1 + cos).
            let sin = cross.abs();
            let room = 0.5 * dist(p, v).min(dist(v, q));
            let rr = rho.min(room * (1.0 + cos) / sin.max(1e-12));
            let t = rr * sin / (1.0 + cos).max(1e-12);
            let t1 = Point2::new(v.x - u.x * t, v.y - u.y * t);
            let t2 = Point2::new(v.x + w.x * t, v.y + w.y * t);
            // Center on the inside of the turn.
            let nrm = if cross > 0.0 { Point2::new(-u.y, u.x) } else { Point2::new(u.y, -u.x) };
            let c = Point2::new(t1.x + nrm.x * rr, t1.y + nrm.y * rr);
            let a0 = (t1.y - c.y).atan2(t1.x - c.x);
            let a1 = (t2.y - c.y).atan2(t2.x - c.x);
            let mut sw = a1 - a0;
            while sw > std::f64::consts::PI {
                sw -= std::f64::consts::TAU;
            }
            while sw < -std::f64::consts::PI {
                sw += std::f64::consts::TAU;
            }
            let k = ((sw.abs() / deg.max(0.5).to_radians()).ceil() as usize).clamp(2, 360);
            for j in 0..=k {
                let a = a0 + sw * j as f64 / k as f64;
                ring.push(Point2::new(c.x + rr * a.cos(), c.y + rr * a.sin()));
            }
            pulled.push((v, dist(v, c) - rr));
        }
        let mut closed = densify(&ring, seg, true);
        if closed.len() >= 2 {
            let first = closed[0];
            if let Some(l) = closed.last_mut() {
                *l = first;
            }
            out.push(closed);
        }
    }
    (out, pulled)
}

/// A cubic Hermite blend from `a` (heading `ta`) to `b` (heading `tb`),
/// sampled about every 0.25 mm, `a` excluded.
pub(super) fn blend(a: Point2, ta: Point2, b: Point2, tb: Point2) -> Vec<Point2> {
    let l = dist(a, b);
    let n = ((l / 0.25).ceil() as usize).clamp(4, 400);
    (1..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let (t2, t3) = (t * t, t * t * t);
            let (h00, h10, h01, h11) = (2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, -2.0 * t3 + 3.0 * t2, t3 - t2);
            Point2::new(
                h00 * a.x + h10 * l * ta.x + h01 * b.x + h11 * l * tb.x,
                h00 * a.y + h10 * l * ta.y + h01 * b.y + h11 * l * tb.y,
            )
        })
        .collect()
}

/// The point `run` mm along the closed `ring` from its start, and the
/// ring's heading there.
pub(super) fn along(ring: &[Point2], run: f64) -> Option<(Point2, Point2)> {
    let mut acc = 0.0;
    for w in ring.windows(2) {
        let l = dist(w[0], w[1]);
        if l > 1e-9 && acc + l >= run {
            let t = (run - acc) / l;
            let p = Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t);
            return Some((p, Point2::new((w[1].x - w[0].x) / l, (w[1].y - w[0].y) / l)));
        }
        acc += l;
    }
    None
}
