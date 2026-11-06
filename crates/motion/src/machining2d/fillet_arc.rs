//! Line-to-arc and arc-to-arc corner fillets (M11.1, owed item): a
//! corner next to an arc rounds with ONE circle tangent to both
//! elements (one G2/G3 move). Provenance: textbook plane geometry - the
//! circle tangent to a line and a circle, or to two circles, whose
//! centre stands `rho` off the line and `R -/+ rho` from an arc's
//! centre - emitted as RS-274 / ISO 6983 G2/G3; clean-room.
//!
//! # The construction
//!
//! At a corner `V` the turn is left or right (sign of `u x w`, the end
//! and start tangents). The fillet sits on the inside of the turn, so
//! each element is offset toward that side by `rho`: a line to the
//! parallel line `rho` off it, an arc of centre `C` and radius `R` to
//! the concentric circle of radius `R - rho` when the turn's inside is
//! the arc's own inside (left of a CCW arc, right of a CW one) and
//! `R + rho` otherwise. The fillet centre `P` is an intersection of the
//! two offsets (line-circle or circle-circle, at most two candidates);
//! its tangent points are the feet of `P` on each element (the
//! projection on a line, `C + R (P - C)/|P - C|` on an arc).
//!
//! # The legality law (Prime Directive: never gouge, never silent)
//!
//! A candidate rounds only when, all proved numerically:
//! - each tangent point lies on its element, in the half next to `V`
//!   (so the fillets at an element's two ends never overlap - the leg
//!   room law of the line case, measured as sweep on an arc);
//! - the fillet is tangent there in the travel sense (the wrong branch of
//!   the intersection, a cusp, an inside-out circle are all refused) and
//!   sweeps under half a turn;
//! - the fillet arc itself crosses no loop of the caller's cutter-centre
//!   region and its midpoint stands on the legal side: a connected arc
//!   that never meets the boundary is legal at every point;
//! - the radius law of the line case: `floor <= rho <= r_max`, halved
//!   ([`SHRINK_TRIES`] times) when a radius does not fit.
//!
//! The fillet circle touches each element's full curve only at its
//! tangent point and lies on the turn's inside of both, so the cutter
//! centre moves to the air side of the sharp path (corner rest, stated as
//! `|V - P| - rho`), never past its material side. Every number is `+ - *
//! / sqrt` only (no libm transcendental), as in `fillet.rs`.

use crate::geo2d::Point2;

use super::fillet::{El, Legal, SHRINK_TRIES, len, unit};
use super::fillet_tally::Verdict;
use super::ring::on_boundary;

/// Slack for "the tangent point is not the corner" and for the
/// endpoint touches of the region test (mm).
const EPS: f64 = 1e-9;

/// Largest sine between the fillet and an element at a tangent point.
const TANGENT_SIN: f64 = 1e-6;

/// An element's offset toward the turn's inside: a line (point, unit
/// direction) or a circle (centre, radius).
#[derive(Clone, Copy)]
enum Off {
    Line(Point2, Point2),
    Circle(Point2, f64),
}

fn sub(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x - b.x, a.y - b.y)
}
fn dot(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}
fn cross(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}
/// `v` turned a quarter in the travel sense (`left`: CCW).
fn quarter(v: Point2, left: bool) -> Point2 {
    if left { Point2::new(-v.y, v.x) } else { Point2::new(v.y, -v.x) }
}

/// Monotone stand-in for the angle from `from` to `to` measured in the
/// travel sense, in `[0, 4)` (the "diamond angle"): comparisons only,
/// exact and transcendental-free.
fn turn(from: Point2, to: Point2, ccw: bool) -> f64 {
    let x = dot(from, to);
    let y = if ccw { cross(from, to) } else { -cross(from, to) };
    let d = x.abs() + y.abs();
    if d <= 0.0 || !d.is_finite() {
        return f64::NAN;
    }
    match (y >= 0.0, x >= 0.0) {
        (true, true) => y / d,
        (true, false) => 1.0 - x / d,
        (false, false) => 2.0 - y / d,
        (false, true) => 3.0 + x / d,
    }
}

/// The element's offset `rho` toward the turn's inside; `v` is the corner
/// (its distance to an arc's centre is the junction radius).
fn offset(el: &El, v: Point2, left: bool, rho: f64) -> Option<Off> {
    match *el {
        El::Line(a, b) => {
            let d = unit(b.x - a.x, b.y - a.y)?;
            let n = quarter(d, left);
            Some(Off::Line(Point2::new(a.x + n.x * rho, a.y + n.y * rho), d))
        }
        El::Arc { center, ccw, .. } => {
            let r = len(v, center);
            let r2 = if left == ccw { r - rho } else { r + rho };
            (r2 > EPS && r2.is_finite()).then_some(Off::Circle(center, r2))
        }
    }
}

