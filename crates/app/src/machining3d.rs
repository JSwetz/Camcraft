//! Mesh-driven 3D pocketing (M2.1a): the level ladder meets the model.
//!
//! Provenance: Z-level pocketing is standard roughing practice, specced
//! from published CAM literature and general machining knowledge; the
//! region algebra is our Clipper2 wrapper and per-level sectioning our
//! marching-slice cutter (`geometry::slice`). The helical entry follows
//! RS-274's G2/G3-with-Z interpolation vocabulary.
//!
//! ## The level law (why every cut is legal by construction)
//!
//! The model is the FINAL part: inside the pocket footprint, material
//! below the model surface must stay; material above it must go. A
//! cutter standing at level z therefore cuts the footprint minus the
//! model's cross-section strictly above z:
//!
//! ```text
//! cut_target(z) = footprint ∖ slice(model, z + ε)
//! ```
//!
//! `slice(model, z+ε)` = columns whose solid reaches above the plane -
//! the per-level islands, computed, not picked. A column whose surface
//! T(x,y) ≤ z is fair game; the ladder then leaves a staircase of at
//! most `stepdown` above T on anything raised (the rest-material bullet,
//! M2.1's simulation work, is what reads that staircase later; a flat
//! floor at `bottom_z` comes out exact, the last level snapped there by
//! the same `cut_levels` law the 2.5-D pockets use). The ε probe means a
//! shelf within one lattice micron of the plane is cut tangentially -
//! quantization noise, never a silent gouge: the reach law verifies the
//! chords at their Z.
//!
//! Entries at a level's first pass: `Plunge` mirrors the 2-D pocket
//! precedent (an axial dive the flutes are built for); `Ramp` feeds the
//! diagonal from the previous level's plane, earned only when that
//! chord stays inside the previous level's cutter-center area - the
//! deepest cut is the most crowded one, so a chord legal there is
//! legal at every plane the descent crosses; `Helix` orbits a circle of
//! `helix_radius` (≤ tool radius, so successive turns overlap) about a
//! scanline midpoint whose room fits the widened disc at both planes,
//! then links along this level's floor to the first pass. Pass ends hug
//! the walls, so an orbit about one is never legal - the dive lives
//! mid-row instead. A diagonal or an orbit into a wall is a gouge, and
//! a fallback that costs a slower dive is never one. The first level
//! always plunges: its plane is the surface itself.

use crate::cl::{lattice, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{chord_clears, cut_levels, plan_passes, GenError, PocketStrategy};

/// How the cutter first engages material at a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Entry3D {
    #[default]
    /// Axial dive at the pass entry, the 2-D pocket motion.
    Plunge,
    /// One straight diagonal feed from the previous level's plane down
    /// to this one (first level: falls back to a plunge - there is no
    /// open floor to start a ramp from).
    Ramp,
    /// Helical interpolation: orbit while descending one stepdown (the
    /// widened arc law, see `cl.rs`). Air orbits above stock are never
    /// posted: the first level's dive is the surface itself.
    Helix,
}

/// Everything `pocket3d` needs, all numbers resolved (heights are the
/// operation layer's business, same seam as `machining2d`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pocket3DParams {
    pub tool_diameter: f64,
    pub stepover: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
    pub strategy: PocketStrategy,
    /// Between-ring hop plane within one level (same law as 2-D).
    pub retract_z: Option<f64>,
    pub entry: Entry3D,
    /// Helix centerline circle radius (mm); validated ≤ tool radius so
    /// successive turns overlap and the dive cuts a clean bore.
    pub helix_radius: f64,
}

