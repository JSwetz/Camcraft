//! The 3-D ladder's descents onto a level (moved out of `ladder.rs` by
//! M11.2 C; the arms' laws unchanged): ramp chord or zig-zag ramp, the
//! vertical-plane arc, the helix at its radius and pitch, the axial
//! plunge in the floor's room - each traversing at the plane the user's
//! level link chose. [`start`] answers where a descent begins, so a
//! keep-down link knows what to reach.
//!
//! Provenance: standard CNC entry motions (plunge, linear and zig-zag
//! ramp, helical interpolation, circular ramp) per public G-code manuals
//! and machining handbooks; the legality laws are M8.1's superset law and
//! the shell's tip law (`machining3d::shell`). No vendor material.

use super::level_ctx::{LevelCtx, bore_meets_lifted_leg, dive_link, feed_leg, substituted_dive, traverse};
use super::params::Entry3D;

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{dive_room, helix_turns_down};
use crate::entry_arc::{arc_lead, pts_dir, push_arc_lead};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{helix_turns, plan_zig, push_zig};
use crate::machining2d::{chord_clears, ARC_FIT_TOL};

/// One level's descent: the motion and how it reads the level.
pub(super) struct Ask<'a> {
    pub arm: Entry3D,
    /// The angle-bounded zig-zag (a Ramp transition) and the pitched
    /// helix - a user's level transition, never the first entry.
    pub transition: bool,
    /// A rest pass: every arm stands down to the wall dive.
    pub rest: bool,
    pub entry: Point2,
    pub first_pass: &'a [Point2],
    pub prev_z: f64,
    /// The previous cut level's plane, `None` before any cut.
    pub prev_cut: Option<f64>,
}

/// The ramp's zig-zag along the first pass, where the plain ladder can
/// hold it (a lifted shell has no proof for it).
fn zig(lvl: &LevelCtx, a: &Ask) -> Result<(Vec<Point2>, usize), &'static str> {
    if lvl.field.is_some() {
        return Err("the zig-zag ramp is not proved onto a lifted shell");
    }
    plan_zig(a.first_pass, a.prev_z - lvl.z, lvl.p.links.ramp_angle)
        .ok_or("the first cut is too short to ramp at the angle")
}

fn arc(lvl: &LevelCtx, a: &Ask) -> Result<crate::entry_arc::ArcLead, &'static str> {
    match lvl.field {
        Some(_) => Err("the arc entry is not proved onto a lifted shell"),
        None => pts_dir(a.entry, a.first_pass)
            .ok_or("the first cut has no direction to be tangent to")
            .and_then(|d| {
                let ok = |s, q| chord_clears(lvl.cen, s, q);
                arc_lead(a.entry, d, a.prev_z, lvl.z, lvl.p.tool_diameter, ARC_FIT_TOL, ok)
            }),
    }
}

/// The helix orbit's dive point and turns, or why not.
fn helix(lvl: &LevelCtx, a: &Ask) -> Result<(Point2, f64), &'static str> {
    let p = lvl.p;
    let c = dive_room(lvl.cen, p.helix_radius + 1e-3).ok_or("the widened disc fits nowhere at this level")?;
    let crosses = match (lvl.field, a.prev_cut) {
        (Some(_), Some(pz)) => bore_meets_lifted_leg(lvl, pz, c),
        _ => false,
    };
    if crosses {
        return Err("the shell lifts legs the bore would cross");
    }
    let pitch = if a.transition { p.links.helix_pitch } else { 0.0 };
    let t = helix_turns(a.prev_z - lvl.z, p.helix_radius, pitch).ok_or("the helix pitch asks too many turns")?;
    Ok((Point2::new(lattice(c.x - p.helix_radius), lattice(c.y)), t))
}

/// Where the descent `a` begins in XY with the tool standing over `leg`.
pub(super) fn start(lvl: &LevelCtx, a: &Ask, leg: Point2) -> Point2 {
    let sub = || dive_room(lvl.cen, 0.0).unwrap_or(a.entry);
    if a.rest {
        return a.entry;
    }
    match a.arm {
        Entry3D::Ramp if a.transition => zig(lvl, a).map_or_else(|_| sub(), |_| a.entry),
        Entry3D::Ramp if chord_clears(lvl.cen, leg, a.entry) => leg,
        Entry3D::Arc => arc(lvl, a).map_or_else(|_| sub(), |l| l.start),
        Entry3D::Helix => helix(lvl, a).map_or_else(|_| sub(), |(d, _)| d),
        Entry3D::Ramp | Entry3D::Plunge => sub(),
    }
}

