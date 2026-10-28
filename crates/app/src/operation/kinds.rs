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
        | OpParams::RotaryOffset { feed, .. }
        | OpParams::MultiAxisRough { feed, .. }
        | OpParams::Flow5D { feed, .. }
        | OpParams::MultiAxisContour { feed, .. }
        // The facing pass feeds along X at the station's Z - the
        // retype's feed number means the same distance-per-minute
        // here as on a mill (M5.2 A). The profile rides feed along
        // the offset curve: the nose's own travel, same units.
        | OpParams::Facing { feed, .. }
        | OpParams::ProfileRough { feed, .. }
        | OpParams::ProfileFinish { feed, .. }
        // A blade path never feeds axially at all - every feed move
        // is a radial plunge at the plunge number, every traverse a
        // rapid - so this number rides the trio and the stamp only,
        // inert but named (M5.2 C).
        | OpParams::GrooveRough { feed, .. }
        | OpParams::GrooveFinish { feed, .. }
        | OpParams::Part { feed, .. }
        // Threading's feed is the helix's own travel per minute -
        // the synchronized spindle carries the distance-per-minute
        // meaning unchanged (M5.2 D).
        | OpParams::Threading { feed, .. } => *feed,
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
        | OpParams::RotaryOffset { plunge_feed, .. }
        | OpParams::MultiAxisRough { plunge_feed, .. }
        | OpParams::Flow5D { plunge_feed, .. }
        | OpParams::MultiAxisContour { plunge_feed, .. }
        // Facing's plunge is the radial approach off the rapids plane
        // onto the station - the same number the ladder's first move
        // posts at (M5.2 A). The profile kinds plunge the same way:
        // radially in to the rung's ride, radially out at the end
        // (M5.2 B).
        | OpParams::Facing { plunge_feed, .. }
        | OpParams::ProfileRough { plunge_feed, .. }
        | OpParams::ProfileFinish { plunge_feed, .. }
        // The blade's plunge IS its cut - the radial sink every rung
        // eats at (M5.2 C).
        | OpParams::GrooveRough { plunge_feed, .. }
        | OpParams::GrooveFinish { plunge_feed, .. }
        // Threading's plunge is the rung's radial sink - the blade's
        // law, one ladder deeper (M5.2 D).
        | OpParams::Part { plunge_feed, .. }
        | OpParams::Threading { plunge_feed, .. } => *plunge_feed,
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
        | OpParams::RotaryOffset { climb, .. }
        | OpParams::MultiAxisRough { climb, .. }
        | OpParams::Flow5D { climb, .. }
        | OpParams::MultiAxisContour { climb, .. } => *climb,
        // A drill has no climb notion - re-typing one to a milling kind
        // starts from the fresh default instead of inventing a carry.
        // Facing's neither: the work spins and the insert traverses;
        // which flank meets the cut is the spindle's direction, not a
        // number this ladder authors (M5.2 A) - and neither do the
        // profile rides, which are the same kinematic (M5.2 B).
        OpParams::Drill { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // Nor the blades: the band's two edges meet the walls
        // alike - which flank shears is again the spindle's
        // direction (M5.2 C).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        // Nor the wave: the V's two flanks alternate with the spin,
        // and which one leads is the spindle's direction (M5.2 D).
        | OpParams::Part { .. }
        | OpParams::Threading { .. } => true,
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
        // The stamped ladder's spacing is the pocket's plain number.
        | OpParams::MultiAxisRough { stepover, .. }
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
        | OpParams::RotaryParallel { .. }
        // Flow authors its spacing the twins' way: derived unless a
        // number is typed over it (M5.1 I). The drive-curve contour
        // has NO spacing at all - the picked chain is the path, one
        // pass over it (M5.1 J).
        | OpParams::Flow5D { .. }
        // The drive-curve contour has NO spacing at all, and facing
        // none either: one radial pass per station runs the profile's
        // whole width - the stepdown IS the spacing (M5.2 A). The
        // profile kinds say the same: the rungs' spacing is the
        // stepdown, the finish's pattern is the skin itself.
        | OpParams::MultiAxisContour { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // A blade has no spacing either: its width IS the pattern's
        // step, tiled by the kernel's tangency law (M5.2 C) - and a
        // threading pass has no spacing a number could name: the
        // helix's PITCH is the lattice, authored here as the thread's
        // own lead, not a stepover (M5.2 D).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        | OpParams::Part { .. }
        | OpParams::Threading { .. } => None,
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
        // Z infeed here, the pocket's law (M5.1 stage E3).
        | OpParams::MultiAxisRough { stepdown, .. }
        // Radial infeed here, the wrap ladder's law, read down the
        // axis instead of out of the skin (M5.2 A).
        | OpParams::Facing { stepdown, .. }
        // The rung ladder's spacing is radial too - the same number
        // read as a shell width off the skin (M5.2 B).
        | OpParams::ProfileRough { stepdown, .. }
        // The blade's infeed is radial too - the rung's sink, and
        // the parting ladder's (M5.2 C).
        | OpParams::GrooveRough { stepdown, .. }
        | OpParams::Part { stepdown, .. }
        | OpParams::Slot { stepdown, .. }
        | OpParams::Trace { stepdown, .. } => Some(*stepdown),
        OpParams::Face { .. } | OpParams::Drill { .. } | OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. } | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. } | OpParams::AdvancedSwarf { .. }
        | OpParams::RotaryContour { .. } | OpParams::RotaryParallel { .. }
        | OpParams::Flow5D { .. }
        // One ride is the whole finish - there is no infeed to carry,
        // and one plunge per center is the groove finish's answer too.
        | OpParams::ProfileFinish { .. }
        | OpParams::GrooveFinish { .. }
        // Threading's ladder spacing is INFEED, its own field - the
        // retype carries no stepover number for it (M5.2 D).
        | OpParams::Threading { .. }
        | OpParams::MultiAxisContour { .. } => None,
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
        | OpParams::RotaryOffset { top_z, .. }
        | OpParams::MultiAxisRough { top_z, .. }
        | OpParams::Flow5D { top_z, .. }
        | OpParams::MultiAxisContour { top_z, .. }
        // Facing's top is the first station above the shoulder the
        // ladder steps down from (M5.2 A); the profile kinds' is the
        // high end of the window their rungs ride (M5.2 B).
        | OpParams::Facing { top_z, .. }
        | OpParams::ProfileRough { top_z, .. }
        | OpParams::ProfileFinish { top_z, .. }
        // The blade windows' high end; parting's is the bar's own
        // top the ladder rises from (M5.2 C).
        | OpParams::GrooveRough { top_z, .. }
        | OpParams::GrooveFinish { top_z, .. }
        | OpParams::Part { top_z, .. }
        // The threading window's high end: the first turn's station
        // the lattice begins from (M5.2 D).
        | OpParams::Threading { top_z, .. } => Some(*top_z),
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
        | OpParams::RotaryOffset { bottom_z, .. }
        | OpParams::MultiAxisRough { bottom_z, .. }
        | OpParams::Flow5D { bottom_z, .. }
        | OpParams::MultiAxisContour { bottom_z, .. }
        // Facing's finished plane: the facing Z every station
        // steps toward (M5.2 A); the profile kinds' is the low end
        // of the station window, the plunge-in end of the ride
        // (M5.2 B).
        | OpParams::Facing { bottom_z, .. }
        | OpParams::ProfileRough { bottom_z, .. }
        | OpParams::ProfileFinish { bottom_z, .. }
        // The blade windows' low end - and parting's plane itself,
        // the station every rung sinks toward (M5.2 C).
        | OpParams::GrooveRough { bottom_z, .. }
        | OpParams::GrooveFinish { bottom_z, .. }
        | OpParams::Part { bottom_z, .. }
        // The threading window's low end - the last center's station
        // and the whole turns whose lattice it completes (M5.2 D).
        | OpParams::Threading { bottom_z, .. } => Some(*bottom_z),
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
        | OpParams::RotaryParallel { retract_z, .. }
        // The stamped ladder hops like the pocket it stamps (E3).
        | OpParams::MultiAxisRough { retract_z, .. }
        | OpParams::Flow5D { retract_z, .. }
        // The drive-curve contour hops between chains like the
        // raster hops between lines (M5.1 J).
        | OpParams::MultiAxisContour { retract_z, .. } => *retract_z,
        // No hop plane to carry: the kernel names its own rapids
        // plane (one infeed and a tool radius off the skin - the
        // facing ladder states it one infeed clear of the profile,
        // the axis way; the profile rungs state theirs the same way,
        // a chord-tolerance clear of the ride's reach), and Face
        // never stored one either.
        OpParams::Face { .. }
        | OpParams::RotaryOffset { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // The blades name their rapids plane too: the whole bar's
        // radius plus tolerance, clear of every shoulder the band
        // traverses over (M5.2 C).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        // Threading's rapids plane is the clearance height itself -
        // the whole bar's radius clears every turn's crest the pass
        // traverses over (M5.2 D).
        | OpParams::Part { .. }
        | OpParams::Threading { .. } => None,
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
        // choices on live operations are never retyped over. The entry
        // family seeds at the dive - a new pocket posts what pockets
        // always posted until M5.1 H made the choice exist; the numbers
        // ride the 3-D pocket's defaults so a switched strategy or
        // entry starts from a legal one.
        strategy: PocketStrategy::Auto,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        max_engagement: 45.0,
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
        shank_avoid: false,
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
        shank_avoid: false,
    }
}

