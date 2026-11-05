//! Split from `machining3d.rs` by M5.0.1; structure only.

use super::params::{Entry3D, Pocket3DParams};
use super::rest::material_above;
use super::shell::Field;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::{chord_hits_plan, dive_room, helix_down};
use crate::entry_arc::{arc_lead, pts_dir, push_arc_lead};
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{
    chord_clears, plan_passes, GenError, ARC_FIT_TOL,
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
///
/// `shell` (M2.5 F) leaves the region law exactly as the plain ladder
/// has it - the level's islands, XY-eroded at `r + leave`, the only
/// XY set the entry arms and the dive superset law ever read - and
/// lifts every CUT VERTEX off the level plane onto the model's offset
/// shell: `min(plane, roof(q, r + ℓ) + leave)`, floored at the plane's
/// own stepdown budget and the pocket's drawn floor, so no level
/// bites deeper axially than a plain ladder's dive already does and
/// none descends past `bottom_z`. ℓ, the chord cap `feed_leg` subdivides
/// every XY leg to, is the tool radius: a vertex decides within one
/// diameter, and because both of a capped leg's endpoints decide
/// within `r + ℓ` of themselves, no interpolation between the two
/// bounds can slip under a part point the leg's tube covers - the
/// rising-leg gouge an uncapped leg is open to (see
/// `machining3d::shell`). A ring XY inside the eroded center has no
/// part above the plane within its disc at all, so a lifted vertex
/// bites part material nowhere, exactly as the flat one never did.
/// Where the surface is steep the roof outgrows the plane and the
/// vertex stays AT it - the waterline the plain ladder always cut
/// there; where the surface is shallow it follows the surface at the
/// leave standoff, and the staircase's pyramid between plane and
/// surface is gone from the single pass. Rapids and hops never lift:
/// they cut nothing. See `machining3d::entry::pocket3d_offset`.
// One slot per law the run threads: the model, the plan, the planes, the
// rest mask, the program mask, the tip source (M2.5 F's `shell`), the
// start - flat reads for one walk, like the march kernel's ten.
#[allow(clippy::too_many_arguments)]
pub(super) fn ladder(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    zs: &[f64],
    rest: Option<&Dexel>,
    mask: Option<&Dexel>,
    start: Point3,
    shell: bool,
) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    let hop_z = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    // The shell's roof query boxes the model once per call; the plain
    // ladder never pays for it and never reads it (every leg then
    // lands at its plane, exactly as always).
    let field = match shell {
        true => Some(Field::new(facets).map_err(GenError::Invalid)?),
        false => None,
    };

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
        // The center's erosion: the XY law, both rungs of the ladder -
        // the shell lifts the legs' tips, never this region (the dive
        // superset law reads exactly this set at exactly this plane).
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
            field: field.as_ref(),
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
                    if lvl.field.is_none() {
                        b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                    } else {
                        // The shell's ramp keeps the plain ladder's
                        // discipline: it rides strictly above the
                        // plane until one chord short of its
                        // destination, and only then descends to the
                        // destination's tip. A diagonal that sank to
                        // a floor tip early would meet this level's
                        // own (tilted) rings at shared depth along
                        // the way - the Crossing the verifier names -
                        // and would interpolate under columns whose
                        // surface only its START's disc saw. In the
                        // last chord every tube point sits within
                        // `r + ℓ` of the destination, whose tip the
                        // decision disc has earned; above it the
                        // move is at or above the plane, where the
                        // chord's center membership is the whole
                        // proof, exactly as the plain law states.
                        let cap = p.tool_diameter * 0.5;
                        let (dx, dy) = (entry.x - leg.x, entry.y - leg.y);
                        let l = (dx * dx + dy * dy).sqrt();
                        let t = if l > cap { 1.0 - cap / l } else { 0.0 };
                        if t > 0.0 {
                            let q1 = Point2::new(leg.x + dx * t, leg.y + dy * t);
                            b.push(MoveKind::Feed, Point3::new(q1.x, q1.y, z), Some(p.feed));
                        }
                        feed_leg(&mut b, &lvl, entry, 0.0);
                    }
                } else {
                    substituted_dive(&mut b, &lvl, entry, "the ramp's chord is blocked");
                }
            }
            // The circular ramp (M11.1): the ramp's chord law on the
            // lead chord behind the entry, the fan posted by
            // `entry_arc`. A shell lifts the destination off the plane
            // and no proof here follows the fan onto a lifted tip, so
            // it substitutes, said.
            Entry3D::Arc if rest.is_none() => {
                let lead = match lvl.field {
                    Some(_) => Err("the arc entry is not proved onto a lifted shell"),
                    None => pts_dir(entry, first_pass)
                        .ok_or("the first cut has no direction to be tangent to")
                        .and_then(|d| {
                            let ok = |s, q| chord_clears(&center, s, q);
                            arc_lead(entry, d, prev_z, z, p.tool_diameter, ARC_FIT_TOL, ok)
                        }),
                };
                match lead {
                    Ok(l) => push_arc_lead(&mut b, &l, p.clearance_z, prev_z, p.feed),
                    Err(why) => substituted_dive(&mut b, &lvl, entry, why),
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
                // The margin certifies the ORBIT's disc, so the found
                // point is taken as the orbit's CENTER and the dive
                // starts at the anchor `rh` west of it - `helix_down`
                // orbits the circle whose near side is the anchor.
                // One radius of center-room is exact: the tube spans
                // `r + rh` from the certified center, and the anchor
                // is inside the certified disc by the triangle
                // inequality. Asking 1 rh OF THE ANCHOR certified
                // half the tube (M2.5 F's dome gouged that way);
                // asking 2 of the anchor over-refused the straight
                // channel (M8.2's slot) the orbit always proved.
                let dive = dive_room(&center, p.helix_radius + 1e-3);
                if let Some(c) = dive {
                    // A landed bore is not yet a Crossing-clean one.
                    // The plain ladder never asks: its legs sit exactly
                    // on their planes, so a bore crossing one shares
                    // only the crossing depth - a measure-zero graze,
                    // the tolerance the ramp law already reads. On the
                    // shell a leg can stand lifted BELOW the plane it
                    // cut at (the roof dipped under its disc), and an
                    // arc chord crossing it then shares a positive Z
                    // interval - the Crossing the verifier names. Only
                    // the previous level's legs can reach into the
                    // bore's band: a leg at any earlier plane floors
                    // at `pz` or above, and this level's own legs are
                    // cut under the bore's last depth, so checking the
                    // one plane is complete. A first level has no cut
                    // legs at all.
                    let crosses = match (lvl.field, prev_state) {
                        (Some(_), Some((pz, _))) => bore_meets_lifted_leg(&lvl, pz, c),
                        _ => false,
                    };
                    if crosses {
                        // Same honorable failure as a disc with no
                        // room: the dive substitutes, the level walks.
                        substituted_dive(
                            &mut b,
                            &lvl,
                            entry,
                            "the shell lifts legs the bore would cross",
                        );
                    } else {
                        let d = Point2::new(c.x - p.helix_radius, c.y);
                        if leg.x != d.x || leg.y != d.y {
                            b.push(MoveKind::Rapid, Point3::new(d.x, d.y, p.clearance_z), None);
                        }
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                        // The orbit's tube is the widened disc about
                        // `d`, so the whole helix lands at the one tip
                        // that decision disc earns (no chord reaches
                        // beyond it: the corridor already carries
                        // `2 * helix_radius`, the tube's full span
                        // about the center).
                        let z_hel = lvl.tip(d, p.helix_radius * 2.0);
                        helix_down(&mut b, d, prev_z, z_hel, p.helix_radius, p.feed);
                        dive_link(&mut b, &lvl, d, entry);
                    }
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
                        // A vertical dive is exact at corridor r - no
                        // chord to cap - so it lands on its own tip.
                        let z_d = lvl.tip(d, 0.0);
                        b.push(MoveKind::Plunge, Point3::new(d.x, d.y, z_d), Some(p.plunge_feed));
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
                    Point3::new(entry.x, entry.y, lvl.tip(entry, 0.0)),
                    Some(p.plunge_feed),
                );
            }
        }
        walk_pass(&mut b, first_pass, &lvl, /*skip_first=*/ true);
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
                // test just proved) - on the shell, vertex by vertex.
                feed_leg(&mut b, &lvl, next_entry, 0.0);
            } else {
                b.push(MoveKind::Rapid, Point3::new(prev.x, prev.y, hop_z), None);
                b.push(MoveKind::Rapid, Point3::new(next_entry.x, next_entry.y, hop_z), None);
                b.push(
                    MoveKind::Plunge,
                    Point3::new(next_entry.x, next_entry.y, lvl.tip(next_entry, 0.0)),
                    Some(p.plunge_feed),
                );
            }
            walk_pass(&mut b, pass, &lvl, /*skip_first=*/ true);
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
    /// The offset shell's memory of the model; None on the plain
    /// ladder, where every tip is the plane itself.
    field: Option<&'a Field>,
}

