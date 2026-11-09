//! The consensus pass of the axis fit (issue #113). `axis.rs` claims
//! an axis only when 90 % of the answering surface meets the plain
//! least-squares line, and `axis_robust.rs` sets features aside only
//! after that gate. A large keyway or flat fails the gate before the
//! robust pass sees it: its walls carry more than a tenth of the
//! surface, or drag the plain line past the sagitta band so that half
//! the honest wall misses it - a D20 shaft with a 20 mm deep keyway is
//! refused as not revolved.
//!
//! So when no candidate passes the plain gate, each candidate
//! direction is fitted again by consensus: the facets whose normal
//! lines meet the current line within a cut set by the kept facets'
//! own scatter (the MAD scale) are kept, the line refitted on them, and
//! the cut tightened as the line settles - at least halved each round -
//! down to the sagitta band. The direction stays the candidate's while
//! the set settles: a kept set that is one pencil of lines (a prism's
//! two-triangle faces all meet at its centre) would let a refined
//! direction swing to any line through that point. The gate is then
//! read about the CONSENSUS line, at a lower share:
//! [`CONSENSUS_SHARE`] of the answering surface must meet it, with the
//! coarseness and pencil gates of `axis.rs` unchanged. A part mostly
//! not revolved (an off-axis lump, a prism, a freeform) fails that and
//! keeps its refusal; a part every plain fit already earns never comes
//! here, so its fit is bit for bit what it was. The consensus line then
//! takes `axis_robust.rs`'s reweighting (direction refinement and the
//! tightened band), every refit judged at the same lower share.
//!
//! Provenance: iteratively reweighted least squares with a hard
//! redescending weight and a MAD-scaled cut that shrinks as the fit
//! settles (Huber, Robust Statistics; Hampel's three-part redescender),
//! the consensus set of RANSAC (Fischler & Bolles, 1981), applied to the
//! normal-line concurrency fit of a surface of revolution (Pottmann &
//! Randrup, 1998). Written from those sources and our own design notes;
//! no vendor material.

use crate::geometry::Point3;

use super::axis::{judge_at, line, Axis, Datum, REL_TOL};
use super::axis_robust::{answering, inliers, median_miss, reweigh, Fit, MAD_SIGMA, MIN_KEPT, MIN_KEPT_SHARE};

/// The share of the answering (non-pole) area that must meet the
/// consensus line: three quarters. A turned part whose keyways, flats
/// and holes take more than a quarter of its non-face surface is mostly
/// a milled part, and stays refused; the plain gate's 90 % stands for
/// every part that earns it.
pub(super) const CONSENSUS_SHARE: f64 = 0.75;
/// The cut, in noise sigmas of the kept facets' misses: Hampel's
/// customary rejection point, wide enough to keep the honest wall a
/// dragged line still half-misses, narrow enough to drop a keyway's
/// walls standing millimetres off it.
const CUT_K: f64 = 2.5;
/// Consensus rounds: the cut halves or better each round, so it meets
/// the sagitta band within a handful and the set settles there.
const ROUNDS: usize = 16;

/// The consensus fit over the candidate directions `dirs` (the eigen
/// and pole directions `axis.rs` tried), the best by residual, reweighed
/// at [`CONSENSUS_SHARE`]; `None` when no direction earns it. `plain`
/// in the result is that direction's plain least-squares line.
pub(super) fn fit(data: &[Datum], dirs: &[Point3], a_total: f64) -> Option<Fit> {
    let mut best: Option<(f64, Axis, Axis)> = None;
    for u in dirs {
        if let Some(sol) = consensus(data, *u, a_total)
            && best.as_ref().is_none_or(|(r, _, _)| sol.0 < *r)
        {
            best = Some(sol);
        }
    }
    let (_, ax, plain) = best?;
    let f = reweigh(data, ax, a_total, CONSENSUS_SHARE);
    Some(Fit { plain, ..f })
}

/// One direction's consensus: (residual, the consensus line, the plain
/// line it started from), or `None` when the kept set falls below the
/// share or the gates refuse the settled line.
fn consensus(data: &[Datum], u: Point3, a_total: f64) -> Option<(f64, Axis, Axis)> {
    let (plain, rho) = line(data, u, a_total)?;
    let tol = (REL_TOL * rho).max(1e-9);
    let all = vec![true; data.len()];
    let ans_all = answering(data, &all, plain).0;
    if ans_all <= 0.0 || !ans_all.is_finite() {
        return None;
    }
    let (mut ax, mut keep, mut cut) = (plain, all, f64::INFINITY);
    for _ in 0..ROUNDS {
        // The MAD cut, and at least halved each round: a line dragged
        // far enough inflates the scatter it is cut by, and would keep
        // its features for ever (graduated, as annealing is).
        let mad = median_miss(data, &keep, ax).map_or(tol, |m| CUT_K * MAD_SIGMA * m);
        cut = mad.min(0.5 * cut).max(tol);
        let next = inliers(data, ax, cut);
        let (ans, rows) = answering(data, &next, ax);
        if rows < MIN_KEPT || ans < MIN_KEPT_SHARE * ans_all {
            return None; // the robust pass's backstop; the gate is judged below
        }
        if next == keep && cut <= tol {
            break;
        }
        let kept: Vec<Datum> = data
            .iter()
            .zip(&next)
            .filter(|(_, k)| **k)
            .map(|(d, _)| Datum { n: d.n, p: d.p, a: d.a })
            .collect();
        let a_kept: f64 = kept.iter().map(|d| d.a).sum();
        let moved = line(&kept, ax.dir, a_kept)?.0;
        (ax, keep) = (moved, next);
    }
    judge_at(data, ax, rho, a_total, CONSENSUS_SHARE).map(|r| (r, ax, plain))
}

#[cfg(test)]
#[path = "axis_consensus_tests.rs"]
mod tests;
