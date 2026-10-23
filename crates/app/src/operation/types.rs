//! Plain operation data: what an operation stores. The component
//! wrapper lives in [`crate::schema::Operation`]; nothing in this module
//! touches the world, an entity, or the schema - which is exactly the
//! law M4.1 R0 draws (docs/M4.1-CRATE-SPLIT.md): the plain types' home
//! imports no schema, so the wrappers and the data can part companies.
//! The world-facing methods of these same types (`resolve_height`,
//! `z`, `stamp_with`, `current_stamp`) stay in the parent module as
//! separate impl blocks.
//!
//! Provenance: our own domain model; the From-reference/offset height
//! expression is general CAM practice (public documentation of
//! heights semantics), no vendor internals.

/// Work coordinate system slots - the machine offsets posted as G54..G59.
pub const WCS_SLOTS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];

use crate::drilling::{CycleChoice, Hole};
use crate::finishing3d;
use crate::geo2d::Point2;
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::machining3d::Entry3D;

/// Where the operation's boundary geometry comes from. Names, never
/// entity ids (AGENTS.md: identity by name keeps commands invertible).
#[derive(Clone, Debug, PartialEq)]
pub enum SrcRef {
    /// The setup's stock solid.
    Stock,
    /// Another named entity's geometry.
    Entity(String),
    /// A feature selection: faces and edge chains picked off named
    /// entities, resolved by their geometry-hash ids at generation
    /// (M1.5). An empty list is refused by the command layer.
    Selection(Vec<GeoSel>),
}

/// One selected feature of one named entity. Identity is the feature's
/// geometry hash, not an index or a facet count: re-imports, facet
/// reorder and rigid moves all keep a pick pointing at the same feature
/// (see `geometry::feature`'s stable-id design).
#[derive(Clone, Debug, PartialEq)]
pub struct GeoSel {
    pub entity: String,
    pub feature: FeatureSel,
}

/// What kind of feature was picked; the numbers are [`FaceId`] and
/// [`ChainId`] hashes, never positional.
///
/// [`FaceId`]: crate::geometry::feature::FaceId
/// [`ChainId`]: crate::geometry::feature::ChainId
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureSel {
    Face(crate::geometry::feature::FaceId),
    Chain(crate::geometry::feature::ChainId),
    /// One logical face of a smooth wall: every planar strip the wall was
    /// tessellated into, welded by their shared `facet_patch` root (see
    /// [`patch_rings`]). Only exists where that weld has more than one
    /// strip - a flat face is already whole, and stays a `Face`.
    ///
    /// [`patch_rings`]: crate::geometry::feature::patch_rings
    Patch(crate::geometry::feature::PatchId),
}

impl core::fmt::Display for FeatureSel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FeatureSel::Face(id) => write!(f, "face {id:#x}"),
            FeatureSel::Chain(id) => write!(f, "edge chain {id:#x}"),
            FeatureSel::Patch(id) => write!(f, "welded face {id:#x}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
    Face,
    Contour,
    Pocket,
    Slot,
    Trace,
    Drill,
    /// Mesh-driven Z-level pocketing: the model is the final part, every
    /// level's islands are computed from it (see [`crate::machining3d`]).
    Pocket3D,
    /// Mesh-driven 3D finishing: ball-nose waterline rings at the scallop
    /// stepdown (see [`crate::finishing3d`]).
    Finish3D,
}

impl core::fmt::Display for OpType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            OpType::Face => "Face",
            OpType::Contour => "2D Contour",
            OpType::Pocket => "2D Pocket",
            OpType::Slot => "Slot",
            OpType::Trace => "Trace",
            OpType::Drill => "Drill",
            OpType::Pocket3D => "3D Pocket",
            OpType::Finish3D => "3D Finish",
        };
        f.write_str(s)
    }
}

