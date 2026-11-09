//! The rotary wrap's seam angle (issue #226): when the default seam (the
//! basis' ±π branch) crosses a feature that is not turned - a D-flat, a
//! keyway - the wrap looks for an azimuth where every facet straddling
//! the seam stands on two alike meridian columns, and splits the
//! development there instead. Only the split moves: the world exit maps
//! the developed coordinates back through the same turned basis, so
//! every point is cut at its true azimuth.
//!
//! The candidates are the azimuths midway between two neighbouring
//! vertex columns (a seam on a column would read its vertices on both
//! sides of the branch). A facet straddles a candidate exactly when the
//! candidate lies inside the shortest arc holding its off-axis vertices;
//! one sweep over the facets hands each candidate its straddlers, and
//! [`rotary_seam::seam_columns`] judges each as it judges the default.
//! Of the turned candidates the wrap takes the one farthest from any
//! refused one - the middle of the widest turned band, away from the
//! feature's edges - and the full development re-judges it before
//! anything is cut.
//!
//! Provenance: circular-interval stabbing (sorted endpoints, binary
//! search) from standard computational-geometry texts; the turned-seam
//! law is this crate's own ([`super::rotary_seam`]). No vendor material.

use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::axis;
use super::rotary_seam;
use super::rotary_sheet::{polar, Basis};

const TAU: f64 = std::f64::consts::TAU;
const PI: f64 = std::f64::consts::PI;
/// Two vertex azimuths closer than this (radians) are one column: the
/// seam law's own column grain (`rotary_seam::SEAM_TH_TOL`).
const COL_TOL: f64 = 1e-6;
/// A gap narrower than this (radians) between two columns takes no
/// seam: its midpoint would sit within float noise of a vertex.
const MIN_GAP: f64 = 1e-4;
/// The most straddler vertices the search hands out before giving up
/// (a mesh whose facets each straddle thousands of candidates): a
/// refusal by name, never a hang.
const BUDGET: usize = 20_000_000;

/// Where to turn the basis `b` so the wrap's seam is turned: `Ok(Some(δ))`
/// turns it δ radians ([`Basis::turned`]), `Ok(None)` when no candidate
/// azimuth is turned (a feature all the way round). `scale` is the
/// work's largest radius, the seam law's quantum.
pub(super) fn turned_seam(
    facets: &[Triangle],
    ax: &axis::Axis,
    b: Basis,
    rho_eps: f64,
    scale: f64,
) -> Result<Option<f64>, GenError> {
    // Every facet's off-axis vertices, azimuths in [0, 2π).
    let polys: Vec<Vec<(f64, f64, f64)>> = facets
        .iter()
        .map(|t| {
            [t.a, t.b, t.c]
                .into_iter()
                .map(|q| polar(ax, b, rho_eps, q))
                .filter(|&(_, _, rho)| rho > rho_eps)
                .map(|(th, v, rho)| (th.rem_euclid(TAU), v, rho))
                .collect()
        })
        .collect();
    let cands = candidates(&polys);
    if cands.is_empty() {
        return Ok(None);
    }
    let mut cols: Vec<Vec<(f64, f64, f64)>> = vec![Vec::new(); cands.len()];
    let mut handed = 0usize;
    for vs in &polys {
        let Some((start, len)) = shortest_arc(vs) else { continue };
        let mut hand = |i: usize| {
            if let Some(c) = cols.get_mut(i) {
                c.extend_from_slice(vs);
                handed = handed.saturating_add(vs.len());
            }
        };
        if len >= PI {
            // Spread over half a turn: straddles wherever the seam
            // splits its azimuths more than half a turn apart.
            for (i, &psi) in cands.iter().enumerate() {
                if straddles_at(vs, psi) {
                    hand(i);
                }
            }
        } else {
            let end = start + len;
            let lo = cands.partition_point(|&c| c <= start);
            let hi = cands.partition_point(|&c| c < end.min(TAU));
            (lo..hi).for_each(&mut hand);
            if end > TAU {
                let hi2 = cands.partition_point(|&c| c < end - TAU);
                (0..hi2).for_each(&mut hand);
            }
        }
        if handed > BUDGET {
            return Err(GenError::Invalid(
                "the rotary wrap's seam crosses a feature, and the mesh is too dense to search for a turned seam angle: rotate a feature clear of the seam".into(),
            ));
        }
    }
    let turned: Vec<bool> = cols.iter().map(|c| rotary_seam::seam_columns(c, scale).is_none()).collect();
    let refused: Vec<f64> = cands.iter().zip(&turned).filter(|(_, ok)| !**ok).map(|(c, _)| *c).collect();
    // The turned candidate farthest from any refused one; a tie goes to
    // the one nearest the default seam (π), then the lower azimuth.
    let mut best: Option<(f64, f64, f64)> = None; // (clearance, -off-default, psi)
    for (&psi, _) in cands.iter().zip(&turned).filter(|(_, ok)| **ok) {
        let clear = nearest(&refused, psi);
        let off = arc_dist(psi, PI);
        let better = match best {
            None => true,
            Some((bc, boff, _)) => clear > bc + COL_TOL || ((clear - bc).abs() <= COL_TOL && off < boff - COL_TOL),
        };
        if better {
            best = Some((clear, off, psi));
        }
    }
    Ok(best.map(|(_, _, psi)| psi - PI))
}

