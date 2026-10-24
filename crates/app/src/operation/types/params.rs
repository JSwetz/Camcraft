//! Split from `types.rs` by M5.0.1; structure only.

use crate::drilling::{CycleChoice, Hole};
use crate::finishing3d;
use crate::geo2d::Point2;
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::machining3d::Entry3D;

use super::height::Height;
use super::sel::OpType;

/// The 3+2 index of a planar operation (M5.1 stage B): the tool holds
/// one pose for the whole path - commanded by the RS-274 letters A and
/// B (degrees, [`crate::cl::pose`]'s convention, C left to the machine's
/// table spin), with the cut plane anchored at `origin` in world mm.
/// Stored, stamped and posted, never normalized: the letters ARE the
/// claim, and [`Tilt::frame`] is the one reading of them. `None` in a
/// parameter field is no index - the ordinary prismatic op.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tilt {
    pub a_deg: f64,
    pub b_deg: f64,
    pub origin: [f64; 3],
}

impl Tilt {
    /// The frame this index commands: None only when the angles state
    /// no direction at all (a non-finite letter - refused at the
    /// parameter door long before generation, but the frame's law is
    /// Option, not panic).
    #[must_use]
    pub fn frame(&self) -> Option<crate::cl::TiltedFrame> {
        crate::cl::TiltedFrame::from_angles(
            crate::geometry::Point3::new(self.origin[0], self.origin[1], self.origin[2]),
            self.a_deg,
            self.b_deg,
        )
    }

    /// Door sanity (M5.1 stage B): every number finite, the anchor
    /// stably small, and the letters canonical - A in [-90, 90] and B
    /// in [-180, 180] - because [`crate::cl`]’s pose convention maps any OTHER
    /// letter pair is already the axis of one of these, and storing a
    /// non-canonical pose would give the same machine position two
    /// file identities (a stamp that changes for no motion).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.a_deg.is_finite()
            && self.b_deg.is_finite()
            && (-90.0..=90.0).contains(&self.a_deg)
            && (-180.0..=180.0).contains(&self.b_deg)
            && self.origin.iter().all(|v| v.is_finite() && v.abs() < 1e9)
    }
}

