//! The operation-kind registry (M5.0.2): every per-kind table collapsed to
//! one row per kind, so a new kind registers here instead of being hunted
//! through the fan-out (docs/M5-PHASE5-PLAN.md, the coupling fact).
//!
//! A kind is: an `OpParams` variant (its fields), a row in [`KindInfo::ALL`]
//! (label, stamp tag, verify mode, retype seed), and exhaustive per-kind
//! glue that stays where it belongs - region builder, height walk, digest
//! body, generator call, DTO encode/decode. Those matches are deliberate:
//! an exhaustive match on a fresh variant fails to compile, which is the
//! machinery that FORCES registration. The registry owns only what a match
//! cannot express as data - and what used to be duplicated as a magic
//! literal (the stamp tag) or a silent `_ =>` arm (the ribbon's retype).
//!
//! Provenance: our own domain model; the kind families and the shared-field
//! carry-over rule are specced from public documentation of commercial CAM
//! workflows. No vendor source was consulted.

use crate::drilling::CycleChoice;
use crate::finishing3d::FinishPattern;
use crate::geo2d::Point2;
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::machining3d::Entry3D;
use crate::verify::Mode;

use super::types::{Height, HeightReference, OpParams, OpType, Tilt};

/// The numbers that ride a retype: every kind stores them under the same
/// name, so the draft rule is one carry, then the row's seed (M1.5.2's
/// law, here since M5.0.2 - `crates/ui` used to hold this table).
#[derive(Clone, Copy, Debug)]
pub struct Carried {
    pub feed: f64,
    pub plunge: f64,
    pub climb: bool,
    pub clearance: Height,
    pub stepover: f64,
    pub stepdown: f64,
    pub top: Height,
    pub bottom: Height,
    pub retract: Height,
    /// The 3+2 index (M5.1 stage B): the planar kinds carry it across
    /// a retype, the non-planar seeds never read it - so a tilted
    /// pocket that becomes a 3D pocket loses the tilt WITH the kind
    /// that owned it, and a tilted pocket that becomes a tilted
    /// contour keeps it.
    pub tilt: Option<Tilt>,
}

impl Carried {
    /// Fold the shared numbers out of any kind. Where a kind has no field
    /// (Face has no bottom plane, Trace no stepover), the fold's default
    /// seeds the draft - never a guess about the old kind.
    #[must_use]
    pub fn of(p: &OpParams) -> Self {
        Self {
            feed: feed_of(p),
            plunge: plunge_of(p),
            climb: climb_of(p),
            clearance: p.clearance(),
            stepover: stepover_of(p).unwrap_or(3.0),
            stepdown: stepdown_of(p).unwrap_or(5.0),
            top: top_of(p).unwrap_or_else(|| Height::of(HeightReference::TopOfModel, 0.0)),
            bottom: bottom_of(p)
                .unwrap_or_else(|| Height::of(HeightReference::WorkOffset(0), 0.0)),
            retract: retract_of(p).unwrap_or_else(|| Height::of(HeightReference::TopOfStock, 10.0)),
            tilt: p.tilt(),
        }
    }
}

fn feed_of(p: &OpParams) -> f64 {
    match p {
        OpParams::Face { feed, .. }
        | OpParams::Contour { feed, .. }
        | OpParams::Pocket { feed, .. }
        | OpParams::Pocket3D { feed, .. }
        | OpParams::Slot { feed, .. }
        | OpParams::Trace { feed, .. }
        | OpParams::Drill { feed, .. }
        | OpParams::Finish3D { feed, .. }
        | OpParams::Contour5D { feed, .. }
        | OpParams::Parallel5D { feed, .. }
        | OpParams::Swarf { feed, .. }
        | OpParams::AdvancedSwarf { feed, .. }
        | OpParams::RotaryContour { feed, .. }
        | OpParams::RotaryParallel { feed, .. }
        | OpParams::RotaryOffset { feed, .. } => *feed,
    }
}