impl LevelCtx<'_> {
    /// The plane an XY cut vertex of this level lands on: the shell
    /// tip at `q` over a decision disc of `r + ℓ` (ℓ = the tool
    /// radius, the chord cap `feed_leg` subdivides to - the disc is
    /// one tool diameter wide), floored at the level's own stepdown
    /// budget and at the pocket's drawn floor - the shell's early
    /// descent stops where the program stops, never deeper; or the
    /// plane itself with no shell mounted, the plain ladder's value
    /// byte for byte. `extra` widens the disc for a vertex whose
    /// sweep is wider than the chords reaching it (a helix orbit, by
    /// its radius).
    fn tip(&self, q: Point2, extra: f64) -> f64 {
        match self.field {
            None => self.z,
            Some(f) => {
                let floor_z = (self.z - self.p.stepdown).max(self.p.bottom_z);
                f.tip(q, self.z, self.p.tool_diameter + extra, self.p.stock_to_leave, floor_z)
            }
        }
    }
}

/// Feed from the builder's current point to `to`: with no shell, the
/// plain ladder's single move at the level plane; with the shell, the
/// leg split into chords of at most ℓ with every vertex landing on
/// its own tip - the cap is what makes the endpoint tips bound the
/// interpolation between them (`machining3d::shell`'s rising-leg
/// gouge). `extra` widens only the LAST vertex's decision disc.
fn feed_leg(b: &mut PathBuilder, lvl: &LevelCtx, to: Point2, extra: f64) {
    let feed = lvl.p.feed;
    let z = lvl.z;
    if lvl.field.is_none() {
        b.push(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed));
        return;
    }
    let at = b.at();
    let cap = lvl.p.tool_diameter * 0.5;
    let n = ((to.x - at.x).hypot(to.y - at.y) / cap).ceil();
    // A leg over a thousand tool diameters long is nonsense
    // geometry, not a reason to hang: it rides the plane, exactly as
    // the plain ladder's leg would - and the plane is safe by the
    // level law. (The comparison also routes a NaN distance here.)
    if n > 1024.0 || !n.is_finite() {
        b.push(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed));
        return;
    }
    let n = (n as usize).max(1);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let q = Point2::new(at.x + (to.x - at.x) * t, at.y + (to.y - at.y) * t);
        let zq = lvl.tip(q, if i == n { extra } else { 0.0 });
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, zq), Some(feed));
    }
}

