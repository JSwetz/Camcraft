//! The star cell of the morph spiral (M11.4 C round 2): an area every
//! point of which the entry sees along a straight ray, described by its
//! radial function about the seed - the classical polar description of
//! a star-shaped polygon (the kernel test: a polygon is star-shaped
//! from `s` iff the polar angle about `s` advances monotonically along
//! its boundary; Preparata & Shamos, *Computational Geometry*, 1985,
//! ch. 2). The rows are the area opened by a ladder of radii (erosion
//! then dilation: every convex corner rounded at that radius, Serra's
//! morphological opening), so the spiral can morph from round rings to
//! the walls' own corners gradually. Ours, no vendor material.

use crate::geo2d::{Point2, Region};

use crate::machining2d::GenError;

/// Angular samples of every radial row.
pub(super) const ANGLES: usize = 4096;

/// A star cell: radial rows of its openings, by opening radius.
#[derive(Clone, Debug)]
pub(super) struct Star {
    /// The pole the rows' rays leave from.
    pub(super) seed: Point2,
    /// (opening radius, radial distance at angle `2 pi j / ANGLES`),
    /// ascending radius; row 0 is the boundary the spiral ends on.
    rows: Vec<(f64, Vec<f64>)>,
}

/// The radial function of one closed loop about `s`, or `None` when the
/// loop is not star-shaped from `s` (its polar angle stalls or turns
/// back somewhere) or `s` is not inside it.
pub(super) fn radial(loop_pts: &[Point2], s: Point2) -> Option<Vec<f64>> {
    let n = loop_pts.len();
    if n < 3 {
        return None;
    }
    // Orientation: CCW about s (positive cross on every edge).
    let cross = |a: Point2, b: Point2| (a.x - s.x) * (b.y - s.y) - (a.y - s.y) * (b.x - s.x);
    let total: f64 = (0..n).map(|i| cross(loop_pts[i], loop_pts[(i + 1) % n])).sum();
    let pts: Vec<Point2> = if total >= 0.0 { loop_pts.to_vec() } else { loop_pts.iter().rev().copied().collect() };
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = (b.x - a.x).hypot(b.y - a.y);
        // A ray must cross every edge forward: a grazing or backward
        // edge is a shadow, the cell is not star-shaped from here.
        if cross(a, b) <= 1e-9 * len.max(1e-9) {
            return None;
        }
    }
    let ang = |p: Point2| (p.y - s.y).atan2(p.x - s.x).rem_euclid(std::f64::consts::TAU);
    // Start at the vertex with the least angle; unwrap ascending.
    let k0 = (0..n).min_by(|&i, &j| ang(pts[i]).total_cmp(&ang(pts[j])))?;
    let mut a: Vec<(f64, Point2)> = Vec::with_capacity(n + 1);
    let mut prev = ang(pts[k0]);
    let base = prev;
    a.push((prev, pts[k0]));
    for m in 1..=n {
        let p = pts[(k0 + m) % n];
        let mut t = ang(p);
        while t < prev {
            t += std::f64::consts::TAU;
        }
        a.push((t, p));
        prev = t;
    }
    if (prev - base - std::f64::consts::TAU).abs() > 1e-6 {
        return None;
    }
    let mut out = vec![0.0; ANGLES];
    let mut e = 0usize;
    // Walk the grid from the first angle at or past `base`, so the
    // angles rise monotonically with the edge pointer.
    let j0 = ((base / std::f64::consts::TAU * ANGLES as f64).ceil() as usize) % ANGLES;
    for m in 0..ANGLES {
        let j = (j0 + m) % ANGLES;
        let mut th = std::f64::consts::TAU * j as f64 / ANGLES as f64;
        if th + 1e-9 < base {
            th += std::f64::consts::TAU;
        }
        let slot = &mut out[j];
        while e + 2 < a.len() && a[e + 1].0 < th {
            e += 1;
        }
        let (pa, pb) = (a[e].1, a[e + 1].1);
        let d = Point2::new(th.cos(), th.sin());
        let (ex, ey) = (pb.x - pa.x, pb.y - pa.y);
        let den = d.x * ey - d.y * ex;
        if den.abs() < 1e-15 {
            return None;
        }
        let r = ((pa.x - s.x) * ey - (pa.y - s.y) * ex) / den;
        if !(r.is_finite() && r > 0.0) {
            return None;
        }
        *slot = r;
    }
    Some(out)
}

/// Is the disc (`c`, `rad`) inside the kernel of the closed loop `pts`
/// (on the inner side of every edge's line by at least `rad`)? Every
/// segment from that disc to the loop then stays inside the loop.
fn disc_in_kernel(pts: &[Point2], c: Point2, rad: f64) -> bool {
    let n = pts.len();
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a.x * b.y - a.y * b.x
        })
        .sum();
    let sgn = if area >= 0.0 { 1.0 } else { -1.0 };
    (0..n).all(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = (b.x - a.x).hypot(b.y - a.y);
        len <= 1e-12 || sgn * ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) / len >= rad
    })
}

