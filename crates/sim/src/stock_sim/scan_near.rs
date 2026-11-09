//! The nearest-ball-center query of the hidden-rest scan (M13.1):
//! a uniform XY grid over the center path, searched ring by ring.
//! Provenance: the uniform-grid nearest-neighbour search of
//! computational-geometry texts (cells bucketed, rings visited
//! outward, the ring's distance a lower bound that ends the walk);
//! nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **Exact, not approximate.** Every segment is indexed in each
//!   grid cell a piece of it (at most one grid cell long) touches, so
//!   the segment's nearest point to any query lies in an indexed
//!   cell. Before ring `k` is visited, any segment not yet seen keeps
//!   all its points in rings `>= k`, at least `(k - 1) * g` away in XY
//!   and at least the path's Z-range gap away in Z: the walk stops
//!   only when that bound reaches the best distance found. The minimum is the same f64 a linear scan
//!   of every segment returns (the same distance, the same segment).
//! - **Early verdict, never a looser one.** A caller passing its
//!   worst rest so far is told as soon as some segment brings this
//!   point's rest down to it - the scan's
//!   "this top cannot raise the worst rest" exit; the number then
//!   returned is an upper bound of the true minimum, never used as
//!   a rest reading.
//! - **Counted.** Every distance paid and every cell visited is
//!   added to `work`, so the scan's budget bounds the walk too.

use super::scan_arc::{max_bulge, seg_dist, ScanArc};

use crate::geometry::Point3;

/// The most grid cells on either axis: 256 x 256 lists at most.
const GRID_MAX: usize = 256;

pub(super) struct SegGrid<'a> {
    segs: &'a [(Point3, Point3)],
    /// Each segment's XY arc piece (#413): distances are read off it.
    arcs: &'a [Option<ScanArc>],
    /// The most any piece stands off its chord: the ring prune's slack.
    bulge: f64,
    gx0: f64,
    gy0: f64,
    g: f64,
    gnx: usize,
    gny: usize,
    /// The path's Z range: a lower bound on every segment's Z gap.
    gz0: f64,
    gz1: f64,
    cells: Vec<Vec<u32>>,
}

