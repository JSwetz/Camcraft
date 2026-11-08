//! The outlier-resistant pass of the axis fit (issue #71). The
//! least-squares concurrency fit of `axis.rs` weighs every facet's
//! normal line, and an off-centre feature's lines do not meet the axis
//! at all: a keyway's walls stand half the key's width off it, a cross
//! hole's walls the hole's whole reach. Their area drags the fitted
//! line - a D20 shaft on G54 with a 10 mm keyway fits 0.13 mm off,
//! two hundred times the lathe spindle tolerance.
//!
//! So the fit is reweighted the classical way: the lines that meet
//! the candidate axis within the concurrency tolerance are the
//! revolved surface (inliers), the rest are set aside (weight 0), and
//! the line - and its direction refinement - is refitted on the
//! inliers alone; repeated until the inlier set stops changing. The
//! refit must still pass the full judgement on EVERY facet (the
//! concurrency share and coarseness gates of `axis.rs`), so this pass
//! can only move an axis the part already earns, never invent one.
//! When nothing misses, the plain fit returns untouched - an evenly
//! revolved part keeps its bits. When too much misses (the inliers
//! carry less than half the answering area, or too few facets), the
//! plain fit stands and is marked untrusted, so the laws that read it
//! can say the part is not read as revolved rather than misplaced.
//!
//! The sagitta band that first sorts the lines is a coarseness budget
//! (3 % of the radius), far wider than a lathe spindle's half micron,
//! so it admits facets that stand NEAR a meridian plane without lying
//! in one: a cross hole's mouth ring, the middle strip of its floor.
//! Their one-sided misses lean the line by a micron (issue #116). So
//! once the inliers settle the band is tightened ONCE to five noise
//! sigmas of the settled inliers (their MAD scale) - a true surface of
//! revolution (cylinder, cone, face) meets the axis to its own noise,
//! so only the facets whose normal leaves the meridian plane fall out -
//! and the line refitted once; each pass is taken only when it lowers
//! a trimmed (clipped) residual and every gate still passes, so a
//! noisy mesh never settles on an axis worse than its plain fit. The
//! set-aside share is still read at the sagitta band, so a noisy plain
//! bar names no features.
//!
//! Provenance: iteratively reweighted least squares with a hard
//! (0/1) redescending weight - the trimmed refit of robust regression
//! (Huber, Robust Statistics; Fischler & Bolles' consensus-set refit,
//! RANSAC, 1981), with the MAD scale estimate
//! (Hampel; Huber) - applied to the normal-line concurrency fit of a
//! surface of revolution (Pottmann & Randrup, 1998). Written from
//! those sources and our own design notes; no vendor material.

use super::axis::{judge, line, Axis, Datum, REFINE_FLOOR, REL_TOL};

/// Below this share of the answering area kept, the refit is not on
/// the part's revolved surface but on a remnant, and is declined.
/// A backstop, with [`MIN_KEPT`]: it rarely fires, because `judge`
/// already requires 90 % of the answering area to meet the axis, so a
/// part the plain fit earned seldom loses half its area to the trim;
/// it guards the refit against a pathological inlier set (few or
/// tiny facets) the share gate alone would not catch.
pub(super) const MIN_KEPT_SHARE: f64 = 0.5;
/// Fewest non-pole facets a refit may rest on (backstop, as above).
pub(super) const MIN_KEPT: usize = 8;
/// Reweighting rounds: the inlier set settles in one or two.
const ROUNDS: usize = 4;
/// The tightened band, in noise sigmas: wide enough to cut no tail of
/// a mesh's own rounding (an f32-stored STL misses by its storage, evenly about
/// the axis), narrow enough to set aside the off-meridian facets whose
/// one-sided misses lean the line.
const BAND_K: f64 = 5.0;
/// The median absolute deviation's normal-consistency factor: sigma =
/// 1.4826 * MAD (Huber, Robust Statistics).
const MAD_SIGMA: f64 = 1.4826;
/// The tightened band's floor, as a fraction of the mean radius: below
/// it the misses are the solve's rounding, not the part's.
const BAND_FLOOR: f64 = 1e-9;

/// What the fit read, for the laws that must explain a refusal: the
/// robust axis, the plain least-squares one it started from, and the
/// share of the answering surface it set aside as not revolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The axis every consumer turns about (see `axis::fit`).
    pub axis: Axis,
    /// The plain fit over every facet - equal to `axis` when nothing
    /// was set aside.
    pub plain: Axis,
    /// The area share (0..1) of the non-pole facets whose normal lines
    /// miss the axis: keyways, flats, cross holes.
    pub outlier_share: f64,
    /// False when the off-axis share was too large to refit on: `axis`
    /// is then the plain fit, biased by the part's off-centre features.
    pub trusted: bool,
}