/// Post the descent onto `a.entry` at the level, traversing at `lvl.tz`.
/// At clearance and the entry's own arm this is the pre-M11.2 ladder,
/// move for move.
pub(super) fn descend(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask) -> Option<Point2> {
    let p = lvl.p;
    let (entry, z, prev_z) = (a.entry, lvl.z, a.prev_z);
    let at = b.at();
    let leg = Point2::new(at.x, at.y);
    if a.rest {
        // The rest ladder's entry: it starts inside planes the coarse
        // run already swept, so it rides the vertical dive at the wall
        // entry the nesting law specced.
        traverse(b, lvl, entry);
        b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, lvl.tip(entry, 0.0)), Some(p.plunge_feed));
        return None;
    }
    match a.arm {
        Entry3D::Ramp if a.transition => match zig(lvl, a) {
            Ok((pre, n)) => {
                traverse(b, lvl, entry);
                b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, prev_z), None);
                push_zig(b, &pre, n, prev_z, z, p.feed);
                None
            }
            Err(why) => substituted_dive(b, lvl, entry, why),
        },
        Entry3D::Ramp => {
            // The diagonal's legality is asked of THIS level's center:
            // islands only grow downward, so the destination's room is a
            // subset of every corridor above it - one chord test covers
            // the whole interpolated move (M8.1).
            if chord_clears(lvl.cen, leg, entry) {
                b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
                if lvl.field.is_none() {
                    b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                } else {
                    // The shell's ramp rides strictly above the plane
                    // until one chord short of its destination, and only
                    // then descends to the destination's tip (ladder.rs).
                    let cap = p.tool_diameter * 0.5;
                    let (dx, dy) = (entry.x - leg.x, entry.y - leg.y);
                    let l = (dx * dx + dy * dy).sqrt();
                    let t = if l > cap { 1.0 - cap / l } else { 0.0 };
                    if t > 0.0 {
                        let q1 = Point2::new(leg.x + dx * t, leg.y + dy * t);
                        b.push(MoveKind::Feed, Point3::new(q1.x, q1.y, z), Some(p.feed));
                    }
                    feed_leg(b, lvl, entry, 0.0);
                }
                None
            } else {
                substituted_dive(b, lvl, entry, "the ramp's chord is blocked")
            }
        }
        // The circular ramp (M11.1): the ramp's chord law on the lead
        // chord behind the entry, the fan posted by `entry_arc`.
        Entry3D::Arc => match arc(lvl, a) {
            Ok(l) => {
                if lvl.kept && (leg.x != l.start.x || leg.y != l.start.y) {
                    traverse(b, lvl, l.start);
                }
                push_arc_lead(b, &l, lvl.tz, prev_z, p.feed);
                None
            }
            Err(why) => substituted_dive(b, lvl, entry, why),
        },
        // The widened disc lands on this level's own floor (M8.1); the
        // found point is the orbit's CENTER, the dive starts `rh` west.
        Entry3D::Helix => match helix(lvl, a) {
            Ok((d, turns)) => {
                traverse(b, lvl, d);
                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                // The turns count the drop the orbit really makes (to
                // the shell tip, the legacy law's own span).
                let z_hel = lvl.tip(d, p.helix_radius * 2.0);
                let pitch = if a.transition { p.links.helix_pitch } else { 0.0 };
                let turns = helix_turns(prev_z - z_hel, p.helix_radius, pitch).unwrap_or(turns);
                helix_turns_down(b, d, prev_z, z_hel, p.helix_radius, turns, p.feed);
                dive_link(b, lvl, d, entry);
                Some(d)
            }
            Err(why) => substituted_dive(b, lvl, entry, why),
        },
        // The axial arm dives where the disc has room, not at the
        // pass start on a wall (M8.1).
        Entry3D::Plunge => match dive_room(lvl.cen, 0.0) {
            Some(d) => {
                traverse(b, lvl, d);
                let z_d = lvl.tip(d, 0.0);
                b.push(MoveKind::Plunge, Point3::new(d.x, d.y, z_d), Some(p.plunge_feed));
                dive_link(b, lvl, d, entry);
                Some(d)
            }
            None => substituted_dive(b, lvl, entry, "the disc fits nowhere here"),
        },
    }
}
