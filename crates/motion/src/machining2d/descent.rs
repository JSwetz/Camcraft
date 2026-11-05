//! The planar ladder's descents, one plan per motion (moved out of
//! `levels.rs` by M11.2 C so a level change can take any of them, from
//! any plane): plunge in the floor's room, the ramp chord, the zig-zag
//! ramp at a maximum angle along the level's first cut, the helix at a
//! radius and pitch, the vertical-plane arc. Each is PLANNED first (so a
//! keep-down link knows where the descent starts) and POSTED second,
//! from whatever plane the tool travels at.
//!
//! Provenance: standard CNC entry motions - straight plunge, linear and
//! zig-zag ramp, helical interpolation, circular ramp - as public G-code
//! manuals and machining handbooks describe them; the certification laws
//! are this crate's (M8.1's superset law, `entry::dive_room`,
//! `entry_arc`). No vendor material consulted.

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{Entry3D, chord_hits_plan, dive_room, helix_turns_down};
use crate::entry_arc::{ArcLead, arc_lead, pts_dir, push_arc_lead, seg_dir};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{helix_turns, plan_zig, push_zig};

use super::arcs::{ARC_FIT_TOL, Seg};
use super::levels::{Cert, Motion};

/// A planned descent onto the pass start `e` at plane `z`.
pub(super) enum Descent {
    /// Axial dive at the floor's room `d`, then the dive link to `e`.
    Plunge(Point2),
    /// The ramp chord from the tool's own XY `leg` down to `e`.
    Ramp(Point2),
    /// The zig-zag ramp along the first cut (from `e`): `n` out-and-back
    /// legs over `pre`.
    Zig(Vec<Point2>, usize),
    /// The helix about the circle whose near side is `d`, `turns` turns.
    Helix(Point2, f64),
    /// The vertical-plane arc onto `e`.
    Arc(ArcLead),
    /// The asked motion was refused for the reason: the floor's axial
    /// dive stands in, said.
    Sub(&'static str),
}

/// What a plan reads of the level: the certified area, the plan's
/// passes, the pass being entered and its fitted walk.
pub(super) struct Lvl<'a> {
    pub cen: &'a Cert<'a>,
    pub passes: &'a [Vec<Point2>],
    pub pass: &'a [Point2],
    pub walk: Option<&'a [Seg]>,
}

/// Plan `arm` onto `e = pass[0]` at `z` from `prev_z`, the tool standing
/// over `leg`. `zig` selects the angle-bounded ramp (a level transition
/// the user set to Ramp) over the entry's single chord.
pub(super) fn plan(arm: Entry3D, zig: bool, l: &Lvl, leg: Point2, z: f64, prev_z: f64, m: &Motion) -> Descent {
    let Some(&e) = l.pass.first() else {
        return Descent::Sub("the pass has no start");
    };
    match arm {
        Entry3D::Ramp if zig => match plan_zig(l.pass, prev_z - z, m.links.ramp_angle) {
            Some((pre, n)) => Descent::Zig(pre, n),
            None => Descent::Sub("the first cut is too short to ramp at the angle"),
        },
        Entry3D::Ramp if l.cen.chord_ok(leg, e) => Descent::Ramp(leg),
        Entry3D::Ramp => Descent::Sub("the ramp's chord is blocked"),
        Entry3D::Helix => match dive_room(l.cen.room(), m.helix_radius + 1e-3) {
            // The found point is the orbit's CENTER; the dive starts at
            // its near side, `rh` west - one radius of center-room
            // certifies the whole swept tube exactly (`entry::dive_room`).
            Some(c) => {
                let pitch = if zig { m.links.helix_pitch } else { 0.0 };
                match helix_turns(prev_z - z, m.helix_radius, pitch) {
                    Some(t) => Descent::Helix(Point2::new(lattice(c.x - m.helix_radius), lattice(c.y)), t),
                    None => Descent::Sub("the helix pitch asks too many turns"),
                }
            }
            None => Descent::Sub("the widened disc fits nowhere at this level"),
        },
        Entry3D::Arc => {
            // The circular ramp: the ramp's chord law on its lead chord
            // is its whole proof (see `entry_arc`).
            let rest = l.pass.get(1..).unwrap_or(&[]);
            let walk = l.walk.and_then(|w| w.first());
            let dir = walk.map_or_else(|| pts_dir(e, rest), |s| seg_dir(e, s));
            let lead = dir.ok_or("the first cut has no direction to be tangent to").and_then(|d| {
                arc_lead(e, d, prev_z, z, m.arc_radius, ARC_FIT_TOL, |s, q| l.cen.chord_ok(s, q))
            });
            match lead {
                Ok(a) => Descent::Arc(a),
                Err(why) => Descent::Sub(why),
            }
        }
        Entry3D::Plunge => match dive_room(l.cen.room(), 0.0) {
            Some(d) => Descent::Plunge(d),
            None => Descent::Sub("the disc fits nowhere here"),
        },
    }
}

