//! The 3-D rough's HSM adaptive levels (M11.4 owed "Pocket3D port of
//! the same planner"): under [`PocketStrategy::Adaptive`] every level
//! whose target is a closed planar area is planned by the planar HSM
//! planner (`machining2d::adaptive`, through `pocket_hsm`) - the spiral
//! out of the entry bore, the growth revolutions and ring bands, the
//! microlift returns, the hop budget, the 5 degree kink law and the
//! proved arc output - exactly as the 2D Adaptive pocket plans its one
//! area, at the same load (ae/D). The ladder keeps everything else: the
//! level law and its computed islands (`rest.rs`), the rest-material
//! mask, the entry and link arms (`descent.rs`) - the plan only tells
//! the entry where it bored (the spot the first pass starts on and the
//! helix radius it fits there, `LevelPlan::params`) - and the verify
//! that follows.
//!
//! A level the planner refuses by name (an island or channel area past
//! its hop or rest budget, a plan that would cross itself) is cut by
//! the Offset rings at the load step and SAID per level, naming its Z;
//! the rest pass and the offset shells keep the rings too, said once:
//! their proofs are the planes' (a rest target is the simulation's
//! crumbs, entered by plunges; a shell lifts every vertex off the plane
//! the microlift returns were proved on).
//!
//! Provenance: `machining2d::adaptive`'s (Stori & Wright 2000; Held &
//! Spielberger 2009; Otkur & Lazoglu 2007) and the ladder's own; no
//! vendor material consulted.

use super::params::Pocket3DParams;

use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::{
    ARC_FIT_TOL, GenError, PocketParams, PocketStrategy, ProofTally, Return, Seg, adaptive_passes, crossing_free,
    fit_proved, microlift_link, plan_passes, push_segs,
};

/// Said once per path where the rest pass runs under Adaptive.
pub const REST_RINGS: &str =
    "adaptive: the rest pass cuts Offset rings at the load step - its targets are what the simulation left, entered by plunges";

/// Said once per path where the offset shells run under Adaptive.
pub const SHELL_RINGS: &str =
    "adaptive: the offset shells cut Offset rings at the load step - the HSM plan's lifted returns are proved on the plane, not on the shell";

/// One level's plan: its cutter-center walks, the microlift return each
/// pass after the first is reached by (`None`: the ladder's pass link),
/// the proved arc walks (`None`: the points), and the helix radius the
/// level's entry orbits.
pub(super) struct LevelPlan {
    pub passes: Vec<Vec<Point2>>,
    pub joins: Vec<Option<Return>>,
    pub arcs: Option<Vec<Vec<Seg>>>,
    pub helix_r: f64,
    /// The HSM plan planned this level: a pass it reached by no microlift
    /// return is reached by the retract hop (the plan's own hop budget),
    /// never by a keep-down chord at depth - the chord would be legal in
    /// the center area yet slot through stock the plan has not cleared.
    pub hsm: bool,
}

impl LevelPlan {
    fn rings(passes: Vec<Vec<Point2>>, helix_r: f64) -> Self {
        Self { passes, joins: Vec::new(), arcs: None, helix_r, hsm: false }
    }

    /// The level's params: the entry's helix narrowed to the bore the
    /// plan spirals out of (the 2-D pocket's `Motion::with_entry`).
    pub fn params(&self, p: &Pocket3DParams) -> Pocket3DParams {
        Pocket3DParams { helix_radius: self.helix_r, ..*p }
    }
}

/// The level's passes at plane `z` over the cutter-center area `center`:
/// the strategy's tiling, or under Adaptive the HSM plan - with the
/// planner's notes on the builder.
pub(super) fn plan_level(
    b: &mut PathBuilder,
    center: &Region,
    p: &Pocket3DParams,
    z: f64,
    hop_z: f64,
    rest: bool,
    shell: bool,
) -> Result<LevelPlan, GenError> {
    let step = p.tiling_step();
    if p.strategy != PocketStrategy::Adaptive {
        return Ok(LevelPlan::rings(plan_passes(center, step, p.climb, p.strategy)?, p.helix_radius));
    }
    if rest || shell {
        b.note_once(if rest { REST_RINGS } else { SHELL_RINGS });
        let passes = plan_passes(center, step, p.climb, PocketStrategy::OffsetRings)?;
        return Ok(LevelPlan::rings(passes, p.helix_radius));
    }
    // One level: the planner's budget counts its moves once.
    let pp = PocketParams {
        tool_diameter: p.tool_diameter,
        stepover: p.stepover,
        stock_to_leave: p.stock_to_leave,
        clearance_z: p.clearance_z,
        top_z: z,
        bottom_z: z,
        stepdown: p.stepdown,
        feed: p.feed,
        plunge_feed: p.plunge_feed,
        climb: p.climb,
        strategy: PocketStrategy::Adaptive,
        retract_z: p.retract_z,
        entry: p.entry,
        helix_radius: p.helix_radius,
        optimal_load: p.optimal_load,
        blend_turns: false,
        links: p.links,
    };
    let what = format!("the level at Z {z:.3}");
    let (passes, notes, s, room, joins) = adaptive_passes(center, &pp, &what)?;
    for n in notes {
        b.note(n);
    }
    if s != PocketStrategy::Adaptive {
        return Ok(LevelPlan::rings(passes, p.helix_radius));
    }
    let helix_r = room.as_ref().map_or(p.helix_radius, |e| e.helix_r);
    // The arc output (the 2-D pocket's law): fitted at the ring budget,
    // every arc proved inside the center area and against the kink law,
    // then the crossing law on the level's walks as they will be posted
    // - the microlift returns and the pass links between them.
    let mut proof = ProofTally::default();
    let runs: Vec<Vec<Seg>> = passes.iter().map(|w| fit_proved(w, ARC_FIT_TOL * 0.25, center, &mut proof)).collect();
    let start = passes.first().and_then(|w| w.first()).map_or(Point3::new(0.0, 0.0, hop_z), |e| Point3::new(e.x, e.y, hop_z));
    let (_, runs) = crossing_free(start, &passes, runs, &mut proof, |bb, runs| {
        for (pi, (pass, segs)) in passes.iter().zip(runs).enumerate() {
            let Some(&e) = pass.first() else { continue };
            match joins.get(pi) {
                Some(Some(ret)) if pi > 0 => microlift_link(bb, ret, z, hop_z, p.feed),
                _ => {
                    let at = bb.at();
                    bb.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop_z), None);
                    bb.push(MoveKind::Rapid, Point3::new(e.x, e.y, hop_z), None);
                    bb.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(p.plunge_feed));
                }
            }
            push_segs(bb, segs, z, p.feed);
        }
    });
    if let Some(n) = proof.note() {
        b.note(format!("Z {z:.3}: {n}"));
    }
    Ok(LevelPlan { passes, joins, arcs: Some(runs), helix_r, hsm: true })
}

/// The microlift return onto the next pass's start at plane `z`.
pub(super) fn microlift(b: &mut PathBuilder, ret: &Return, z: f64, hop_z: f64, feed: f64) {
    microlift_link(b, ret, z, hop_z, feed);
}
