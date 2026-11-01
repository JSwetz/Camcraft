//! Split from `machining3d.rs` by M5.0.1; structure only.

use super::params::{Entry3D, Pocket3DParams};
use super::rest::material_above;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::{chord_hits_plan, dive_room, helix_down};
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{
    chord_clears, plan_passes, GenError,
};
use crate::stock_sim::Dexel;
/// One ladder run: the level loop shared by both entry points. `zs` is
/// the plane list, `rest` the post-coarse state whose still-standing
/// columns the pass is aimed at (None: the pure level law), `mask` the
/// state LEFT BY PREVIOUS OPERATIONS that bounds the coarse run
/// itself (M3.1 D: rest machining - a column already machined at or
/// below this plane is cut by nobody twice), and `start` where the
/// builder begins - the home position for the coarse run, the coarse
/// path's end for the rest run, so the two legs chain into one path.
///
/// Why the mask cannot endanger an entry motion: a ramp's legality is
/// the islands superset - a chord legal in the deepest (most crowded)
/// center is legal at every plane above it. The mask term shrinks
/// upward too, but a chord point the mask removes at an intermediate
/// plane is a column whose standing top sits below that plane, and a
/// cutter whose bottom travels at or above a column's top touches
/// nothing there. Air, never a wall: the island term is the only one
/// with walls to prove against, and the island superset is untouched.
pub(super) fn ladder(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    zs: &[f64],
    rest: Option<&Dexel>,
    mask: Option<&Dexel>,
    start: Point3,
) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    let hop_z = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);

    let mut b = PathBuilder::new(start);
    let step = p.tiling_step();
    // The last level that actually cut: its plane and center area, the
    // most crowded the entry motions of the next level can meet (a
    // deeper level's target only grows). None until one has cut.
    let mut prev_state: Option<(f64, Region)> = None;
    for &z in zs {
        // The level law, in two region ops: islands from the model, the
        // cutter-center area by one erosion.
        let islands = material_above(facets, z)?;
        let target = footprint.difference(&islands)?;
        let target = match rest {
            // The level law on rest terms: only columns the simulated
            // state still has standing above this plane. The islands
            // subtraction already ran, and the nesting law keeps the
            // intersection honest: a part column above z is an island,
            // so the sim's coarser top cannot smuggle part material
            // into a rest target.
            None => target,
            Some(dex) => target.intersect(&dex.above(z)?)?,
        };
        let target = match mask {
            // The same intersection for what EARLIER operations left
            // standing (M3.1 D): a column they machined to at or below
            // this plane must not be tiled again. Pure subtraction -
            // the masked target is a subset of the unmasked one, and
            // skipping a column with nothing standing above the plane
            // cannot cut anything.
            None => target,
            Some(dex) => target.intersect(&dex.above(z)?)?,
        };
        if target.is_empty() {
            continue; // nothing above this plane: model material across
                      // the whole footprint, or nothing standing at all
        }
        let center = target.offset(-(r + p.stock_to_leave))?;
        if center.is_empty() {
            continue; // narrower than the tool at this level: the
                      // staircase (or a wall this tool cannot enter) -
                      // rest material's later problem, never a lie now
        }
        let passes = plan_passes(&center, step, p.climb, p.strategy)?;
        let Some(first_pass) = passes.first() else {
            continue;
        };
        let Some(&entry) = first_pass.first() else {
            continue;
        };
        // A level change always climbs to clearance first: the new
        // level engages material the previous one never touched, and
        // the entry motions below all start their bite from there (or
        // from the previous plane, when that is what legality asks).
        let at = b.at();
        if at.z != p.clearance_z {
            b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
        }
        let leg = Point2::new(at.x, at.y);
        // The plane an entry descends from: the previous cutting
        // level's own, or - nothing has cut below the surface yet,
        // because the skim plane was all air or these steps fall
        // under the part's first solid - the safe plane. The superset
        // law makes both honest: room at the destination is room
        // across every plane above it. (M8.1: the arms read this at
        // the FIRST level too - the `li > 0` gates stayed in cc5d89d
        // and left the family's word unwritten at level 0, where a
        // chosen Helix still opened with a straight wall plunge; with
        // the gates gone, this fallback is the whole story of
        // prev_z.)
        let prev_z = prev_state.as_ref().map_or(p.clearance_z, |(q, _)| *q);
        let lvl = LevelCtx {
            cen: &center,
            passes: &passes,
            z,
            hop_z,
            p,
        };
        match p.entry {
            // Ramp and helix are the coarse ladder's motions: a rest
            // pass starts inside planes that are already cut, where a
            // diagonal would cross floors the coarse pass owns. The
            // vertical dive needs no such proof (the nesting law
            // carries it), so every rest entry lands in the plunge arm
            // below, at the wall entry the nesting law specced.
            Entry3D::Ramp if rest.is_none() => {
                // The diagonal's legality is asked of THIS level's
                // center: islands only grow downward, so the plane
                // the descent ENDS in is its deepest and most
                // crowded, and the destination's room is a subset of
                // every corridor above it - one chord test covers the
                // whole interpolated move, at level 0 included.
                // (M8.1: asking the PREVIOUS level's center was sound
                // only where walls stand vertical; on a slope it
                // tested the widest plane of the corridor and missed
                // the narrowest.)
                let open = chord_clears(&center, leg, entry);
                if open {
                    b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
                    b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                } else {
                    substituted_dive(&mut b, &lvl, entry, "the ramp's chord is blocked");
                }
            }
            Entry3D::Helix if rest.is_none() => {
                // The widened disc lands on this level's own floor
                // (M8.1): the destination center eroded by the helix
                // radius, taken at its widest open triangle. By the
                // superset law this leg is a subset of the previous
                // level's, so room here is room across the whole
                // crossed depth and the old two-leg scan was asking the
                // right question of the wrong data: on an all-arcs ring
                // plan the chord midpoints sit on the ring lines, never
                // in the eroded leg, `dive` resolved None on every
                // tessellated model this ladder meets, and the orbit
                // died into a wall plunge without a word.
                let dive = dive_room(&center, p.helix_radius + 1e-3);
                if let Some(d) = dive {
                    if leg.x != d.x || leg.y != d.y {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, p.clearance_z), None);
                    }
                    b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                    helix_down(&mut b, d, prev_z, z, p.helix_radius, p.feed);
                    dive_link(&mut b, &lvl, d, entry);
                } else {
                    substituted_dive(
                        &mut b,
                        &lvl,
                        entry,
                        "the widened disc fits nowhere at this level",
                    );
                }
            }
            Entry3D::Plunge if rest.is_none() => {
                // The family's axial arm at a material level dives
                // where the disc has room, not at `passes[0]`'s start
                // on a wall (M8.1); the link runs over this level's
                // fresh plane under the same chord law as every arm.
                match dive_room(&center, 0.0) {
                    Some(d) => {
                        if leg.x != d.x || leg.y != d.y {
                            b.push(MoveKind::Rapid, Point3::new(d.x, d.y, p.clearance_z), None);
                        }
                        b.push(
                            MoveKind::Plunge,
                            Point3::new(d.x, d.y, z),
                            Some(p.plunge_feed),
                        );
                        dive_link(&mut b, &lvl, d, entry);
                    }
                    None => {
                        substituted_dive(&mut b, &lvl, entry, "the disc fits nowhere here");
                    }
                }
            }
            // What is left under this arm is the rest ladder's entry:
            // it starts inside planes the coarse run already swept,
            // so it rides the vertical dive at the wall entry the
            // nesting law specced, as the comment above the match
            // states.
            _ => {
                if leg.x != entry.x || leg.y != entry.y {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                }
                b.push(
                    MoveKind::Plunge,
                    Point3::new(entry.x, entry.y, z),
                    Some(p.plunge_feed),
                );
            }
        }
        walk_pass(&mut b, first_pass, z, p.feed, /*skip_first=*/ true);
        // Remaining passes: connect as far as the floor allows - and no
        // further than the level's own planned paths allow (the boss
        // bug: a chord can clear the cutter-center area yet cross every
        // ring scheduled under it, which `verify` then reports as a
        // Crossing at the cut depth; fly over it instead).
        for pass in &passes[1..] {
            let Some(&next_entry) = pass.first() else {
                continue;
            };
            let prev = b.at();
            let p2 = Point2::new(prev.x, prev.y);
            if chord_clears(&center, p2, next_entry) && !chord_hits_plan(p2, next_entry, &passes) {
                // Feed straight on at this level (the join chord the
                // test just proved).
                b.push(MoveKind::Feed, Point3::new(next_entry.x, next_entry.y, z), Some(p.feed));
            } else {
                b.push(MoveKind::Rapid, Point3::new(prev.x, prev.y, hop_z), None);
                b.push(MoveKind::Rapid, Point3::new(next_entry.x, next_entry.y, hop_z), None);
                b.push(
                    MoveKind::Plunge,
                    Point3::new(next_entry.x, next_entry.y, z),
                    Some(p.plunge_feed),
                );
            }
            walk_pass(&mut b, pass, z, p.feed, /*skip_first=*/ true);
        }
        // Hand this level's plan to the next one's entry legality: it
        // is the deepest cut so far, so its center bounds every plane
        // a descent from here will cross.
        prev_state = Some((z, center));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if path.moves.is_empty() {
        return Err(GenError::Empty(
            "nothing to cut: the model fills the pocket at every level".into(),
        ));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// The level a dive lands on, bundled once per iteration for the link
/// and substitute arms: the cutter-center region, the level's planned
/// paths, the plane, the hop height, and the op params. These five
/// arrived together at every call site and always will - a dive
/// without a level is not a question anyone can answer.
struct LevelCtx<'a> {
    cen: &'a Region,
    passes: &'a [Vec<Point2>],
    z: f64,
    hop_z: f64,
    p: &'a Pocket3DParams,
}

