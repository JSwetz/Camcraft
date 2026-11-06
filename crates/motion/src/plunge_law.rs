//! The centre-cutting plunge law (M11.3, owed from B-D (4)): a tool whose
//! end edges stop short of its axis may never feed straight down into
//! solid - not on a level's entry, and not on any plunge between passes
//! inside a level (a zigzag row restart, a ring-to-ring dive, a slot or
//! bore re-entry, a 3-D ladder's pass hop, a second dive, a contour's
//! lead-in or stay-down plunge). ONE check every axial descent of the
//! planar, contour and 3-D ladders asks before it posts: is the column
//! under the tool already cleared at the plunge's depth by this path?
//!
//! The reading: solid is everything under the material's top that no
//! earlier cutting move of the same path has removed. A column counts as
//! cleared at depth `z` when an earlier feed - a straight, an arc, a
//! helix chord, a ramp leg - lying wholly at or below `z` passed within
//! `(1 - CORE)` tool radii of the plunge's axis, so the uncut core
//! (`CORE` radii about the axis, where a non-centre-cutting end mill
//! leaves a stub) sits over removed material. Conservative by design: a
//! ramp or helix still descending past `z` clears nothing yet, a rapid
//! never cuts, and stock the generator cannot see is solid. A wrong
//! "solid" only costs a helix, a ramp or a named refusal; a wrong
//! "cleared" would snap a tool, so there is none.
//!
//! Posed paths (the multiaxis kinds) are judged by the same law read in
//! the tool's own frame: a descent is axial when it runs down the posed
//! axis (within the core's cone, [`posed_axial`]), and a column is
//! cleared along that axis ([`cleared_along`]). An unposed move keeps
//! the vertical reading exactly.
//!
//! Provenance: the centre-cutting / non-centre-cutting end mill
//! distinction of machining handbooks (a non-centre-cutting end mill may
//! ramp or helix into material but not plunge); the cleared-column test
//! is this crate's own geometry. No vendor material consulted.

