//! The plan's own crossing check (M11.4 C round 2): the verifier's
//! crossing law (`verify`, law (a): two feeds proper-crossing in XY at
//! one depth) asked of the planned walks before they are posted, so a
//! plan that would cross itself is refused here, by name, not posted
//! for the verifier to flag. Proper crossing by the orientation
//! predicate (O'Rourke, *Computational Geometry in C*, the textbook
//! segment test), segments bucketed on a uniform grid; ours.

use crate::geo2d::Point2;

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Do segments `ab` and `cd` cross properly (each strictly straddles
/// the other's line)?
fn proper(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let (d1, d2) = (orient(a, b, c), orient(a, b, d));
    let (d3, d4) = (orient(c, d, a), orient(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// The first place where two segments of the walks cross properly
/// (consecutive segments of one walk share a vertex and never count),
/// or `None`.
pub(crate) fn first_crossing(walks: &[Vec<Point2>]) -> Option<Point2> {
    let segs: Vec<(usize, usize, Point2, Point2)> = walks
        .iter()
        .enumerate()
        .flat_map(|(w, pts)| pts.windows(2).enumerate().map(move |(i, s)| (w, i, s[0], s[1])))
        .filter(|s| (s.3.x - s.2.x).hypot(s.3.y - s.2.y) > 1e-12)
        .collect();
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for s in &segs {
        for p in [s.2, s.3] {
            lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
    }
    if segs.len() < 2 || !(lo.x.is_finite() && hi.x.is_finite()) {
        return None;
    }
    let n = ((segs.len() as f64).sqrt().ceil() as usize).clamp(1, 512);
    let (cw, ch) = (((hi.x - lo.x) / n as f64).max(1e-9), ((hi.y - lo.y) / n as f64).max(1e-9));
    let cell = |x: f64, y: f64| {
        let ix = (((x - lo.x) / cw) as usize).min(n - 1);
        let iy = (((y - lo.y) / ch) as usize).min(n - 1);
        (ix, iy)
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); n * n];
    for (k, s) in segs.iter().enumerate() {
        let (a, b) = (cell(s.2.x.min(s.3.x), s.2.y.min(s.3.y)), cell(s.2.x.max(s.3.x), s.2.y.max(s.3.y)));
        for iy in a.1..=b.1 {
            for ix in a.0..=b.0 {
                grid[ix + iy * n].push(k);
            }
        }
    }
    for bucket in &grid {
        for (u, &i) in bucket.iter().enumerate() {
            for &j in &bucket[u + 1..] {
                let (s, t) = (segs[i], segs[j]);
                if s.0 == t.0 && s.1.abs_diff(t.1) <= 1 {
                    continue;
                }
                if proper(s.2, s.3, t.2, t.3) {
                    return Some(s.3);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_eight_crosses_and_a_spiral_does_not() {
        let eight = vec![Point2::new(0.0, 0.0), Point2::new(2.0, 2.0), Point2::new(2.0, 0.0), Point2::new(0.0, 2.0)];
        assert!(first_crossing(&[eight]).is_some());
        let spiral: Vec<Point2> = (0..2000)
            .map(|k| {
                let a = k as f64 * 0.02;
                let r = 1.0 + 0.1 * a;
                Point2::new(r * a.cos(), r * a.sin())
            })
            .collect();
        assert!(first_crossing(&[spiral]).is_none());
    }
}
