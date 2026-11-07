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
//! Provenance: iteratively reweighted least squares with a hard
//! (0/1) redescending weight - the trimmed refit of robust regression
//! (Huber, Robust Statistics; Fischler & Bolles' consensus-set refit,
//! RANSAC, 1981) - applied to the normal-line concurrency fit of a
//! surface of revolution (Pottmann & Randrup, 1998). Written from
//! those sources and our own design notes; no vendor material.

use super::axis::{judge, line, Axis, Datum, REFINE_FLOOR, REL_TOL};

/// Below this share of the answering area kept, the refit is not on
/// the part's revolved surface but on a remnant, and is declined.
const MIN_KEPT_SHARE: f64 = 0.5;
/// Fewest non-pole facets a refit may rest on.
const MIN_KEPT: usize = 8;
/// Reweighting rounds: the inlier set settles in one or two.
const ROUNDS: usize = 4;

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
    let tol = (REL_TOL * rho).max(1e-9);
    let mut keep = inliers(data, plain, tol);
    if keep.iter().all(|k| *k) {
        return as_is(0.0, true);
    }
    let ans_all = answering(data, &vec![true; data.len()], plain).0;
    let mut ax = plain;
    for _ in 0..ROUNDS {
        let (ans_kept, rows) = answering(data, &keep, ax);
        if rows < MIN_KEPT || ans_kept < MIN_KEPT_SHARE * ans_all {
            return as_is(share(data, &keep, ax), false);
        }
        let kept: Vec<Datum> = data
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(d, _)| Datum { n: d.n, p: d.p, a: d.a })
            .collect();
        let a_kept: f64 = kept.iter().map(|d| d.a).sum();
        let Some(next) = refit(&kept, ax, a_kept) else {
            return as_is(share(data, &keep, ax), false);
        };
        if judge(data, next, rho, a_total).is_none() {
            return as_is(share(data, &keep, ax), false);
        }
        ax = next;
        let again = inliers(data, ax, tol);
        if again == keep {
            break;
        }
        keep = again;
    }
    Fit { axis: ax, plain, outlier_share: share(data, &keep, ax), trusted: true }
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
fn dist(d: &Datum, ax: Axis) -> Option<f64> {
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
mod tests {
    use crate::geometry::{Point3, Triangle};
    use crate::multiaxis::axis::{fit, fit_report};

    const SEGS: i64 = 72;

    fn ring(j: i64, z: f64) -> Point3 {
        let th = std::f64::consts::TAU * j as f64 / SEGS as f64;
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
    }

    /// A D20 shaft on world Z, z -30..30; `half > 0` cuts a keyway on +x
    /// over z 0..10 whose walls meet the ring at +-`half` segments.
    fn shaft(half: i64) -> Vec<Triangle> {
        let mut f = Vec::new();
        let zs = [-30.0, 0.0, 10.0, 30.0];
        for j in -SEGS / 2..SEGS / 2 {
            for (k, w) in zs.windows(2).enumerate() {
                if k == 1 && j >= -half && j < half {
                    continue;
                }
                let (a1, a2, b1, b2) = (ring(j, w[0]), ring(j + 1, w[0]), ring(j, w[1]), ring(j + 1, w[1]));
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
            let (c0, c1) = (Point3::new(0.0, 0.0, -30.0), Point3::new(0.0, 0.0, 30.0));
            f.push(Triangle::new(c0, ring(j + 1, -30.0), ring(j, -30.0)));
            f.push(Triangle::new(c1, ring(j, 30.0), ring(j + 1, 30.0)));
        }
        if half > 0 {
            let (xt, ys, xf) = (ring(half, 0.0).x, ring(half, 0.0).y, 8.0);
            let q = Point3::new;
            f.push(Triangle::new(q(xf, -ys, 0.0), q(xf, ys, 0.0), q(xf, ys, 10.0)));
            f.push(Triangle::new(q(xf, -ys, 0.0), q(xf, ys, 10.0), q(xf, -ys, 10.0)));
            f.push(Triangle::new(q(xf, ys, 0.0), q(xt, ys, 0.0), q(xt, ys, 10.0)));
            f.push(Triangle::new(q(xf, ys, 0.0), q(xt, ys, 10.0), q(xf, ys, 10.0)));
            f.push(Triangle::new(q(xt, -ys, 0.0), q(xf, -ys, 0.0), q(xf, -ys, 10.0)));
            f.push(Triangle::new(q(xt, -ys, 0.0), q(xf, -ys, 10.0), q(xt, -ys, 10.0)));
        }
        f
    }

    #[test]
    fn a_plain_revolve_keeps_its_plain_fit_bit_for_bit() {
        let r = fit_report(&shaft(0)).expect("a revolve fits");
        assert_eq!(r.axis, r.plain, "nothing set aside, nothing moved");
        assert!(r.trusted && r.outlier_share == 0.0, "{r:?}");
        assert_eq!(r.axis.origin, Point3::new(0.0, 0.0, 0.0), "{r:?}");
        assert_eq!(r.axis.dir, Point3::new(0.0, 0.0, 1.0), "{r:?}");
        assert_eq!(fit(&shaft(0)).ok(), Some(r.axis));
    }

    #[test]
    fn a_keyway_no_longer_drags_the_axis_and_keeps_the_plain_direction() {
        for half in [3, 6] {
            let r = fit_report(&shaft(half)).expect("a keyed shaft fits");
            let off = |p: Point3| (p.x * p.x + p.y * p.y).sqrt();
            println!("half {half}: plain off {:.4e} mm, robust off {:.4e} mm, set aside {:.3}", off(r.plain.origin), off(r.axis.origin), r.outlier_share);
            assert!(off(r.plain.origin) > 0.01, "the plain fit is dragged (the case under test): {r:?}");
            assert!(off(r.axis.origin) < 1e-9, "half {half}: the robust fit sits on the true axis: {r:?}");
            assert!(r.trusted && r.outlier_share > 0.0 && r.outlier_share < 0.1, "{r:?}");
            // The refine guard on a non-revolved remnant: the keyway
            // cannot tilt a straight shaft, so the direction is the
            // plain fit's, bit for bit.
            assert_eq!(r.axis.dir, r.plain.dir, "half {half}: {r:?}");
            assert_eq!(r.axis.dir, Point3::new(0.0, 0.0, 1.0), "half {half}: {r:?}");
        }
    }
}
