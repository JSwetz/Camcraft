//! 2D machining generators: region + parameters in, CL path out.
//!
//! Provenance: serpentine facing, offset contouring, zigzag and waterline
//! pocketing are textbook CAM strategies as described in machine-tool
//! handbooks and published CAM literature; every number (parameter model,
//! entry/exit overruns, snap rules) is ours. Pure module: no flecs, no
//! world access - the command layer resolves geometry into regions, this
//! module only cuts them.
//!
//! Prime Directive: parameters are validated before any geometry is
//! touched; impossible shapes return `GenError` (or a clean empty result
//! where the plan says collapse means empty), never a partially linked
//! path. Every returned path is chained, finite, and posts real motion.

use std::f64::consts::PI;

use thiserror::Error;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{GeoError, Loop, Point2, Region};
use crate::geometry::Point3;

#[derive(Error, Debug, PartialEq)]
pub enum GenError {
    #[error("invalid machining parameters: {0}")]
    Invalid(String),
    #[error("nothing to machine: {0}")]
    Empty(String),
    #[error(transparent)]
    Geo(#[from] GeoError),
}

/// Facing: clear the top plane of a footprint in straight XY passes.
///
/// The path lives in world coordinates (regions arrive in the frame the
/// stock is drawn in). Rows band the footprint's Y extent at `stepover`,
/// centered a tool radius in from each edge so the full width is covered;
/// X runs overrun the bbox by a radius for clean entry and exit. Passes
/// snake back and forth at cut depth - rapids appear only at the two
/// ends, matching how facers actually run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceParams {
    pub tool_diameter: f64,
    /// Pass spacing in Y; the overlap 2r - stepover is what leaves the
    /// scallop height, which facing simply accepts.
    pub stepover: f64,
    /// Rapid plane above the material.
    pub clearance_z: f64,
    /// The finished surface height.
    pub cut_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling: the first pass runs -X to +X. Conventional starts
    /// the other way.
    pub climb: bool,
}

impl FaceParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stepover,
            self.clearance_z,
            self.cut_z,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        Ok(())
    }
}

/// Serpentine face pass over `region` at `p.cut_z`. A stepover wider than
/// the tool leaves an uncut band between rows - visible in the path, never
/// silently smoothed over, and never a crash: facing simply cuts where it
/// passes.
pub fn face(region: &Region, p: &FaceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let (lo, hi) = match region.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty footprint".into())),
    };
    let r = p.tool_diameter * 0.5;
    let ys = band(lo.y, hi.y, r, p.stepover);
    let (x_in, x_out) = (lo.x - r, hi.x + r);

    let start_x = if p.climb { x_in } else { x_out };
    let mut b = PathBuilder::new(Point3::new(start_x, ys[0], p.clearance_z));
    b.push(
        MoveKind::Plunge,
        Point3::new(start_x, ys[0], p.cut_z),
        Some(p.plunge_feed),
    );
    for (i, y) in ys.iter().enumerate() {
        // Serpentine: even rows run the climb direction, odd rows return.
        let forward = p.climb == (i % 2 == 0);
        b.push(
            MoveKind::Feed,
            Point3::new(if forward { x_out } else { x_in }, *y, p.cut_z),
            Some(p.feed),
        );
        if i + 1 < ys.len() {
            let at = b.at();
            b.push(
                MoveKind::Feed,
                Point3::new(at.x, ys[i + 1], p.cut_z),
                Some(p.feed),
            );
        }
    }
    let at = b.at();
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Row centers banding [a + r, b - r]: start at the low edge's coverage
/// center, stride by `step`, and always end exactly at the high edge's
/// coverage center (a stride that would overshoot is replaced by that
/// snap; a band thinner than one tool diameter gets the single center
/// row). Never empty, never unsorted.
fn band(a: f64, b: f64, r: f64, step: f64) -> Vec<f64> {
    let (first, last) = (a + r, b - r);
    let span = last - first;
    if span <= 0.0 {
        return vec![(a + b) * 0.5];
    }
    let tol = span * 1e-9;
    let mut ys = vec![first];
    let mut y = first;
    while y + step < last - tol {
        y += step;
        ys.push(y);
    }
    if last - *ys.last().unwrap_or(&first) > tol {
        ys.push(last);
    }
    ys
}