/// Reweigh the plain fit `plain` of `data` (total area `a_total`).
pub(super) fn reweigh(data: &[Datum], plain: Axis, a_total: f64) -> Fit {
    let as_is = |share: f64, trusted: bool| Fit { axis: plain, plain, outlier_share: share, trusted };
    let Some((_, rho)) = line(data, plain.dir, a_total) else {
        return as_is(0.0, true); // cannot happen for a judged fit; keep it
    };
    // The wide band: the sagitta budget, or five noise sigmas of every
    // facet's miss about the plain fit when the mesh is that noisy -
    // a band inside the noise cuts its tails one-sidedly about a biased
    // line and drifts the trimmed refit (review of PR #168). A feature
    // (keyway, hole wall) misses by millimetres and stays outside it.
    let all = vec![true; data.len()];
    let sigma = median_miss(data, &all, plain).map_or(0.0, |m| MAD_SIGMA * m);
    let tol = (REL_TOL * rho).max(BAND_K * sigma).max(1e-9);
    let keep = inliers(data, plain, tol);
    let ans_all = answering(data, &all, plain).0;
    let gate = Gate { rho, a_total, ans_all };
    // The wide pass: trim at the sagitta band until the inliers settle,
    // taken only when it lowers the trimmed residual of every facet
    // (a noisy mesh's tails cut at the band would otherwise drift it).
    let (wide, coarse) = if keep.iter().all(|k| *k) {
        (plain, keep)
    } else {
        match settle(data, plain, keep, tol, &gate) {
            Ok((ax, k)) if trimmed(data, None, ax, tol) < trimmed(data, None, plain, tol) => (ax, k),
            Ok(_) => (plain, inliers(data, plain, tol)),
            Err(keep) => return as_is(share(data, &keep, plain), false),
        }
    };
    // A fit trusted at the wide band stays trusted if the tightening
    // is declined.
    let ax = tighten(data, wide, &coarse, tol, rho, &gate).unwrap_or(wide);
    // The share is read at the wide band: the tight band's drop-outs on
    // a noisy mesh are its noise, not features to name.
    let aside = share(data, &inliers(data, ax, tol), ax);
    Fit { axis: ax, plain, outlier_share: aside, trusted: true }
}

/// The tightening (issue #116), done ONCE: the sagitta band admits
/// facets that stand near a meridian without lying in one - a cross
/// hole's mouth ring, a hole floor's centre strip - and their
/// one-sided misses lean the line by microns. The mesh's own noise is
/// read off the coarse inliers about the settled axis (the MAD scale,
/// about zero), the band set to [`BAND_K`] sigmas - far enough out that
/// it cuts no noise tail, only facets clearly off a meridian - and the
/// line refitted once on what meets it, with no re-trim (re-trimming
/// about a moving estimate sorts the facets by the sign of their
/// noise). Taken only when it passes every gate and lowers the coarse
/// inliers' trimmed residual; `None` keeps the settled fit. An exact
/// revolve meets the band everywhere and keeps its bits.
fn tighten(data: &[Datum], wide: Axis, coarse: &[bool], tol: f64, rho: f64, g: &Gate) -> Option<Axis> {
    let med = median_miss(data, coarse, wide)?;
    let band = (BAND_K * MAD_SIGMA * med).max(BAND_FLOOR * rho).max(1e-12);
    if band >= tol {
        return None;
    }
    let tight = inliers(data, wide, band);
    if tight.iter().zip(coarse).all(|(t, c)| *t == *c) {
        return None;
    }
    let ax = refit_on(data, &tight, wide, g)?;
    (trimmed(data, Some(coarse), ax, band) < trimmed(data, Some(coarse), wide, band)).then_some(ax)
}

/// The area-weighted mean squared miss about `ax`, each miss clipped
/// at `band` (a trimmed residual: a feature off the axis counts the
/// same about any candidate, noise counts in full), over `keep` (all
/// facets when `None`).
fn trimmed(data: &[Datum], keep: Option<&[bool]>, ax: Axis, band: f64) -> f64 {
    let (mut sq, mut a) = (0.0, 0.0);
    for (i, d) in data.iter().enumerate() {
        if keep.is_some_and(|k| !k.get(i).copied().unwrap_or(false)) {
            continue;
        }
        if let Some(e) = dist(d, ax).filter(|e| e.is_finite()) {
            let c = e.min(band);
            sq += d.a * c * c;
            a += d.a;
        }
    }
    if a > 0.0 { sq / a } else { f64::INFINITY }
}

/// The fixed terms every refit is judged by: the plain fit's mean
/// radius, the total area, and the answering area of the whole part.
struct Gate {
    rho: f64,
    a_total: f64,
    ans_all: f64,
}

