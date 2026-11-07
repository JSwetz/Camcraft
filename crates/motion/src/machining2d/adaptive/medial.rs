//! The medial line of a channel-like center area (issue #5): the
//! skeleton a trochoidal clearing advances along. A thin area is offset
//! inward by most of its half-width (the grassfire picture of the
//! medial axis: Blum 1967, "A transformation for extracting new
//! descriptors of shape"; offsets as its level sets, Held 1991, "On the
//! Computational Geometry of Pocket Machining"): what is left is a thin
//! strip about the medial line (an open channel - its two sides,
//! averaged by arc length) or a thin annulus (a closed band - midway
//! between its two loops). Anything else is no single channel and is
//! refused by name. Ours, no vendor material.

use crate::geo2d::{Point2, Region};

use super::GenError;
use super::front::{densify, dist};

/// The inward offset, as a share of the half-width, that leaves the
/// thin strip about the medial line.
const SHRINK: f64 = 0.92;

/// An open line keeps the points at least this share of the half-width
/// from the walls.
const KEEP: f64 = 0.985;

/// The medial line, sampled about every [`SAMPLE`] mm.
const SAMPLE: f64 = 0.25;

/// A channel's medial line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Spine {
    /// An open channel: the line from one end to the other.
    Open(Vec<Point2>),
    /// A closed band: the line round it (first point not repeated).
    Closed(Vec<Point2>),
}

impl Spine {
    pub(crate) fn points(&self) -> &[Point2] {
        match self {
            Spine::Open(p) | Spine::Closed(p) => p,
        }
    }
}

/// The medial line of `center` and its half-width there (the widest
/// clearance), or why the area is no single channel.
pub(crate) fn spine(center: &Region) -> Result<Result<(Spine, f64), String>, GenError> {
    let Some((_, c)) = super::kernel::widest_point(center) else {
        return Ok(Err("the area has no room".into()));
    };
    let thin = center.offset(-(SHRINK * c))?;
    let loops = thin.loops();
    let line = match loops {
        [one] => open_spine(one.points()),
        [a, b] if (a.area() > 0.0) != (b.area() > 0.0) => {
            let (outer, hole) = if a.area() > 0.0 { (a, b) } else { (b, a) };
            closed_spine(outer.points(), hole.points())
        }
        _ => return Ok(Err(format!("the area is no single channel ({} loops about its medial line)", loops.len()))),
    };
    let line = match line {
        Some(Spine::Open(pts)) => trim(center, &pts, KEEP * c).map(Spine::Open),
        other => other,
    };
    Ok(line.map(|s| (s, c)).ok_or_else(|| "its medial line is degenerate".to_owned()))
}

/// An open line's ends run out toward the channel's corners (the medial
/// axis branches there, and the averaged sides bend): the longest run of
/// points whose clearance is at least `keep` is the line kept.
fn trim(center: &Region, pts: &[Point2], keep: f64) -> Option<Vec<Point2>> {
    let mut best: (usize, usize) = (0, 0);
    let mut from = None;
    for (i, &p) in pts.iter().enumerate() {
        let ok = super::loops::boundary_dist(center, p) >= keep && center.contains(p);
        match (ok, from) {
            (true, None) => from = Some(i),
            (false, Some(f)) => {
                if i - f > best.1 - best.0 {
                    best = (f, i);
                }
                from = None;
            }
            _ => {}
        }
    }
    if let Some(f) = from
        && pts.len() - f > best.1 - best.0
    {
        best = (f, pts.len());
    }
    let run = pts.get(best.0..best.1)?;
    (run.len() >= 2).then(|| run.to_vec())
}

/// Arc length along a polyline, per vertex.
pub(crate) fn lengths(pts: &[Point2]) -> Vec<f64> {
    let mut acc = 0.0;
    let mut out = Vec::with_capacity(pts.len());
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            acc += dist(pts[i - 1], *p);
        }
        out.push(acc);
    }
    out
}

/// The point at arc length `t` along `pts` (with its `lengths`).
pub(crate) fn at_len(pts: &[Point2], len: &[f64], t: f64) -> Point2 {
    let k = len.partition_point(|&l| l < t).clamp(1, pts.len().saturating_sub(1).max(1));
    let (Some(&a), Some(&b), Some(&la), Some(&lb)) = (pts.get(k - 1), pts.get(k), len.get(k - 1), len.get(k)) else {
        return pts.first().copied().unwrap_or_default();
    };
    let f = if lb - la > 1e-12 { ((t - la) / (lb - la)).clamp(0.0, 1.0) } else { 0.0 };
    Point2::new(a.x + f * (b.x - a.x), a.y + f * (b.y - a.y))
}