/// Entry/exit motion appended to each contour subpath. Tangential leads
/// keep the tool from dwelling (and gouging) at a plunge point directly on
/// the finished wall; linear leads simply overrun the corner in air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lead {
    /// Plunge straight onto the path.
    None,
    /// Start `length` behind the path along its tangent, run in, and
    /// overrun the same way on exit.
    Linear { length: f64 },
    /// Quarter arc of `radius`, tangent to the path at entry and exit,
    /// bulging to the air side. Tessellated to `tolerance` chord error.
    Arc { radius: f64 },
}

/// 2D contouring: trim a material region's boundary at successive depths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourParams {
    pub tool_diameter: f64,
    /// Thickness of material deliberately left on the finished faces for
    /// a later finishing pass.
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// Plane tab hops travel in, between the cut and `clearance_z`.
    pub retract_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb travels each offset loop with the region's canonical
    /// orientation (material on the left); conventional reverses it.
    pub climb: bool,
    pub lead: Lead,
    /// Chord tolerance for lead-arc tessellation.
    pub tolerance: f64,
    /// Tabs: `tab_count` bridges of `tab_width`, holding the part until
    /// the cut drops below `bottom_z + tab_height`.
    pub tab_count: u32,
    pub tab_width: f64,
    pub tab_height: f64,
}

impl ContourParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.retract_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.tolerance,
            self.tab_width,
            self.tab_height,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.tolerance <= 0.0 {
            return Err(GenError::Invalid("tolerance must be positive".into()));
        }
        match self.lead {
            Lead::None => {}
            Lead::Linear { length } if length >= 0.0 && length.is_finite() => {}
            Lead::Arc { radius } if radius > 0.0 => {}
            _ => return Err(GenError::Invalid("bad lead-in parameters".into())),
        }
        if self.tab_count > 0 && (self.tab_width <= 0.0 || self.tab_height < 0.0) {
            return Err(GenError::Invalid("tabs need positive width".into()));
        }
        Ok(())
    }

    /// Cut levels from `top_z` down to `bottom_z`, always ending exactly
    /// at the bottom (same snap rule as facing rows).
    fn levels(&self) -> Vec<f64> {
        let span = self.top_z - self.bottom_z;
        if span <= 0.0 {
            return vec![self.bottom_z];
        }
        let tol = span * 1e-9;
        let mut zs = Vec::new();
        let mut z = self.top_z;
        while z > self.bottom_z + tol {
            zs.push(z);
            z -= self.stepdown;
        }
        zs.push(self.bottom_z);
        zs
    }
}