impl<'a> SegGrid<'a> {
    /// Indexes the segments; `None` when there are none, a non-finite
    /// end, or more than `u32` can name (the caller scans linearly).
    pub(super) fn new(segs: &'a [(Point3, Point3)], arcs: &'a [Option<ScanArc>], hint: f64) -> Option<Self> {
        if segs.is_empty() || segs.len() > u32::MAX as usize {
            return None;
        }
        let (mut x0, mut y0, mut x1, mut y1) =
            (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let (mut gz0, mut gz1) = (f64::INFINITY, f64::NEG_INFINITY);
        for (a, b) in segs {
            if !(a.is_finite() && b.is_finite()) {
                return None;
            }
            x0 = x0.min(a.x).min(b.x);
            x1 = x1.max(a.x).max(b.x);
            y0 = y0.min(a.y).min(b.y);
            y1 = y1.max(a.y).max(b.y);
            gz0 = gz0.min(a.z).min(b.z);
            gz1 = gz1.max(a.z).max(b.z);
        }
        let span = (x1 - x0).max(y1 - y0);
        let g = hint.max(span / GRID_MAX as f64).max(1e-6);
        if !g.is_finite() {
            return None;
        }
        let gnx = (((x1 - x0) / g).floor() as usize + 1).min(GRID_MAX);
        let gny = (((y1 - y0) / g).floor() as usize + 1).min(GRID_MAX);
        let mut cells: Vec<Vec<u32>> = (0..gnx * gny).map(|_| Vec::new()).collect();
        let clampi = |v: f64, n: usize| (v.floor().max(0.0) as usize).min(n - 1);
        for (k, (a, b)) in segs.iter().enumerate() {
            // Pieces no longer than a grid cell: each one's box spans at
            // most 2 x 2 cells, so a diagonal pass indexes its own path,
            // never its whole bounding box.
            let len = (b.x - a.x).hypot(b.y - a.y);
            let n = ((len / g).ceil() as usize).clamp(1, 4 * GRID_MAX);
            for s in 0..n {
                let (t0, t1) = (s as f64 / n as f64, (s + 1) as f64 / n as f64);
                let (ax, ay) = (a.x + (b.x - a.x) * t0, a.y + (b.y - a.y) * t0);
                let (bx, by) = (a.x + (b.x - a.x) * t1, a.y + (b.y - a.y) * t1);
                let i0 = clampi((ax.min(bx) - x0) / g, gnx);
                let i1 = clampi((ax.max(bx) - x0) / g, gnx);
                let j0 = clampi((ay.min(by) - y0) / g, gny);
                let j1 = clampi((ay.max(by) - y0) / g, gny);
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        let list = &mut cells[i + j * gnx];
                        if list.last() != Some(&(k as u32)) {
                            list.push(k as u32);
                        }
                    }
                }
            }
        }
        let bulge = max_bulge(arcs);
        // An arc piece's Z stays within its chord's: the range holds.
        Some(SegGrid { segs, arcs, bulge, gx0: x0, gy0: y0, g, gnx, gny, gz0, gz1, cells })
    }

    /// Segment `s`'s XY arc piece, when it stands for one.
    fn arc(&self, s: u32) -> Option<&ScanArc> {
        self.arcs.get(s as usize).and_then(Option::as_ref)
    }

    /// The least 3-D distance from `q` to any segment, or the first
    /// one whose rest `(d - r - floor).max(0)` - the scan's own
    /// expression, bit for bit - stands at or under `worst` (see the
    /// module law). `hint` is the segment that answered the previous
    /// query, tried first (neighbouring columns share their nearest
    /// pass) and updated to this answer's. `work` grows by every cell
    /// visited and every distance paid.
    pub(super) fn nearest(
        &self,
        q: Point3,
        (r, floor, worst): (f64, f64, f64),
        hint: &mut Option<u32>,
        work: &mut usize,
    ) -> f64 {
        let qi = ((q.x - self.gx0) / self.g).floor();
        let qj = ((q.y - self.gy0) / self.g).floor();
        if !(qi.is_finite() && qj.is_finite() && q.z.is_finite()) {
            return f64::INFINITY;
        }
        let done = |d: f64| (d - r - floor).max(0.0) <= worst;
        let mut best = f64::INFINITY;
        if let Some(s) = *hint
            && let Some((a, b)) = self.segs.get(s as usize)
        {
            *work = work.saturating_add(1);
            best = seg_dist(q, &(*a, *b), self.arc(s));
            if done(best) {
                return best;
            }
        }
        // Every segment point stands within the path's Z range, so the
        // Z gap bounds every unseen segment alongside the ring's XY.
        let zgap = (self.gz0 - q.z).max(q.z - self.gz1).max(0.0);
        let (qi, qj) = (qi.clamp(-1e9, 1e9) as i64, qj.clamp(-1e9, 1e9) as i64);
        let (nx, ny) = (self.gnx as i64, self.gny as i64);
        // Rings closer than the grid's own rectangle are empty: start
        // at the first that touches it, end at the last that does.
        let gap = |v: i64, n: i64| if v < 0 { -v } else if v >= n { v - n + 1 } else { 0 };
        let k0 = gap(qi, nx).max(gap(qj, ny));
        let k1 = qi.abs().max((nx - 1 - qi).abs()).max(qj.abs()).max((ny - 1 - qj).abs());
        let mut k = k0;
        while k <= k1 {
            // Every segment unseen lies wholly in rings at k or past it.
            // An arc piece stands up to `bulge` off its indexed chord.
            if ((k - 1).max(0) as f64 * self.g - self.bulge).max(0.0).hypot(zgap) >= best {
                break;
            }
            let (j_lo, j_hi) = ((qj - k).max(0), (qj + k).min(ny - 1));
            for j in j_lo..=j_hi {
                let edge_row = j == qj - k || j == qj + k;
                let cols: [Option<(i64, i64)>; 2] = if edge_row {
                    [Some(((qi - k).max(0), (qi + k).min(nx - 1))), None]
                } else {
                    let (lo, hi) = (qi - k, qi + k);
                    [
                        (lo >= 0 && lo < nx).then_some((lo, lo)),
                        (hi >= 0 && hi < nx && hi != lo).then_some((hi, hi)),
                    ]
                };
                for (i_lo, i_hi) in cols.into_iter().flatten() {
                    for i in i_lo..=i_hi {
                        *work = work.saturating_add(1);
                        let Some(list) = self.cells.get((i + j * nx) as usize) else { continue };
                        for &s in list {
                            let Some((a, b)) = self.segs.get(s as usize) else { continue };
                            *work = work.saturating_add(1);
                            let d = seg_dist(q, &(*a, *b), self.arc(s));
                            if d < best {
                                best = d;
                                *hint = Some(s);
                                if done(best) {
                                    return best;
                                }
                            }
                        }
                    }
                }
            }
            k += 1;
        }
        best
    }
}