fn plunge_of(p: &OpParams) -> f64 {
    match p {
        OpParams::Face { plunge_feed, .. }
        | OpParams::Contour { plunge_feed, .. }
        | OpParams::Pocket { plunge_feed, .. }
        | OpParams::Pocket3D { plunge_feed, .. }
        | OpParams::Slot { plunge_feed, .. }
        | OpParams::Trace { plunge_feed, .. }
        | OpParams::Drill { plunge_feed, .. }
        | OpParams::Finish3D { plunge_feed, .. }
        | OpParams::Contour5D { plunge_feed, .. }
        | OpParams::Parallel5D { plunge_feed, .. }
        | OpParams::Swarf { plunge_feed, .. }
        | OpParams::AdvancedSwarf { plunge_feed, .. }
        | OpParams::RotaryContour { plunge_feed, .. }
        | OpParams::RotaryParallel { plunge_feed, .. }
        | OpParams::RotaryOffset { plunge_feed, .. } => *plunge_feed,
    }
}

fn climb_of(p: &OpParams) -> bool {
    match p {
        OpParams::Face { climb, .. }
        | OpParams::Contour { climb, .. }
        | OpParams::Pocket { climb, .. }
        | OpParams::Pocket3D { climb, .. }
        | OpParams::Slot { climb, .. }
        | OpParams::Trace { climb, .. }
        | OpParams::Finish3D { climb, .. }
        | OpParams::Contour5D { climb, .. }
        | OpParams::Parallel5D { climb, .. }
        | OpParams::Swarf { climb, .. }
        | OpParams::AdvancedSwarf { climb, .. }
        | OpParams::RotaryContour { climb, .. }
        | OpParams::RotaryParallel { climb, .. }
        | OpParams::RotaryOffset { climb, .. } => *climb,
        // A drill has no climb notion - re-typing one to a milling kind
        // starts from the fresh default instead of inventing a carry.
        OpParams::Drill { .. } => true,
    }
}

fn stepover_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Face { stepover, .. }
        | OpParams::Pocket { stepover, .. }
        | OpParams::Pocket3D { stepover, .. }
        // The wrap ladder's spacing is a plain number like the
        // pocket's, not the twins' derived Option (variant docs).
        | OpParams::RotaryOffset { stepover, .. }
        | OpParams::Slot { stepover, .. } => Some(*stepover),
        // Parallel5D's spacing - and the swarf kinds', which run the
        // same derivation - is an Option the same way Finish3D's
        // raster's is: a derivation by default, so the fold reads the
        // field as "not authored by a number to carry" - the retype
        // starts a fresh derived line set, never a borrowed override.
        OpParams::Contour { .. }
        | OpParams::Trace { .. }
        | OpParams::Drill { .. }
        | OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. } => None,
    }
}

fn stepdown_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Contour { stepdown, .. }
        | OpParams::Pocket { stepdown, .. }
        | OpParams::Pocket3D { stepdown, .. }
        // The ladder's infeed is radial here, but it is still the
        // number the retype carries.
        | OpParams::RotaryOffset { stepdown, .. }
        | OpParams::Slot { stepdown, .. }
        | OpParams::Trace { stepdown, .. } => Some(*stepdown),
        OpParams::Face { .. } | OpParams::Drill { .. } | OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. } | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. } | OpParams::AdvancedSwarf { .. }
        | OpParams::RotaryContour { .. } | OpParams::RotaryParallel { .. } => None,
    }
}

/// The section plane, present or not (Face has no `top_z` field - its
/// region rides `region_height`). `crates/ui`'s height wording reads it.
#[must_use]
pub fn top_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Pocket3D { top_z, .. }
        | OpParams::Slot { top_z, .. }
        | OpParams::Trace { top_z, .. }
        | OpParams::Drill { top_z, .. }
        | OpParams::Finish3D { top_z, .. }
        | OpParams::Contour5D { top_z, .. }
        | OpParams::Parallel5D { top_z, .. }
        | OpParams::Swarf { top_z, .. }
        | OpParams::AdvancedSwarf { top_z, .. }
        | OpParams::RotaryContour { top_z, .. }
        | OpParams::RotaryParallel { top_z, .. }
        | OpParams::RotaryOffset { top_z, .. } => Some(*top_z),
        OpParams::Face { .. } => None,
    }
}