use crate::approach::CENTER_CUT_REFUSAL;
use crate::cl::{Move, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// The uncut core of a non-centre-cutting end mill, as a fraction of its
/// radius: half the radius (a quarter of the diameter) - wider than the
/// centre holes and gashes such tools carry, so the law errs to solid.
pub const CORE: f64 = 0.5;

/// Height slack: a cut on the plunge's own plane counts as at its depth.
const Z_EPS: f64 = 1e-6;

/// Chord tolerance the cleared test reads arcs and helices at (mm).
const CHORD_TOL: f64 = 0.01;

/// Has an earlier cutting move in `moves` cleared the column at `xy`
/// down to `z` for a tool of radius `tool_r` (see the module law)? A
/// non-positive or non-finite radius clears nothing.
#[must_use]
pub fn column_cleared(moves: &[Move], xy: Point2, z: f64, tool_r: f64) -> bool {
    let reach = tool_r * (1.0 - CORE);
    if !(reach.is_finite() && reach > 0.0 && z.is_finite()) {
        return false;
    }
    moves.iter().rev().filter(|m| m.kind != MoveKind::Rapid).any(|m| {
        if m.from.z.min(m.to.z) > z + Z_EPS {
            return false;
        }
        m.segments3(CHORD_TOL).iter().any(|(a, b)| {
            a.z.max(b.z) <= z + Z_EPS && seg_dist(xy, Point2::new(a.x, a.y), Point2::new(b.x, b.y)) <= reach
        })
    })
}

/// Would an axial plunge at `xy` down to `z` be INTO SOLID - below the
/// material's top `mat_top`, over a column the path has not cleared?
#[must_use]
pub fn into_solid(b: &PathBuilder, xy: Point2, z: f64, mat_top: f64, tool_r: f64) -> bool {
    z < mat_top - Z_EPS && !column_cleared(b.moves(), xy, z, tool_r)
}

/// The check every axial descent posts through: for a tool that is not
/// centre-cutting, a plunge into solid refuses the operation by name (the
/// [`CENTER_CUT_REFUSAL`] note, said once per path - the generator turns
/// it into its error, `approach::refusal`). `site` names the motion.
/// Returns whether it refused. A centre-cutting tool is never refused.
pub fn guard(
    b: &mut PathBuilder,
    center_cutting: bool,
    tool_r: f64,
    xy: Point2,
    z: f64,
    mat_top: f64,
    site: &str,
) -> bool {
    if center_cutting || !into_solid(b, xy, z, mat_top, tool_r) {
        return false;
    }
    if !b.notes().iter().any(|n| n.starts_with(CENTER_CUT_REFUSAL)) {
        b.note(format!(
            "{CENTER_CUT_REFUSAL} - {site} at ({:.3}, {:.3}) would plunge axially into solid down to Z {z:.3}, \
             and no helix or ramp stands in there; pick a centre-cutting tool or a ramp/helix entry with room",
            xy.x, xy.y
        ));
    }
    true
}

/// Distance from `p` to the segment `a -> b`.
fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 && dd.is_finite() { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// What the after-the-fact audit reads as solid (M11.3 owed (5)): the
/// generators that carry no [`crate::approach::Approach`] - the 3-D
/// finishes and the facer - are judged on their finished path. `top` is
/// the material's top; `footprint` the stock's XY outline when it is
/// known. A plunge whose tool disc stays clear of the footprint falls
/// through air; with no footprint, everything under `top` is solid.
#[derive(Clone, Copy, Debug)]
pub struct Solid<'a> {
    pub top: f64,
    pub footprint: Option<&'a Region>,
}

impl Solid<'_> {
    /// Does a tool disc of radius `tool_r` at `xy` overlap the stock?
    #[must_use]
    pub fn under(&self, xy: Point2, tool_r: f64) -> bool {
        self.footprint.is_none_or(|f| disc_overlaps(f, xy, tool_r))
    }
}

/// Does a disc of radius `r` at `xy` overlap `region` - its centre
/// inside, or its rim reaching past an edge?
#[must_use]
pub fn disc_overlaps(region: &Region, xy: Point2, r: f64) -> bool {
    if region.contains(xy) {
        return true;
    }
    region.loops().iter().any(|l| {
        let pts = l.points();
        pts.iter().zip(pts.iter().cycle().skip(1)).any(|(a, b)| seg_dist(xy, *a, *b) < r - Z_EPS)
    })
}

/// The first axial descent of `t` into solid: a cutting move going
/// down its tool's axis below `solid.top`, over the stock, onto a
/// column no earlier cut of the path has cleared. `Some((xy, z))` names
/// it (the tip's end). An unposed move is read the 3-axis way (no XY
/// run, going down Z); a posed one (M11.3 owed, multiaxis) by
/// [`posed_axial`] and [`cleared_along`] - axial with respect to the
/// pose, not world Z.
#[must_use]
pub fn first_solid_plunge(t: &Toolpath, tool_r: f64, solid: &Solid) -> Option<(Point2, f64)> {
    t.moves.iter().enumerate().find_map(|(i, m)| {
        let xy = Point2::new(m.to.x, m.to.y);
        let hit = match m.axis {
            None => {
                let axial = m.kind != MoveKind::Rapid
                    && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) <= Z_EPS
                    && m.to.z < m.from.z - Z_EPS
                    && m.to.z < solid.top - Z_EPS;
                axial && solid.under(xy, tool_r) && !column_cleared(&t.moves[..i], xy, m.to.z, tool_r)
            }
            Some(ax) => {
                let a = Point3::new(ax[0], ax[1], ax[2]);
                m.kind != MoveKind::Rapid
                    && m.arc.is_none()
                    && posed_axial(m, a)
                    && m.to.z < solid.top - Z_EPS
                    && solid.under(xy, tool_r)
                    && !cleared_along(&t.moves[..i], m.to, a, tool_r)
            }
        };
        hit.then_some((xy, m.to.z))
    })
}

