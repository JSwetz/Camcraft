//! The cleared-area loops (M11.4 C): where the engagement front has met
//! the walls it is no longer a closed curve, but the area the cutter may
//! stand in without biting more than the step - the center area minus
//! the material grown by `e` - still has a closed boundary: front arcs
//! where material stands, wall runs where the walls already stop it.
//! Riding that boundary keeps the spiral going past the first wall
//! contact (the wall runs are the stay-down links, in cleared stock),
//! the way offset-morph spirals ride a pocket's own boundary once they
//! reach it (Held & Spielberger 2009, provenance in `spiral`). Junction
//! corners are blended G1 with a cubic Bezier inside the corner (the
//! classical tangent-continuous corner blend of CAGD - Farin, *Curves
//! and Surfaces for CAGD*, ch. on Bezier curves), so no direction
//! changes in cut.

use crate::geo2d::{Point2, Region};

use crate::machining2d::GenError;

use super::front::{densify, dist, len, thin, unit};
use super::raster::Raster;


/// A closed cleared-area loop, ready to cut.
#[derive(Clone, Debug)]
pub(super) struct ClearLoop {
    /// Closed ring, first point repeated last.
    pub(super) pts: Vec<Point2>,
    /// Per edge: is it front (off the center area's own boundary)?
    pub(super) flags: Vec<bool>,
    /// Length of the parts that are front (material within reach).
    pub(super) front: f64,
    /// Length biting at least the probe's work angle.
    pub(super) strong: f64,
    pub(super) total: f64,
}

/// Distance from `p` to the nearest boundary edge of `reg`.
pub(crate) fn boundary_dist(reg: &Region, p: Point2) -> f64 {
    dist(p, nearest_on_boundary(reg, p))
}

/// The point of `reg`'s boundary nearest `p`.
pub(super) fn nearest_on_boundary(reg: &Region, p: Point2) -> Point2 {
    let mut best = (f64::INFINITY, p);
    for l in reg.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            let q = Point2::new(a.x + dx * t, a.y + dy * t);
            let d = dist(p, q);
            if d < best.0 {
                best = (d, q);
            }
        }
    }
    best.1
}

/// The boundary loops of `center` minus `mat` grown by `e`, oriented
/// so the material is on the right for `climb`, corners blended at
/// `rho`, each with its front length (points off the center area's own
/// boundary). `probe` is the in-process stock, the rim it is read at and
/// the contact angle that counts as real work (`ClearLoop::strong`) and
/// the one that counts as a bite at all (a wall touch reads below it).
pub(super) fn cleared_loops(
    mat: &Region,
    e: f64,
    center: &Region,
    (climb, seg, rho): (bool, f64, f64),
    probe: (&Raster, f64, f64, f64),
) -> Result<Vec<ClearLoop>, GenError> {
    let room = center.difference(&mat.offset(e)?)?;
    let mut out = Vec::new();
    for l in room.loops() {
        // Clipper keeps the room on the left; the material is outside
        // it, on the right - the climb side.
        let mut pts = thin(l.points(), seg * 0.25);
        if !climb {
            pts.reverse();
        }
        if pts.len() < 3 {
            continue;
        }
        let ring = round_ring(&densify(&pts, seg, true), rho, climb, seg);
        // An edge is front where the cutter standing on it would bite -
        // read on the in-process stock, not inferred from the outline
        // (a wall run over a strip is cutting; a blended corner over
        // cleared floor is not).
        let bites: Vec<f64> = ring
            .windows(2)
            .map(|w| {
                let m = Point2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5);
                probe.0.engage(m, unit(w[0], w[1]), probe.1)
            })
            .collect();
        let flags: Vec<bool> = bites.iter().map(|&b| b > probe.3).collect();
        let (mut front, mut strong) = (0.0, 0.0);
        for (w, &b) in ring.windows(2).zip(&bites) {
            let l = dist(w[0], w[1]);
            if b > probe.3 {
                front += l;
            }
            if b >= probe.2 {
                strong += l;
            }
        }
        let total = len(&ring);
        out.push(ClearLoop { pts: ring, flags, front, strong, total });
    }
    Ok(out)
}