/// Offset-contour `region` (a material cross-section): every level walks
/// the cutter-center loops of the region grown by `r + leave`, so the
/// tool edge grazes the boundary with `leave = 0` and stops a finishing
/// allowance short of it otherwise. The plan phrases this as
/// `- (r - leave)`; that is the same curve written from the void side -
/// here the region is material, so both outer walls and island/hole
/// walls come out of one positive offset. Loop order is deterministic
/// (largest absolute area first, canonically oriented, and traversed
/// forward for climb, reversed for conventional).
///
/// Between subpaths (tab hops) the tool retracts to `retract_z`, rapids
/// across the bridge in air, and plunges again; between loops and levels
/// it lifts to `clearance_z`. Rapids inside the footprint below the safe
/// plane are the verifier's business (step 7), flagged, never hidden.
pub fn contour(region: &Region, p: &ContourParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty footprint".into()));
    }
    let r = p.tool_diameter * 0.5;
    let center = region.offset(r + p.stock_to_leave)?;
    if center.is_empty() {
        return Err(GenError::Empty("offset boundary collapsed".into()));
    }
    let zs = p.levels();
    let mut loops: Vec<&Loop> = center.loops().iter().collect();
    loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let mut first_entry = true;
    for &z in &zs {
        let tabbed = p.tab_count > 0 && z < p.bottom_z + p.tab_height;
        for l in &loops {
            // Canonical orientation is material-on-the-left travel;
            // conventional climbing reverses it. Air is the other side.
            let pts: Vec<Point2> = if p.climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            let ring = Ring::new(pts);
            let keeps = if tabbed {
                ring.tab_keeps(p.tab_count as usize, p.tab_width)
            } else {
                vec![(0.0, ring.len)]
            };
            for (k, &(a, e)) in keeps.iter().enumerate() {
                let poly = ring.arc_between(a, e);
                if poly.len() < 2 {
                    continue;
                }
                let t0 = unit(poly[1].x - poly[0].x, poly[1].y - poly[0].y);
                let (entry, lead_in) = lead_in(poly[0], t0, air_side(t0, p.climb), p);
                if k == 0 {
                    if !first_entry {
                        let cur = b.at();
                        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
                    }
                    let cur = b.at();
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, cur.z), None);
                } else {
                    let cur = b.at();
                    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, p.retract_z), Some(p.feed));
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.retract_z), None);
                }
                b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(p.plunge_feed));
                first_entry = false;
                for q in lead_in {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(p.feed));
                }
                for q in &poly[1..] {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(p.feed));
                }
                let n = poly.len();
                let t1 = unit(poly[n - 1].x - poly[n - 2].x, poly[n - 1].y - poly[n - 2].y);
                for q in lead_out(poly[n - 1], t1, air_side(t1, p.climb), p) {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(p.feed));
                }
            }
        }
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Which side of travel is air: canonical (climb) travel keeps material on
/// the left, so air is right; reversed travel flips both.
fn air_side(t: Point2, climb: bool) -> Point2 {
    if climb {
        Point2::new(t.y, -t.x)
    } else {
        Point2::new(-t.y, t.x)
    }
}

fn unit(dx: f64, dy: f64) -> Point2 {
    let m = (dx * dx + dy * dy).sqrt();
    if m == 0.0 || !m.is_finite() {
        Point2::new(1.0, 0.0)
    } else {
        Point2::new(dx / m, dy / m)
    }
}

/// (plunge point, feed points from it onto the path, ending exactly at
/// `s`).
fn lead_in(s: Point2, t: Point2, air: Point2, p: &ContourParams) -> (Point2, Vec<Point2>) {
    match p.lead {
        Lead::None => (s, Vec::new()),
        Lead::Linear { length } => (
            Point2::new(s.x - t.x * length, s.y - t.y * length),
            vec![s],
        ),
        Lead::Arc { radius } => {
            let o = Point2::new(s.x + air.x * radius, s.y + air.y * radius);
            let e = Point2::new(
                s.x + (air.x - t.x) * radius,
                s.y + (air.y - t.y) * radius,
            );
            let v = arc_tess(o, radius, e, s, p.tolerance);
            (e, v)
        }
    }
}

/// Feed points carrying the tool off the path end (not including it).
fn lead_out(e: Point2, t: Point2, air: Point2, p: &ContourParams) -> Vec<Point2> {
    match p.lead {
        Lead::None => Vec::new(),
        Lead::Linear { length } => vec![Point2::new(e.x + t.x * length, e.y + t.y * length)],
        Lead::Arc { radius } => {
            let o = Point2::new(e.x + air.x * radius, e.y + air.y * radius);
            let q = Point2::new(
                e.x + (air.x + t.x) * radius,
                e.y + (air.y + t.y) * radius,
            );
            arc_tess(o, radius, e, q, p.tolerance)
        }
    }
}

