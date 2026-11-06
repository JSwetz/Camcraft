//! The planar pocket's HSM adaptive door (M11.4 C): where the entry
//! stands the tool, which bore it leaves, and one adaptive plan per
//! connected piece of the center area. The plan itself is `adaptive`
//! (provenance there). For a helix or plunge entry the plan chooses the
//! spot (round 2): the morph spiral wants the entry in the area's
//! kernel near its centroid (`adaptive::preferred_seed`), the fronts at
//! its widest point - and the ladder's entry law
//! (`levels::run_level_walks`, consumed, not changed) is handed that
//! spot as its certified room, so the plan starts exactly where the
//! dive ends. A helix too wide for the area is narrowed to the widest
//! it holds, and said.

use crate::entry::{Entry3D, dive_room};
use crate::geo2d::{Loop, Point2, Region};

use super::adaptive::{AdaptiveSpec, boundary_dist, corner_radius, first_crossing, plan, preferred_seed, widest_point};
use super::face::GenError;
use super::pocket::{PocketParams, PocketStrategy, plan_passes};

/// Where the plan put the entry: the spot, the radius of the disc
/// about it that is inside the center area, the helix radius the
/// ladder must orbit, and the triangle the ladder's `dive_room` finds
/// the spot in (its centroid, by construction).
#[derive(Clone, Debug)]
pub(super) struct EntryRoom {
    pub seed: Point2,
    pub clear: f64,
    pub helix_r: f64,
    pub tess: Region,
}

/// The Adaptive pocket's passes: the HSM plan, or - when it would leave
/// more than [`HSM_REST_MAX`] of the reachable stock uncut within twice
/// the load - the Offset rings, named in the notes (an incomplete plan
/// is never posted as if it were the clearing). Returns the strategy
/// that actually planned them and, for the HSM plan, its entry.
pub(super) fn adaptive_passes(
    center: &Region,
    p: &PocketParams,
) -> Result<(Walks, Vec<String>, PocketStrategy, Option<EntryRoom>), GenError> {
    let (passes, notes, rest, room) = hsm_passes(center, p)?;
    // A plan that would cross itself is never posted (the verifier's
    // crossing law, asked of the plan first).
    let cross = first_crossing(&passes);
    if rest <= HSM_REST_MAX && cross.is_none() {
        return Ok((passes, notes, PocketStrategy::Adaptive, room));
    }
    let s = PocketStrategy::OffsetRings;
    let note = match cross {
        Some(x) => format!(
            "adaptive: the HSM plan for this area (engagement fronts: it is no star cell from the entry) crosses itself at ({:.2}, {:.2}) - Offset rings cut this pocket instead",
            x.x, x.y
        ),
        None => format!(
            "adaptive: the HSM plan left {:.0}% of the area it could not cut within twice the load - Offset rings cut this pocket instead",
            rest * 100.0
        ),
    };
    let step = crate::load::load_step(p.tool_diameter, p.optimal_load);
    Ok((plan_passes(center, step, p.climb, s)?, vec![note], s, None))
}

/// Cutter-center walks, one per pass.
type Walks = Vec<Vec<Point2>>;

/// The contact-angle margin the planner holds (fraction of the
/// optimal load; negative = under it): the plan is read on the
/// measuring stock's own law, and the margin covers the grid phase and
/// the arc fit between the plan and the posted moves.
pub const PLAN_TOL: f64 = 0.05;

/// The share of the reachable stock an HSM plan may leave standing (grid
/// crumbs) before the pocket falls back to Offset rings.
pub const HSM_REST_MAX: f64 = 0.02;

/// The lead/link arc radius as a fraction of the tool radius.
pub const SMOOTH_PER_R: f64 = 0.5;

/// The connected pieces of `center`: each outer loop with the holes it
/// carries.
fn components(center: &Region) -> Result<Vec<Region>, GenError> {
    let (outers, holes): (Vec<&Loop>, Vec<&Loop>) = center.loops().iter().partition(|l| l.area() > 0.0);
    let mut out = Vec::with_capacity(outers.len());
    for o in &outers {
        let mut loops = vec![(*o).clone()];
        for h in &holes {
            if let Some(&p) = h.points().first()
                && Region::new(vec![(*o).clone()])?.contains(p)
            {
                loops.push((*h).clone());
            }
        }
        out.push(Region::new(loops)?);
    }
    Ok(out)
}