impl ClearLoop {
    /// The loop's cutting segments: every run of front edges, grown by
    /// `margin` of wall run on both sides (a tangent arrival lands on
    /// the wall's thin strip, never square onto the front), runs closer
    /// than `ride` along the loop merged - riding the wall between them
    /// is cheaper than a link. A loop that is front all round comes back
    /// whole (closed).
    pub(super) fn segments(&self, margin: f64, ride: f64) -> Vec<Vec<Point2>> {
        let n = self.flags.len();
        if n == 0 {
            return Vec::new();
        }
        let Some(gap0) = self.flags.iter().position(|f| !f) else {
            return vec![self.pts.clone()];
        };
        // Arc length at each vertex.
        let mut s = vec![0.0; n + 1];
        for i in 0..n {
            s[i + 1] = s[i] + dist(self.pts[i], self.pts[i + 1]);
        }
        let total = s[n];
        // Front runs as arc-length intervals, walked from a gap edge so
        // none wraps the seam.
        let mut runs: Vec<(f64, f64)> = Vec::new();
        let mut cur: Option<f64> = None;
        for k in 1..=n {
            let i = (gap0 + k) % n;
            let off = if gap0 + k >= n { total } else { 0.0 };
            match (cur, self.flags[i]) {
                (None, true) => cur = Some(s[i] + off),
                (Some(a), false) => {
                    runs.push((a, s[i] + off));
                    cur = None;
                }
                _ => {}
            }
        }
        if let Some(a) = cur {
            runs.push((a, s[gap0] + total));
        }
        // Grow, then merge neighbours closer than `ride` (cyclically).
        let mut grown: Vec<(f64, f64)> = runs.iter().map(|&(a, b)| (a - margin, b + margin)).collect();
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for g in grown.drain(..) {
            match merged.last_mut() {
                Some(m) if g.0 - m.1 < ride => m.1 = g.1,
                _ => merged.push(g),
            }
        }
        if merged.len() > 1
            && let (Some(&first), Some(&last)) = (merged.first(), merged.last())
            && first.0 + total - last.1 < ride
        {
            merged.pop();
            merged[0] = (last.0, first.1 + total);
        }
        if merged.len() == 1 && merged[0].1 - merged[0].0 >= total {
            return vec![self.pts.clone()];
        }
        merged.into_iter().map(|(a, b)| self.slice(&s, a, b)).collect()
    }

    /// The loop between arc lengths `a < b` (either may run past the
    /// seam), as a polyline.
    fn slice(&self, s: &[f64], a: f64, b: f64) -> Vec<Point2> {
        let n = self.flags.len();
        let total = s[n];
        let at = |x: f64| {
            let x = x.rem_euclid(total);
            let k = s.partition_point(|&v| v <= x).clamp(1, n);
            let (p, q) = (self.pts[k - 1], self.pts[k]);
            let t = ((x - s[k - 1]) / (s[k] - s[k - 1]).max(1e-12)).clamp(0.0, 1.0);
            Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)
        };
        let mut out = vec![at(a)];
        let lap0 = (a / total).floor();
        for lap in [lap0, lap0 + 1.0, lap0 + 2.0] {
            for (i, &sv) in s.iter().enumerate().take(n) {
                let x = sv + lap * total;
                if x > a && x < b {
                    out.push(self.pts[i]);
                }
            }
        }
        out.push(at(b));
        out
    }
}

