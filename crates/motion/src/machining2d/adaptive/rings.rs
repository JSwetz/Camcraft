//! The growth rings' geometry (M11.4 C round 3, `grow`): the
//! morphological opening the rings are filleted by (Serra 1982), the
//! piece of a grown area that holds the entry, the fillet radii an area
//! admits, and the revolution between two nested rings - every point of
//! the inner one blended toward its nearest point on the outer by the
//! share walked (the offset morph of Held & Spielberger 2009, CAD 41(7)).
//! Ours, no vendor material.

use crate::geo2d::{Loop, Point2, Region};

use super::GenError;
use super::front::{densify, dist, len, rotate_to, thin};
use super::near::SegIndex;

/// Arc flattening of the rings' fillets (mm): about 2.6 degrees a chord
/// on a 1 mm fillet.
pub(super) const ARC_TOL: f64 = 2.5e-4;

/// Ring vertices closer than this (mm) are merged: the boolean engine's
/// micron flats say nothing about direction.
pub(super) const MERGE: f64 = 0.01;

/// Fillet radii a ring or the last lap may use (mm, descending).
const LADDER: [f64; 12] = [2.5, 2.0, 1.6, 1.25, 1.0, 0.8, 0.6, 0.45, 0.3, 0.2, 0.12, 0.08];

/// The last lap's corner radius is at most this share of the narrowest
/// half-width of the area (a channel's opening must not vanish).
pub(super) const NARROW: f64 = 0.45;

/// The morph samples its source ring at most this far apart (mm).
pub(super) const SEG: f64 = 0.2;

/// The morph's nearest-point grid (mm).
pub(super) const NEAR_CELL: f64 = 0.5;

pub(super) fn open(r: &Region, rho: f64) -> Result<Region, GenError> {
    Ok(r.offset_fine(-rho, ARC_TOL)?.offset_fine(rho, ARC_TOL)?)
}

pub(super) fn one_loop(r: &Region) -> bool {
    matches!(r.loops(), [l] if l.area() > 0.0)
}

/// Even-odd point in polygon.
pub(super) fn inside(pts: &[Point2], p: Point2) -> bool {
    let n = pts.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

/// The piece of `y` holding `seed` as a one-loop region and its ring
/// (closed, climb-wise, merged), or `None` (no such piece, or it has a
/// hole: the rings must stay one simple loop).
pub(super) fn piece(y: &Region, seed: Point2, climb: bool) -> Result<Option<(Region, Vec<Point2>)>, GenError> {
    let Some(l) = y.loops().iter().find(|l| l.area() > 0.0 && inside(l.points(), seed)) else { return Ok(None) };
    if y.loops().iter().any(|h| h.area() < 0.0 && h.points().first().is_some_and(|&p| inside(l.points(), p))) {
        return Ok(None);
    }
    let mut pts = l.points().to_vec();
    if !climb {
        pts.reverse();
    }
    if let Some(&f) = pts.first() {
        pts.push(f);
    }
    let mut ring = thin(&pts, MERGE);
    if let Some(&f) = ring.first()
        && let Some(last) = ring.last_mut()
    {
        *last = f;
    }
    if ring.len() < 4 {
        return Ok(None);
    }
    // The region is the merged ring itself: the next growth starts from
    // what is walked, and the vertex count stays bounded.
    let body: Vec<Point2> = ring[..ring.len() - 1].to_vec();
    let body = if climb { body } else { body.into_iter().rev().collect() };
    Ok(Some((Region::new(vec![Loop::new(body)?])?, ring)))
}

/// One revolution from ring `a` (starting where the walk stands) onto
/// ring `b`: every point of `a` blended toward its nearest point on `b`
/// by the share of `a` walked, ending on `b`'s seam; returns it and `b`
/// rotated to that seam.
pub(super) fn morph_rev(a: &[Point2], b: &[Point2]) -> (Vec<Point2>, Vec<Point2>) {
    // Densified: the seam's drop-out stays a fraction of a chord, never a
    // wall run's length.
    let a = densify(a, SEG, false);
    let a = a.as_slice();
    let Some(&a0) = a.first() else { return (Vec::new(), b.to_vec()) };
    let b = rotate_to(b, a0);
    let ix = SegIndex::new(&b, NEAR_CELL);
    let total = len(a);
    let seam = b.first().copied().unwrap_or(a0);
    let near = 0.75 * a.windows(2).map(|s| dist(s[0], s[1])).fold(0.0, f64::max);
    let mut out: Vec<Point2> = Vec::with_capacity(a.len() + 1);
    let mut acc = 0.0;
    let n = a.len().saturating_sub(1);
    for j in 0..n {
        if j > 0 {
            acc += dist(a[j - 1], a[j]);
        }
        let t = if total > 1e-12 { acc / total } else { 1.0 };
        let q = ix.nearest(a[j]);
        let p = Point2::new(a[j].x + (q.x - a[j].x) * t, a[j].y + (q.y - a[j].y) * t);
        if j > n / 2 && dist(p, seam) < near {
            break;
        }
        if out.last().is_none_or(|&l| dist(l, p) > 1e-6) {
            out.push(p);
        }
    }
    if out.last().is_none_or(|&l| dist(l, seam) > 1e-6) {
        out.push(seam);
    }
    // Thinned to the morph spiral's chord law (sag well under the rings'
    // gap): wall runs post as long chords, not thousands of moves.
    (super::morph::thin_curve(&out), b)
}

/// The largest ladder radius at most `cap` for which `ok` holds.
pub(super) fn pick(cap: f64, mut ok: impl FnMut(f64) -> Result<bool, GenError>) -> Result<Option<f64>, GenError> {
    for &rho in LADDER.iter().filter(|&&r| r <= cap + 1e-9) {
        if ok(rho)? {
            return Ok(Some(rho));
        }
    }
    Ok(None)
}

/// The largest fillet radius at most `cap` the area `d` stays one loop
/// under (its narrowest channel survives the opening), by bisection.
pub(super) fn widest_opening(d: &Region, cap: f64) -> Result<Option<f64>, GenError> {
    let ok = |p: f64| -> Result<bool, GenError> { Ok(one_loop(&d.offset(-1.03 * p)?)) };
    let (mut lo, mut hi) = (0.05f64, cap);
    if hi < lo || !ok(lo)? {
        return Ok(None);
    }
    if ok(hi)? {
        return Ok(Some(hi));
    }
    for _ in 0..12 {
        let m = 0.5 * (lo + hi);
        if ok(m)? {
            lo = m
        } else {
            hi = m
        }
    }
    Ok(Some(lo))
}


/// A circle takes at least this many chords: 3.75 degrees a joint, under
/// the 5-degree in-cut break law (round 4; 72 sat on it).
const MIN_CIRCLE: usize = 96;

/// A closed circle about `c` of radius `rr` starting at `start`'s angle,
/// travelled counter-clockwise for climb (material outside on the right)
/// and clockwise otherwise.
pub(super) fn circle(c: Point2, rr: f64, start: Point2, climb: bool, seg: f64) -> Vec<Point2> {
    let a0 = (start.y - c.y).atan2(start.x - c.x);
    let n = ((std::f64::consts::TAU * rr / seg.max(1e-6)).ceil() as usize).clamp(MIN_CIRCLE, 4096);
    let dir = if climb { 1.0 } else { -1.0 };
    let pts: Vec<Point2> = (0..=n)
        .map(|k| {
            let a = a0 + dir * std::f64::consts::TAU * k as f64 / n as f64;
            Point2::new(c.x + rr * a.cos(), c.y + rr * a.sin())
        })
        .collect();
    densify(&pts, seg, false)
}
