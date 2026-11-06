//! Islands cut open (M11.4 C round 3): an area with holes (the islands'
//! no-go discs) is made simply connected by a thin slit from each hole
//! to the next boundary, so ONE family of nested closed rings can grow
//! from the entry around every island - the rings wrap each island as a
//! C and close against the two faces of its slit, never around it. The
//! cut is the classical one that turns a multiply connected polygonal
//! domain into a simply connected one (the hole-to-boundary "bridge" of
//! polygon triangulation and of spiral pocketing around islands - Held,
//! *On the Computational Geometry of Pocket Machining*, LNCS 500, 1991,
//! ch. 6; Held & Spielberger 2009 for the spiral it serves), placed on
//! the far side of the island as seen from the entry, where the fronts
//! going round it either way meet (the island's cut locus). Ours.

use crate::geo2d::{Loop, Point2, Region};

use crate::machining2d::GenError;

/// Slit width (mm, center space): the two faces the rings close
/// against stay this far apart, so the last lap's passes down and up
/// the slit never touch.
pub(super) const SLIT_W: f64 = 0.02;

/// Slits are capped (never hang on a pathological area).
const MAX_SLITS: usize = 64;

/// Where the ray `o + t d` (t > 0) crosses the closed loop `pts`: every
/// crossing parameter, ascending.
fn ray_hits(pts: &[Point2], o: Point2, d: Point2) -> Vec<f64> {
    let n = pts.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (ex, ey) = (b.x - a.x, b.y - a.y);
        let den = d.x * ey - d.y * ex;
        if den.abs() < 1e-15 {
            continue;
        }
        let (wx, wy) = (a.x - o.x, a.y - o.y);
        let t = (wx * ey - wy * ex) / den;
        let u = (wx * d.y - wy * d.x) / den;
        if t > 1e-9 && (0.0..1.0).contains(&u) {
            out.push(t);
        }
    }
    out.sort_by(f64::total_cmp);
    out
}

fn centroid(pts: &[Point2]) -> Option<Point2> {
    let l = Loop::new(pts.to_vec()).ok()?;
    let r = Region::new(vec![l]).ok()?;
    super::kernel::centroid(&r)
}

/// `center` cut open along one slit per hole, as one loop; `None` when
/// a slit cannot be placed clear of the entry (`keep`: the disc about
/// `seed` no slit may touch) or the cut area is not one loop.
pub(super) fn open_islands(center: &Region, seed: Point2, keep: f64) -> Result<Option<Region>, GenError> {
    let mut dom = center.clone();
    for _ in 0..MAX_SLITS {
        let loops = dom.loops();
        let Some(hole) = loops.iter().find(|l| l.area() < 0.0) else {
            return Ok((dom.loops().len() == 1).then_some(dom));
        };
        let hp = hole.points();
        let Some(c) = centroid(hp) else { return Ok(None) };
        // Away from the entry, through the hole: the far side of the
        // hole, then the first boundary past it.
        let (dx, dy) = (c.x - seed.x, c.y - seed.y);
        let l = dx.hypot(dy);
        let d = if l > 1e-9 { Point2::new(dx / l, dy / l) } else { Point2::new(1.0, 0.0) };
        let Some(&t_far) = ray_hits(hp, c, d).last() else { return Ok(None) };
        let p = Point2::new(c.x + d.x * t_far, c.y + d.y * t_far);
        let t_hit = loops
            .iter()
            .filter(|o| !std::ptr::eq(*o, hole))
            .filter_map(|o| ray_hits(o.points(), p, d).first().copied())
            .fold(f64::INFINITY, f64::min);
        if !t_hit.is_finite() {
            return Ok(None);
        }
        // A hair into the hole and past the boundary it reaches, so the
        // cut is a true bridge, not a sliver left between.
        let a = Point2::new(p.x - d.x * 0.05, p.y - d.y * 0.05);
        let b = Point2::new(p.x + d.x * (t_hit + 0.05), p.y + d.y * (t_hit + 0.05));
        if seg_dist(seed, a, b) < keep {
            return Ok(None);
        }
        let (nx, ny) = (-d.y * 0.5 * SLIT_W, d.x * 0.5 * SLIT_W);
        let rect = Loop::new(vec![
            Point2::new(a.x + nx, a.y + ny),
            Point2::new(a.x - nx, a.y - ny),
            Point2::new(b.x - nx, b.y - ny),
            Point2::new(b.x + nx, b.y + ny),
        ])?;
        let before = dom.loops().len();
        dom = dom.difference(&Region::new(vec![rect])?)?;
        if dom.loops().len() >= before {
            return Ok(None);
        }
    }
    Ok(None)
}

/// Distance from `p` to the segment `a b`.
fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - dx * t).hypot(p.y - a.y - dy * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
        let Ok(l) = Loop::new(pts) else { panic!("rect") };
        l
    }

    #[test]
    fn an_island_is_cut_open_on_its_far_side() {
        let Ok(a) = Region::new(vec![rect(5.0, 5.0, 55.0, 35.0), rect(19.0, 9.0, 41.0, 31.0)]) else { panic!("area") };
        let seed = Point2::new(12.0, 20.0);
        let Ok(Some(d)) = open_islands(&a, seed, 3.0) else { panic!("no slit") };
        assert_eq!(d.loops().len(), 1);
        // The slit runs east of the island at the seed's height: both its
        // sides are out, the area beside it is in.
        assert!(!d.contains(Point2::new(48.0, 20.0)));
        assert!(d.contains(Point2::new(48.0, 20.05)) && d.contains(Point2::new(48.0, 19.95)));
        assert!(d.contains(seed) && (d.area() - (a.area() - 14.0 * SLIT_W)).abs() < 0.01);
    }

    #[test]
    fn an_area_without_islands_is_left_as_it_is() {
        let Ok(a) = Region::new(vec![rect(0.0, 0.0, 10.0, 4.0)]) else { panic!("area") };
        let Ok(Some(d)) = open_islands(&a, Point2::new(5.0, 2.0), 1.0) else { panic!("refused") };
        assert!((d.area() - 40.0).abs() < 1e-9);
    }
}