/// Where a planned descent starts in XY - the point a keep-down link
/// must reach at the level above.
pub(super) fn start(d: &Descent, l: &Lvl, leg: Point2) -> Point2 {
    let e = l.pass.first().copied().unwrap_or(leg);
    match d {
        Descent::Plunge(p) | Descent::Helix(p, _) => *p,
        Descent::Ramp(leg) => *leg,
        Descent::Zig(..) => e,
        Descent::Arc(a) => a.start,
        Descent::Sub(_) => dive_room(l.cen.room(), 0.0).unwrap_or(e),
    }
}

/// Post a planned descent. `tz` is the plane the tool traverses at to
/// reach the descent's start (clearance, the retract plane, or - after a
/// keep-down link - the level above itself); `prev_z` the plane the cut
/// descends from. At `tz` = clearance every arm posts exactly what the
/// pre-M11.2 ladder posted. Returns the dive point when the descent
/// ends with a dive link `d -> e` on the level's plane - a planar cut a
/// later keep-down link at this plane must not cross (the verifier's
/// Crossing law).
#[allow(clippy::too_many_arguments)]
pub(super) fn push(b: &mut PathBuilder, d: &Descent, l: &Lvl, z: f64, prev_z: f64, tz: f64, m: &Motion) -> Option<Point2> {
    let &e = l.pass.first()?;
    match d {
        Descent::Plunge(dp) => {
            let cur = b.at();
            if cur.x != dp.x || cur.y != dp.y {
                b.push(MoveKind::Rapid, Point3::new(dp.x, dp.y, tz), None);
            }
            b.push(MoveKind::Plunge, Point3::new(dp.x, dp.y, z), Some(m.plunge_feed));
            dive_link(b, l.cen, l.passes, *dp, e, z, m);
            Some(*dp)
        }
        Descent::Ramp(leg) => {
            b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
            b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
            None
        }
        Descent::Zig(pre, n) => {
            let cur = b.at();
            if cur.x != e.x || cur.y != e.y {
                b.push(MoveKind::Rapid, Point3::new(e.x, e.y, tz), None);
            }
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, prev_z), None);
            push_zig(b, pre, *n, prev_z, z, m.feed);
            None
        }
        Descent::Helix(dp, turns) => {
            let cur = b.at();
            if cur.x != dp.x || cur.y != dp.y {
                b.push(MoveKind::Rapid, Point3::new(dp.x, dp.y, tz), None);
            }
            b.push(MoveKind::Rapid, Point3::new(dp.x, dp.y, prev_z), None);
            helix_turns_down(b, *dp, prev_z, z, m.helix_radius, *turns, m.feed);
            dive_link(b, l.cen, l.passes, *dp, e, z, m);
            Some(*dp)
        }
        Descent::Arc(a) => {
            push_arc_lead(b, a, tz, prev_z, m.feed);
            None
        }
        Descent::Sub(why) => substituted_dive(b, l.cen, l.passes, e, z, m, why, tz),
    }
}

/// The link from a just-completed dive at `d` up to plane `z` to the
/// pass start `e`: feed it when the chord is proven (inside the
/// center, crossing none of the level's planned paths), fly it over -
/// hop, traverse, dive - when it is not. Shared by every arm's entry
/// so the law reads once: an unproven chord never drags the tool
/// across a wall face it has no right to cross. `pub(super)` since
/// M8.2 A: the contour ladder runs the same law with its rings as
/// the plan.
pub(super) fn dive_link(
    b: &mut PathBuilder,
    cen: &Cert,
    passes: &[Vec<Point2>],
    d: Point2,
    e: Point2,
    z: f64,
    m: &Motion,
) {
    if cen.chord_ok(d, e) && !chord_hits_plan(d, e, passes) {
        b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
    } else {
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.hop_z), None);
        b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
    }
}

/// The entry family's honorable failure (M8.1): the stated motion
/// could not be honored at this level for `why`, so the ladder
/// substitutes the axial dive at the floor's own room - and says so on
/// the status line. A floor with no room either (a region too dense to
/// triangulate, an erosion that vanished) falls back to the wall dive
/// the pocket used to post, still loud. `tz` is the traverse plane
/// (clearance before M11.2's links). `pub(super)` since M8.2 A: the
/// contour ladder's substitutions need the same words spoken the same
/// way.
#[allow(clippy::too_many_arguments)]
pub(super) fn substituted_dive(
    b: &mut PathBuilder,
    cen: &Cert,
    passes: &[Vec<Point2>],
    e: Point2,
    z: f64,
    m: &Motion,
    why: &str,
    tz: f64,
) -> Option<Point2> {
    match dive_room(cen.room(), 0.0) {
        Some(d) => {
            b.note(format!("entry: {why} - axial dive on the floor instead"));
            let cur = b.at();
            if cur.x != d.x || cur.y != d.y {
                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, tz), None);
            }
            b.push(MoveKind::Plunge, Point3::new(d.x, d.y, z), Some(m.plunge_feed));
            dive_link(b, cen, passes, d, e, z, m);
            Some(d)
        }
        None => {
            b.note(format!(
                "entry: {why} - the floor has no dive room either; dived at the pass start"
            ));
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, tz), None);
            b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(m.plunge_feed));
            None
        }
    }
}
