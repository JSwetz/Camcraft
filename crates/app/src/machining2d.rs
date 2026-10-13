//! 2D machining generators: region + parameters in, CL path out.
//!
//! Provenance: serpentine facing, offset contouring, zigzag and waterline
//! pocketing, stadium slotting rows and engraved trace paths are textbook
//! CAM strategies as described in machine-tool handbooks and published CAM
//! literature; every number (parameter model, entry/exit overruns, snap
//! rules) is ours. Pure module: no flecs, no
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
        cut_levels(self.top_z, self.bottom_z, self.stepdown)
    }
}

/// Cut levels from `top` down to `bottom`, snapped so the last level is
/// `bottom` exactly. Shared by depth-stepping operations.
fn cut_levels(top: f64, bottom: f64, stepdown: f64) -> Vec<f64> {
    let span = top - bottom;
    if span <= 0.0 {
        return vec![bottom];
    }
    let tol = span * 1e-9;
    let mut zs = Vec::new();
    let mut z = top;
    while z > bottom + tol {
        zs.push(z);
        z -= stepdown;
    }
    zs.push(bottom);
    zs
}

/// Run XY passes at each of the cut levels: every pass starts with a lift
/// (except the very first entry) and a rapid transfer along the clearance
/// plane, then plunges at the pass start and feeds along it. The builder
/// drops zero-length moves, so the first entry's lift collapses when the
/// path already starts at clearance above the entry point. Rapids
/// therefore always run at `clearance_z`, never inside the cut. Shared by
/// the depth-stepping area operations (pocket, slot, trace).
fn run_levels(
    b: &mut PathBuilder,
    passes: &[Vec<Point2>],
    zs: &[f64],
    clearance_z: f64,
    feed: f64,
    plunge_feed: f64,
) {
    let mut first = true;
    for &z in zs {
        for pass in passes {
            let Some((entry, rest)) = pass.split_first() else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            if !first {
                let cur = b.at();
                b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, clearance_z), None);
            }
            b.push(
                MoveKind::Rapid,
                Point3::new(entry.x, entry.y, clearance_z),
                None,
            );
            b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(plunge_feed));
            first = false;
            for q in rest {
                b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
            }
        }
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

/// True when `p` sits on some loop edge, within a nanometer - exact-zero
/// float tests cannot work here: a computed point on an edge (a chord's
/// midpoint) has round-off of a few ulps against the cross product, and
/// boundary points otherwise read as neither inside nor on. The tolerance
/// is far under the micron quantization of the geometry and far over the
/// cancellation error.
pub fn on_boundary(region: &Region, p: Point2) -> bool {
    const TOL: f64 = 1e-9;
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let d = Point2::new(b.x - a.x, b.y - a.y);
            let c = Point2::new(p.x - a.x, p.y - a.y);
            let cross = c.x * d.y - c.y * d.x;
            let dd = d.x * d.x + d.y * d.y;
            if dd == 0.0 || cross * cross > TOL * TOL * dd {
                continue;
            }
            let u = (c.x * d.x + c.y * d.y) / dd;
            if (-TOL..=1.0 + TOL).contains(&u) {
                return true;
            }
        }
    }
    false
}

// ---- pocketing ----

/// How a pocket is filled: straight rows back and forth, or closed rings
/// marching inward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketStrategy {
    Zigzag,
    Waterline,
}

/// 2D pocketing: clear every bit of material inside a region down to
/// `bottom_z`. Islands (region holes) are never entered; the cutter
/// center stays inside the region eroded by the tool radius, so the tool
/// edge never crosses a wall it is meant to leave standing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PocketParams {
    pub tool_diameter: f64,
    /// Distance between successive rows (zigzag) or rings (waterline).
    /// Validated against the tool radius: a wider step leaves bands the
    /// path never reaches, and that is refused rather than accepted
    /// silently.
    pub stepover: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Zigzag: the first row runs -X to +X. Waterline: rings are
    /// travelled with material on the left of the cut.
    pub climb: bool,
    pub strategy: PocketStrategy,
}

impl PocketParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.stepover,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        // Two parallel cut lines a stepover apart cover every point
        // between them (distance at most stepover/2 to the nearer one),
        // and zigzag puts grazing rows on the extreme coverage centers,
        // so stepover <= diameter covers the region. Waterline's innermost
        // ring can stand a whole stepover from the material it collapses
        // around, so it must stay within one radius.
        let reach = match self.strategy {
            PocketStrategy::Zigzag => self.tool_diameter,
            PocketStrategy::Waterline => self.tool_diameter * 0.5,
        };
        if self.stepover > reach {
            return Err(GenError::Invalid(
                "stepover leaves bands the tool cannot reach".into(),
            ));
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
        Ok(())
    }
}