/// Arc from `a` to `b` on the circle centered at `o`, the short way, with
/// chord error under `tol`; the final point is `b` exactly. Empty for a
/// zero sweep (tessellating from a0==a1 would emit a bogus move).
fn arc_tess(o: Point2, r: f64, a: Point2, b: Point2, tol: f64) -> Vec<Point2> {
    let a0 = (a.y - o.y).atan2(a.x - o.x);
    let a1 = (b.y - o.y).atan2(b.x - o.x);
    let mut d = a1 - a0;
    while d <= -PI {
        d += 2.0 * PI;
    }
    while d > PI {
        d -= 2.0 * PI;
    }
    if d == 0.0 {
        return Vec::new();
    }
    // Sagitta r(1 - cos(step/2)) <= tol bounds the sweep per segment.
    let max_step = 2.0 * (1.0 - (tol / r).clamp(-1.0, 1.0)).clamp(-1.0, 1.0).acos();
    let n = if max_step > 0.0 {
        (d.abs() / max_step).ceil().max(1.0) as usize
    } else {
        1
    };
    let mut v = Vec::with_capacity(n);
    for j in 1..n {
        let t = a0 + d * (j as f64 / n as f64);
        v.push(Point2::new(o.x + r * t.cos(), o.y + r * t.sin()));
    }
    v.push(b);
    v
}

/// A closed polyline parametrized by arc length, for tab windows.
struct Ring {
    pts: Vec<Point2>,
    /// Cumulative length at each vertex; the closing edge runs from
    /// `cum[n-1]` to `len`.
    cum: Vec<f64>,
    len: f64,
}

impl Ring {
    fn new(pts: Vec<Point2>) -> Ring {
        let mut p: Vec<Point2> = Vec::with_capacity(pts.len());
        for q in pts {
            if p.last().is_none_or(|r| *r != q) {
                p.push(q);
            }
        }
        if p.len() > 1 && p.first() == p.last() {
            p.pop();
        }
        let n = p.len();
        let mut cum = vec![0.0; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + dist(p[i - 1], p[i]);
        }
        let len = if n > 1 {
            cum[n - 1] + dist(p[n - 1], p[0])
        } else {
            0.0
        };
        Ring { pts: p, cum, len }
    }