/// Is the straight move `m` a descent along the posed unit axis `a`
/// (tip toward shank)? It must advance toward the tip, and lean off
/// `-a` by no more than the core's half-angle: `sin θ <= CORE`. Inside
/// that cone the tool's leading point lies in its uncut core (for a
/// ball the core spans the sphere within `asin(CORE)` of the tip; a
/// flat end drags its whole face, core included) - so a 5-axis
/// stamp's world-vertical drop under a lean of less than 30 degrees is
/// a plunge, as is a drop straight down a tilted axis. Wider leans are
/// ramps, the sloped-entry laws' business.
#[must_use]
pub fn posed_axial(m: &Move, a: Point3) -> bool {
    let d = m.to - m.from;
    let len = d.dot(d).sqrt();
    let along = -d.dot(a);
    if !(len.is_finite() && along.is_finite() && along > Z_EPS) {
        return false;
    }
    let lateral = (len * len - along * along).max(0.0).sqrt();
    lateral <= CORE * len
}

/// [`column_cleared`] along a posed axis: has an earlier cutting move
/// cleared the column down the unit axis `a` to the tip `end`? The
/// cut must lie wholly at or below `end` both along `a` and in world Z
/// (the stricter reading of the two - errs to solid), within
/// `(1 - CORE)` radii of the axis line, measured across the axis. A
/// degenerate axis or radius clears nothing.
#[must_use]
pub fn cleared_along(moves: &[Move], end: Point3, a: Point3, tool_r: f64) -> bool {
    let reach = tool_r * (1.0 - CORE);
    let Some((u, v)) = across(a) else { return false };
    if !(reach.is_finite() && reach > 0.0 && end.is_finite()) {
        return false;
    }
    let h = end.dot(a);
    let flat = |p: Point3| Point2::new(p.dot(u), p.dot(v));
    let e2 = flat(end);
    moves.iter().rev().filter(|m| m.kind != MoveKind::Rapid).any(|m| {
        m.segments3(CHORD_TOL).iter().any(|(p, q)| {
            p.z.max(q.z) <= end.z + Z_EPS
                && p.dot(a).max(q.dot(a)) <= h + Z_EPS
                && seg_dist(e2, flat(*p), flat(*q)) <= reach
        })
    })
}

/// An orthonormal pair spanning the plane across the unit axis `a`;
/// None for an axis that states no direction.
fn across(a: Point3) -> Option<(Point3, Point3)> {
    let n = a.dot(a).sqrt();
    if !(n.is_finite() && (n - 1.0).abs() < 1e-6) {
        return None;
    }
    let helper = if a.z.abs() < 0.9 { Point3::new(0.0, 0.0, 1.0) } else { Point3::new(1.0, 0.0, 0.0) };
    let w = a.cross(helper);
    let wn = w.dot(w).sqrt();
    if !(wn.is_finite() && wn > 1e-9) {
        return None;
    }
    let u = Point3::new(w.x / wn, w.y / wn, w.z / wn);
    Some((u, a.cross(u)))
}

/// The audit's verdict: a centre-cutting tool always passes; otherwise
/// the first plunge into solid refuses the operation by name
/// ([`CENTER_CUT_REFUSAL`]), `site` naming the motion.
pub fn audit(t: &Toolpath, center_cutting: bool, tool_r: f64, solid: &Solid, site: &str) -> Result<(), GenError> {
    match first_solid_plunge(t, tool_r, solid).filter(|_| !center_cutting) {
        None => Ok(()),
        Some((xy, z)) => Err(GenError::Invalid(format!(
            "{CENTER_CUT_REFUSAL} - {site} at ({:.3}, {:.3}) would plunge axially into solid down to Z {z:.3}; \
             pick a centre-cutting tool or an entry that starts outside the stock",
            xy.x, xy.y
        ))),
    }
}