/// The flowline seed (M5.1 I): the parallel's numbers whole - a flow
/// pass IS the raster's law on lines that turn, so what a fresh one
/// starts with is what a fresh parallel starts with. The direction
/// field is not a parameter: the generator reads the surface's own
/// least-curving direction off the model, and plain ground degrades
/// to the straight passes silently (the kernel's fallback law).
fn seed_flow5d(c: &Carried) -> OpParams {
    OpParams::Flow5D {
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
        shank_avoid: false,
    }
}

/// The drive-curve seed (M5.1 J): the flow's numbers minus the
/// stepover - the spacing question does not exist when the chain IS
/// the path. What it DOES need that no seed can know is the
/// selection: a fresh one cuts nothing until edge chains are picked
/// in its Geometry section, and generation refuses by name until
/// then. The tolerance is the verify budget only (variant docs).
fn seed_multi_axis_contour(c: &Carried) -> OpParams {
    OpParams::MultiAxisContour {
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
        shank_avoid: false,
    }
}

/// The M5.2 stage A seed: the facing ladder's numbers, read the
/// wrap's way - `stepdown` is the AXIAL infeed one station takes off
/// the end. The carried window is the station range: whatever way
/// the machine stands the bar, the face to cut lies between top and
/// bottom heights.
fn seed_facing(c: &Carried) -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        // The one number NOT folded: an infeed is bounded by twice
        // the nose's reach (the kernel's law), and the mill default
        // the fold carries would refuse against the house insert's
        // 0.8 mm nose the moment a user created the op. 0.5 is the
        // handbook rough-facing infeed, and it stands under 2r of
        // every TN row in the catalog.
        stepdown: 0.5,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The M5.2 stage B rough seed: the facing ladder's shape spoken