/// Pocket `region` (the material to remove; its loops' interiors are
/// islands to leave standing). The cutter-center area is the region
/// eroded by `r + leave` - the opposite sign to contouring, because here
/// the tool must stay inside the material rather than outside it. Every
/// feed point lies in that area, so the tool edge never crosses a wall.
///
/// Transitions: between levels the tool lifts to `clearance_z`, rapids
/// over and plunges again; within a level a straight connection between
/// two cut segments is kept as a Feed exactly when [`chord_clears`]
/// proves it never leaves the cutter-center area (boundary grazing counts
/// as staying in - it is the tangency wall passes are built on), and
/// otherwise it becomes lift-rapid-plunge. Rapids inside the footprint
/// below the safe plane are the verifier's business (step 7).
pub fn pocket(region: &Region, p: &PocketParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let r = p.tool_diameter * 0.5;
    let center = region.offset(-(r + p.stock_to_leave))?;
    if center.is_empty() {
        // Material exists that no legal tool position can reach.
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    let passes = match p.strategy {
        PocketStrategy::Zigzag => zigzag_passes(&center, p)?,
        PocketStrategy::Waterline => waterline_passes(&center, p)?,
    };
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    run_levels(&mut b, &passes, &zs, p.clearance_z, p.feed, p.plunge_feed);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Serpentine rows across the cutter-center area, each scanline split
/// where the even-odd parity says material stands (islands fall out of
/// the interval walk for free). Rows band [lo.y, hi.y] at `stepover` with
/// the extremes kept: a row on the area's own boundary is the pass whose
/// edge grazes the wall, and dropping it would leave the wall's last band
/// standing.
fn zigzag_passes(center: &Region, p: &PocketParams) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    let ys = band(lo.y, hi.y, 0.0, p.stepover);
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        let segs = scan_row(center, y, y == hi.y);
        let left_to_right = p.climb == (i % 2 == 0);
        for k in 0..segs.len() {
            let (x0, x1) = segs[if left_to_right { k } else { segs.len() - 1 - k }];
            let (s, e) = if left_to_right {
                (Point2::new(x0, y), Point2::new(x1, y))
            } else {
                (Point2::new(x1, y), Point2::new(x0, y))
            };
            let join = match cur.last() {
                Some(&prev) => chord_clears(center, prev, s),
                None => false,
            };
            if join {
                // The corner matters: the connection feeds prev -> s (the
                // tested chord) and only then along the row to e. Pushing
                // only e would cut prev -> e, a diagonal the join test
                // never proved.
                cur.push(s);
                cur.push(e);
            } else {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                cur = vec![s, e];
            }
        }
    }
    if cur.len() >= 2 {
        passes.push(cur);
    }
    Ok(passes)
}

/// Concentric rings marching inward: ring k is the cutter-center area
/// eroded by `k * stepover`, travelled closed. Offsets are taken from
/// `center` at absolute distances (never buffer-of-buffer), so micron
/// quantization cannot compound across rings.
fn waterline_passes(center: &Region, p: &PocketParams) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    // Erosion by more than the diagonal can never be non-empty; the cap
    // only fires if some Clipper path stops shrinking.
    let cap = (dist(lo, hi) / p.stepover + 10.0).max(16.0) as u32;
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for k in 0..=cap {
        let ring = if k == 0 {
            center.clone()
        } else {
            center.offset(-f64::from(k) * p.stepover)?
        };
        if ring.is_empty() {
            if cur.len() >= 2 {
                passes.push(cur);
            }
            return Ok(passes);
        }
        let mut loops: Vec<&Loop> = ring.loops().iter().collect();
        loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
        for l in loops {
            let pts: Vec<Point2> = if p.climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            if pts.len() < 2 {
                continue;
            }
            let join = match cur.last() {
                Some(&prev) => chord_clears(center, prev, pts[0]),
                None => false,
            };
            if !join {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                cur = Vec::new();
            }
            cur.extend_from_slice(&pts);
            cur.push(pts[0]);
        }
    }
    Err(GenError::Invalid(
        "waterline rings did not converge; check the stepover".into(),
    ))
}

/// Intersect the horizontal line `y` with `region`'s loops and pair the
/// crossings by even-odd parity: the cut intervals of one scanline.
/// Crossing rule matches `Region::contains` exactly (lower endpoint
/// inclusive); when the row is the region's top edge it flips to
/// upper-inclusive so the grazing wall pass still finds its intervals.
fn scan_row(region: &Region, y: f64, upper_inclusive: bool) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            let hit = if upper_inclusive {
                ylo < y && y <= yhi
            } else {
                ylo <= y && y < yhi
            };
            if hit {
                xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    let mut k = 0;
    while k + 1 < xs.len() {
        // A zero-width pair is a tangent vertex or the closing double
        // count of a vertex shared by two edges: nothing to cut.
        if xs[k + 1] - xs[k] > 1e-9 {
            out.push((xs[k], xs[k + 1]));
        }
        k += 2;
    }
    out
}