/// The link from a dive that just finished at `d` up on plane `z` to
/// the pass start `e`: feed it when the chord is proven (inside this
/// level's center, crossing none of the level's planned paths), fly it
/// over - hop, traverse, dive - when it is not. Shared by every entry
/// arm so the law reads once (the 3-D seat of the proof; the 2-D
/// ladder runs the same law in `machining2d::levels`).
fn dive_link(b: &mut PathBuilder, lvl: &LevelCtx, d: Point2, e: Point2) {
    if chord_clears(lvl.cen, d, e) && !chord_hits_plan(d, e, lvl.passes) {
        b.push(MoveKind::Feed, Point3::new(e.x, e.y, lvl.z), Some(lvl.p.feed));
    } else {
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, lvl.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lvl.hop_z), None);
        b.push(
            MoveKind::Plunge,
            Point3::new(e.x, e.y, lvl.z),
            Some(lvl.p.plunge_feed),
        );
    }
}

/// The entry arm's honorable failure (M8.1): the stated motion could
/// not be honored at this level for `why`, so the ladder substitutes
/// the axial dive at the disc's own floor room and says so in the
/// path's notes. A floor with no room either - an erosion that
/// vanished, a region past the triangulation cap - falls back to the
/// wall dive this ladder knew before Phase 8, still loudly.
fn substituted_dive(b: &mut PathBuilder, lvl: &LevelCtx, e: Point2, why: &str) {
    match dive_room(lvl.cen, 0.0) {
        Some(d) => {
            b.note(format!("entry: {why} - axial dive on the floor instead"));
            let at = b.at();
            if at.x != d.x || at.y != d.y {
                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, lvl.p.clearance_z), None);
            }
            b.push(
                MoveKind::Plunge,
                Point3::new(d.x, d.y, lvl.z),
                Some(lvl.p.plunge_feed),
            );
            dive_link(b, lvl, d, e);
        }
        None => {
            b.note(format!(
                "entry: {why} - the floor has no dive room either; dived at the pass start"
            ));
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lvl.p.clearance_z), None);
            b.push(
                MoveKind::Plunge,
                Point3::new(e.x, e.y, lvl.z),
                Some(lvl.p.plunge_feed),
            );
        }
    }
}

/// Feed-walk a pass's points at plane `z` (the first point is the
/// current position; `skip_first` avoids a zero move). Ring closure is
/// a property of the point DATA, not a motion added here: a waterline
/// loop arrives with its start point repeated at the end (each ring is
/// emitted closed by `waterline_passes`, and a chain of rings carries a
/// closing point per ring), so the walk's final feed lands each ring
/// back on its own start. Zigzag rows arrive open and stay open. An
/// earlier draft closed passes here by feeding the walk's last point
/// back to the first; for a lone ring that was a zero-length duplicate
/// of the data's own closing, but for a chained multi-ring pass it
/// dragged a diagonal across every intermediate ring - the boss crossing
/// bug. The law is: walk the data, add nothing.
pub(super) fn walk_pass(b: &mut PathBuilder, pass: &[Point2], z: f64, feed: f64, skip_first: bool) {
    for q in pass.iter().skip(if skip_first { 1 } else { 0 }) {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
    }
}
