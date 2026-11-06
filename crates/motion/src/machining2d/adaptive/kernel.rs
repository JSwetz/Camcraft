//! Where the morph spiral wants its entry (M11.4 C round 2): inside the
//! kernel of the area - the points that see its whole boundary, the
//! intersection of the inner half-planes of its edges (Lee & Preparata
//! 1979, "An optimal algorithm for finding the kernel of a polygon",
//! J. ACM 26(3); here the plain half-plane clipping of Sutherland &
//! Hodgman 1974 against each edge, which is exact and enough at these
//! sizes) - shrunk by the entry orbit so the orbit's disc is in it, at
//! the point nearest the area's centroid (the rays from there are the
//! most alike, so the spiral's revolutions step evenly). Ours, no
//! vendor material.

use crate::geo2d::{Point2, Region};

use crate::machining2d::GenError;

/// How far the seed moves from the kernel point nearest the centroid
/// toward the kernel's centroid.
const KERNEL_MIX: f64 = 0.5;

/// Area centroid of a region's loops (holes subtract).
pub(super) fn centroid(reg: &Region) -> Option<Point2> {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for l in reg.loops() {
        let p = l.points();
        let n = p.len();
        for i in 0..n {
            let (u, v) = (p[i], p[(i + 1) % n]);
            let c = u.x * v.y - v.x * u.y;
            a += c;
            cx += (u.x + v.x) * c;
            cy += (u.y + v.y) * c;
        }
    }
    (a.abs() > 1e-12).then(|| Point2::new(cx / (3.0 * a), cy / (3.0 * a)))
}

/// The kernel of the closed CCW loop `pts`, every half-plane moved
/// inward by `m` (a convex polygon, possibly empty).
fn kernel(pts: &[Point2], m: f64) -> Vec<Point2> {
    let n = pts.len();
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for p in pts {
        lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let mut poly = vec![lo, Point2::new(hi.x, lo.y), hi, Point2::new(lo.x, hi.y)];
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = (b.x - a.x).hypot(b.y - a.y);
        if len <= 1e-12 {
            continue;
        }
        // Signed distance to the left of a -> b, minus the margin.
        let f = |p: Point2| ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) / len - m;
        let k = poly.len();
        let mut out = Vec::with_capacity(k + 1);
        for j in 0..k {
            let (p, q) = (poly[j], poly[(j + 1) % k]);
            let (fp, fq) = (f(p), f(q));
            if fp >= 0.0 {
                out.push(p);
            }
            if (fp >= 0.0) != (fq >= 0.0) {
                let t = fp / (fp - fq);
                out.push(Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t));
            }
        }
        poly = out;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