/// Slotting: a straight channel between two XY points, cleared in rows
/// parallel to its axis. The channel is the stadium swept by a disc of
/// radius `width / 2` along `a` -> `b`; shrinking a stadium is narrowing
/// one, so the cutter-center area is in closed form - the stadium of
/// half-width `h = width / 2 - r - leave` on the same endpoints, no
/// Clipper round trip. `h < 0` means no tool position keeps the edge
/// inside the walls; `h == 0` is the everyday full-width slot - one row
/// on the centerline, the tool's own edges sweeping the walls.
///
/// Rows band the width `[-h, h]` at `stepover` (same snap law as facing
/// and pocket rows), and the row at offset `d` runs `a - q u + d n` to
/// `b + q u + d n` with `q = sqrt(h^2 - d^2)`, so each row reaches the
/// round ends exactly as far as it can. The center area is convex, so
/// the serpentine joins with lateral feeds and no in-cut rapids are
/// even possible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotParams {
    pub tool_diameter: f64,
    /// Width of the finished channel.
    pub width: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub stepover: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// The first row runs from `a` toward `b`; rows alternate from there.
    pub climb: bool,
}

impl SlotParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.width,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.stepover,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        if self.stepover > self.tool_diameter {
            return Err(GenError::Invalid(
                "stepover leaves bands the tool cannot reach".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.width <= 0.0 {
            return Err(GenError::Invalid("slot width must be positive".into()));
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
        Ok(())
    }
}

/// Mill the slot between `a` and `b` (see [`SlotParams`] for the shape
/// and the row law). One serpentine pass per level; `run_levels` lifts
/// and plunges only between levels.
pub fn slot(a: Point2, b: Point2, p: &SlotParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 || !len.is_finite() {
        return Err(GenError::Invalid("slot endpoints must be distinct".into()));
    }
    let r = p.tool_diameter * 0.5;
    let h = p.width * 0.5 - r - p.stock_to_leave;
    if h < 0.0 {
        return Err(GenError::Invalid("slot is narrower than the tool".into()));
    }
    let u = Point2::new(dx / len, dy / len);
    let n = Point2::new(-u.y, u.x);
    let ds = band(-h, h, 0.0, p.stepover);
    let mut pass: Vec<Point2> = Vec::with_capacity(2 * ds.len());
    for (i, &d) in ds.iter().enumerate() {
        let q = (h * h - d * d).max(0.0).sqrt();
        let s = Point2::new(a.x - u.x * q + n.x * d, a.y - u.y * q + n.y * d);
        let e = Point2::new(b.x + u.x * q + n.x * d, b.y + u.y * q + n.y * d);
        // Even rows run a -> b under climb; odd rows return. The join is
        // then a short lateral step at the far end, never a diagonal.
        let (s, e) = if p.climb == (i % 2 == 0) { (s, e) } else { (e, s) };
        pass.push(s);
        pass.push(e);
    }

    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    let mut bld = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    run_levels(&mut bld, &[pass], &zs, p.clearance_z, p.feed, p.plunge_feed);
    let cur = bld.at();
    bld.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = bld.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// 2D tracing: the cutter center runs exactly on the region's loops at
/// successive depths - the drawn line itself, engraved (the plan's
/// "buffer 0"). Wall finishing is contour's job; how the tool EDGE gets
/// onto the line (G41/G42, or an r-offset at the post) is the same
/// recorded-compensation question contour already defers to M1.5.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceParams {
    pub clearance_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Canonical (material-on-the-left) travel; conventional reverses
    /// each loop.
    pub climb: bool,
}

impl TraceParams {
    fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.stepdown <= 0.0 {
            return Err(GenError::Invalid("stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        Ok(())
    }
}

/// Trace every loop of `region` - outer loops and holes alike are just
/// lines to engrave - ordered by `|area|` descending for determinism,
/// each traversal closed so the ring's cut joins at its start.
pub fn trace(region: &Region, p: &TraceParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty trace region".into()));
    }
    let mut loops: Vec<&Loop> = region.loops().iter().collect();
    loops.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    let mut passes: Vec<Vec<Point2>> = Vec::with_capacity(loops.len());
    for l in &loops {
        let mut pass: Vec<Point2> = if p.climb {
            l.points().to_vec()
        } else {
            l.points().iter().rev().copied().collect()
        };
        if let Some(&start) = pass.first() {
            pass.push(start);
        }
        passes.push(pass);
    }

    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    run_levels(&mut b, &passes, &zs, p.clearance_z, p.feed, p.plunge_feed);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// True when the straight chord `a` -> `b` never passes outside the
/// closed region: both ends in it, no loop edge properly crossing the
/// open chord, and the chord midpoint inside or on the boundary. Without
/// a proper crossing the open chord lies in one connected piece of the
/// plane complement, so the midpoint decides for all of it - and a chord
/// running along a boundary edge is exactly the tangency a wall pass is,
/// so grazing reads as clear.
pub fn chord_clears(region: &Region, a: Point2, b: Point2) -> bool {
    if (!region.contains(a) && !on_boundary(region, a))
        || (!region.contains(b) && !on_boundary(region, b))
    {
        return false;
    }
    if a == b {
        return true;
    }
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let m = Point2::new(a.x + d.x * 0.5, a.y + d.y * 0.5);
    let eps = 1e-9;
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let c = pts[i];
            let e = pts[(i + 1) % n];
            let g = Point2::new(e.x - c.x, e.y - c.y);
            let ca = Point2::new(c.x - a.x, c.y - a.y);
            let den = d.x * g.y - d.y * g.x;
            if den == 0.0 {
                // Parallel to this edge: touching it is decided by the
                // midpoint test below (a graze along a boundary line is
                // exactly what wall and hole-ceiling passes are).
                continue;
            }
            let t = (ca.x * g.y - ca.y * g.x) / den;
            let u = (ca.x * d.y - ca.y * d.x) / den;
            if !(eps < t && t < 1.0 - eps) {
                continue;
            }
            if u > eps && u < 1.0 - eps {
                return false; // crosses the open edge: a real crossing
            }
            if u >= -eps && u <= 1.0 + eps {
                // The chord goes through a loop vertex. That cuts the
                // boundary only if the two edges meeting there are on
                // opposite sides of the chord line; both on one side is
                // tangency (a fillet touching the straight it joins).
                let j = if u < 0.5 { i } else { (i + 1) % n };
                let p0 = pts[(j + n - 1) % n];
                let p2 = pts[(j + 1) % n];
                let h0 = (p0.x - a.x) * d.y - (p0.y - a.y) * d.x;
                let h2 = (p2.x - a.x) * d.y - (p2.y - a.y) * d.x;
                if h0 * h2 < 0.0 {
                    return false;
                }
            }
        }
    }
    // No proper crossing: the open chord lives in one connected piece of
    // the plane minus the loops, and the midpoint decides which. This
    // also decides every parallel/graze case (on_boundary is tolerant).
    region.contains(m) || on_boundary(region, m)
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

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        Loop::new(vec![
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ])
        .expect("test loops are valid")
    }

    #[test]
    fn scan_row_agrees_with_contains() {
        // 40x40 plate with a 10x10 island dead center.
        let region = Region::new(vec![sq(0.0, 0.0, 40.0, 40.0), sq(15.0, 15.0, 25.0, 25.0)])
            .expect("nested loops");
        let segs = scan_row(&region, 20.0, false);
        assert_eq!(segs, vec![(0.0, 15.0), (25.0, 40.0)]);
        for &(a, b) in &segs {
            assert!(region.contains(Point2::new((a + b) * 0.5, 20.0)));
        }
        assert!(!region.contains(Point2::new(20.0, 20.0)), "island interior");

        // The island's local minimum row (y = 15) has both bottom edges
        // counted by the lower-inclusive rule and splits like any other
        // straddling row; only a local maximum bridges. At y = 25 the
        // island contributes no crossings, and the one interval's points
        // over the island's span graze its top wall without entering it.
        assert_eq!(scan_row(&region, 15.0, false), vec![(0.0, 15.0), (25.0, 40.0)]);
        assert_eq!(scan_row(&region, 25.0, false), vec![(0.0, 40.0)]);

        // The topmost row needs the upper-inclusive flip to find its
        // crossings at all.
        assert_eq!(scan_row(&region, 40.0, false), Vec::new());
        assert_eq!(scan_row(&region, 40.0, true), vec![(0.0, 40.0)]);
    }

    #[test]
    fn chord_clears_separates_grazing_from_crossing() {
        let region = Region::new(vec![sq(0.0, 0.0, 40.0, 40.0), sq(15.0, 15.0, 25.0, 25.0)])
            .expect("nested loops");
        let p = |x: f64, y: f64| Point2::new(x, y);
        // Through the island: refused.
        assert!(!chord_clears(&region, p(5.0, 20.0), p(35.0, 20.0)));
        // Along the outer wall: the tangency a row connector uses.
        assert!(chord_clears(&region, p(40.0, 5.0), p(40.0, 35.0)));
        // Inside material, no crossings.
        assert!(chord_clears(&region, p(5.0, 5.0), p(10.0, 35.0)));
        // Starting outside: refused.
        assert!(!chord_clears(&region, p(-1.0, 5.0), p(35.0, 20.0)));
        // Boundary corners read as closed-inside.
        assert!(chord_clears(&region, p(40.0, 40.0), p(40.0, 30.0)));
    }
}
