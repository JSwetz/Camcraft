//! Split from `machining3d.rs` by M5.0.1; structure only.

use super::params::{Entry3D, Pocket3DParams};
use super::rest::{chord_hits_plan, dive_point, material_above};

use crate::cl::{lattice, MoveKind, PathBuilder, Toolpath};
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
    for (li, &z) in zs.iter().enumerate() {
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
        let prev_z = prev_state.as_ref().map_or(z, |(q, _)| *q);
        match p.entry {
            // Ramp and helix are the coarse ladder's motions: a rest
            // pass starts inside planes that are already cut, where a
            // diagonal would cross floors the coarse pass owns. The
            // vertical dive needs no such proof (the nesting law
            // carries it), so the rest ladder always takes the plunge
            // arm below.
            Entry3D::Ramp if li > 0 && rest.is_none() => {
                // The diagonal's legality is asked of the PREVIOUS
                // level's center: it is the deepest cut so far, its
                // islands stand above everything this descent passes,
                // and every shallower plane's target is a superset -
                // one chord test covers the whole interpolated move.
                let open = prev_state
                    .as_ref()
                    .is_some_and(|(_, pc)| chord_clears(pc, leg, entry));
                if open {
                    b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, prev_z), None);
                    b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                } else {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                    b.push(
                        MoveKind::Plunge,
                        Point3::new(entry.x, entry.y, z),
                        Some(p.plunge_feed),
                    );
                }
            }
            Entry3D::Helix if li > 0 && rest.is_none() => {
                // Pass ends sit on the walls by construction, so the
                // orbit is never legal about one: the dive lives at the
                // midpoint of the level's longest row, where the
                // widened disc (tool radius + helix radius) has room.
                // Legality is the same superset law as the ramp's, at
                // the previous center; the link to the first pass then
                // runs along this level's own floor.
                let leg_prev = prev_state
                    .as_ref()
                    .map(|(_, pc)| pc.offset(-(p.helix_radius + 1e-3)))
                    .transpose()?;
                let leg_here = center.offset(-(p.helix_radius + 1e-3))?;
                let dive = leg_prev
                    .as_ref()
                    .and_then(|lp| dive_point(&passes, lp, &leg_here));
                if let Some(d) = dive {
                    if leg.x != d.x || leg.y != d.y {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, p.clearance_z), None);
                    }
                    b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                    helix_down(&mut b, d, prev_z, z, p.helix_radius, p.feed);
                    if chord_clears(&center, d, entry) && !chord_hits_plan(d, entry, &passes) {
                        b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(p.feed));
                    } else {
                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, hop_z), None);
                        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, hop_z), None);
                        b.push(
                            MoveKind::Plunge,
                            Point3::new(entry.x, entry.y, z),
                            Some(p.plunge_feed),
                        );
                    }
                } else {
                    b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, p.clearance_z), None);
                    b.push(
                        MoveKind::Plunge,
                        Point3::new(entry.x, entry.y, z),
                        Some(p.plunge_feed),
                    );
                }
            }
            // The first level's plane IS the stock surface: there is
            // nothing to ramp or spiral into, and a dive there cuts
            // air. One honest Plunge move says so.
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

/// One helical dive between two planes: at least one full turn (split
/// into half-circle arcs so no move is a zero-sweep full circle), Z
/// descending linearly with the orbit. The builder must stand at `xy`
/// on plane `z0` (the caller's rapid to the dive point); an even
/// number of half turns ends back on `xy` at `z1` by construction -
/// each turn is two 180° arcs whose XY net is zero.
pub(super) fn helix_down(b: &mut PathBuilder, xy: Point2, z0: f64, z1: f64, rh: f64, feed: f64) {
    let span = z0 - z1;
    if span <= 0.0 || rh <= 0.0 {
        return;
    }
    // At least one turn per dive; one more when the span is deeper than
    // the circle's own reach, so the bore's slope stays inside the tool
    // diameter's width.
    let turns = (span / (rh * 2.0)).ceil().max(1.0);
    let half_turns = ((turns * 2.0).ceil() as u32).max(2);
    // Orbit about the circle whose near side is the entry point: center
    // offset +rh in X. Endpoints sit lattice-snapped (the arc law wants
    // a 1 um center; the builder keeps the chord endpoints exact).
    let center = Point2::new(lattice(xy.x + rh), lattice(xy.y));
    for h in 0..half_turns {
        let frac = f64::from(h + 1) / f64::from(half_turns);
        let tz = z0 - span * frac;
        // Alternating far/near point of the circle; even counts close on
        // xy at the last leg.
        let px = if h % 2 == 0 { xy.x + 2.0 * rh } else { xy.x };
        // ccw: right -> left over the +Y side.
        b.push_arc(
            MoveKind::Feed,
            Point3::new(lattice(px), lattice(xy.y), lattice(tz)),
            Some(feed),
            center,
            true,
        );
    }
}