/// The point of the convex polygon `k` nearest `p` (`p` itself when
/// inside).
fn nearest_in(k: &[Point2], p: Point2) -> Point2 {
    let n = k.len();
    let inside = (0..n).all(|i| {
        let (a, b) = (k[i], k[(i + 1) % n]);
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= 0.0
    });
    if inside {
        return p;
    }
    let mut best = (f64::INFINITY, p);
    for i in 0..n {
        let (a, b) = (k[i], k[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let q = Point2::new(a.x + dx * t, a.y + dy * t);
        let d = (q.x - p.x).hypot(q.y - p.y);
        if d < best.0 {
            best = (d, q);
        }
    }
    best.1
}

/// The entry the morph spiral wants in `center` (one loop, no holes):
/// the point of the area's kernel - its corners rounded at `rho_c`, the
/// spiral's last ring - shrunk by `margin`, nearest the centroid; `None`
/// when the area has holes or no such kernel.
pub(crate) fn preferred_seed(center: &Region, rho_c: f64, margin: f64) -> Result<Option<Point2>, GenError> {
    if center.loops().len() != 1 {
        return Ok(None);
    }
    let opened = center.offset(-rho_c)?.offset(rho_c)?;
    let [l] = opened.loops() else { return Ok(None) };
    let mut pts = l.points().to_vec();
    if l.area() < 0.0 {
        pts.reverse();
    }
    let k = kernel(&pts, margin);
    if k.len() < 3 {
        return Ok(None);
    }
    let Some(c) = centroid(center) else { return Ok(None) };
    let q = nearest_in(&k, c);
    // A hair further in than the hull's edge, so the kernel test that
    // follows never stands on the margin's float noise.
    let kc = crate::geo2d::Loop::new(k.clone())
        .ok()
        .and_then(|l| Region::new(vec![l]).ok())
        .and_then(|r| centroid(&r))
        .unwrap_or(q);
    // Halfway from there to the kernel's own centroid: off its edge, so
    // no wall is seen at a grazing angle from the seed.
    let w = KERNEL_MIX;
    let s = Point2::new(q.x + (kc.x - q.x) * w.max(1e-3), q.y + (kc.y - q.y) * w.max(1e-3));
    Ok(opened.contains(s).then_some(s))
}

/// The widest point of `center` - the center of its largest inscribed
/// circle, approximately (a grid search refined by a shrinking pattern
/// search over the boundary distance) - and that distance. The
/// engagement fronts start there when the area is no star cell.
pub(crate) fn widest_point(center: &Region) -> Option<(Point2, f64)> {
    let (lo, hi) = center.bounds()?;
    let span = (hi.x - lo.x).max(hi.y - lo.y);
    if !(span.is_finite() && span > 0.0) {
        return None;
    }
    let h = span / 120.0;
    let clear = |p: Point2| if center.contains(p) { super::loops::boundary_dist(center, p) } else { -1.0 };
    let mut best = (-1.0f64, lo);
    // Ties (a channel's whole center line is widest) go to the point
    // nearest the centroid: the middle of the run, not its end.
    let mid = centroid(center).unwrap_or(lo);
    let off = |p: Point2| (p.x - mid.x).hypot(p.y - mid.y);
    let (nx, ny) = (((hi.x - lo.x) / h).ceil() as usize, ((hi.y - lo.y) / h).ceil() as usize);
    for iy in 0..=ny {
        for ix in 0..=nx {
            let p = Point2::new(lo.x + ix as f64 * h, lo.y + iy as f64 * h);
            let d = clear(p);
            if d > best.0 + 1e-3 * h || (d > best.0 - 1e-3 * h && off(p) < off(best.1)) {
                best = (d, p);
            }
        }
    }
    let mut step = h;
    while step > 1e-4 {
        let mut moved = false;
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let p = Point2::new(best.1.x + dx * step, best.1.y + dy * step);
            let d = clear(p);
            if d > best.0 {
                best = (d, p);
                moved = true;
            }
        }
        if !moved {
            step *= 0.5;
        }
    }
    (best.0 > 0.0).then_some((best.1, best.0))
}

/// The entry the growth rings want in an area that is no star cell
/// (round 3): among the roomy spots (clearance within [`ROOMY`] of the
/// widest), the one whose farthest point is nearest along the area
/// (least geodesic eccentricity: the graph center of the area's grid,
/// by Dijkstra's shortest paths, 1959) - the fronts from it finish
/// soonest, and around an island its arms arrive together. With the
/// clearance there.
pub(crate) fn central_point(center: &Region) -> Option<(Point2, f64)> {
    let (lo, hi) = center.bounds()?;
    let span = (hi.x - lo.x).max(hi.y - lo.y);
    if !(span.is_finite() && span > 0.0) {
        return None;
    }
    let h = (span / 150.0).max(0.1);
    let (nx, ny) = (((hi.x - lo.x) / h).ceil() as usize + 1, ((hi.y - lo.y) / h).ceil() as usize + 1);
    let at = |i: usize, j: usize| Point2::new(lo.x + i as f64 * h, lo.y + j as f64 * h);
    let clear: Vec<f64> = (0..nx * ny)
        .map(|k| {
            let p = at(k % nx, k / nx);
            if center.contains(p) { super::loops::boundary_dist(center, p) } else { -1.0 }
        })
        .collect();
    let cmax = clear.iter().copied().fold(0.0, f64::max);
    if cmax <= 0.0 {
        return None;
    }
    let roomy: Vec<usize> = (0..clear.len()).filter(|&k| clear[k] >= ROOMY * cmax).collect();
    let stride = (roomy.len() / MAX_CANDIDATES).max(1);
    let mut best: Option<(f64, f64, usize)> = None;
    for &k in roomy.iter().step_by(stride) {
        let ecc = eccentricity(&clear, (nx, ny), h, k);
        if best.is_none_or(|b| ecc < b.0 - 1e-9 || (ecc < b.0 + 1e-9 && clear[k] > b.1)) {
            best = Some((ecc, clear[k], k));
        }
    }
    best.map(|(_, c, k)| (at(k % nx, k / nx), c))
}

/// Roomy spots: clearance at least this share of the widest.
const ROOMY: f64 = 0.85;

/// At most this many roomy spots are weighed.
const MAX_CANDIDATES: usize = 48;

/// The farthest any inside node of the grid is from node `k` along the
/// grid's 8-neighbour paths.
fn eccentricity(clear: &[f64], (nx, ny): (usize, usize), h: f64, k: usize) -> f64 {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut d = vec![f64::INFINITY; clear.len()];
    let mut q = BinaryHeap::new();
    d[k] = 0.0;
    q.push(Reverse((0u64, k)));
    let key = |x: f64| (x * 1e6) as u64;
    let mut far = 0.0f64;
    while let Some(Reverse((dk, u))) = q.pop() {
        let du = d[u];
        if key(du) < dk {
            continue;
        }
        far = far.max(du);
        let (i, j) = ((u % nx) as i64, (u / nx) as i64);
        for (di, dj, w) in [(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0), (1, 1, 1.5), (1, -1, 1.5), (-1, 1, 1.5), (-1, -1, 1.5)] {
            let (a, b) = (i + di, j + dj);
            if a < 0 || b < 0 || a >= nx as i64 || b >= ny as i64 {
                continue;
            }
            let v = a as usize + b as usize * nx;
            // 1.5 for a diagonal: an octile metric that never cuts a
            // corner shorter than it is (sqrt 2 rounded up).
            let nd = du + w * h;
            if clear[v] > 0.0 && nd < d[v] {
                d[v] = nd;
                q.push(Reverse((key(nd), v)));
            }
        }
    }
    far
}