/// What a height is measured from. Every source but [`HeightReference::Fixed`]
/// is a world truth re-read at resolve time - the stock and model bounding
/// boxes and the work-offset slot placements - so a height that follows the
/// moved setup is expressed, never stale. Rotations are not part of a Z
/// reference: only the source's height feeds an offset (a full WCS frame
/// transform is out of 2D scope, like 3D itself).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeightReference {
    /// The offset is the Z itself, in world (model) coordinates. What
    /// pre-reference files and hand-typed absolute values carry.
    Fixed,
    TopOfStock,
    BottomOfStock,
    TopOfModel,
    BottomOfModel,
    /// The Z=0 plane of a work-offset slot: `WCS_SLOTS[i]`'s origin. An
    /// existing-but-unplaced slot reads as the world origin, exactly as
    /// the rest of the world treats an entity nobody has moved.
    WorkOffset(usize),
}

impl HeightReference {
    /// Panel combo entries, in this order; WorkOffset gets its slot name
    /// appended by the panel.
    pub const KIND_LABELS: [&'static str; 6] = [
        "Fixed value",
        "Top of stock",
        "Bottom of stock",
        "Top of model",
        "Bottom of model",
        "Work offset",
    ];

    /// Which entry of [`Self::KIND_LABELS`] this reference's kind is.
    #[must_use]
    pub fn kind_index(&self) -> usize {
        match self {
            HeightReference::Fixed => 0,
            HeightReference::TopOfStock => 1,
            HeightReference::BottomOfStock => 2,
            HeightReference::TopOfModel => 3,
            HeightReference::BottomOfModel => 4,
            HeightReference::WorkOffset(_) => 5,
        }
    }

    /// The slot index for WorkOffset references, 0 for everything else.
    #[must_use]
    pub fn wcs_slot(&self) -> usize {
        match self {
            HeightReference::WorkOffset(i) => *i,
            _ => 0,
        }
    }

    /// Replace the slot part, keeping the kind - only meaningful on a
    /// WorkOffset reference.
    #[must_use]
    pub fn with_wcs_slot(&self, i: usize) -> Self {
        match self {
            HeightReference::WorkOffset(_) => HeightReference::WorkOffset(i),
            other => *other,
        }
    }
}

impl core::fmt::Display for HeightReference {
    /// Chip text for a plane label: short, and the G-code word for a
    /// work offset so it names what the user selected in Setup.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HeightReference::Fixed => f.write_str("fixed"),
            HeightReference::TopOfStock => f.write_str("stock top"),
            HeightReference::BottomOfStock => f.write_str("stock bottom"),
            HeightReference::TopOfModel => f.write_str("model top"),
            HeightReference::BottomOfModel => f.write_str("model bottom"),
            HeightReference::WorkOffset(i) => match WCS_SLOTS.get(*i) {
                Some(name) => f.write_str(name),
                None => write!(f, "work offset #{i}"),
            },
        }
    }
}

/// One height: From-reference plus offset in mm (HSMWorks-style "From:
/// model top, Offset: 0.5"). The stored truth is the expression, never a
/// resolved Z - so a height cannot disagree with the setup it references.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Height {
    pub reference: HeightReference,
    pub offset: f64,
}

impl Height {
    /// A height that stands on its own: the value is the world Z.
    #[must_use]
    pub fn fixed(z: f64) -> Self {
        Height { reference: HeightReference::Fixed, offset: z }
    }

    #[must_use]
    pub fn of(reference: HeightReference, offset: f64) -> Self {
        Height { reference, offset }
    }

