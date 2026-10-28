//! The nose offset (M5.2 B): where a ball of radius d can RIDE the
//! turned profile - the locus of centers at distance exactly d from
//! the material, sampled in (station, radius).
//!
//! The construction is the Minkowski reading, not a line-offset
//! with joined corners: every boundary piece of the profile says a
//! radius its distance-d neighbors must stand outside -
//!
//! - a segment of slope b, at every station of its window: the
//!   parallel line raised by d*sqrt(1 + b^2) (offset along the
//!   normal),
//! - every vertex: the circle of radius d about it, within d of its
//!   station,
//!
//! and the offset curve is the MAXIMUM of those claims. A point at
//! the max stands at distance >= d from EVERY piece (each claim is
//! exactly "distance >= d from this piece at this station"), so a
//! ball centered on the curve can never touch material it shouldn't:
//! no gouge is a theorem here, not a corner-case table, and the
//! narrow-groove and step-shadow folds that defeat naive
//! line-offset-and-intersect settle themselves: the tall claim wins
//! and the feature is bridged exactly as a real nose rolls over it.
//! Where claims cross, the curve kinks; where a vertex's circle
//! peaks, it arcs. Sampling: candidates at every vertex station and
//! vertex station +/- d (so step faces of the offset appear as
//! verticals, and arcs and flats meet at their tangents), bisection
//! wherever a chord of the (piecewise flat-or-circular, always
//! C0-continuous) curve stands off the true value by more than the
//! caller's share of the tolerance. Chords of circular pieces sit
//! INSIDE the arc by at most that share - the nose bites no deeper
//! than the budget says, which is the whole promise of a
//! tolerance-driven profile pass.
//!
//! Provenance: tool-radius compensation on a revolved profile -
//! the offset-curve/Minkowski-sum construction as any CAM theory
//! text and the RS-274 radius-compensation vocabulary describe it;
//! written from that and our own design notes. No vendor material
//! consulted.

use crate::machining2d::GenError;

type SR = (f64, f64);

/// The hard sample budget for one offset curve.
const MAX_OFFSET_PTS: usize = 1 << 16;

/// The ride of a ball of radius `d` on the silhouette `pts`
/// (ascending stations, verticals allowed), over the station window
/// [s_lo, s_hi], settling every chord within `tol`.
pub fn profile_offset(
    pts: &[SR],
    d: f64,
    tol: f64,
    s_lo: f64,
    s_hi: f64,
) -> Result<Vec<SR>, GenError> {
    if pts.len() < 2 {
        return Err(GenError::Empty(
            "the turned profile has nothing to ride".into(),
        ));
    }
    if d <= 0.0 || !d.is_finite() {
        return Err(GenError::Invalid(format!(
            "an offset of {d} is not a size a ball can ride"
        )));
    }
    if tol <= 0.0 || !tol.is_finite() {
        return Err(GenError::Invalid(format!(
            "the offset's chord tolerance must be a positive finite size, not {tol}"
        )));
    }
    if s_hi <= s_lo || !s_lo.is_finite() || !s_hi.is_finite() {
        return Err(GenError::Empty(
            "the turned profile has no station window to ride".into(),
        ));
    }
    // The claims. Circles ride sorted by station for the range
    // scan; segment windows partition the stations (the silhouette
    // is a function there), so a bounded probe of neighbors finds
    // every window containing s.
    let segs: Vec<(f64, f64, f64, f64)> = pts
        .windows(2)
        .filter(|w| w[0].0 < w[1].0)
        .map(|w| (w[0].0, w[1].0, w[0].1, (w[1].1 - w[0].1) / (w[1].0 - w[0].0)))
        .collect();
    let d2 = d * d;
    let val = |s: f64| -> f64 {
        let mut r: f64 = 0.0;
        let clo = pts.partition_point(|&(sv, _)| sv < s - d);
        let chi = pts.partition_point(|&(sv, _)| sv <= s + d);
        for &(sv, rv) in &pts[clo..chi] {
            let q = d2 - (s - sv) * (s - sv);
            if q >= 0.0 {
                r = r.max(rv + q.sqrt());
            }
        }
        let i = segs.partition_point(|&(s0, _, _, _)| s0 <= s);
        for &(s0, s1, a, b) in &segs[i.saturating_sub(1)..(i + 2).min(segs.len())] {
            if s0 <= s && s <= s1 {
                r = r.max(a + b * (s - s0) + d * (1.0 + b * b).sqrt());
            }
        }
        r
    };

    // Candidate stations: the window's ends and every vertex's
    // s - d / s / s + d: the arcs' peaks, ends and the verticals
    // they leave behind.
    let mut cs: Vec<f64> = vec![s_lo, s_hi];
    for &(sv, _) in pts {
        for c in [sv - d, sv, sv + d] {
            if s_lo < c && c < s_hi {
                cs.push(c);
            }
        }
    }
    cs.sort_unstable_by(f64::total_cmp);
    cs.dedup();

    let mut out: Vec<SR> = Vec::with_capacity(cs.len());
    let mut budget = MAX_OFFSET_PTS;
    let mut prev = (cs[0], val(cs[0]));
    out.push(prev);
    for &c in &cs[1..] {
        let here = (c, val(c));
        settle(&val, prev, here, tol, &mut out, &mut budget)?;
        out.push(here);
        prev = here;
    }
    out.dedup();
    Ok(out)
}