/// The link from a dive that just finished at `d` up on plane `z` to
/// the pass start `e`: feed it when the chord is proven (inside this
/// level's center, crossing none of the level's planned paths), fly it
/// over - hop, traverse, dive - when it is not. Shared by every entry
/// arm so the law reads once (the 3-D seat of the proof; the 2-D
/// ladder runs the same law in `machining2d::levels`).
fn dive_link(b: &mut PathBuilder, lvl: &LevelCtx, d: Point2, e: Point2) {
    if chord_clears(lvl.cen, d, e) && !chord_hits_plan(d, e, lvl.passes) {
        feed_leg(b, lvl, e, 0.0);
    } else {
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, lvl.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lvl.hop_z), None);
        b.push(
            MoveKind::Plunge,
            Point3::new(e.x, e.y, lvl.tip(e, 0.0)),
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
                Point3::new(d.x, d.y, lvl.tip(d, 0.0)),
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
                Point3::new(e.x, e.y, lvl.tip(e, 0.0)),
                Some(lvl.p.plunge_feed),
            );
        }
    }
}

/// Does the helix bore about orbit center `c` cross a leg the shell
/// left lifted under the previous plane `pz`? A leg's XY is level
/// invariant - one tiling walked at every plane - so the candidates
/// are the previous level's own legs, sampled with the cap and tip
/// that level's walker posted (the `extra = 0` disc: the narrowest
/// read, the conservative one). Crossing is the verifier's law,
/// exactly: XY reaches the bore's circle, and the leg's Z interval
/// overlaps the bore's band `(z, pz)` with positive measure - which
/// for legs floored at `z` reads as `min tip < pz` and `max tip > z`.
/// Legs at any earlier plane floor at `pz` or above and cannot
/// overlap the band at all; this level's legs are cut under the
/// bore's last depth; so the one plane is the complete ask.
fn bore_meets_lifted_leg(lvl: &LevelCtx, pz: f64, c: Point2) -> bool {
    let Some(f) = lvl.field else {
        return false;
    };
    let p = lvl.p;
    let (rh, cap) = (p.helix_radius, (p.tool_diameter * 0.5).max(f64::EPSILON));
    let floor = (pz - p.stepdown).max(p.bottom_z);
    let z = lvl.z;
    lvl.passes.iter().any(|pass| {
        pass.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            if seg_reach(a, b, c) >= rh - 1e-9 {
                return false; // stays clear of the bore's circle
            }
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let n = (dx.hypot(dy) / cap).ceil();
            if !n.is_finite() || n > 1024.0 {
                return true; // nonsense geometry: refuse loudly
            }
            let n = (n as usize).max(1);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let q = Point2::new(a.x + dx * t, a.y + dy * t);
                let zq = f.tip(q, pz, p.tool_diameter, p.stock_to_leave, floor);
                lo = lo.min(zq);
                hi = hi.max(zq);
            }
            lo < pz - 1e-9 && hi > z + 1e-9
        })
    })
}

/// The closest an XY segment comes to a point: 0 when it straddles
/// or contains it.
fn seg_reach(a: Point2, b: Point2, c: Point2) -> f64 {
    let (ux, uy) = (b.x - a.x, b.y - a.y);
    let uu = ux * ux + uy * uy;
    let t = if uu > 0.0 {
        (((c.x - a.x) * ux + (c.y - a.y) * uy) / uu).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.x + ux * t - c.x).hypot(a.y + uy * t - c.y)
}

/// Feed-walk a pass's points at the level (plane `z`, or the shell
/// tip of every chord between them; the first point is the current
/// position; `skip_first` avoids a zero move). Ring closure is
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
fn walk_pass(b: &mut PathBuilder, pass: &[Point2], lvl: &LevelCtx, skip_first: bool) {
    for q in pass.iter().skip(if skip_first { 1 } else { 0 }) {
        feed_leg(b, lvl, *q, 0.0);
    }
}