/// radially - the same stepdown 0.5 (under twice the house insert's
/// nose, the same law read as a shell width), plus the leave the
/// finish takes off. 0.25 is the handbook finish allowance: a whole
/// nose's worth of chord sag at tolerance 0.05 leaves far less, so
/// the ride's own budget, not this number, shapes the finish.
fn seed_profile_rough(c: &Carried) -> OpParams {
    OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The M5.2 stage B finish seed: the window and the feeds - there is
/// no infeed to answer for. The carried stepdown is deliberately
/// dropped: one ride IS the pattern.
fn seed_profile_finish(c: &Carried) -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The M5.2 stage C rough seed: the profile rough's shape on a
/// blade. The stepdown is the same 0.5 - under twice the blade's
/// tip reaches nothing here (the band has no nose to chord), but it
/// is the handbook grooving infeed, and it stays far under any
/// real bar's column. The leave is the same 0.25: a full skin the
/// finish plunge takes off.
fn seed_groove_rough(c: &Carried) -> OpParams {
    OpParams::GrooveRough {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The stage C finish seed: the window and the feeds - one plunge
/// per center is the whole pattern, so there is no infeed to
/// answer for (and no leave: the floor IS the model).
fn seed_groove_finish(c: &Carried) -> OpParams {
    OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The parting seed: the bar's top and the parting plane carried
/// as the window pair, the rung ladder's 0.5 infeed, the feeds.
/// The plane defaults to the document's bottom - where a part gets
/// cut off from the bar it was turned on.
fn seed_part(c: &Carried) -> OpParams {
    OpParams::Part {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        stepdown: 0.5,
        feed: c.feed,
        plunge_feed: c.plunge,
    }
}

/// The threading seed: the window and feeds carried, and the
/// commonest thread in the shop as the modeled numbers - a 2 mm
/// pitch, its ISO 0.541 x P depth, and a quarter-pitch-of-depth
/// infeed ladder (three or four rungs to the root). The V itself
/// comes from the tool, never authored here (M5.2 D).
fn seed_threading(c: &Carried) -> OpParams {
    OpParams::Threading {
        tolerance: 0.05,
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        pitch: 2.0,
        depth: 1.08,
        infeed: 0.4,
        feed: c.feed,
        plunge_feed: c.plunge,
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

/// The stage E3 seed: the pocket ladder's numbers verbatim - the
/// stamp moves no position, so the draft rule reads the SAME carried
/// stepover/stepdown/strategy the plain pocket does - crossed with
/// the twins' pose trio, defaulted inert (lead and lag at zero, cone
/// wide open: a fresh op poses by the tangency law alone until the
/// machinist asks for more).
fn seed_multiaxis_rough(c: &Carried) -> OpParams {
    OpParams::MultiAxisRough {
        tolerance: 0.05,
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
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        max_engagement: 45.0,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
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
    /// Whose machine this kind drives (M5.2 stage A): the family the
    /// document's machine kind must admit for the op to exist at all -
    /// the command-level gate reads this row, never a hand-typed list.
    pub family: Family,
    pub seed: fn(&Carried) -> OpParams,
}

/// The two machine families a document's `.mch` kind can belong to
/// (M5.2 stage A): a mill and its live tooling speak Mill, a lathe
/// speaks Turn, and a mill-turn's machine kind admits both. The
/// ribbon's darkening is a courtesy; this pairing, enforced at the
/// command door, is the law.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Mill,
    Turn,
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
            family: Family::Mill,
            seed: seed_face,
        },
        KindInfo {
            ty: OpType::Contour,
            label: "2D Contour",
            caption: "Contour",
            tag: 1,
            mode: Mode::Boundary,
            family: Family::Mill,
            seed: seed_contour,
        },
        KindInfo {
            ty: OpType::Pocket,
            label: "2D Pocket",
            caption: "Pocket",
            tag: 2,
            // The pocket hollows the region's inside: area laws rule.
            mode: Mode::Area,
            family: Family::Mill,
            seed: seed_pocket,
        },
        KindInfo {
            ty: OpType::Slot,
            label: "Slot",
            caption: "Slot",
            tag: 3,
            mode: Mode::Area,
            family: Family::Mill,
            seed: seed_slot,
        },
        KindInfo {
            ty: OpType::Trace,
            label: "Trace",
            caption: "Trace",
            tag: 4,
            // The center runs ON the line: tangency laws stand down.
            mode: Mode::Engrave,
            family: Family::Mill,
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
            family: Family::Mill,
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
            family: Family::Mill,
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
            family: Family::Mill,
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
            family: Family::Mill,
            seed: seed_contour5d,
        },
        KindInfo {
            ty: OpType::Parallel5D,
            label: "5-Axis Parallel",
            caption: "5-Axis Parallel",
            tag: 9,
            mode: Mode::Envelope,
            family: Family::Mill,
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
            family: Family::Mill,
            seed: seed_swarf,
        },
        KindInfo {
            ty: OpType::AdvancedSwarf,
            label: "Advanced Swarf",
            caption: "Adv. Swarf",
            tag: 11,
            mode: Mode::Envelope,
            family: Family::Mill,
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
            family: Family::Mill,
            seed: seed_rotary_contour,
        },
        KindInfo {
            ty: OpType::RotaryParallel,
            label: "Rotary Parallel",
            caption: "Rotary Parallel",
            tag: 13,
            mode: Mode::Envelope,
            family: Family::Mill,
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
            family: Family::Mill,
            seed: seed_rotary_offset,
        },
        KindInfo {
            ty: OpType::MultiAxisRough,
            label: "Multi-Axis Rough",
            caption: "Multi-Axis Rough",
            tag: 15,
            // The positions are the pocket ladder's, bit for bit, and
            // a sphere's envelope is rotation-invariant about its own
            // center: law (f) measures what the vertical rough would
            // have cut, so the pose cannot lie to the verdict (M5.1
            // stage E3). The tolerance is that law's budget, its only
            // reading - the ladder itself chords by the pocket's own
            // lattice laws.
            mode: Mode::Envelope,
            family: Family::Mill,
            seed: seed_multiaxis_rough,
        },
        KindInfo {
            ty: OpType::Flow5D,
            label: "5-Axis Flow",
            caption: "5-Axis Flow",
            tag: 16,
            // The parallel's verdict law on lines that turn (M5.1 I):
            // a sphere's envelope is rotation-invariant, so the field
            // decides where the lines GO and the lean law where the
            // spindle points, and the measured scan judges both.
            mode: Mode::Envelope,
            family: Family::Mill,
            seed: seed_flow5d,
        },
        KindInfo {
            ty: OpType::MultiAxisContour,
            label: "Multi-Axis Contour",
            caption: "Multi-Axis Contour",
            tag: 17,
            // The ball rides the picked chains wherever they stand
            // (M5.1 J): there is no authored pattern for a law to
            // distrust, and the verdict is the same sphere-envelope
            // scan - rest-height errors at a vertex fan and a gouged
            // chain are exactly what the sweep measures.
            mode: Mode::Envelope,
            family: Family::Mill,
            seed: seed_multi_axis_contour,
        },
        KindInfo {
            ty: OpType::Facing,
            label: "Facing",
            caption: "Facing",
            tag: 18,
            // The facing verdict is measured like the finish's (law
            // f): the ball is the insert nose circle swept true - and
            // a meridian circle revolved IS a sphere about the axis,
            // so the scan reads the lathe's own cut wherever the fit
            // stands it. The nose kinds keep this law after stage C:
            // the revolve verdict is the BLADE's, where no sphere
            // stands - the band is the tool.
            mode: Mode::Envelope,
            family: Family::Turn,
            seed: seed_facing,
        },
        KindInfo {
            ty: OpType::ProfileRough,
            label: "Profile Rough",
            caption: "Profile Rough",
            tag: 19,
            // The rungs' verdict is the same sphere-envelope law
            // (M5.2 B): a nose circle revolved IS a sphere about the
            // axis, so the scan measures each rung's shell exactly -
            // and the standing leave the verdict reports is what the
            // finish pass owns.
            mode: Mode::Envelope,
            family: Family::Turn,
            seed: seed_profile_rough,
        },
        KindInfo {
            ty: OpType::ProfileFinish,
            label: "Profile Finish",
            caption: "Profile Finish",
            tag: 20,
            // The ride is the pattern: no authored spacing for a law
            // to distrust, and the tolerance is exactly the
            // envelope's budget - the chord law the kernel settled
            // the ride with is the same promise the scan checks.
            mode: Mode::Envelope,
            family: Family::Turn,
            seed: seed_profile_finish,
        },
        KindInfo {
            ty: OpType::GrooveRough,
            label: "Groove Rough",
            caption: "Groove Rough",
            tag: 21,
            // No sphere can judge a band: a grooved part is undercut
            // BY DESIGN, and material standing beside the groove is
            // out of every ball's reach along Z. The verdict is the
            // meridian scan of `turning::revolve` - the boxcar min
            // of the cuts against the silhouette, exact on E's own
            // lattice (M5.2 C).
            mode: Mode::Revolve,
            family: Family::Turn,
            seed: seed_groove_rough,
        },
        KindInfo {
            ty: OpType::GrooveFinish,
            label: "Groove Finish",
            caption: "Groove Finish",
            tag: 22,
            // The same meridian law, leave-free: one plunge per
            // center to the band's floor, and the floor no band
            // reached rests by the window shell - silence is what a
            // missed pass must never buy (M5.2 C).
            mode: Mode::Revolve,
            family: Family::Turn,
            seed: seed_groove_finish,
        },
        KindInfo {
            ty: OpType::Part,
            label: "Parting",
            caption: "Part",
            tag: 23,
            // Separation as a band cut: the through-axis kerf is
            // judged silent inside its own kerf and right of it
            // (the workpiece left with the scrap), and the verdict
            // that reads it is the same meridian scan (M5.2 C).
            mode: Mode::Revolve,
            family: Family::Turn,
            seed: seed_part,
        },
        KindInfo {
            ty: OpType::Threading,
            label: "Threading",
            caption: "Threading",
            tag: 24,
            // The blade verdict with the wave clause (M5.2 D): the
            // tool's V, not a boxcar, is the footprint - at every
            // center's station the floor stands deeper by the flank
            // slope's cotangent times the distance along the axis.
            // The lattice is the helix's own, so the scan judges
            // every `pitch` of station on the model's phase, and
            // the shell owns the crest above the thread.
            mode: Mode::Revolve,
            family: Family::Turn,
            seed: seed_threading,
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
    KindInfo::ALL[15].label,
    KindInfo::ALL[16].label,
    KindInfo::ALL[17].label,
    KindInfo::ALL[18].label,
    KindInfo::ALL[19].label,
    KindInfo::ALL[20].label,
    KindInfo::ALL[21].label,
    KindInfo::ALL[22].label,
    KindInfo::ALL[23].label,
    KindInfo::ALL[24].label,
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
            OpType::MultiAxisRough => 15,
            OpType::Flow5D => 16,
            OpType::MultiAxisContour => 17,
            OpType::Facing => 18,
            OpType::ProfileRough => 19,
            OpType::ProfileFinish => 20,
            OpType::GrooveRough => 21,
            OpType::GrooveFinish => 22,
            OpType::Part => 23,
            OpType::Threading => 24,
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
