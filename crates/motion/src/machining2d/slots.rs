//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::face::{GenError, band};
use super::levels::{Motion, cut_levels, run_levels};
use super::ring::on_boundary;
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
    /// Hop plane for moves that stay inside already-cut material. None
    /// hops to `clearance_z` (the legacy shape). A slot is one continuous
    /// serpentine per level, so the hop never actually engages here; the
    /// plane is validated like everywhere else so a stale value cannot
    /// hide inside a file that later gains multi-pass rows.
    pub retract_z: Option<f64>,
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
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
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
        finite &= self.retract_z.is_none_or(f64::is_finite);
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
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
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
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    run_levels(&mut bld, &[pass], &zs, &motion, None, None);
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
    /// Hop plane between loops of the same level (engraved lines already
    /// cut their own groove, so a low hop is safe there). None hops to
    /// `clearance_z` - the legacy shape, byte-stable goldens.
    pub retract_z: Option<f64>,
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
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
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
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
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
    // Trace engraves the region's own vertices - the point is to ride
    // the boundary as picked, so no arc fitter runs here.
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    run_levels(&mut b, &passes, &zs, &motion, None, None);
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