impl Pocket3DParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.stepover,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.helix_radius,
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
        if self.stepover <= 0.0 || self.stepover > self.tool_diameter {
            return Err(GenError::Invalid(
                "stepover must be positive and within the tool diameter".into(),
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
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        if self.entry == Entry3D::Helix {
            let r = self.tool_diameter * 0.5;
            if self.helix_radius <= 0.0 || self.helix_radius > r {
                return Err(GenError::Invalid(
                    "helix radius must be positive and at most the tool radius".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Half-open slices sit exactly on vertices for tessellated steps; this
/// is the same lattice the weld uses, so a grazing plane nudges rather
/// than lies.
const EPS: f64 = 1e-6;

fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The model's cross-section strictly above `z`, nudge-retried when a
/// plane grazes a tessellated shelf (reading an errored slice as
/// no-island would be a silent gouge - the retry lands above the shelf
/// instead, failing safe). Public because it is the islands law itself:
/// the generator reads it to shape each level, the verify pass to prove
/// them, and the tests to pin the monotonicity the ladder assumes -
/// one law, three readers (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    let mut err = None;
    for k in 0..3 {
        let probe = z + f64::from(k + 1) * EPS;
        match slice::slice(facets, probe) {
            Ok(r) => return Ok(r),
            Err(e) => err = Some(e),
        }
    }
    Err(err.map_or(
        GenError::Invalid("cannot section the model".into()),
        from_slice,
    ))
}

/// The helix's dive point: the midpoint of the longest straight run in
/// the level plan that has room for the widened disc at both planes it
/// crosses (the leg regions, each the level's center eroded by the
/// helix radius plus a micron's insurance against buffer-of-buffer
/// quantization). A row's midpoint is the furthest from every wall a
/// planar pass offers; the scanline ends sit on the boundary by
/// construction. First choice is the longest run - the most room, and
/// the row a climb pass would most want to start mid-span on.
fn dive_point(passes: &[Vec<Point2>], leg_prev: &Region, leg_here: &Region) -> Option<Point2> {
    let mut best: Option<(f64, Point2)> = None;
    for pass in passes {
        for pair in pass.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 0.0 || best.is_some_and(|(l, _)| l >= len) {
                continue;
            }
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            if leg_prev.contains(mid) && leg_here.contains(mid) {
                best = Some((len, mid));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// The per-level cutter plan and its walk, level after level - the 3-D
/// sibling of `machining2d::pocket`'s run, with the passes recomputed
/// from the model at every z (that is the whole difference: 2-D knows
/// its plan once, 3-D's plan IS the ladder).
pub fn pocket3d(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    if facets.is_empty() {
        return Err(GenError::Empty("no model facets to section".into()));
    }
    if footprint.is_empty() {
        return Err(GenError::Empty("empty pocket footprint".into()));
    }
    let r = p.tool_diameter * 0.5;
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown);
    let hop_z = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let close_rings = matches!(p.strategy, PocketStrategy::Waterline);
    // The last level that actually cut: its plane and center area, the
    // most crowded the entry motions of the next level can meet (a
    // deeper level's target only grows). None until one has cut.
    let mut prev_state: Option<(f64, Region)> = None;
    for (li, &z) in zs.iter().enumerate() {
        // The level law, in two region ops: islands from the model, the
        // cutter-center area by one erosion.
        let islands = material_above(facets, z)?;
        let target = footprint.difference(&islands)?;
        if target.is_empty() {
            continue; // model material across the whole footprint here
        }
        let center = target.offset(-(r + p.stock_to_leave))?;
        if center.is_empty() {
            continue; // narrower than the tool at this level: the
                      // staircase (or a wall this tool cannot enter) -
                      // rest material's later problem, never a lie now
        }
        let passes = plan_passes(&center, p.stepover, p.climb, p.strategy)?;
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
            Entry3D::Ramp if li > 0 => {
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
            Entry3D::Helix if li > 0 => {
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
                    if chord_clears(&center, d, entry) {
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
        walk_pass(&mut b, first_pass, z, p.feed, /*skip_first=*/ true, close_rings);
        // Remaining passes: connect as far as the floor allows.
        for pass in &passes[1..] {
            let Some(&next_entry) = pass.first() else {
                continue;
            };
            let prev = b.at();
            if chord_clears(&center, Point2::new(prev.x, prev.y), next_entry) {
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
            walk_pass(&mut b, pass, z, p.feed, /*skip_first=*/ true, close_rings);
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
/// current position; `skip_first` avoids a zero move). `close_rings`
/// is the strategy's own law, not a guess from the point list: a
/// waterline pass is a spiral of concentric rings whose last point is
/// the innermost ring's start, so it is closed by a feed back to its
/// first - that chord runs the 45° corner line and touches every ring
/// only at a corner, which no law counts as a crossing. A zigzag pass
/// is an open scan: closing it would drag a diagonal across every row
/// of the level, and nothing about the point list tells the two apart
/// (both end where they are, first != last either way).
fn walk_pass(
    b: &mut PathBuilder,
    pass: &[Point2],
    z: f64,
    feed: f64,
    skip_first: bool,
    close_rings: bool,
) {
    for q in pass.iter().skip(if skip_first { 1 } else { 0 }) {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
    }
    if close_rings && let Some(&a) = pass.first() {
        b.push(MoveKind::Feed, Point3::new(a.x, a.y, z), Some(feed));
    }
}

/// One helical dive between two planes: at least one full turn (split
/// into half-circle arcs so no move is a zero-sweep full circle), Z
/// descending linearly with the orbit. The builder must stand at `xy`
/// on plane `z0` (the caller's rapid to the dive point); an even
/// number of half turns ends back on `xy` at `z1` by construction -
/// each turn is two 180° arcs whose XY net is zero.
fn helix_down(b: &mut PathBuilder, xy: Point2, z0: f64, z1: f64, rh: f64, feed: f64) {
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
