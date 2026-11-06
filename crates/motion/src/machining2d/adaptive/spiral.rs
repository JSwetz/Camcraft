//! The core spiral (M11.4 C): while the engagement fronts are closed
//! loops around the entry bore, consecutive loops are not cut as rings
//! joined by a step - each revolution morphs from one loop to the next,
//! so the radial step grows continuously from zero at the bore to the
//! loop schedule's width and no link ever cuts across. Provenance: the
//! spiral-from-offsets morph of Held & Spielberger (2009, "A smooth
//! spiral tool path for high speed machining of 2D pocket-shaped
//! objects", Computer-Aided Design 41(7)), here between the
//! engagement-front loops instead of medial-axis offsets, with the
//! nearest-point correspondence between neighbouring loops (our
//! simplification); the bore-to-first-loop turn is Bieterman &
//! Sandstrom's (2003, "A curvilinear tool-path method for pocket
//! machining", J. Manuf. Sci. Eng. 125(4)) point that a spiral, not a
//! ring stack, is what keeps the load continuous out of the entry.

use crate::geo2d::Point2;

use super::front::{dist, rotate_to};

/// Nearest point on the polyline `b` to `p`.
fn project(b: &[Point2], p: Point2) -> Point2 {
    let mut best = (f64::INFINITY, b.first().copied().unwrap_or(p));
    for w in b.windows(2) {
        let (a, c) = (w[0], w[1]);
        let (dx, dy) = (c.x - a.x, c.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let q = Point2::new(a.x + dx * t, a.y + dy * t);
        let d = dist(p, q);
        if d < best.0 {
            best = (d, q);
        }
    }
    best.1
}

/// One revolution of the spiral: it leaves ring `a` at its seam and
/// arrives on ring `b`'s seam (both closed, first point repeated last,
/// travelled the same way, `b` rotated to start nearest `a`'s start),
/// the blend weight running 0 -> 1 with `a`'s arc length. Ending on
/// `b[0]`, it chains straight into the revolution after it.
pub(super) fn revolution(a: &[Point2], b: &[Point2]) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::new();
    let b = rotate_to(b, a.first().copied().unwrap_or_default());
    {
        let b = &b;
        let total: f64 = a.windows(2).map(|s| dist(s[0], s[1])).sum();
        if total <= 1e-9 {
            return b.clone();
        }
        let mut acc = 0.0;
        let n = a.len().saturating_sub(1);
        // The revolution ends ON the next ring's seam: the last morph
        // points within a step of it are dropped, so the join never
        // backs up over a point the projection put just past the seam.
        let seam = b.first().copied().unwrap_or(a[0]);
        let near = 0.75 * a.windows(2).map(|s| dist(s[0], s[1])).fold(0.0, f64::max);
        for j in 0..n {
            if j > 0 {
                acc += dist(a[j - 1], a[j]);
            }
            let t = acc / total;
            let q = project(b, a[j]);
            let p = Point2::new(a[j].x + (q.x - a[j].x) * t, a[j].y + (q.y - a[j].y) * t);
            if j > n / 2 && dist(p, seam) < near {
                break;
            }
            if out.last().is_none_or(|&l| dist(l, p) > 1e-6) {
                out.push(p);
            }
        }
        if let Some(&s) = b.first()
            && out.last().is_none_or(|&l| dist(l, s) > 1e-6)
        {
            out.push(s);
        }
    }
    out
}