impl Star {
    /// The star cell of `center` (one loop, no holes) about the pole
    /// `pole`: rows for the openings by `radii` (ascending) while each
    /// stays one loop that is star-shaped from the pole and holds the
    /// entry orbit (`orbit`: center, radius) in its kernel - so every
    /// ring blended between the orbit and the row stays inside the
    /// area; `None` when even the first row fails.
    pub(super) fn new(center: &Region, pole: Point2, radii: &[f64], orbit: (Point2, f64)) -> Result<Option<Star>, GenError> {
        if center.loops().len() != 1 {
            return Ok(None);
        }
        let mut rows = Vec::new();
        for &rho in radii {
            let opened = center.offset(-rho)?.offset(rho)?;
            let [l] = opened.loops() else { break };
            if !(disc_in_kernel(l.points(), orbit.0, orbit.1 + 1e-3) && disc_in_kernel(l.points(), pole, 1e-3)) {
                break;
            }
            let Some(row) = radial(l.points(), pole) else { break };
            rows.push((rho, row));
        }
        Ok((!rows.is_empty()).then_some(Star { seed: pole, rows }))
    }

    /// The radius of the opening the last row stands for.
    pub(super) fn max_rho(&self) -> f64 {
        self.rows.last().map_or(0.0, |r| r.0)
    }

    /// The radius the spiral ends at (row 0's opening).
    pub(super) fn min_rho(&self) -> f64 {
        self.rows.first().map_or(0.0, |r| r.0)
    }

    /// Shortest ray of the final row.
    pub(super) fn shortest(&self) -> f64 {
        self.rows.first().map_or(0.0, |r| r.1.iter().copied().fold(f64::INFINITY, f64::min))
    }

    /// Longest ray of the final row.
    pub(super) fn reach(&self) -> f64 {
        self.rows.first().map_or(0.0, |r| r.1.iter().copied().fold(0.0, f64::max))
    }

    /// Radial distance at angle `th` of the opening by `rho` (rows
    /// interpolated linearly in the radius and in the angle).
    pub(super) fn at(&self, th: f64, rho: f64) -> f64 {
        let n = self.rows.len();
        if n == 0 {
            return 0.0;
        }
        let k = self.rows.partition_point(|r| r.0 <= rho);
        let (i, j, w) = if k == 0 {
            (0, 0, 0.0)
        } else if k >= n {
            (n - 1, n - 1, 0.0)
        } else {
            let (r0, r1) = (self.rows[k - 1].0, self.rows[k].0);
            (k - 1, k, ((rho - r0) / (r1 - r0).max(1e-12)).clamp(0.0, 1.0))
        };
        let f = th.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU * ANGLES as f64;
        let j0 = (f.floor() as usize) % ANGLES;
        let j1 = (j0 + 1) % ANGLES;
        let u = f - f.floor();
        let row = |m: usize| self.rows[m].1[j0] * (1.0 - u) + self.rows[m].1[j1] * u;
        row(i) * (1.0 - w) + row(j) * w
    }
}

/// The star admission (round 4): the area is one morph spiral's only
/// where it is [`STAR_WIDTH_PER_R`] tool radii wide everywhere - the
/// erosion by that half-width stays one piece, and its opening comes
/// back to within the half-width of every vertex (only corner wedges of
/// 60 degrees and wider are lost). A narrower channel (a 22 mm one with
/// a D10, round as its rays are) overloads a morph front that sweeps it
/// sideways; the growth rings (`grow`) take it. Width by morphological
/// erosion and opening, Serra (1982); the threshold is ours.
pub(super) fn wide_enough(center: &Region, half: f64) -> Result<bool, GenError> {
    let eroded = center.offset(-half)?;
    let [l] = eroded.loops() else { return Ok(false) };
    if l.area() <= 0.0 {
        return Ok(false);
    }
    let opened = eroded.offset(half)?;
    let far = |v: Point2| !opened.contains(v) && super::loops::boundary_dist(&opened, v) > half;
    Ok(!center.loops().iter().flat_map(|l| l.points().iter()).any(|&v| far(v)))
}

/// The star cell's width law: [`wide_enough`] at half this many radii
/// (1.4 D wide).
pub(super) const STAR_WIDTH_PER_R: f64 = 2.8;

/// The star cell the morph spiral runs over, when the whole center area
/// is one, seen from the seed, and wide enough ([`wide_enough`]);
/// `None` sends the plan to the growth rings or the engagement fronts.
pub(super) fn star_cell(center: &Region, s: &super::AdaptiveSpec) -> Result<Option<Star>, GenError> {
    let rho_c = corner_radius(center, s.r);
    let radii: Vec<f64> = (0..12).map(|k| rho_c * 1.6f64.powi(k)).collect();
    let orbit = (s.seed, orbit_radius(s));
    if !wide_enough(center, 0.5 * STAR_WIDTH_PER_R * s.r)? {
        return Ok(None);
    }
    // The rays leave from the seed (`kernel::preferred_seed` put it
    // where they are most alike, when the entry let it choose).
    let cell = Star::new(center, s.seed, &radii, orbit)?;
    Ok(cell.filter(|c| (c.min_rho() - rho_c).abs() < 1e-9))
}

/// The radius the morph spiral's last ring rounds the walls' corners
/// at: [`super::CORNER_PER_R`] of the tool radius, or under half the area's
/// inscribed radius where the area is narrower (a channel's opening must
/// not vanish).
pub(crate) fn corner_radius(center: &Region, r: f64) -> f64 {
    let rho = r * super::CORNER_PER_R;
    super::widest_point(center).map_or(rho, |w| rho.min(0.45 * w.1))
}

/// The radius of the entry's orbit: the helix's, or a hair out of a
/// tool-size bore.
pub(super) fn orbit_radius(s: &super::AdaptiveSpec) -> f64 {
    if s.helix_r > 1e-6 { s.helix_r } else { (0.05 * s.r).min(s.bore.max(1e-3)) }
}