    fn at(&self, s: f64) -> Point2 {
        let n = self.pts.len();
        if n == 0 || self.len <= 0.0 {
            return self.pts.first().copied().unwrap_or(Point2::new(0.0, 0.0));
        }
        let sm = s.rem_euclid(self.len);
        let mut i = n - 1;
        while i > 0 && self.cum[i] > sm {
            i -= 1;
        }
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let seg = if i + 1 < n { self.cum[i + 1] - self.cum[i] } else { self.len - self.cum[i] };
        let t = if seg > 0.0 { ((sm - self.cum[i]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    /// Vertices of the ring sampled from arc position `a` to `b`
    /// (b - a <= len), endpoints interpolated. Vertex i recurs at
    /// `cum[i] + m*len` for every lap m; the walk emits them ordered by
    /// absolute position, which matters for intervals that wrap the
    /// parametrization seam.
    fn arc_between(&self, a: f64, b: f64) -> Vec<Point2> {
        let mut out = vec![self.at(a)];
        let n = self.pts.len();
        let l = self.len;
        if n == 0 || l <= 0.0 || b <= a {
            return out;
        }
        let eps = l * 1e-12;
        // b - a <= len, so each vertex recurs in at most one lap here.
        let m_lo = (a / l).floor() as i64 - 1;
        let m_hi = (b / l).floor() as i64 + 1;
        for m in m_lo..=m_hi {
            for i in 0..n {
                let pos = self.cum[i] + m as f64 * l;
                if pos > a + eps
                    && pos < b - eps
                    && out.last().is_none_or(|q| *q != self.pts[i])
                {
                    out.push(self.pts[i]);
                }
            }
        }
        let e = self.at(b);
        if out.last().is_none_or(|q| *q != e) {
            out.push(e);
        }
        out
    }

    /// Arc intervals to actually cut: the circle minus `count` tab
    /// windows of `width`, centered at equal spacing. Windows wider than
    /// their spacing merge; a fully tabbed ring keeps nothing (the loop
    /// contributes no motion at that level).
    fn tab_keeps(&self, count: usize, width: f64) -> Vec<(f64, f64)> {
        let l = self.len;
        if count == 0 || l <= 0.0 {
            return vec![(0.0, l.max(0.0))];
        }
        let spacing = l / count as f64;
        let hw = width * 0.5;
        if hw >= spacing * 0.5 {
            return Vec::new();
        }
        let eps = l * 1e-12;
        let mut ws: Vec<(f64, f64)> = Vec::new();
        for i in 0..count {
            let c = (i as f64 + 0.5) * spacing;
            let (mut w0, mut w1) = (c - hw, c + hw);
            while w0 < 0.0 {
                w0 += l;
                w1 += l;
            }
            while w0 >= l {
                w0 -= l;
                w1 -= l;
            }
            if w1 <= l {
                ws.push((w0, w1));
            } else {
                ws.push((w0, l));
                ws.push((0.0, w1 - l));
            }
        }
        ws.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut merged: Vec<(f64, f64)> = Vec::with_capacity(ws.len());
        for w in ws {
            match merged.last_mut() {
                Some(last) if w.0 <= last.1 + eps => last.1 = last.1.max(w.1),
                _ => merged.push(w),
            }
        }
        let mut keeps: Vec<(f64, f64)> = Vec::with_capacity(merged.len() + 1);
        let mut cur = 0.0;
        for w in &merged {
            if w.0 > cur + eps {
                keeps.push((cur, w.0));
            }
            cur = cur.max(w.1);
        }
        if cur < l - eps {
            keeps.push((cur, l));
        }
        // Keep intervals at both ends of the parametrization are one
        // arc crossing the seam.
        if keeps.len() > 1 && keeps[0].0 <= eps && keeps[keeps.len() - 1].1 >= l - eps {
            let last = keeps.pop().unwrap_or((0.0, 0.0));
            keeps[0] = (last.0, keeps[0].1 + l);
        }
        keeps
    }
}

fn dist(a: Point2, b: Point2) -> f64 {
    ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_snaps_to_both_coverage_centers() {
        assert_eq!(band(0.0, 20.0, 5.0, 5.0), vec![5.0, 10.0, 15.0]);
        assert_eq!(band(0.0, 20.0, 5.0, 12.0), vec![5.0, 15.0]);
        assert_eq!(band(0.0, 20.0, 5.0, 7.0), vec![5.0, 12.0, 15.0]);
        // Thinner than one tool: the single center row, still covering.
        assert_eq!(band(0.0, 6.0, 5.0, 3.0), vec![3.0]);
    }

    fn ring(n: usize) -> Ring {
        // Unit circle-ish square: 4 vertices, perimeter 4 (0,0)-(1,0)-(1,1)-(0,1).
        let _ = n;
        Ring::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ])
    }

    #[test]
    fn ring_walks_by_arc_length() {
        let r = ring(0);
        assert_eq!(r.len, 4.0);
        assert_eq!(r.at(0.5), Point2::new(0.5, 0.0));
        assert_eq!(r.at(1.5), Point2::new(1.0, 0.5));
        assert_eq!(r.at(4.5), r.at(0.5), "wraps");
        let closed = r.arc_between(0.0, r.len);
        assert_eq!(&closed[..4], &r.pts[..], "full ring = all vertices");
        assert_eq!(closed[4], closed[0], "closes exactly on the start");
        assert_eq!(closed.len(), 5);
        let mid = r.arc_between(0.5, 2.5);
        assert_eq!(
            mid,
            vec![
                Point2::new(0.5, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.5, 1.0),
            ]
        );
    }

    #[test]
    fn tab_windows_leave_even_keeps_and_merge_across_the_seam() {
        let r = ring(0);
        // Two tabs (width 1) centered at s = 1 and 3: the keeps between
        // and around them; the seam keeps merge into one crossing arc.
        assert_eq!(r.tab_keeps(2, 1.0), vec![(3.5, 4.5), (1.5, 2.5)]);
        let k = r.tab_keeps(2, 0.5);
        assert_eq!(k, vec![(3.25, 4.75), (1.25, 2.75)]);
        // Tabs touching end to end leave nothing to cut.
        assert_eq!(r.tab_keeps(4, 1.0 + 1e-9), Vec::new());
    }
}