/// The triangle whose centroid is `c` and whose inscribed circle has
/// radius `inr`: `dive_room` finds exactly `c` in it.
fn triangle_about(c: Point2, inr: f64) -> Result<Region, GenError> {
    let pts = (0..3)
        .map(|k| {
            let a = std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::TAU / 3.0;
            Point2::new(c.x + 2.0 * inr * a.cos(), c.y + 2.0 * inr * a.sin())
        })
        .collect();
    Ok(Region::new(vec![Loop::new(pts)?])?)
}

/// The entry the plan chooses for a helix or plunge: the morph spiral's
/// kernel spot when the area is one star cell, else the widest point;
/// the helix narrowed to fit. `None` leaves the ladder's own choice.
fn choose_entry(center: &Region, p: &PocketParams, notes: &mut Vec<String>) -> Result<Option<EntryRoom>, GenError> {
    let helix = p.entry == Entry3D::Helix;
    if !(helix || p.entry == Entry3D::Plunge) {
        return Ok(None);
    }
    let r = p.tool_diameter * 0.5;
    let hr0 = if helix { p.helix_radius } else { 0.0 };
    let pick = preferred_seed(center, corner_radius(center, r), hr0 + 0.02)?;
    let Some(cand) = pick.or_else(|| widest_point(center).map(|w| w.0)) else { return Ok(None) };
    let clear = boundary_dist(center, cand);
    let hr = if helix { hr0.min(clear - 0.01) } else { 0.0 };
    if helix && hr < 1e-3 {
        return Ok(None);
    }
    let tess = triangle_about(cand, hr + 0.01)?;
    let Some(seed) = dive_room(&tess, if helix { hr + 1e-3 } else { 0.0 }) else { return Ok(None) };
    if helix && hr < hr0 {
        notes.push(format!(
            "entry: a helix of radius {hr0:.3} does not fit the area - radius {hr:.3}, the widest it holds, used"
        ));
    }
    Ok(Some(EntryRoom { seed, clear: boundary_dist(center, seed) - 1e-6, helix_r: hr, tess }))
}

/// Cutter-center walks for an Adaptive pocket, the plan's notes, the
/// share of the reachable stock it leaves standing and its entry.
fn hsm_passes(center: &Region, p: &PocketParams) -> Result<(Walks, Vec<String>, f64, Option<EntryRoom>), GenError> {
    let r = p.tool_diameter * 0.5;
    let mut notes = Vec::new();
    let mut passes = Vec::new();
    let (mut rest, mut area) = (0.0, 0.0);
    let room = choose_entry(center, p, &mut notes)?;
    // Where the entry law stands the tool (`run_level_walks`): the room
    // the plan chose, or the ladder's own - a helix orbits the widest
    // room's center and ends on its west side; every other arm dives
    // where the disc fits.
    let (seed, start, bore, helix_r) = match &room {
        Some(e) => (e.seed, Point2::new(e.seed.x - e.helix_r, e.seed.y), r + e.helix_r, e.helix_r),
        None => {
            let helix = (p.entry == Entry3D::Helix).then(|| dive_room(center, p.helix_radius + 1e-3)).flatten();
            match helix {
                Some(c) => (c, Point2::new(c.x - p.helix_radius, c.y), r + p.helix_radius, p.helix_radius),
                None => {
                    let Some(d) = dive_room(center, 0.0) else {
                        return Err(GenError::Invalid("adaptive: the center area has no room for an entry".into()));
                    };
                    (d, d, r, 0.0)
                }
            }
        }
    };
    let mut comps = components(center)?;
    // The entry's own piece first.
    comps.sort_by_key(|c| !c.contains(seed));
    let first = AdaptiveSpec {
        r,
        load: p.optimal_load,
        tol: PLAN_TOL,
        climb: p.climb,
        seed,
        start,
        bore,
        helix_r,
        smooth: r * SMOOTH_PER_R,
    };
    for (i, comp) in comps.iter().enumerate() {
        let reach = comp.offset(r)?;
        let spec = if i == 0 {
            first
        } else {
            let Some(d) = dive_room(comp, 0.0) else {
                notes.push("adaptive: a piece of the area has no entry room - left uncut".into());
                continue;
            };
            notes.push(format!(
                "adaptive: a separate piece of the area is entered by an axial dive at ({:.2}, {:.2})",
                d.x, d.y
            ));
            AdaptiveSpec { seed: d, start: d, bore: r, helix_r: 0.0, ..first }
        };
        let pl = plan(&reach, comp, &spec)?;
        rest += pl.rest;
        area += reach.area();
        passes.extend(pl.passes);
        notes.extend(pl.notes);
    }
    Ok((passes, notes, if area > 0.0 { rest / area } else { 0.0 }, room))
}
