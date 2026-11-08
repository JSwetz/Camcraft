//! The point-concurrency guard of the axis fit (issue #208). The
//! concurrency law of `axis.rs` asks whether each facet's normal line
//! meets the candidate axis, and that test cannot tell a pencil of
//! lines through ONE POINT from lines meeting a line: a two-triangle
//! face anchors its line at the face centre, so a box's faces all
//! pass through the box centre, and a single-band wall's lines all
//! pass through the bar's mid point. Lines through one point meet
//! EVERY line through it. On a plate or cube with a small turned boss
//! the boss lends its bearings to the coarseness gate and the plate's
//! faces "meet" a 45-degree line through their centre: a trusted
//! wrong axis.
//!
//! A pencil of lines through a point carries no direction of its own,
//! so it may certify an axis only when it is itself rotationally
//! symmetric about it - the Gauss map of a surface of revolution is a
//! union of circles about the axis, so the facets that meet at a point
//! must spread their area evenly over the bearings about it. So the
//! inliers are peeled into pencils (the point their lines meet best,
//! least trimmed squares on the area, then the next among the rest)
//! and each pencil is judged by itself: its low trigonometric moments
//! about the axis must vanish (a box face set or a crossing rod's wall
//! piles its area on 2 or 4 bearings), and a pencil holding half the
//! inliers must pass the coarseness gate alone. The coarseness gate
//! itself reads only facets that meet the axis (`axis.rs`), so an
//! outlier boss lends nothing. A true revolve's rings pass whatever
//! their band count; a point-concurrent prism or a cross never does.
//!
//! Provenance: the Gauss map of a surface of revolution (do Carmo,
//! Differential Geometry of Curves and Surfaces) and the concurrency
//! of its normal lines with the axis (Pottmann & Randrup 1998);
//! least-squares point of nearest approach to a set of lines, least
//! trimmed squares (Rousseeuw 1984), and the trigonometric moments of
//! circular data (Mardia & Jupp, Directional Statistics). Written from those sources and
//! our own design notes; no vendor material.

use crate::geometry::Point3;

use super::axis::{Axis, Datum, REL_TOL};
use super::axis_robust::dist;
use super::linalg::{solve3, unit};

/// Share of the inlier (non-pole) area meeting at one point past which
/// the point, not the axis, is what the lines agree on.
const POINT_SHARE: f64 = 0.5;
/// Trimming rounds that find the point a pencil of lines meets.
const ROUNDS: usize = 6;
/// Pencils peeled off the inliers at most (a banded wall is one per
/// band; past this the rest are too small to bias anything).
const MAX_PENCILS: usize = 12;
/// Smallest pencil, as a share of the inlier area, worth judging.
const MIN_PENCIL: f64 = 0.05;
/// The uneven area of all pencils together, as a share of the inlier
/// area: the same 10 % the concurrency share lets miss the axis.
const BIAS_SHARE: f64 = 0.1;
/// Trigonometric moments read: up to four-fold (a box, a jack).
const HARMONICS: usize = 4;

/// The widest empty skip (radians) between the bearings about `u` of
/// the normals `ns`; `None` when too few bear an azimuth to call
/// anything round. `sqrt(8 * REL_TOL)` is the chord-sagitta budget in
/// angle: a skip past it is a polygon pretending to be a wall.
pub(super) fn widest_skip(ns: impl Iterator<Item = Point3>, u: Point3) -> Option<f64> {
    let (e1, e2) = frame(u)?;
    let mut angs: Vec<f64> = ns.filter_map(|n| bearing(n, u, e1, e2)).collect();
    if angs.len() <= 2 {
        return None;
    }
    angs.sort_by(|x, y| x.total_cmp(y));
    let (first, last) = (angs.first().copied()?, angs.last().copied()?);
    let gap = angs.windows(2).fold(std::f64::consts::TAU - last + first, |g, w| g.max(w[1] - w[0]));
    Some(gap)
}

/// The sagitta budget written in angle (see [`widest_skip`]).
pub(super) fn skip_budget() -> f64 {
    (8.0 * REL_TOL).sqrt()
}

/// True when the inliers of `ax` (normal lines within `tol`) bear the
/// axis out. The inliers are peeled into pencils - the lines through
/// one point, found by [`pencil_point`], then the next point among the
/// rest - and each pencil is judged ON ITS OWN (review of PR #223: two
/// crossed single-band cylinders are two pencils at one point, and the
/// union of a ring and a two-bearing pencil looked round). A pencil of
/// a revolve is a ring (or zone) about the axis, so its area spreads
/// evenly over the bearings: its unevenness (low trigonometric
/// moments, [`unevenness`]), weighted by the pencil's area, may total
/// at most [`BIAS_SHARE`] of the inliers. A pencil holding half the
/// inliers must also pass the coarseness gate by itself.
pub(super) fn borne_out(data: &[Datum], ax: Axis, tol: f64) -> bool {
    let mut rest: Vec<&Datum> = data.iter().filter(|d| dist(d, ax).is_some_and(|e| e <= tol)).collect();
    let a_in: f64 = rest.iter().map(|d| d.a).sum();
    if a_in <= 0.0 || !a_in.is_finite() {
        return true; // judge's share gate speaks for an empty set
    }
    let mut biased = 0.0;
    for _ in 0..MAX_PENCILS {
        let Some(c) = pencil_point(&rest) else {
            break; // the lines fix no point: no pencil left to fear
        };
        let (near, far): (Vec<&Datum>, Vec<&Datum>) = rest.iter().partition(|d| miss(d, c) <= tol);
        let a_near: f64 = near.iter().map(|d| d.a).sum();
        if near.len() < 3 || a_near < MIN_PENCIL * a_in {
            break;
        }
        if a_near >= POINT_SHARE * a_in && widest_skip(near.iter().map(|d| d.n), ax.dir).is_none_or(|g| g > skip_budget()) {
            return false;
        }
        biased += unevenness(&near, ax.dir).unwrap_or(1.0) * a_near;
        if biased > BIAS_SHARE * a_in {
            return false;
        }
        rest = far;
    }
    true
}

