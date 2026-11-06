//! Nearest point on a polyline, bucketed (M11.4 C round 3): the growth
//! rings' morph projects every point of one ring onto the next, and a
//! ring holds thousands of chords. A uniform grid of segment buckets
//! searched ring by ring outward until no farther cell can hold a
//! nearer chord - the textbook cell-grid proximity query (de Berg et
//! al., *Computational Geometry*, the grid-bucketing idiom). Ours.

use crate::geo2d::Point2;

/// The bucketed chords of one polyline.
pub(super) struct SegIndex<'a> {
    pts: &'a [Point2],
    x0: f64,
    y0: f64,
    c: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

/// Most cells a side (the grid grows its cell past this).
const MAX_SIDE: usize = 1024;

impl<'a> SegIndex<'a> {
    /// Bucket the chords of `pts` on cells of about `cell` mm.
    pub(super) fn new(pts: &'a [Point2], cell: f64) -> Self {
        let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
        for p in pts {
            lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        if !(lo.x.is_finite() && hi.x.is_finite() && cell > 0.0) {
            return SegIndex { pts, x0: 0.0, y0: 0.0, c: 1.0, nx: 1, ny: 1, cells: vec![Vec::new()] };
        }
        let c = cell.max((hi.x - lo.x).max(hi.y - lo.y) / MAX_SIDE as f64).max(1e-6);
        let nx = (((hi.x - lo.x) / c).floor() as usize + 1).min(MAX_SIDE + 1);
        let ny = (((hi.y - lo.y) / c).floor() as usize + 1).min(MAX_SIDE + 1);
        let mut ix = SegIndex { pts, x0: lo.x, y0: lo.y, c, nx, ny, cells: vec![Vec::new(); nx * ny] };
        for k in 0..pts.len().saturating_sub(1) {
            let (a, b) = (pts[k], pts[k + 1]);
            let (i0, j0) = ix.cell(Point2::new(a.x.min(b.x), a.y.min(b.y)));
            let (i1, j1) = ix.cell(Point2::new(a.x.max(b.x), a.y.max(b.y)));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    ix.cells[i + j * nx].push(k as u32);
                }
            }
        }
        ix
    }

    fn cell(&self, p: Point2) -> (usize, usize) {
        let i = ((p.x - self.x0) / self.c).floor().clamp(0.0, (self.nx - 1) as f64) as usize;
        let j = ((p.y - self.y0) / self.c).floor().clamp(0.0, (self.ny - 1) as f64) as usize;
        (i, j)
    }

    /// The point of the polyline nearest `p` (`p` itself for an empty one).
    pub(super) fn nearest(&self, p: Point2) -> Point2 {
        let (ci, cj) = self.cell(p);
        let mut best = (f64::INFINITY, p);
        let kmax = self.nx.max(self.ny);
        for k in 0..=kmax {
            let (i0, i1) = (ci.saturating_sub(k), (ci + k).min(self.nx - 1));
            let (j0, j1) = (cj.saturating_sub(k), (cj + k).min(self.ny - 1));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    // Only the ring at Chebyshev distance k is new.
                    if i.abs_diff(ci) != k && j.abs_diff(cj) != k {
                        continue;
                    }
                    for &s in &self.cells[i + j * self.nx] {
                        let q = foot(self.pts[s as usize], self.pts[s as usize + 1], p);
                        let d = (q.x - p.x).hypot(q.y - p.y);
                        if d < best.0 {
                            best = (d, q);
                        }
                    }
                }
            }
            // Every cell not yet read is at least k cells away.
            if best.0 <= k as f64 * self.c {
                break;
            }
        }
        best.1
    }
}

/// The foot of `p` on the segment `a b`.
pub(super) fn foot(a: Point2, b: Point2, p: Point2) -> Point2 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    Point2::new(a.x + dx * t, a.y + dy * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bucketed_nearest_point_is_the_brute_force_one() {
        let ring: Vec<Point2> = (0..=400)
            .map(|k| {
                let a = k as f64 * std::f64::consts::TAU / 400.0;
                Point2::new(20.0 * a.cos() + 3.0 * (5.0 * a).sin(), 8.0 * a.sin())
            })
            .collect();
        let ix = SegIndex::new(&ring, 0.5);
        for k in 0..200 {
            let p = Point2::new(-30.0 + 0.3 * k as f64, -12.0 + 0.11 * k as f64);
            let brute = ring
                .windows(2)
                .map(|w| foot(w[0], w[1], p))
                .min_by(|a, b| (a.x - p.x).hypot(a.y - p.y).total_cmp(&(b.x - p.x).hypot(b.y - p.y)))
                .unwrap_or(p);
            let q = ix.nearest(p);
            let (dq, db) = ((q.x - p.x).hypot(q.y - p.y), (brute.x - p.x).hypot(brute.y - p.y));
            assert!((dq - db).abs() < 1e-12, "at {p:?}: {dq} vs {db}");
        }
    }
}