/// Everything an operation needs beyond its tool and source. The tool
/// diameter is deliberately absent - it lives in the tool definition and
/// is injected at generation, so a tool edit cannot disagree with a
/// parameter field. Heights are [`Height`] expressions (From + offset);
/// a file's pre-reference absolute numbers arrive as `Height::fixed`
/// through the project migration, so their behavior is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum OpParams {
    Face {
        stepover: f64,
        clearance_z: Height,
        cut_z: Height,
        /// Staged facing: the first cut level. `None` faces in one level
        /// at `cut_z` - the legacy default every pre-stepdown file
        /// carries. `Some(t)` descends `t` down to `cut_z` by `stepdown`.
        top_z: Option<Height>,
        /// Level spacing; positive when `top_z` is set, ignored when it
        /// is not.
        stepdown: Option<f64>,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index (M5.1 stage B): the plane this face cuts, tilted;
        /// `None` is the world XY the kind has always meant.
        tilt: Option<Tilt>,
    },
    Contour {
        stock_to_leave: f64,
        clearance_z: Height,
        retract_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Which side of the boundary the cutter rides; see
        /// [`machining2d::Side`]. Auto is answered by the material-fill
        /// check in the selection walk, never stored as a verdict.
        side: Side,
        lead: Lead,
        tolerance: f64,
        tab_count: u32,
        tab_width: f64,
        tab_height: f64,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    Pocket {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between passes of one level; None hops to
        /// clearance - the legacy shape.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    Slot {
        a: Point2,
        b: Point2,
        width: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        stepover: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    Trace {
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// Hole machining. The hole list is params data - recognized from the
    /// model by an action in the panel, or typed - so it is undoable,
    /// visible, and part of the stamp; a model edit that moves the holes
    /// turns the op Stale through the source digest, never by re-running
    /// recognition behind the user's back. `bottom_z` overrides the depth
    /// of every hole at once; `None` (the normal shape) machines each
    /// hole to its own floor, plus the through-hole exit allowance.
    Drill {
        clearance_z: Height,
        /// Hop plane between holes; `None` hops to clearance, like the
        /// pocket family.
        retract_z: Option<Height>,
        /// The cycle's R plane: where the feed move starts in each hole.
        top_z: Height,
        bottom_z: Option<Height>,
        cycle: CycleChoice,
        /// Depth per peck; `0.0` lets the Auto law pick.
        peck: f64,
        /// Seconds at the bottom; posted as the dwell word on the cycles
        /// that honor one.
        dwell: f64,
        feed: f64,
        plunge_feed: f64,
        holes: Vec<Hole>,
    },
    /// Mesh-driven 3D pocketing. The model is the final part: every
    /// level cuts the footprint minus the model's section strictly above
    /// it - islands are computed, never picked (the level law,
    /// [`crate::machining3d`]). The region is the model's silhouette,
    /// not a section: unlike the 2.5-D kinds, the cut area changes with
    /// Z, so a selection could only disagree with the mesh.
    Pocket3D {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        /// The first level's plane - the stock surface the ladder
        /// descends from.
        top_z: Height,
        /// The last level snaps here: the model's lowest floor.
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
        /// How the cutter engages each level; see
        /// [`crate::machining3d::Entry3D`].
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is
        /// Helix, validated there against the tool radius at generate.
        helix_radius: f64,
        /// Close the level ladder's staircase with a rest-material
        /// pass aimed at what the dexel sim says still stands
        /// ([`crate::machining3d::pocket3d_with_rest`], M2.1b).
        rest_rough: bool,
        /// Degrees of cutter contact the `Adaptive` strategy keeps its
        /// ring step under via [`crate::machining3d::engagement_step`]
        /// (M2.1c); read only when `strategy` is Adaptive, bounds
        /// checked there at generate.
        max_engagement: f64,
        /// Modulate every planar feed's rate by the engagement the
        /// dexel sim says the move actually meets, median-anchored to
        /// the programmed feed ([`crate::machining3d::optimize_feed`],
        /// M2.1d). Geometry is untouched; only feed words move.
        feed_opt: bool,
    },
    /// Ball-nose surface finishing. The model is the finished
    /// surface. `Waterline`: each ring is the ball-center tangent
    /// locus of the forbidden region at its center plane, levels
    /// stepped by the scallop law - `Δz = 2√(r·tol)` - so there is no
    /// stepdown field: the tolerance IS the depth increment's author
    /// (see [`crate::finishing3d`]). `Parallel`/`ParallelMulti`/
    /// `Scallop`: a Z-following raster over the shallow terrain (once
    /// along X, crossed along the axes, crossed along the diagonals),
    /// then the same scallop ladder's rings on the steep flanks - the
    /// 45° auto-split, one jurisdiction line for both halves. Region,
    /// heights and refusals follow the 3-D pocket's laws exactly:
    /// silhouette footprint, resolved top/bottom, mesh-computed
    /// everything.
    Finish3D {
        /// Which kinematic generates the path. Waterline is the
        /// M2.2a default; Parallel adds the raster (M2.2b),
        /// ParallelMulti and Scallop cross it (M2.2c).
        pattern: finishing3d::FinishPattern,
        /// Raster line spacing, mm; `None` (always for Waterline)
        /// follows the stepover cos-law from the tolerance instead -
        /// same "the tolerance is the author" law as the stepdown.
        stepover: Option<f64>,
        /// Surface deviation budget, mm, both directions: it sets the
        /// stepdown, the ring's scan resolution and the arc-fit chord
        /// together, and it is what the envelope verdict measures
        /// against.
        tolerance: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        /// Tip-Z of the first (highest) ring.
        top_z: Height,
        /// Tip-Z of the last ring - the model's lowest point the ball
        /// can touch (the generator's levels snap here).
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Chain the lines of one family with diagonal feeds instead
        /// of hops where the ground allows (M2.2e). `false` is every
        /// pre-ramp file: hop-to-clearance, unchanged.
        ramp: bool,
        /// Floor for the march's refinement, mm (M2.2e surface
        /// finish). `None` rides the march's own floor - what every
        /// pre-v18 file ran.
        min_step: Option<f64>,
        /// The steepest ramp descent, degrees (M2.2e surface
        /// finish). 45.0 is the law's own gift - shallow ground is
        /// exactly 1:1 - so the default certifies exactly what
        /// v17's chain certified. Inert while `ramp` is false.
        ramp_angle: f64,
    },
    /// Simultaneous 5-axis level finishing (M5.1 stage C): the waterline
    /// ring ladder a [`Self::Finish3D`] Waterline cuts, with a tool-axis
    /// pose stamped on every cutting move. The positions are the
    /// ball-center paths - a sphere's envelope is rotation-invariant;
    /// what makes the path 5-axis is where the spindle points: the
    /// contact normal, leaned `lead_deg` forward along the feed and
    /// `lag_deg` sideways across it, folded onto the cone of
    /// `max_tilt_deg` wherever that lean would pass the machine's
    /// reach ([`crate::multiaxis`]'s law). The angles exist to keep the
    /// shank clear of steep walls; the envelope verifier measures what
    /// the ball leaves, and the stage-A leaning band law judges the
    /// steel. Region, heights, levels and refusals follow the finish
    /// op's laws exactly.
    Contour5D {
        /// Surface deviation budget, mm - the same author as the
        /// finish op's: it sets the scallop stepdown, the scan
        /// resolution and the arc-fit chord, and it is what the
        /// envelope verdict measures against.
        tolerance: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        /// Tip-Z of the first (highest) ring.
        top_z: Height,
        /// Tip-Z of the last ring.
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Forward lean off the contact normal, degrees, in the
        /// direction of travel (the cutting literature's lead angle).
        lead_deg: f64,
        /// Sideways lean off the contact normal, degrees, away from
        /// the uncut wall (the literature's tilt/side-lean angle).
        lag_deg: f64,
        /// The cone half-angle the pose may never leave, degrees -
        /// the machine's tilt reach stated by the operation.
        max_tilt_deg: f64,
    },
    /// The raster sibling of [`Self::Contour5D`]: the Z-following
    /// parallel pass with the same axis law. No `pattern` field - the
    /// kind IS the choice of kinematic (the two-kinds-not-one-table
    /// decision, RECOMMENDED.md).
    Parallel5D {
        tolerance: f64,
        /// Raster line spacing, mm; `None` follows the stepover
        /// cos-law from the tolerance instead.
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// Ruled-surface (bladed) finishing (M5.1 stage D): the raster run
    /// ALONG the direction [`crate::multiaxis::ruling`] reads off the
    /// mesh, stepping across it at the same scallop law - and the tool
    /// LYING WITH the flank (axis tangent along the ruling, the
    /// side-machining contact) wherever a facet holds one. Where the
    /// surface says nothing of a ruling - floors, rims, domes between
    /// blades - the [`Self::Parallel5D`] lean law poses the move
    /// instead. A doubly-curved model, or one ruled only along Z,
    /// refuses by name and points at the kind that speaks for it.
    Swarf {
        tolerance: f64,
        /// Spacing ACROSS the ruling, mm; `None` follows the stepover
        /// cos-law from the tolerance instead.
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The swarf family's tolerance ladder (M5.1 stage D): the same
    /// path run coarse-first - pass i from the top at `tol·4^i`, every
    /// scallop law tolerance-authored, so each pass doubles the
    /// spacing and the stepdown - finishing at the nominal tolerance.
    /// Every pass's centers stand tangent to the MODEL, never to the
    /// previous pass, so a wrong pass count cannot hide a gouge.
    AdvancedSwarf {
        tolerance: f64,
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Coarse-to-fine passes, 1..4; 1 is the plain ladder alone.
        passes: u32,
    },
}

impl OpParams {
    #[must_use]
    pub fn kind(&self) -> OpType {
        match self {
            OpParams::Face { .. } => OpType::Face,
            OpParams::Contour { .. } => OpType::Contour,
            OpParams::Pocket { .. } => OpType::Pocket,
            OpParams::Slot { .. } => OpType::Slot,
            OpParams::Trace { .. } => OpType::Trace,
            OpParams::Drill { .. } => OpType::Drill,
            OpParams::Pocket3D { .. } => OpType::Pocket3D,
            OpParams::Finish3D { .. } => OpType::Finish3D,
            OpParams::Contour5D { .. } => OpType::Contour5D,
            OpParams::Parallel5D { .. } => OpType::Parallel5D,
            OpParams::Swarf { .. } => OpType::Swarf,
            OpParams::AdvancedSwarf { .. } => OpType::AdvancedSwarf,
        }
    }

    /// The 3+2 index, if this kind carries one (M5.1 stage B): the
    /// five planar kinds do, the rest answer None - an exhaustive
    /// match, so giving tilt to another kind means answering here
    /// with the compiler witnessing it.
    #[must_use]
    pub fn tilt(&self) -> Option<Tilt> {
        match self {
            OpParams::Face { tilt, .. }
            | OpParams::Contour { tilt, .. }
            | OpParams::Pocket { tilt, .. }
            | OpParams::Slot { tilt, .. }
            | OpParams::Trace { tilt, .. } => *tilt,
            // The 5-axis kinds (M5.1 C) are the third answer, not the
            // second: the PART stands still and the TOOL AXIS moves -
            // a 3+2 index inside them would command the machine to
            // hold a pose the stamp is already writing per move.
            OpParams::Drill { .. }
            | OpParams::Pocket3D { .. }
            | OpParams::Finish3D { .. }
            | OpParams::Contour5D { .. }
            | OpParams::Parallel5D { .. }
            | OpParams::Swarf { .. }
            | OpParams::AdvancedSwarf { .. } => None,
        }
    }

    /// The rapids plane, as expressed (every kind has one).
    #[must_use]
    pub fn clearance(&self) -> Height {
        match self {
            OpParams::Face { clearance_z, .. }
            | OpParams::Contour { clearance_z, .. }
            | OpParams::Pocket { clearance_z, .. }
            | OpParams::Slot { clearance_z, .. }
            | OpParams::Trace { clearance_z, .. }
            | OpParams::Drill { clearance_z, .. }
            | OpParams::Pocket3D { clearance_z, .. }
            | OpParams::Finish3D { clearance_z, .. }
            | OpParams::Contour5D { clearance_z, .. }
            | OpParams::Parallel5D { clearance_z, .. }
            | OpParams::Swarf { clearance_z, .. }
            | OpParams::AdvancedSwarf { clearance_z, .. } => *clearance_z,
        }
    }

    /// The cutting region's height expression: the section/stadium plane
    /// for the depth-bounded kinds, the skim (or first-stage) plane for
    /// Face. Resolve it for a Z; the viewport draws the region loops
    /// there.
    #[must_use]
    pub fn region_height(&self) -> Height {
        match self {
            OpParams::Face { top_z, cut_z, .. } => top_z.unwrap_or(*cut_z),
            OpParams::Contour { top_z, .. }
            | OpParams::Pocket { top_z, .. }
            | OpParams::Slot { top_z, .. }
            | OpParams::Trace { top_z, .. }
            | OpParams::Drill { top_z, .. }
            | OpParams::Pocket3D { top_z, .. }
            | OpParams::Finish3D { top_z, .. }
            | OpParams::Contour5D { top_z, .. }
            | OpParams::Parallel5D { top_z, .. }
            | OpParams::Swarf { top_z, .. }
            | OpParams::AdvancedSwarf { top_z, .. } => *top_z,
        }
    }

    /// The same set `stamp_with` feeds through `wh`/`wh_opt` - the two
    /// soup lists must agree, and the compiler enforces it here because
    /// every Height-typed field is visited exactly once (M3.1 C, the
    /// `stale_reason` height questions; a missed field is a silent
    /// blind spot, so keep new Height params in both lists).
    pub fn for_each_height<F: FnMut(Height)>(&self, mut f: F) {
        fn opt<F: FnMut(Height)>(h: &Option<Height>, f: &mut F) {
            if let Some(h) = h {
                f(*h);
            }
        }
        match self {
            OpParams::Face { clearance_z, cut_z, top_z, .. } => {
                f(*clearance_z);
                f(*cut_z);
                opt(top_z, &mut f);
            }
            OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                f(*retract_z);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Pocket { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Slot { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Drill { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                opt(bottom_z, &mut f);
            }
            OpParams::Pocket3D { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Finish3D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Contour5D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Parallel5D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Swarf { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::AdvancedSwarf { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
        }
    }


    // (the world-feeding stamp_with stays in the parent module)
}