/// Blend every corner of a closed ring (first point repeated last) that
/// turns more than 8 degrees toward the inside (`left` for a
/// left-inside ring) with a cubic Bezier from `t` before it to `t` after
/// it, `t = rho * tan(theta/2)` capped at 0.4 of the run to the
/// neighbouring corners. Turns the other way (into the material) stay:
/// a blend there would cut outside the area.
pub(super) fn round_ring(ring: &[Point2], rho: f64, left: bool, seg: f64) -> Vec<Point2> {
    let n = ring.len().saturating_sub(1);
    if n < 3 || rho <= 0.0 {
        return ring.to_vec();
    }
    // Arc length at each vertex.
    let mut s = vec![0.0; n + 1];
    for i in 1..=n {
        s[i] = s[i - 1] + dist(ring[i - 1], ring[i]);
    }
    let total = s[n];
    let turn = |i: usize| {
        let (p, v, q) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let (u, w) = (unit(p, v), unit(v, q));
        let cross = u.x * w.y - u.y * w.x;
        let ang = (u.x * w.x + u.y * w.y).clamp(-1.0, 1.0).acos();
        if (cross > 0.0) == left { ang } else { 0.0 }
    };
    let corners: Vec<(usize, f64)> =
        (0..n).map(|i| (i, turn(i))).filter(|&(_, a)| a > 8f64.to_radians() && a < 3.1).collect();
    if corners.is_empty() {
        return ring.to_vec();
    }
    let at = |x: f64| -> (Point2, Point2) {
        let x = x.rem_euclid(total);
        let k = s.partition_point(|&v| v <= x).clamp(1, n);
        let (a, b) = (ring[k - 1], ring[k]);
        let l = (s[k] - s[k - 1]).max(1e-12);
        let t = ((x - s[k - 1]) / l).clamp(0.0, 1.0);
        (Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), unit(a, b))
    };
    // Each corner's blend window [s - t, s + t], clipped by neighbours.
    let m = corners.len();
    let mut windows: Vec<(f64, f64)> = Vec::with_capacity(m);
    for (j, &(i, ang)) in corners.iter().enumerate() {
        let prev = if m == 1 { total } else { (s[i] - s[corners[(j + m - 1) % m].0]).rem_euclid(total) };
        let next = if m == 1 { total } else { (s[corners[(j + 1) % m].0] - s[i]).rem_euclid(total) };
        let t = (rho * (ang * 0.5).tan()).min(0.4 * prev).min(0.4 * next);
        windows.push((s[i] - t, s[i] + t));
    }
    // Rebuild: walk from the first window's end, emitting ring vertices
    // outside windows and a Bezier across each window.
    let mut out: Vec<Point2> = Vec::with_capacity(ring.len() + m * 16);
    for j in 0..m {
        let (_, b1) = windows[j];
        let (a2, b2) = windows[(j + 1) % m];
        let a2 = if j + 1 == m { a2 + total } else { a2 };
        let b2 = if j + 1 == m { b2 + total } else { b2 };
        // Plain run from the end of window j to the start of j+1.
        let (pb, _) = at(b1);
        out.push(pb);
        // The run's vertices in arc order (a run may wrap past the seam,
        // hence both laps, then one sort by arc length).
        let mut run: Vec<(f64, Point2)> = Vec::new();
        for (i, &sv) in s.iter().enumerate().take(n) {
            for lap in [0.0, total] {
                let x = sv + lap;
                if x > b1 && x < a2 {
                    run.push((x, ring[i]));
                }
            }
        }
        run.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.extend(run.into_iter().map(|(_, p)| p));
        // Bezier across window j+1.
        let (p0, t0) = at(a2);
        let (p3, t1) = at(b2);
        let k = dist(p0, p3) * 0.39;
        let c1 = Point2::new(p0.x + t0.x * k, p0.y + t0.y * k);
        let c2 = Point2::new(p3.x - t1.x * k, p3.y - t1.y * k);
        let steps = ((dist(p0, p3) / (seg * 0.5).max(1e-6)).ceil() as usize).clamp(4, 64);
        for q in 0..=steps {
            let u = q as f64 / steps as f64;
            let v = 1.0 - u;
            out.push(Point2::new(
                v * v * v * p0.x + 3.0 * v * v * u * c1.x + 3.0 * v * u * u * c2.x + u * u * u * p3.x,
                v * v * v * p0.y + 3.0 * v * v * u * c1.y + 3.0 * v * u * u * c2.y + u * u * u * p3.y,
            ));
        }
    }
    // Close the ring.
    if let Some(&f) = out.first() {
        out.push(f);
    }
    thin(&out, 1e-6)
}