/// `pts` resampled at `n + 1` points evenly by arc length.
fn resample(pts: &[Point2], n: usize) -> Vec<Point2> {
    let len = lengths(pts);
    let total = len.last().copied().unwrap_or(0.0);
    (0..=n).map(|k| at_len(pts, &len, total * k as f64 / n.max(1) as f64)).collect()
}

/// An open strip's medial line: its two ends are the farthest pair of
/// its points (a double sweep), its two sides between them averaged
/// point for point by arc length.
fn open_spine(strip: &[Point2]) -> Option<Spine> {
    let far = |from: Point2| {
        strip.iter().enumerate().max_by(|a, b| dist(from, *a.1).total_cmp(&dist(from, *b.1))).map(|(i, _)| i)
    };
    let a = far(*strip.first()?)?;
    let b = far(strip[a])?;
    let n = strip.len();
    let walk = |from: usize, to: usize| -> Vec<Point2> {
        let mut out = vec![strip[from]];
        let mut i = from;
        while i != to {
            i = (i + 1) % n;
            out.push(strip[i]);
        }
        out
    };
    let one = walk(a, b);
    let mut two = walk(b, a);
    two.reverse();
    let len = lengths(&one).last().copied().unwrap_or(0.0).max(lengths(&two).last().copied().unwrap_or(0.0));
    let k = ((len / SAMPLE).ceil() as usize).clamp(2, 100_000);
    let (one, two) = (resample(&one, k), resample(&two, k));
    let mid: Vec<Point2> = one.iter().zip(&two).map(|(p, q)| Point2::new(0.5 * (p.x + q.x), 0.5 * (p.y + q.y))).collect();
    (mid.len() >= 2 && lengths(&mid).last().is_some_and(|&l| l > 1e-6)).then_some(Spine::Open(mid))
}

/// A thin annulus's medial line: each point of its outer loop moved
/// halfway to the nearest point of its hole.
fn closed_spine(outer: &[Point2], hole: &[Point2]) -> Option<Spine> {
    let outer = densify(outer, SAMPLE, true);
    let hole = densify(hole, SAMPLE, true);
    let mid: Vec<Point2> = outer
        .iter()
        .filter_map(|&p| {
            let q = hole.iter().copied().min_by(|a, b| dist(p, *a).total_cmp(&dist(p, *b)))?;
            Some(Point2::new(0.5 * (p.x + q.x), 0.5 * (p.y + q.y)))
        })
        .collect();
    (mid.len() >= 3).then_some(Spine::Closed(mid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Loop;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        let Ok(l) = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]) else {
            panic!("loop")
        };
        l
    }

    #[test]
    fn a_channel_medial_line_runs_down_its_middle() {
        // The 22 x 60 channel's center area for a D10: 50 x 12.
        let Ok(c) = Region::new(vec![rect(5.0, 5.0, 55.0, 17.0)]) else { panic!("region") };
        let Ok(Ok((Spine::Open(s), w))) = spine(&c) else { panic!("an open channel") };
        assert!((w - 6.0).abs() < 0.05, "half-width {w}");
        assert!(s.iter().all(|p| (p.y - 11.0).abs() < 0.6), "off the middle: {:?} .. {:?} ({})", &s[..3], &s[s.len() - 3..], s.len());
        let span = s.iter().map(|p| p.x).fold(f64::NAN, f64::max) - s.iter().map(|p| p.x).fold(f64::NAN, f64::min);
        assert!(span > 36.0, "runs the channel (its medial axis is 38 long): {span}");
    }

    #[test]
    fn a_band_medial_line_closes_round_it_and_an_island_pocket_is_refused() {
        let Ok(part) = Region::new(vec![rect(0.0, 0.0, 60.0, 50.0)]) else { panic!("part") };
        let (Ok(o), Ok(i)) = (part.offset(9.0), part.offset(5.0)) else { panic!("offsets") };
        let Ok(band) = o.difference(&i) else { panic!("band") };
        let Ok(Ok((Spine::Closed(s), w))) = spine(&band) else { panic!("a closed band") };
        assert!((w - 2.0).abs() < 0.05, "half-width {w}");
        assert!(s.iter().all(|&p| (super::super::loops::boundary_dist(&band, p) - 2.0).abs() < 0.3), "midway");
        let Ok(isl) = Region::new(vec![rect(0.0, 0.0, 60.0, 40.0), rect(24.0, 14.0, 36.0, 26.0)]) else { panic!("island") };
        let Ok(c) = isl.offset(-5.0) else { panic!("center") };
        assert!(matches!(spine(&c), Ok(Err(_))), "an island pocket is no single channel");
    }
}