/// The bearing of `n` about `u` in the frame (e1, e2); `None` for a
/// pole (a normal along the axis bears no azimuth).
fn bearing(n: Point3, u: Point3, e1: Point3, e2: Point3) -> Option<f64> {
    let s = n.dot(u);
    let p = Point3::new(n.x - u.x * s, n.y - u.y * s, n.z - u.z * s);
    (p.dot(p) > 1e-4).then(|| p.dot(e2).atan2(p.dot(e1)).rem_euclid(std::f64::consts::TAU))
}

/// A frame perpendicular to `u`.
fn frame(u: Point3) -> Option<(Point3, Point3)> {
    let any = if u.z.abs() < 0.9 { Point3::new(0.0, 0.0, 1.0) } else { Point3::new(1.0, 0.0, 0.0) };
    let e1 = unit(u.cross(any))?;
    Some((e1, u.cross(e1)))
}

/// How far the facets' area-weighted bearings about `u` stand from
/// an even ring: the largest of the first [`HARMONICS`] trigonometric
/// moments `|sum a e^(ik theta)| / sum a` (Mardia & Jupp, Directional
/// Statistics - the Rayleigh test is k = 1). An even ring of N facets
/// gives 0 for every k < N; a pencil holding share s of the area on
/// two opposite bearings (a crossing rod) gives s at k = 2, on four (a
/// box, a jack) s at k = 4; a D-flat that lumps a cut arc's area on
/// one bearing keeps the low moments nearly whole. `None` when no
/// facet bears an azimuth.
fn unevenness(ds: &[&Datum], u: Point3) -> Option<f64> {
    let (e1, e2) = frame(u)?;
    let (mut c, mut sn, mut total) = ([0.0f64; HARMONICS], [0.0f64; HARMONICS], 0.0);
    for d in ds {
        let Some(t) = bearing(d.n, u, e1, e2) else {
            continue;
        };
        for (k, (ck, sk)) in c.iter_mut().zip(sn.iter_mut()).enumerate() {
            let kt = (k + 1) as f64 * t;
            *ck += d.a * kt.cos();
            *sk += d.a * kt.sin();
        }
        total += d.a;
    }
    if total <= 0.0 || !total.is_finite() {
        return None;
    }
    Some(c.iter().zip(&sn).fold(0.0f64, |m, (x, y)| m.max((x * x + y * y).sqrt() / total)))
}

/// The point the lines meet best: the area-weighted least-squares
/// point of nearest approach, refitted on the area half that misses it
/// least until that half settles. `None` when the lines fix no point
/// (all parallel) or the solve fails.
fn pencil_point(inl: &[&Datum]) -> Option<Point3> {
    let mut c = point_of(inl.iter().copied())?;
    let total: f64 = inl.iter().map(|d| d.a).sum();
    for _ in 0..ROUNDS {
        let mut m: Vec<(f64, &Datum)> = inl.iter().map(|d| (miss(d, c), *d)).collect();
        m.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut acc = 0.0;
        let half: Vec<&Datum> = m
            .iter()
            .take_while(|(_, d)| {
                let before = acc;
                acc += d.a;
                before < 0.5 * total
            })
            .map(|(_, d)| *d)
            .collect();
        let next = point_of(half.into_iter())?;
        let step = next - c;
        c = next;
        if step.dot(step).sqrt() <= 1e-12 * (1.0 + c.dot(c).sqrt()) {
            break;
        }
    }
    c.is_finite().then_some(c)
}

/// Least-squares point nearest the lines (p, n) weighted by area:
/// `sum a (I - n n^T) c = sum a (I - n n^T) p`.
fn point_of<'a>(ds: impl Iterator<Item = &'a Datum>) -> Option<Point3> {
    let (mut m, mut b) = ([[0.0f64; 3]; 3], [0.0f64; 3]);
    let mut tr = 0.0;
    for d in ds {
        let n = [d.n.x, d.n.y, d.n.z];
        let p = [d.p.x, d.p.y, d.p.z];
        for i in 0..3 {
            for j in 0..3 {
                let pij = if i == j { 1.0 } else { 0.0 } - n[i] * n[j];
                m[i][j] += d.a * pij;
                b[i] += d.a * pij * p[j];
            }
        }
        tr += 2.0 * d.a;
    }
    if tr <= 0.0 {
        return None;
    }
    // A pencil of lines in one plane (a single-band wall) fixes its
    // point fully; parallel lines leave a direction free and fix none.
    let (vals, _) = super::linalg::sym_eigen(m)?;
    if vals[0] <= 1e-6 * tr {
        return None;
    }
    solve3(m, b).filter(|c| c.is_finite())
}

/// The distance from `c` to the facet's normal line.
fn miss(d: &Datum, c: Point3) -> f64 {
    let w = c - d.p;
    let s = w.dot(d.n);
    let r = Point3::new(w.x - d.n.x * s, w.y - d.n.y * s, w.z - d.n.z * s);
    r.dot(r).sqrt()
}

#[cfg(test)]
#[path = "axis_point_tests.rs"]
mod tests;