/// The midpoints of every gap between two neighbouring vertex columns
/// at least [`MIN_GAP`] wide, sorted, in [0, 2π).
fn candidates(polys: &[Vec<(f64, f64, f64)>]) -> Vec<f64> {
    let mut az: Vec<f64> = polys.iter().flatten().map(|&(th, _, _)| th).collect();
    az.sort_by(f64::total_cmp);
    az.dedup_by(|a, b| (*a - *b).abs() <= COL_TOL);
    let (Some(&first), Some(&last)) = (az.first(), az.last()) else { return Vec::new() };
    let mut out: Vec<f64> = az
        .windows(2)
        .filter(|w| w[1] - w[0] >= MIN_GAP)
        .map(|w| 0.5 * (w[0] + w[1]))
        .collect();
    let wrap_gap = first + TAU - last;
    if wrap_gap >= MIN_GAP && az.len() > 1 {
        out.push((0.5 * (last + first + TAU)).rem_euclid(TAU));
    }
    out.sort_by(f64::total_cmp);
    out
}

/// The shortest arc `(start, length)` holding every azimuth of `vs`:
/// the circle less its widest gap. `None` for fewer than two columns.
fn shortest_arc(vs: &[(f64, f64, f64)]) -> Option<(f64, f64)> {
    let mut a: Vec<f64> = vs.iter().map(|v| v.0).collect();
    a.sort_by(f64::total_cmp);
    let (&first, &last) = (a.first()?, a.last()?);
    if last - first <= COL_TOL {
        return None;
    }
    // The widest gap, the wrap-round one included; the arc starts after it.
    let (mut gap, mut start) = (first + TAU - last, first);
    for w in a.windows(2) {
        if w[1] - w[0] > gap {
            gap = w[1] - w[0];
            start = w[1];
        }
    }
    Some((start, TAU - gap))
}

/// Does a facet with these azimuths straddle a seam at `psi`: read from
/// the seam, are its azimuths spread over more than half a turn (the
/// sheet's own straddle law, [`super::rotary_sheet::lay`])?
fn straddles_at(vs: &[(f64, f64, f64)], psi: f64) -> bool {
    let rel = |th: f64| (th - psi).rem_euclid(TAU);
    let (mn, mx) = vs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        let r = rel(v.0);
        (lo.min(r), hi.max(r))
    });
    mx - mn > PI
}

/// The angular distance between two azimuths, in [0, π].
fn arc_dist(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(TAU);
    d.min(TAU - d)
}

/// The angular distance from `psi` to the nearest of `sorted` (∞ when
/// there is none).
fn nearest(sorted: &[f64], psi: f64) -> f64 {
    let i = sorted.partition_point(|&c| c < psi);
    let n = sorted.len();
    if n == 0 {
        return f64::INFINITY;
    }
    let a = sorted.get(i % n).copied().unwrap_or(psi);
    let b = sorted.get((i + n - 1) % n).copied().unwrap_or(psi);
    arc_dist(psi, a).min(arc_dist(psi, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortest_arc_runs_across_the_zero_azimuth() {
        let vs = [(0.1, 0.0, 1.0), (TAU - 0.1, 0.0, 1.0)];
        let (start, len) = shortest_arc(&vs).expect("two columns");
        assert!((start - (TAU - 0.1)).abs() < 1e-12 && (len - 0.2).abs() < 1e-12, "{start} {len}");
    }

    #[test]
    fn the_candidates_include_the_gap_across_zero() {
        let polys = vec![vec![(0.5, 0.0, 1.0), (2.0, 0.0, 1.0), (5.0, 0.0, 1.0)]];
        let c = candidates(&polys);
        assert_eq!(c.len(), 3);
        let wrap = (0.5 * (5.0 + 0.5 + TAU)).rem_euclid(TAU);
        assert!(c.iter().any(|x| (x - wrap).abs() < 1e-12), "{c:?}");
    }
}