    /// Structural sanity, no world: a Fixed height must be finite, and
    /// any offset must be finite. Ordering rules run on resolved values
    /// ([`crate::operation::depth_sane`]).
    /// Structural sanity: a finite offset, and a WorkOffset reference
    /// naming a slot that exists. Ordering lives on resolved values
    /// ([`crate::operation::depth_sane`]).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.offset.is_finite() && self.reference.wcs_slot() < WCS_SLOTS.len()
    }

    #[must_use]
    pub fn is_fixed(&self) -> bool {
        matches!(self.reference, HeightReference::Fixed)
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
            | OpParams::Finish3D { clearance_z, .. } => *clearance_z,
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
            | OpParams::Finish3D { top_z, .. } => *top_z,
        }
    }

    /// Shallow sanity for command construction, no world: finite numbers,
    /// positive increments, in-range slots. Ordering needs resolved
    /// heights and runs in [`crate::operation::depth_sane`] wherever a command holds the
    /// world; the generators own the deep rules (stepover vs diameter,
    /// lead sizes), which still fire inside `GenerateOperation`'s
    /// constructor.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        match self {
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && clearance_z.is_sane()
                    && cut_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && top_z.is_none_or(|t| t.is_sane())
                    && stepdown.is_none_or(|s| s.is_finite() && s > 0.0)
            }
            OpParams::Contour {
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                lead,
                tolerance,
                tab_width,
                tab_height,
                ..
            } => {
                nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*tolerance)
                    && nneg(*tab_width)
                    && nneg(*tab_height)
                    && lead_sane(lead)
            }
            OpParams::Pocket {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Pocket3D {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                max_engagement,
                ..
            } => {
                // Structural only, like the pocket's: the helix radius
                // is checked against the tool radius by the generator,
                // which is the only reader that owns a tool. A Plunge
                // entry never consults it, so any positive finite value
                // - the default 1.0 among them - is sane here. Same
                // law for the engagement angle: it must be finite
                // always, and inside its own (0, 90] only when the
                // Adaptive strategy actually reads it - the generator
                // repeats the bound as the refusal of record.
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (max_engagement.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive)
                            || (*max_engagement > 0.0 && *max_engagement <= 90.0)))
            }
            OpParams::Finish3D {
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                min_step,
                ramp_angle,
                ..
            } => {
                // No stepdown to check: the scallop law derives it from
                // tolerance and the tool radius, and the generator's
                // level cap refuses a tolerance that would need more
                // rings than the machine of record may hold.
                pos(*tolerance)
                    && stepover.is_none_or(pos)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && min_step.is_none_or(pos)
                    // (0, 90): a flat ramp never descends - that is
                    // a traverse with a name it doesn't earn; a
                    // vertical one is a plunge, and the run's first
                    // contact already owns those. Both refusals
                    // happen at the gate, never at the emitter.
                    && (0.0..90.0).contains(ramp_angle)
            }
            OpParams::Slot {
                a,
                b,
                width,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                stepover,
                feed,
                plunge_feed,
                ..
            } => {
                a.x.is_finite()
                    && a.y.is_finite()
                    && b.x.is_finite()
                    && b.y.is_finite()
                    && pos(*width)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*stepover)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                peck,
                dwell,
                feed,
                plunge_feed,
                holes,
                ..
            } => {
                // An empty hole list is sane here: a new Drill op exists
                // before its holes are detected, and creation must be
                // allowed to get the panel on screen. Generation is the
                // gate that refuses it, by name.
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_none_or(|b| b.is_sane())
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && peck.is_finite()
                    && *peck >= 0.0
                    && dwell.is_finite()
                    && *dwell >= 0.0
                    && holes.iter().all(|h| h.is_usable())
            }
        }
    }

    /// Every height expression these parameters carry, in field order.
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
            OpParams::Finish3D { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
        }
    }


    // (the world-feeding stamp_with stays in the parent module)
}

fn lead_sane(lead: &Lead) -> bool {
    match lead {
        Lead::None => true,
        Lead::Linear { length } => length.is_finite() && *length >= 0.0,
        Lead::Arc { radius } => radius.is_finite() && *radius > 0.0,
        Lead::Tangential { radius } => radius.is_finite() && *radius > 0.0,
    }
}

/// The digest of everything a generated path depends on. Not a
/// cross-version identifier - the algorithm behind it is `DefaultHasher`,
/// stable within a build, and a spurious mismatch after a toolchain bump
/// only asks for a re-generate, never cuts anything wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenStamp(pub u64);

/// What the tree and properties panel show beside an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpStatus {
    /// Never generated (or the cache is gone).
    Never,
    /// Cached, and every input still matches the stamp taken at generation.
    Ok,
    /// Cached, but some input moved. The path on screen may not match the
    /// model or tool behind it.
    Stale,
}

