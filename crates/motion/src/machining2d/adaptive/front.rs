//! Polyline helpers of the adaptive planner (M11.4 C): distances,
//! densify / thin, seams and end headings. Elementary plane geometry;
//! ours, no vendor material.

use crate::geo2d::Point2;

pub(super) fn dist(a: Point2, b: Point2) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

pub(super) fn len(pts: &[Point2]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Insert points so no segment is longer than `max` (closed: the
/// wrap-around segment too, returned with the first point repeated).
pub(super) fn densify(pts: &[Point2], max: f64, closed: bool) -> Vec<Point2> {
    let mut out = Vec::with_capacity(pts.len() * 2);
    let n = pts.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    if n == 0 {
        return out;
    }
    out.push(pts[0]);
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let k = ((dist(a, b) / max).ceil() as usize).clamp(1, 4096);
        for j in 1..=k {
            let t = j as f64 / k as f64;
            out.push(Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

/// Drop points closer than `min` to the last kept one (the last point
/// is always kept): clipper's micron round-join flats become one step.
pub(super) fn thin(pts: &[Point2], min: f64) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::with_capacity(pts.len());
    for (i, &p) in pts.iter().enumerate() {
        let last = i + 1 == pts.len();
        match out.last() {
            Some(&q) if dist(q, p) < min && !last => {}
            Some(&q) if dist(q, p) < min && last && out.len() > 1 => {
                if let Some(l) = out.last_mut() {
                    *l = p;
                }
            }
            _ => out.push(p),
        }
    }
    out
}

/// Rotate a closed ring (first point repeated last) to start at the
/// vertex nearest `p`.
pub(super) fn rotate_to(ring: &[Point2], p: Point2) -> Vec<Point2> {
    let n = ring.len().saturating_sub(1);
    if n == 0 {
        return ring.to_vec();
    }
    let k = (0..n).min_by(|&i, &j| dist(ring[i], p).total_cmp(&dist(ring[j], p))).unwrap_or(0);
    let mut out: Vec<Point2> = (0..=n).map(|j| ring[(k + j) % n]).collect();
    if let Some(l) = out.last_mut() {
        *l = ring[k];
    }
    out
}

/// Unit direction from `a` to `b` (x-axis for a degenerate pair).
pub(super) fn unit(a: Point2, b: Point2) -> Point2 {
    let l = dist(a, b);
    if l <= 1e-12 { Point2::new(1.0, 0.0) } else { Point2::new((b.x - a.x) / l, (b.y - a.y) / l) }
}

/// Heading at the start / end of a polyline. (A micron-short end
/// segment - a slice landing next to a vertex - says nothing about
/// direction: the first point a hair away decides.)
pub(super) fn head(pts: &[Point2]) -> Point2 {
    let Some(&a) = pts.first() else { return Point2::new(1.0, 0.0) };
    pts.iter().skip(1).find(|&&q| dist(a, q) > 1e-4).map_or(Point2::new(1.0, 0.0), |&q| unit(a, q))
}

pub(super) fn tail(pts: &[Point2]) -> Point2 {
    let Some(&b) = pts.last() else { return Point2::new(1.0, 0.0) };
    pts.iter().rev().skip(1).find(|&&q| dist(q, b) > 1e-4).map_or(Point2::new(1.0, 0.0), |&q| unit(q, b))
}
