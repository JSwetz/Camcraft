//! The planar ladder's level and pass links (M11.2 D): lift to
//! clearance, lift to the retract plane, or keep down - the keep-down
//! feed proved inside the certified area, crossing no planned pass and no
//! dive link already cut at that plane, within the stay-down limit; every
//! lift it falls back to tallied and named. Provenance: the link
//! strategies of published CAM practice (`links.rs`); the proof is the
//! plan law M8.1 wrote for dive links. No vendor material consulted.

use crate::approach::CENTER_CUT_REFUSAL;
use crate::cl::{MoveKind, PathBuilder};
use crate::entry::{Entry3D, chord_hits_plan};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{Link, LinkTally, Links, NOT_PROVED, TOO_LONG};

use super::arcs::Seg;
use super::descent::{Lvl, feed_down, plan, push, restart, start};
use crate::plunge_law;
use super::levels::{Cert, Motion};

/// One pass's place in the ladder, as a descent onto it reads it.
pub(super) struct LevelWalk<'a> {
    pub passes: &'a [Vec<Point2>],
    pub pass: &'a [Point2],
    pub walk: Option<&'a [Seg]>,
    pub z: f64,
    pub prev_z: f64,
    /// The material's top at this level (M11.3 B's plunge height).
    pub mat_top: f64,
}

/// What the links remember across one ladder: the lifts to name, and the
/// dive link cut at the level last entered - a planar feed a keep-down
/// link on that plane must not cross (until the next level begins, it is
/// the level the tool stands on).
#[derive(Default)]
pub(super) struct LinkState {
    pub tally: LinkTally,
    level: Vec<Vec<Point2>>,
}

impl LinkState {
    /// A new level was entered at `e`; `dove` is its dive link's start.
    pub(super) fn begin_level(&mut self, e: Point2, dove: Option<Point2>) {
        self.level.clear();
        if let Some(d) = dove {
            self.level.push(vec![d, e]);
        }
    }
}

/// A descent onto `lv.pass[0]` at `lv.z`, traversing at `tz`: the
/// planned arm where a certified area exists, the climb-and-dive (rapid
/// over the start, plunge) where none does - the slot/trace shape no arm
/// can change without proofs. Returns the dive link's start, if any.
pub(super) fn descend(
    b: &mut PathBuilder,
    arm: Entry3D,
    zig: bool,
    cert: Option<&Cert<'_>>,
    lv: &LevelWalk,
    tz: f64,
    m: &Motion,
) -> Option<Point2> {
    let cur = b.at();
    let leg = Point2::new(cur.x, cur.y);
    match cert {
        Some(cen) => {
            let l = Lvl { cen, passes: lv.passes, pass: lv.pass, walk: lv.walk, mat_top: lv.mat_top };
            let d = plan(arm, zig, &l, leg, lv.z, lv.prev_z, m);
            push(b, &d, &l, lv.z, tz, m)
        }
        None => {
            let e = lv.pass.first()?;
            if !m.links.approach.center_cutting {
                // Nothing certified, so no helix or ramp can stand in: a
                // tool that may not plunge into solid refuses the op.
                b.note_once(&format!(
                    "{CENTER_CUT_REFUSAL} - the dive at the pass start ({:.3}, {:.3}) has no certified room for a helix or ramp",
                    e.x, e.y
                ));
            }
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, tz), None);
            feed_down(b, *e, lv.z, lv.mat_top, m, m.links.approach.plunge(m.plunge_feed));
            None
        }
    }
}