/// The finished plane, present or not (Face none, Drill only an explicit
/// Depth override). Read by `Carried::of` and by the height wording.
#[must_use]
pub fn bottom_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { bottom_z, .. }
        | OpParams::Pocket { bottom_z, .. }
        | OpParams::Pocket3D { bottom_z, .. }
        | OpParams::Slot { bottom_z, .. }
        | OpParams::Trace { bottom_z, .. }
        | OpParams::Finish3D { bottom_z, .. }
        | OpParams::Contour5D { bottom_z, .. }
        | OpParams::Parallel5D { bottom_z, .. }
        | OpParams::Swarf { bottom_z, .. }
        | OpParams::AdvancedSwarf { bottom_z, .. }
        | OpParams::RotaryContour { bottom_z, .. }
        | OpParams::RotaryParallel { bottom_z, .. }
        | OpParams::RotaryOffset { bottom_z, .. } => Some(*bottom_z),
        // A drill's floor is normally the hole's own, from the model;
        // only an explicit Depth override is a plane to carry over.
        OpParams::Drill { bottom_z, .. } => *bottom_z,
        OpParams::Face { .. } => None,
    }
}

/// The between-pass hop plane, present or not: Contour always stores one,
/// the other hopped kinds keep it in an option (None hops to clearance).
fn retract_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { retract_z, .. } => Some(*retract_z),
        OpParams::Pocket { retract_z, .. }
        | OpParams::Pocket3D { retract_z, .. }
        | OpParams::Slot { retract_z, .. }
        | OpParams::Trace { retract_z, .. }
        | OpParams::Drill { retract_z, .. }
        | OpParams::Finish3D { retract_z, .. }
        | OpParams::Contour5D { retract_z, .. }
        | OpParams::Parallel5D { retract_z, .. }
        | OpParams::Swarf { retract_z, .. }
        | OpParams::AdvancedSwarf { retract_z, .. }
        | OpParams::RotaryContour { retract_z, .. }
        | OpParams::RotaryParallel { retract_z, .. } => *retract_z,
        // No hop plane to carry: the kernel names its own rapids
        // plane (one infeed and a tool radius off the skin), and
        // Face never stored one either.
        OpParams::Face { .. } | OpParams::RotaryOffset { .. } => None,
    }
}

// The seeds, one fn per row: what quick-add starts, and what a retype
// builds from the carried numbers. Bodies are M1.5.2's draft rule, moved
// verbatim out of `crates/ui/src/operations.rs::retype` (M5.0.2) - the
// comments there explain every default.

fn seed_face(c: &Carried) -> OpParams {
    OpParams::Face {
        stepover: c.stepover,
        clearance_z: c.clearance,
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        // A fresh face cuts one level; staging is opt-in in the
        // panel, never a surprise default.
        top_z: None,
        stepdown: None,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        tilt: c.tilt,
    }
}

fn seed_contour(c: &Carried) -> OpParams {
    OpParams::Contour {
        side: Side::Auto,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: c.retract,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        tilt: c.tilt,
    }
}

fn seed_pocket(c: &Carried) -> OpParams {
    OpParams::Pocket {
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        // New pockets let the shape come from the outline; explicit
        // choices on live operations are never retyped over.
        strategy: PocketStrategy::Auto,
        tilt: c.tilt,
    }
}

fn seed_slot(c: &Carried) -> OpParams {
    OpParams::Slot {
        a: Point2::new(-20.0, 0.0),
        b: Point2::new(20.0, 0.0),
        width: 4.0,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        stepover: c.stepover,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        tilt: c.tilt,
    }
}

fn seed_trace(c: &Carried) -> OpParams {
    OpParams::Trace {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        tilt: c.tilt,
    }
}

fn seed_drill(c: &Carried) -> OpParams {
    OpParams::Drill {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        // No shared bottom: each hole's floor rides the hole itself
        // (or the explicit Depth field if one is set); the G54
        // default below would be a plane no drilled hole implies.
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: c.feed,
        plunge_feed: c.plunge,
        // Creation is legal with an empty list; the generator
        // refuses to run one, and Detect holes fills it.
        holes: Vec::new(),
    }
}

fn seed_pocket3d(c: &Carried) -> OpParams {
    OpParams::Pocket3D {
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        strategy: PocketStrategy::Auto,
        // The dive the 2-D pocket already makes; ramp and helix are
        // opt-in once the walls are known to allow them.
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        // The staircase is honest noise until the user asks for
        // the closing pass: rest roughing costs a second ladder.
        rest_rough: false,
        // The law's default (r·(1−cos 45°) ≈ 15% of the diameter);
        // reads only under Adaptive, which the fresh op does not pick.
        max_engagement: 45.0,
        // Feed modulation costs a sim run; the fresh op posts the
        // programmed feed everywhere until the user asks otherwise.
        feed_opt: false,
    }
}

