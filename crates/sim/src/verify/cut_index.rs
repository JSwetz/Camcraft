//! Uniform-grid broad phase over the rest model's cutting chords (M13.7 cut order) - the uniform-grid technique of Franklin 1989 ("Uniform grids: a technique for intersection detection on serial and parallel machines", Auto-Carto 9), the same public source as the crossing law's grid; our own conservative filing rule.
//!
//! A column query asks "which chords pass within the tool radius of q",
//! and with cut order it can no longer stop at the first in-reach chord
//! (that chord may come AFTER the judged move). The grid keeps the walk
//! local: a chord is filed in every cell whose centre lies within
//! `r + half-diagonal` of it - any q in that cell within `r` of the
//! chord passes that test by the triangle inequality, so no clearing
//! chord is ever missed. A chord whose box covers more than [`WIDE`]
//! cells rides the always-tested `wide` list instead. Both lists hold
//! indices into the z-sorted cut array, ascending, and are merged in
//! that order, so a query walks candidates deepest plane first - the
//! order every reader of [`super::rest::Rest`] relies on.

use super::laws::dist2_seg;
use super::rest::Cut;

use crate::geo2d::Point2;

/// Cells per axis at most.
const MAX_SIDE: f64 = 512.0;
/// A chord whose box covers more cells than this is filed as wide.
const WIDE: usize = 65_536;

/// The chord grid of one path.
pub(super) struct CutIndex {
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<usize>>,
    wide: Vec<usize>,
}

impl CutIndex {
    /// File every finite chord of `cut` (z-sorted) for reach `r`. A
    /// non-finite chord never clears a column (`dist2_seg` is NaN), so
    /// leaving it out is the same answer.
    pub(super) fn new(cut: &[Cut], r: f64) -> Self {
        let r = if r.is_finite() { r.max(0.0) } else { 0.0 };
        let pad = r + 1e-6;
        let fin = |c: &Cut| c.0.x.is_finite() && c.0.y.is_finite() && c.1.x.is_finite() && c.1.y.is_finite();
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for c in cut.iter().filter(|c| fin(c)) {
            x0 = x0.min(c.0.x.min(c.1.x) - pad);
            y0 = y0.min(c.0.y.min(c.1.y) - pad);
            x1 = x1.max(c.0.x.max(c.1.x) + pad);
            y1 = y1.max(c.0.y.max(c.1.y) + pad);
        }
        let mut idx = Self { x0, y0, cell: 1.0, nx: 0, ny: 0, cells: Vec::new(), wide: Vec::new() };
        if !(x1 >= x0 && y1 >= y0 && (x1 - x0).is_finite() && (y1 - y0).is_finite()) {
            return idx; // no finite chord: nothing ever clears
        }
        let span = (x1 - x0).max(y1 - y0);
        let cell = (2.0 * r).max(span / MAX_SIDE).max(1e-3);
        let nx = (((x1 - x0) / cell).ceil() as usize).clamp(1, MAX_SIDE as usize + 1);
        let ny = (((y1 - y0) / cell).ceil() as usize).clamp(1, MAX_SIDE as usize + 1);
        idx.cell = cell;
        idx.nx = nx;
        idx.ny = ny;
        idx.cells = vec![Vec::new(); nx.saturating_mul(ny)];
        let reach = pad + cell * std::f64::consts::FRAC_1_SQRT_2;
        let reach2 = reach * reach;
        for (k, c) in cut.iter().enumerate().filter(|(_, c)| fin(c)) {
            let (ix0, iy0) = idx.cell_of(c.0.x.min(c.1.x) - pad, c.0.y.min(c.1.y) - pad);
            let (ix1, iy1) = idx.cell_of(c.0.x.max(c.1.x) + pad, c.0.y.max(c.1.y) + pad);
            let count = (ix1 - ix0 + 1).saturating_mul(iy1 - iy0 + 1);
            if count > WIDE {
                idx.wide.push(k);
                continue;
            }
            for iy in iy0..=iy1 {
                for ix in ix0..=ix1 {
                    let centre = Point2::new(x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                    if dist2_seg(centre, c.0, c.1) <= reach2
                        && let Some(list) = idx.cells.get_mut(iy * nx + ix)
                    {
                        list.push(k);
                    }
                }
            }
        }
        idx
    }

    /// The (clamped) cell holding (x, y).
    fn cell_of(&self, x: f64, y: f64) -> (usize, usize) {
        let fx = ((x - self.x0) / self.cell).floor();
        let fy = ((y - self.y0) / self.cell).floor();
        let cx = if fx.is_finite() { fx.clamp(0.0, (self.nx.max(1) - 1) as f64) as usize } else { 0 };
        let cy = if fy.is_finite() { fy.clamp(0.0, (self.ny.max(1) - 1) as f64) as usize } else { 0 };
        (cx, cy)
    }

    /// Every chord index from `from` up that can clear q's column,
    /// ascending (deepest plane first). Outside the grid no chord
    /// reaches q, so the list is empty there.
    pub(super) fn near(&self, q: Point2, from: usize) -> impl Iterator<Item = usize> + '_ {
        let inside = self.nx > 0
            && q.x >= self.x0
            && q.y >= self.y0
            && q.x <= self.x0 + self.nx as f64 * self.cell
            && q.y <= self.y0 + self.ny as f64 * self.cell;
        let (a, b): (&[usize], &[usize]) = if inside {
            let (ix, iy) = self.cell_of(q.x, q.y);
            (self.cells.get(iy * self.nx + ix).map_or(&[][..], |v| v.as_slice()), &self.wide)
        } else {
            (&[], &[])
        };
        let mut i = a.partition_point(|&k| k < from);
        let mut j = b.partition_point(|&k| k < from);
        std::iter::from_fn(move || match (a.get(i), b.get(j)) {
            (Some(&x), Some(&y)) if x < y => {
                i += 1;
                Some(x)
            }
            (_, Some(&y)) => {
                j += 1;
                Some(y)
            }
            (Some(&x), None) => {
                i += 1;
                Some(x)
            }
            (None, None) => None,
        })
    }
}