/// Refit on `keep` and re-trim at `tol` until the inlier set stops
/// changing: the settled axis and its inliers, or (`Err`) the inlier
/// set a backstop or the full judgement refused.
fn settle(data: &[Datum], start: Axis, mut keep: Vec<bool>, tol: f64, g: &Gate) -> Result<(Axis, Vec<bool>), Vec<bool>> {
    let mut ax = start;
    for _ in 0..ROUNDS {
        let Some(next) = refit_on(data, &keep, ax, g) else {
            return Err(keep);
        };
        ax = next;
        let again = inliers(data, ax, tol);
        if again == keep {
            break;
        }
        keep = again;
    }
    Ok((ax, keep))
}

/// One refit on `keep` from `ax`, through the backstops (enough rows
/// and kept area) and the full judgement on every facet.
fn refit_on(data: &[Datum], keep: &[bool], ax: Axis, g: &Gate) -> Option<Axis> {
    let (ans_kept, rows) = answering(data, keep, ax);
    if rows < MIN_KEPT || ans_kept < MIN_KEPT_SHARE * g.ans_all {
        return None;
    }
    let kept: Vec<Datum> = data
        .iter()
        .zip(keep)
        .filter(|(_, k)| **k)
        .map(|(d, _)| Datum { n: d.n, p: d.p, a: d.a })
        .collect();
    let a_kept: f64 = kept.iter().map(|d| d.a).sum();
    let next = refit(&kept, ax, a_kept)?;
    judge(data, next, g.rho, g.a_total).map(|_| next)
}

/// The area-weighted median miss of the kept non-pole facets about
/// `ax`; `None` when none answer.
fn median_miss(data: &[Datum], keep: &[bool], ax: Axis) -> Option<f64> {
    let mut v: Vec<(f64, f64)> = data
        .iter()
        .zip(keep)
        .filter(|(_, k)| **k)
        .filter_map(|(d, _)| dist(d, ax).map(|e| (e, d.a)))
        .filter(|(e, a)| e.is_finite() && *a > 0.0)
        .collect();
    let total: f64 = v.iter().map(|(_, a)| a).sum();
    if v.is_empty() || total <= 0.0 {
        return None;
    }
    v.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut run = 0.0;
    v.iter().find(|(_, a)| {
        run += a;
        run >= 0.5 * total
    }).map(|(e, _)| *e)
}

/// The line over the kept facets about `ax`'s direction, then the
/// direction refinement - kept only past the noise floor and when the
/// kept facets' own residual does not grow (axis.rs's guard, on the
/// revolved surface alone).
fn refit(kept: &[Datum], ax: Axis, a_kept: f64) -> Option<Axis> {
    let (base, _) = line(kept, ax.dir, a_kept)?;
    let Some(v) = super::axis_refine::refine_dir(kept, base.dir, base.origin) else {
        return Some(base);
    };
    let (c, d) = (v.dot(base.dir), v.cross(base.dir));
    if d.dot(d).sqrt().atan2(c.abs()) <= REFINE_FLOOR {
        return Some(base);
    }
    match line(kept, v, a_kept) {
        Some((ax2, _)) if rms(kept, ax2) <= rms(kept, base) => Some(ax2),
        _ => Some(base),
    }
}

/// Which facets' normal lines meet `ax` within `tol` (pole facets bear
/// no row and are kept: they cannot bias the line).
fn inliers(data: &[Datum], ax: Axis, tol: f64) -> Vec<bool> {
    data.iter().map(|d| dist(d, ax).is_none_or(|e| e <= tol)).collect()
}

/// The normal line's miss of `ax`; `None` for a pole facet.
pub(super) fn dist(d: &Datum, ax: Axis) -> Option<f64> {
    let k = d.n.cross(ax.dir);
    let kn = k.dot(k).sqrt();
    if kn <= 1e-4 {
        return None;
    }
    let w = ax.origin - d.p;
    Some((k.dot(w) / kn).abs())
}

/// The kept non-pole area (about `ax`) and facet count.
fn answering(data: &[Datum], keep: &[bool], ax: Axis) -> (f64, usize) {
    data.iter()
        .zip(keep)
        .filter(|(d, k)| **k && dist(d, ax).is_some())
        .fold((0.0, 0), |(a, n), (d, _)| (a + d.a, n + 1))
}

/// The set-aside share of the non-pole area about `ax`.
fn share(data: &[Datum], keep: &[bool], ax: Axis) -> f64 {
    let all = answering(data, &vec![true; data.len()], ax).0;
    let kept = answering(data, keep, ax).0;
    if all > 0.0 { ((all - kept) / all).clamp(0.0, 1.0) } else { 0.0 }
}

/// Area-weighted RMS miss of the kept facets' lines.
fn rms(kept: &[Datum], ax: Axis) -> f64 {
    let (mut sq, mut a) = (0.0, 0.0);
    for d in kept {
        if let Some(e) = dist(d, ax) {
            sq += d.a * e * e;
            a += d.a;
        }
    }
    if a > 0.0 { (sq / a).sqrt() } else { f64::INFINITY }
}

#[cfg(test)]
#[path = "axis_robust_tests.rs"]
mod tests;