fn line_circle(p0: Point2, d: Point2, c: Point2, r: f64) -> Vec<Point2> {
    let f = sub(p0, c);
    let b = dot(f, d);
    let disc = b * b - (dot(f, f) - r * r);
    if disc.is_nan() || disc < 0.0 {
        return Vec::new();
    }
    let q = disc.sqrt();
    [-b - q, -b + q].iter().map(|t| Point2::new(p0.x + d.x * t, p0.y + d.y * t)).collect()
}

fn circle_circle(c1: Point2, r1: f64, c2: Point2, r2: f64) -> Vec<Point2> {
    let e = sub(c2, c1);
    let d = len(c1, c2);
    if d.is_nan() || d <= EPS {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h2 = r1 * r1 - a * a;
    if h2.is_nan() || h2 < 0.0 {
        return Vec::new();
    }
    let h = h2.sqrt();
    let (ux, uy) = (e.x / d, e.y / d);
    let base = Point2::new(c1.x + ux * a, c1.y + uy * a);
    vec![Point2::new(base.x - uy * h, base.y + ux * h), Point2::new(base.x + uy * h, base.y - ux * h)]
}

fn centres(a: Off, b: Off) -> Vec<Point2> {
    match (a, b) {
        (Off::Line(p, d), Off::Circle(c, r)) | (Off::Circle(c, r), Off::Line(p, d)) => line_circle(p, d, c, r),
        (Off::Circle(c1, r1), Off::Circle(c2, r2)) => circle_circle(c1, r1, c2, r2),
        // Two lines are `fillet.rs`'s case; never routed here.
        (Off::Line(..), Off::Line(..)) => Vec::new(),
    }
}

/// The unit direction from an arc's centre to its midpoint.
fn arc_mid(from: Point2, center: Point2, ccw: bool, to: Point2) -> Option<Point2> {
    let s = sub(from, center);
    let s = unit(s.x, s.y)?;
    let e = sub(to, center);
    let e = unit(e.x, e.y)?;
    let sense = if ccw { cross(s, e) } else { -cross(s, e) };
    let sum = Point2::new(s.x + e.x, s.y + e.y);
    let half = quarter(s, ccw);
    if sense > 0.0 {
        unit(sum.x, sum.y).or(Some(half))
    } else if sense < 0.0 {
        unit(-sum.x, -sum.y).or(Some(half))
    } else if dot(s, e) > 0.0 {
        Some(s)
    } else {
        Some(half)
    }
}

/// Foot of `p` on the element's curve, when it lies in the half next to
/// the corner `v` (`end`: `v` is the element's end) and is not `v`.
fn foot(el: &El, p: Point2, v: Point2, end: bool) -> Option<Point2> {
    let t = match *el {
        El::Line(a, b) => {
            let d = unit(b.x - a.x, b.y - a.y)?;
            let k = dot(sub(p, a), d);
            let t = Point2::new(a.x + d.x * k, a.y + d.y * k);
            let trim = if end { dot(sub(v, t), d) } else { dot(sub(t, v), d) };
            (trim > EPS && trim <= 0.5 * len(a, b)).then_some(t)?
        }
        El::Arc { from, center, ccw, to } => {
            let q = sub(p, center);
            let m = len(p, center);
            if m.is_nan() || m <= EPS {
                return None;
            }
            let r = len(v, center);
            let t = Point2::new(center.x + q.x * r / m, center.y + q.y * r / m);
            let mid = arc_mid(from, center, ccw, to)?;
            let (tv, vv) = (sub(t, center), sub(v, center));
            let ok = if end {
                turn(mid, tv, ccw) < turn(mid, vv, ccw)
            } else {
                turn(vv, tv, ccw) <= turn(vv, mid, ccw)
            };
            (ok && len(t, v) > EPS).then_some(t)?
        }
    };
    Some(t)
}

/// Travel tangent of the element at a point `q` on it.
fn tangent_at(el: &El, q: Point2) -> Option<Point2> {
    match *el {
        El::Line(a, b) => unit(b.x - a.x, b.y - a.y),
        El::Arc { center, ccw, .. } => {
            let d = quarter(sub(q, center), ccw);
            unit(d.x, d.y)
        }
    }
}

fn tangent_ok(el: &El, q: Point2, p: Point2, rho: f64, left: bool) -> bool {
    let f = quarter(sub(q, p), left);
    let (Some(f), Some(t)) = (unit(f.x, f.y), tangent_at(el, q)) else {
        return false;
    };
    (len(q, p) - rho).abs() <= 1e-7 * (1.0 + rho) && dot(f, t) > 0.0 && cross(f, t).abs() <= TANGENT_SIN
}

/// The region half of the law, on the fillet arc itself: no loop edge
/// meets it away from its ends, and its midpoint is on the legal side.
fn arc_clear(legal: &Legal, p: Point2, rho: f64, t1: Point2, t2: Point2, left: bool) -> bool {
    let (a, b) = (sub(t1, p), sub(t2, p));
    let Some(m) = unit(a.x + b.x, a.y + b.y) else {
        return false;
    };
    let mid = Point2::new(p.x + m.x * rho, p.y + m.y * rho);
    if !(on_boundary(legal.region, mid) || legal.region.contains(mid) == legal.inside) {
        return false;
    }
    let sweep = turn(a, b, left);
    for l in legal.region.loops() {
        let pts = l.points();
        for (i, &c) in pts.iter().enumerate() {
            let d = pts[(i + 1) % pts.len()];
            let Some(dir) = unit(d.x - c.x, d.y - c.y) else { continue };
            let el = len(c, d);
            for q in line_circle(c, dir, p, rho) {
                let k = dot(sub(q, c), dir);
                let on_edge = k >= -EPS && k <= el + EPS;
                let at_end = len(q, t1) <= EPS || len(q, t2) <= EPS;
                if on_edge && !at_end && turn(a, sub(q, p), left) <= sweep {
                    return false;
                }
            }
        }
    }
    true
}

/// One radius tried: the fillet `(t1, t2, centre)`, `Err(true)` when only
/// the region refused it, `Err(false)` when no tangent circle fits.
fn try_radius(a: &El, b: &El, v: Point2, left: bool, rho: f64, legal: &Legal) -> Result<(Point2, Point2, Point2), bool> {
    let (Some(oa), Some(ob)) = (offset(a, v, left, rho), offset(b, v, left, rho)) else {
        return Err(false);
    };
    let mut best: Option<(f64, Point2, Point2, Point2)> = None;
    for p in centres(oa, ob) {
        if !(p.x.is_finite() && p.y.is_finite()) {
            continue;
        }
        let (Some(t1), Some(t2)) = (foot(a, p, v, true), foot(b, p, v, false)) else { continue };
        let swept = cross(sub(t1, p), sub(t2, p));
        let under_half = if left { swept > 0.0 } else { swept < 0.0 };
        if under_half && tangent_ok(a, t1, p, rho, left) && tangent_ok(b, t2, p, rho, left) {
            let d = len(p, v);
            if best.is_none_or(|(bd, ..)| d < bd) {
                best = Some((d, t1, t2, p));
            }
        }
    }
    let (_, t1, t2, p) = best.ok_or(false)?;
    if arc_clear(legal, p, rho, t1, t2, left) { Ok((t1, t2, p)) } else { Err(true) }
}

/// Judge a corner with an arc on at least one side (`left`: the turn's
/// sense, `c`: the cosine of its deflection). Same radius law as the
/// line case: `r_max`, halved on a refusal, never under `floor`.
pub(super) fn judge_curved(a: &El, b: &El, left: bool, c: f64, r_max: f64, floor: f64, legal: &Legal) -> Verdict {
    if legal.air_left.is_some_and(|air_left| air_left != left) {
        return Verdict::TowardPart;
    }
    // A cusp (tangents reversed) has no tangent fillet on this side.
    if 1.0 + c <= 1e-9 {
        return Verdict::Curved;
    }
    let v = a.to();
    let mut rho = if r_max.is_finite() && r_max > 0.0 { r_max } else { 0.0 };
    if rho < floor {
        return Verdict::Tiny;
    }
    let mut blocked = false;
    for _ in 0..=SHRINK_TRIES {
        if rho < floor {
            break;
        }
        match try_radius(a, b, v, left, rho, legal) {
            Ok((t1, t2, p)) => return Verdict::Round(t1, t2, p, left, (len(p, v) - rho).max(0.0)),
            Err(region) => blocked |= region,
        }
        rho *= 0.5;
    }
    if blocked { Verdict::Blocked } else { Verdict::Curved }
}