/// The ride of a ball of radius `d` INSIDE a hole whose wall is
/// `pts` (M5.2 F): the max-of-claims offset read through the
/// mirror. Every wall piece now says "stand BELOW me by d": a
/// segment's parallel lowered by d*sqrt(1 + b^2), a vertex's
/// circle near side rv - sqrt(d^2 - ds^2) - and the center curve
/// is the MINIMUM of those claims, the tightest position that is
/// legal against every piece at once. The same Minkowski theorem
/// as the OD ride keeps it gouge-free, and the same candidate set
/// and settle law sample it. The seed is +INFINITY, not the axis:
/// reusing `profile_offset` on station-flipped points would pin
/// every negative claim at its r = 0.0 floor and walk the bar down
/// the axis - the inside ride is its own curve, not a mirror
/// image of the outside one.
pub fn profile_offset_in(
    pts: &[SR],
    d: f64,
    tol: f64,
    s_lo: f64,
    s_hi: f64,
) -> Result<Vec<SR>, GenError> {
    if pts.len() < 2 {
        return Err(GenError::Empty("the hole's wall has nothing to ride".into()));
    }
    if d <= 0.0 || !d.is_finite() {
        return Err(GenError::Invalid(format!(
            "an offset of {d} is not a size a ball can ride"
        )));
    }
    if tol <= 0.0 || !tol.is_finite() {
        return Err(GenError::Invalid(format!(
            "the offset's chord tolerance must be a positive finite size, not {tol}"
        )));
    }
    if s_hi <= s_lo || !s_lo.is_finite() || !s_hi.is_finite() {
        return Err(GenError::Empty(
            "the hole's wall has no station window to ride".into(),
        ));
    }
    if pts[0].0 > s_lo || pts[pts.len() - 1].0 < s_hi {
        return Err(GenError::Empty(
            "the hole's wall does not span the window".into(),
        ));
    }
    let segs: Vec<(f64, f64, f64, f64)> = pts
        .windows(2)
        .filter(|w| w[0].0 < w[1].0)
        .map(|w| (w[0].0, w[1].0, w[0].1, (w[1].1 - w[0].1) / (w[1].0 - w[0].0)))
        .collect();
    let d2 = d * d;
    let val = |s: f64| -> f64 {
        let mut r: f64 = f64::INFINITY;
        let clo = pts.partition_point(|&(sv, _)| sv < s - d);
        let chi = pts.partition_point(|&(sv, _)| sv <= s + d);
        for &(sv, rv) in &pts[clo..chi] {
            let q = d2 - (s - sv) * (s - sv);
            if q >= 0.0 {
                r = r.min(rv - q.sqrt());
            }
        }
        let i = segs.partition_point(|&(s0, _, _, _)| s0 <= s);
        for &(s0, s1, a, b) in &segs[i.saturating_sub(1)..(i + 2).min(segs.len())] {
            if s0 <= s && s <= s1 {
                r = r.min(a + b * (s - s0) - d * (1.0 + b * b).sqrt());
            }
        }
        // A station no piece claims would have to lie off the wall
        // polyline, which the spanning door above rules out.
        debug_assert!(r.is_finite());
        r
    };

    let mut cs: Vec<f64> = vec![s_lo, s_hi];
    for &(sv, _) in pts {
        for c in [sv - d, sv, sv + d] {
            if s_lo < c && c < s_hi {
                cs.push(c);
            }
        }
    }
    cs.sort_unstable_by(f64::total_cmp);
    cs.dedup();

    let mut out: Vec<SR> = Vec::with_capacity(cs.len());
    let mut budget = MAX_OFFSET_PTS;
    let mut prev = (cs[0], val(cs[0]));
    out.push(prev);
    for &c in &cs[1..] {
        let here = (c, val(c));
        settle(&val, prev, here, tol, &mut out, &mut budget)?;
        out.push(here);
        prev = here;
    }
    out.dedup();
    Ok(out)
}

/// Bisect a candidate gap until its chord stands within `tol` of
/// the curve (the max of flats and circle tops is C0 and
/// piecewise smooth, so the test converges), left half first so
/// the walk is born in station order.
fn settle<F: Fn(f64) -> f64 + Copy>(
    val: &F,
    p0: SR,
    p1: SR,
    tol: f64,
    out: &mut Vec<SR>,
    budget: &mut usize,
) -> Result<(), GenError> {
    let m = p0.0 + (p1.0 - p0.0) * 0.5;
    if m <= p0.0 || m >= p1.0 {
        return Ok(());
    }
    let fm = val(m);
    if ((p0.1 + p1.1) * 0.5 - fm).abs() > tol {
        if *budget == 0 {
            return Err(GenError::Invalid(format!(
                "the offset profile does not settle its chord at tolerance {tol} - \
                 coarsen the model or widen the tolerance"
            )));
        }
        *budget -= 1;
        settle(val, p0, (m, fm), tol, out, budget)?;
        out.push((m, fm));
        settle(val, (m, fm), p1, tol, out, budget)?;
    }
    Ok(())
}