fn seed_finish3d(c: &Carried) -> OpParams {
    OpParams::Finish3D {
        pattern: FinishPattern::Parallel,
        // None = the stepover cos-law derives spacing from the
        // tolerance; the panel can override it per line set.
        stepover: None,
        // A general 3-D finish budget: half the common 0.1 mm shop
        // tolerance - coarse enough that the scallop stepdown on a
        // typical R3 ball stays well under the level cap, fine enough
        // that the rings read as a finish.
        tolerance: 0.05,
        clearance_z: c.clearance,
        // None hops rings to clearance - the pocket family's law.
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        // Fresh finishes hop: the ramp is opt-in, like the pocket's
        // between-ring entry - the certified diagonal is a claim
        // about the ground, and a new op has none on record yet.
        ramp: false,
        // The march refines to its own floor until a machinist says
        // otherwise; the ramp's slope cap waits for a ramp to govern,
        // at the law's own 45 degrees.
        min_step: None,
        ramp_angle: 45.0,
    }
}

/// The 5-axis stage C seeds: the finish seed's numbers with the pose
/// at rest - lead and lag 0 (straight up the normal, the identity
/// pose) and a 90-degree cone (the reach the machine letters refuse
/// at posting anyway; the cone only folds when a lean is asked for).
/// A retype FROM a tilted planar op loses the index with the kind
/// that owned it, like every non-planar seed (`Carried`'s tilt note).
fn seed_contour5d(c: &Carried) -> OpParams {
    OpParams::Contour5D {
        tolerance: 0.05,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

fn seed_parallel5d(c: &Carried) -> OpParams {
    OpParams::Parallel5D {
        tolerance: 0.05,
        // None = the stepover cos-law derives spacing from the
        // tolerance, the finish's own law.
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// The stage D seeds: the raster's numbers whole - a swarf path IS
/// the raster run along the surface's own ruling, so what a fresh
/// one starts with is what a fresh parallel starts with, pose at
/// rest. The ruling is not a parameter: the generator reads it off
/// the model and refuses by name when the model has none.
fn seed_swarf(c: &Carried) -> OpParams {
    OpParams::Swarf {
        tolerance: 0.05,
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// The ladder's default rungs: 2 - rough then finish. One would
/// make the kind a spelling of plain swarf, and the panel's whole
/// claim is the coarse-first pass; the generator's named range runs
/// to 4 (motion refuses past it).
fn seed_advanced_swarf(c: &Carried) -> OpParams {
    OpParams::AdvancedSwarf {
        tolerance: 0.05,
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        passes: 2,
    }
}

/// The stage E seeds: the finish's numbers whole, the pose at rest -
/// lead and lag 0 stand the tool exactly on the RADIAL (the rotary
/// identity) and the 90-degree cone states the reach. The axis is not
/// a parameter: the generator fits it to the model and refuses by
/// name when the model says nothing of turning, like the ruling
/// before it.
fn seed_rotary_contour(c: &Carried) -> OpParams {
    OpParams::RotaryContour {
        tolerance: 0.05,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

fn seed_rotary_parallel(c: &Carried) -> OpParams {
    OpParams::RotaryParallel {
        tolerance: 0.05,
        stepover: None,
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// The stage E2 seed: the pocket ladder's numbers, read the wrap's
/// way - `stepdown` is the radial infeed one level peels off the
/// skin, `stepover` the spacing around it in developed arc length.
/// The dive it starts with is the one the 2-D pocket already makes
/// and the twins' law allows: a plunge down the radial, no rest
/// fields the kernel cannot honor (variant docs in types::params).
fn seed_rotary_offset(c: &Carried) -> OpParams {
    OpParams::RotaryOffset {
        tolerance: 0.05,
        stepover: c.stepover,
        stock_to_leave: 0.0,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        climb: c.climb,
        strategy: PocketStrategy::Auto,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        max_engagement: 45.0,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    }
}

/// One kind, as data: its name, the digest byte `stamp_with` writes (the
/// tag was a magic literal in eight arms until M5.0.2 - it is order, and
/// order belongs in the table), the verify law the generator picks, and
/// the retype seed. The row order is the product order: combo index,
/// ribbon index, `OpType::ordinal` - all one sequence (the law the UI
/// section tests quote).
pub struct KindInfo {
    pub ty: OpType,
    pub label: &'static str,
    /// The ribbon cell's short caption - imgui's wording, kept verbatim
    /// as the product's UI dialect (the tips table test guards it).
    pub caption: &'static str,
    pub tag: u8,
    pub mode: Mode,
    pub seed: fn(&Carried) -> OpParams,
}

impl KindInfo {
    pub const ALL: &'static [KindInfo] = &[
        KindInfo {
            ty: OpType::Face,
            label: "Face",
            caption: "Face",
            tag: 0,
            // Face trims the footprint silhouette: boundary laws rule.
            mode: Mode::Boundary,
            seed: seed_face,
        },
        KindInfo {
            ty: OpType::Contour,
            label: "2D Contour",
            caption: "Contour",
            tag: 1,
            mode: Mode::Boundary,
            seed: seed_contour,
        },
        KindInfo {
            ty: OpType::Pocket,
            label: "2D Pocket",
            caption: "Pocket",
            tag: 2,
            // The pocket hollows the region's inside: area laws rule.
            mode: Mode::Area,
            seed: seed_pocket,
        },
        KindInfo {
            ty: OpType::Slot,
            label: "Slot",
            caption: "Slot",
            tag: 3,
            mode: Mode::Area,
            seed: seed_slot,
        },
        KindInfo {
            ty: OpType::Trace,
            label: "Trace",
            caption: "Trace",
            tag: 4,
            // The center runs ON the line: tangency laws stand down.
            mode: Mode::Engrave,
            seed: seed_trace,
        },
        KindInfo {
            ty: OpType::Drill,
            label: "Drill",
            caption: "Drill",
            tag: 5,
            // Drill verifies as Boundary on purpose: its region's rim
            // loops put the gouge law's tangency test at the hole wall,
            // where a matched drill passes and an oversized tool is
            // flagged. What the move-by-move laws cannot see for a
            // straight deep plunge (it clears its own kerf, so the band
            // sampling never fires) the cycle reach law in `verify`
            // checks over `path.cycles`.
            mode: Mode::Boundary,
            seed: seed_drill,
        },
        KindInfo {
            ty: OpType::Pocket3D,
            label: "3D Pocket",
            caption: "3D Pocket",
            tag: 6,
            // The 3-D pocket verifies as Area with the mesh in hand
            // (the `model` field `VerifyContext` carries): the region
            // is its silhouette, the per-level islands come from the
            // facets.
            mode: Mode::Area,
            seed: seed_pocket3d,
        },
        KindInfo {
            ty: OpType::Finish3D,
            label: "3D Finish",
            caption: "3D Finish",
            tag: 7,
            // The waterline's verdict is measured, not deduced from
            // tangency: law (f) of `verify` scans the swept ball against
            // the mesh (the `finish_tol` field carries the budget).
            mode: Mode::Envelope,
            seed: seed_finish3d,
        },
        KindInfo {
            ty: OpType::Contour5D,
            label: "5-Axis Contour",
            caption: "5-Axis Contour",
            tag: 8,
            // Same measured envelope law (M5.1 stage C): the ball is
            // the cutter and a sphere's envelope is rotation-invariant,
            // so the pose changes what the SHANK sees, not what the
            // ball leaves - and the shank's question is the stage-A
            // leaning band law's, run on the stamped moves.
            mode: Mode::Envelope,
            seed: seed_contour5d,
        },
        KindInfo {
            ty: OpType::Parallel5D,
            label: "5-Axis Parallel",
            caption: "5-Axis Parallel",
            tag: 9,
            mode: Mode::Envelope,
            seed: seed_parallel5d,
        },
        KindInfo {
            ty: OpType::Swarf,
            label: "Swarf",
            caption: "Swarf",
            tag: 10,
            // The same measured envelope, read along the ruling (M5.1
            // stage D): the ball's path is the raster's, so the verdict
            // law is the finish's - the pose question (the blade lies
            // under a leaning shank) is the stage-A band law again.
            mode: Mode::Envelope,
            seed: seed_swarf,
        },
        KindInfo {
            ty: OpType::AdvancedSwarf,
            label: "Advanced Swarf",
            caption: "Adv. Swarf",
            tag: 11,
            mode: Mode::Envelope,
            seed: seed_advanced_swarf,
        },
        KindInfo {
            ty: OpType::RotaryContour,
            label: "Rotary Contour",
            caption: "Rotary Contour",
            tag: 12,
            // The rings are the finish's, posed radially (M5.1 stage
            // E): the ball's path decides the verdict, the fitted
            // axis and the lean answer only where the spindle points.
            mode: Mode::Envelope,
            seed: seed_rotary_contour,
        },
        KindInfo {
            ty: OpType::RotaryParallel,
            label: "Rotary Parallel",
            caption: "Rotary Parallel",
            tag: 13,
            mode: Mode::Envelope,
            seed: seed_rotary_parallel,
        },
        KindInfo {
            ty: OpType::RotaryOffset,
            label: "Rotary Offset",
            caption: "Rotary Offset",
            tag: 14,
            // The rough ladder's verdict is the measured envelope too
            // (M5.1 stage E2): law (f) sweeps a sphere, so the pose
            // cannot lie to it on a bar lying or standing, and the
            // kind's tolerance is exactly that law's budget.
            mode: Mode::Envelope,
            seed: seed_rotary_offset,
        },
    ];
}

/// The kinds, in product order (combo, ribbon, digest tags).
pub const KIND_COUNT: usize = KindInfo::ALL.len();

/// The panel labels, table order (was `crates/ui` panes::TYPES - now the
/// same list as [`OpType::Display`] reads).
pub const ALL_LABELS: [&str; KIND_COUNT] = [
    KindInfo::ALL[0].label,
    KindInfo::ALL[1].label,
    KindInfo::ALL[2].label,
    KindInfo::ALL[3].label,
    KindInfo::ALL[4].label,
    KindInfo::ALL[5].label,
    KindInfo::ALL[6].label,
    KindInfo::ALL[7].label,
    KindInfo::ALL[8].label,
    KindInfo::ALL[9].label,
    KindInfo::ALL[10].label,
    KindInfo::ALL[11].label,
    KindInfo::ALL[12].label,
    KindInfo::ALL[13].label,
    KindInfo::ALL[14].label,
];

impl OpType {
    /// The table index. Exhaustive: a new variant must join the registry
    /// (and a row in [`KindInfo::ALL`] in the same order) to compile.
    #[must_use]
    pub fn ordinal(self) -> usize {
        match self {
            OpType::Face => 0,
            OpType::Contour => 1,
            OpType::Pocket => 2,
            OpType::Slot => 3,
            OpType::Trace => 4,
            OpType::Drill => 5,
            OpType::Pocket3D => 6,
            OpType::Finish3D => 7,
            OpType::Contour5D => 8,
            OpType::Parallel5D => 9,
            OpType::Swarf => 10,
            OpType::AdvancedSwarf => 11,
            OpType::RotaryContour => 12,
            OpType::RotaryParallel => 13,
            OpType::RotaryOffset => 14,
        }
    }

    /// The kind's registry row.
    #[must_use]
    pub fn info(self) -> &'static KindInfo {
        let row = &KindInfo::ALL[self.ordinal()];
        debug_assert_eq!(
            row.ty, self,
            "KindInfo::ALL is out of step with OpType::ordinal"
        );
        row
    }
}

/// The type-change draft rule (M1.5.2), one law for both callers: the
/// page's Type combo and the ribbon's Create cells (through
/// `actions::add_operation`). The carried numbers meet the kind's row,
/// and the row answers with a fresh parameter set. An out-of-range index
/// is a caller bug - the ribbon and combo only ever pass table
/// positions - so debug builds stop, and a release build seeds a Face
/// and says so rather than panicking on a UI path (Prime Directive).
#[must_use]
pub fn retype_params(cur: &OpParams, kind: usize) -> OpParams {
    let c = Carried::of(cur);
    let row = match KindInfo::ALL.get(kind) {
        Some(r) => r,
        None => {
            debug_assert!(false, "op kind index {kind} is past the table");
            eprintln!("kinds: unknown op kind index {kind}, seeding a Face");
            &KindInfo::ALL[0]
        }
    };
    (row.seed)(&c)
}