/// The level change, by the user's level link.
pub(super) fn level_link(
    b: &mut PathBuilder,
    arm: Entry3D,
    zig: bool,
    cert: Option<&Cert<'_>>,
    lv: &LevelWalk,
    m: &Motion,
    st: &mut LinkState,
) {
    let cur = b.at();
    let at = Point2::new(cur.x, cur.y);
    let e = lv.pass.first().copied().unwrap_or(at);
    let lift = |b: &mut PathBuilder, to: f64| b.push(MoveKind::Rapid, Point3::new(at.x, at.y, to), None);
    let dove = match (m.links.level, cert) {
        (Link::Clearance, _) => {
            lift(b, m.clearance_z);
            descend(b, arm, zig, cert, lv, m.clearance_z, m)
        }
        (Link::Retract, _) => {
            lift(b, m.hop_z);
            descend(b, arm, zig, cert, lv, m.hop_z, m)
        }
        (Link::KeepDown, Some(cen)) => {
            // Stay on the level just cut: the descent's start is reached
            // at that plane by a feed proved like every planar link -
            // inside the certified area (one region at every plane),
            // crossing none of that level's cuts, within the limit.
            let l = Lvl { cen, passes: lv.passes, pass: lv.pass, walk: lv.walk, mat_top: lv.mat_top };
            let d = plan(arm, zig, &l, at, lv.z, lv.prev_z, m);
            let s = start(&d, &l, at, m);
            match link_proof(cen, &[lv.passes, &st.level], at, s, &m.links) {
                Ok(()) => {
                    b.push(MoveKind::Feed, Point3::new(s.x, s.y, lv.prev_z), Some(m.feed));
                    push(b, &d, &l, lv.z, lv.prev_z, m)
                }
                Err(why) => {
                    st.tally.lift(why, at, lv.prev_z);
                    lift(b, m.hop_z);
                    push(b, &d, &l, lv.z, m.hop_z, m)
                }
            }
        }
        (Link::KeepDown, None) => {
            // Nothing certified to prove a link against: lift, named.
            st.tally.lift(NOT_PROVED, at, lv.prev_z);
            lift(b, m.hop_z);
            descend(b, arm, zig, None, lv, m.hop_z, m)
        }
    };
    st.begin_level(e, dove);
}

/// The move between two passes of one level, by the pass link.
pub(super) fn pass_link(b: &mut PathBuilder, lv: &LevelWalk, m: &Motion, cert: Option<&Cert<'_>>, st: &mut LinkState) {
    let (passes, z, mat_top) = (lv.passes, lv.z, lv.mat_top);
    let Some(&e) = lv.pass.first() else {
        return;
    };
    let cur = b.at();
    let at = Point2::new(cur.x, cur.y);
    let plane = match m.links.pass {
        Link::Clearance => m.clearance_z,
        Link::Retract => m.hop_z,
        Link::KeepDown => {
            let proof = cert.map_or(Err(NOT_PROVED), |c| link_proof(c, &[passes, &st.level], at, e, &m.links));
            match proof {
                Ok(()) => {
                    b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.feed));
                    return;
                }
                Err(why) => {
                    st.tally.lift(why, at, z);
                    m.hop_z
                }
            }
        }
    };
    // The climb-and-dive every planar ladder has posted between passes:
    // the floor its neighbours already cleared, crossed in air.
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, plane), None);
    // M11.3 (the centre-cutting law between passes): a tool that may not
    // plunge into solid enters a pass start its neighbours have not
    // cleared like a level - the helix onto it, else the ramp along the
    // pass - or the op is refused by name (`plunge_law`).
    if !m.links.approach.center_cutting
        && plunge_law::into_solid(b, e, z, mat_top, m.tool_r())
        && let Some(cen) = cert
    {
        let l = Lvl { cen, passes, pass: lv.pass, walk: lv.walk, mat_top };
        let from = (mat_top + m.links.approach.plunge_height).min(plane);
        let d = restart(&l, at, z, from, m);
        if let Some(dv) = push(b, &d, &l, z, plane, m) {
            st.level.push(vec![dv, e]);
        }
        return;
    }
    b.push(MoveKind::Rapid, Point3::new(e.x, e.y, plane), None);
    feed_down(b, e, z, mat_top, m, m.plunge_feed);
}

/// The keep-down link law: `a -> s` within the stay-down limit, inside
/// the certified area, crossing none of `cut` (the level's planned
/// passes and dive links - the verifier's Crossing law). `Err` is the
/// reason the tally names.
fn link_proof(
    cen: &Cert<'_>,
    cut: &[&[Vec<Point2>]],
    a: Point2,
    s: Point2,
    links: &Links,
) -> Result<(), &'static str> {
    if !links.within_stay_down((s.x - a.x).hypot(s.y - a.y)) {
        return Err(TOO_LONG);
    }
    if a.x == s.x && a.y == s.y {
        return Ok(());
    }
    if cen.chord_ok(a, s) && !cut.iter().any(|c| chord_hits_plan(a, s, c)) {
        Ok(())
    } else {
        Err(NOT_PROVED)
    }
}
