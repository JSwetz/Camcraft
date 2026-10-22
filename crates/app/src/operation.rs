//! Machining operations: what to cut, from where, and the generation
//! orchestration that turns an operation into a verified toolpath.
//!
//! Provenance: our own domain model - operation types, source geometry and
//! staleness tracking are specced from public documentation of commercial
//! CAM workflows; the algorithms invoked here live in `machining2d` and
//! `verify` with their own provenance. No vendor source was consulted.
//!
//! The operation is data (`SrcRef`, `OpParams`, `GenStamp`); the behavior
//! is the two free functions at the bottom:
//!
//! - [`generate`] resolves tool and source geometry in the current world,
//!   builds the cutting region, runs the matching `machining2d` generator
//!   with the tool's diameter injected, and verifies the result. Pure read:
//!   it never touches the world, so commands can run it at construction
//!   and replay its stored output byte-identically through undo/redo.
//! - [`current_stamp`] digests everything the cached path depends on.
//!   Staleness is derived from a stamp mismatch (AGENTS.md: no stored
//!   `Stale` flag), so an edit anywhere the digest reads - tool geometry,
//!   stock size and placement, model move, parameters - turns the cache
//!   grey without any command writing a status.
//!
//! Region rule per operation, at the level the paths live in:
//! - Face: the XY silhouette of the source (`slice::project`), cut plane
//!   from `cut_z`.
//! - Contour / Pocket / Trace: the source's cross-section at `top_z`
//!   (`slice::slice`, half-open; a grazing slice is retried one micron in).
//! - Slot: the closed-form stadium between the two endpoints.
//! - Drill: the XY silhouette again, plus one rim loop per hole whose rim
//!   the projection did not already carry - the rim is what makes the
//!   gouge law's tangency test mean something at a plunge.
//!
//! Meshes are sliced because STEP import is tessellate-only (no curve data
//! survives), which is also what makes STL, OBJ and STEP sources take the
//! identical path.
//!
//! A `SrcRef::Selection` builds its region from the picked features
//! instead: faces contribute their boundary loops projected to XY, closed
//! edge chains their ring projected to XY, coincident rims dedupe. Face
//! ops demand a near-horizontal face, Pocket demands a closed boundary,
//! and an open run is either a Contour centerline pass along its points
//! or, for a facing op, a limit extended out to the stock (M1.5.1) -
//! never a silent drop. Preview and generation read the same walk, so
//! the two cannot disagree about where that pass or that boundary runs.

use std::collections::HashMap;
use std::hash::Hasher;

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::drilling::{self, CycleChoice, Hole};
use crate::finishing3d::{self, Finish3DParams};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::feature::{self, LoopKind};
use crate::geometry::revision;
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, transform_point};
use crate::machining2d::{
    self, ContourParams, FaceParams, GenError, Lead, PocketParams, PocketStrategy, Side,
    SlotParams, TraceParams,
};
use crate::machining3d::{self, Entry3D, Pocket3DParams};
use crate::model;
use crate::schema::Operation;
use crate::stock::StockShape;
use crate::verify::{self, Mode, VerifyContext, Warning};

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
            HeightReference::WorkOffset(i) => match crate::schema::WCS_SLOTS.get(*i) {
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

    /// Resolve against the current world; None when the source is not
    /// available (no stock or model yet, no entity in the slot). The
    /// default setup's world - a caller standing in another setup uses
    /// [`Self::z_for`] (M3.2 stage A).
    #[must_use]
    pub fn z(&self, world: &flecs_ecs::prelude::World) -> Option<f64> {
        self.z_for(world, None)
    }

    /// The same resolve in a declared setup context ([`crate::setup`]).
    #[must_use]
    pub fn z_for(
        &self,
        world: &flecs_ecs::prelude::World,
        setup: Option<&str>,
    ) -> Option<f64> {
        resolve_height(world, setup, *self)
    }

    /// Structural sanity, no world: a Fixed height must be finite, and
    /// any offset must be finite. Ordering rules run on resolved values
    /// ([`depth_sane`]).
    /// Structural sanity: a finite offset, and a WorkOffset reference
    /// naming a slot that exists. Ordering lives on resolved values
    /// ([`depth_sane`]).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.offset.is_finite() && self.reference.wcs_slot() < crate::schema::WCS_SLOTS.len()
    }

    #[must_use]
    pub fn is_fixed(&self) -> bool {
        matches!(self.reference, HeightReference::Fixed)
    }
}

/// Resolve one height to a world Z. `Fixed` reads through; the other
/// sources ask the world for the current geometry (model bounds exclude
/// stock and WCS by `solid_world_aabb`'s rule; a slot entity that exists
/// but was never moved reads at the origin, matching `transform_of`'s
/// own default-everything semantics).
/// `setup` is the declared setup of the asking operation (`None` = the
/// first setup, which in a document without setups is the old global
/// lookup exactly - the implicit-setup law of [`crate::setup`]). The
/// model bounds are document-wide: the part is the part whatever setup
/// holds it; only the stock box and the offset planes follow the setup.
#[must_use]
pub fn resolve_height(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    h: Height,
) -> Option<f64> {
    let base = match h.reference {
        HeightReference::Fixed => return (h.offset.is_finite()).then_some(h.offset),
        HeightReference::TopOfStock => model::stock_world_aabb_for(world, setup)?.max.z,
        HeightReference::BottomOfStock => model::stock_world_aabb_for(world, setup)?.min.z,
        HeightReference::TopOfModel => model::solid_world_aabb(world)?.max.z,
        HeightReference::BottomOfModel => model::solid_world_aabb(world)?.min.z,
        HeightReference::WorkOffset(i) => {
            let entity = crate::setup::wcs_entity(world, setup, i)?;
            model::transform_of(world, &entity).unwrap_or_default().pos.z
        }
    };
    (base + h.offset).is_finite().then_some(base + h.offset)
}

/// What a reference is missing, phrased for a diagnostic: the user can
/// act on "no stock yet". None when the source stands.
#[must_use]
pub fn missing_source(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    r: HeightReference,
) -> Option<&'static str> {
    match r {
        HeightReference::Fixed => None,
        HeightReference::TopOfStock | HeightReference::BottomOfStock => {
            model::stock_world_aabb_for(world, setup)
                .is_none()
                .then_some("no stock yet - set it up in Setup > Stock")
        }
        HeightReference::TopOfModel | HeightReference::BottomOfModel => {
            model::solid_world_aabb(world).is_none().then_some("no model loaded")
        }
        HeightReference::WorkOffset(i) => match crate::setup::wcs_entity_name(world, setup, i) {
            None => Some("work offset slot out of range (G54..G59)"),
            Some(name) => model::find_by_name(world, &name)
                .is_none()
                .then_some("that work offset has no origin yet - place it in Setup"),
        },
    }
}

/// The Z the operation's region lives at, resolved: the section/stadium
/// plane for the depth kinds, the skim (or first stage) plane for Face.
/// None when that height's reference has no source.
#[must_use]
pub fn region_z(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> Option<f64> {
    match params {
        OpParams::Face { top_z, cut_z, .. } => top_z
            .map(|t| resolve_height(world, setup, t))
            .unwrap_or_else(|| resolve_height(world, setup, *cut_z)),
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Slot { top_z, .. }
        | OpParams::Trace { top_z, .. }
        | OpParams::Drill { top_z, .. }
        | OpParams::Pocket3D { top_z, .. }
        | OpParams::Finish3D { top_z, .. } => resolve_height(world, setup, *top_z),
    }
}

/// Every height of one params set, resolved at once for a generation or
/// a panel pass. Err carries the field name and the source that is
/// missing, so the refusal is actionable by name - never a guessed z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedHeights {
    /// Rapids plane, every kind.
    pub clearance: f64,
    /// Depth kinds: section top. Face: None (it has [`Self::face_top`]).
    pub top: Option<f64>,
    /// Depth kinds: last level. Face: the finished cut plane.
    pub bottom: Option<f64>,
    /// Face: first stage when stepped, None for one-level facing.
    pub face_top: Option<f64>,
    /// Contour resolves Some; pocket-family None hops at clearance.
    pub retract: Option<f64>,
}

impl ResolvedHeights {
    /// The height-plane Zs of one kind: (top, bottom/cut) as the panels
    /// and verify want them, from the resolved bag.
    #[must_use]
    pub fn depth(&self) -> (Option<f64>, Option<f64>) {
        if self.face_top.is_some() || self.top.is_some() {
            (self.top.or(self.face_top), self.bottom)
        } else {
            (self.face_top, self.bottom)
        }
    }
}

/// Resolve every height in `params`; Err is "<field> (From: <ref> <offset>):
/// <why>" - each refusal names the height and its source.
pub fn resolve_params(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> Result<ResolvedHeights, String> {
    let req = |what: &str, h: Height| -> Result<f64, String> {
        resolve_height(world, setup, h).ok_or_else(|| unresolved_msg(what, h, world, setup))
    };
    let opt = |what: &str, h: Option<Height>| -> Result<Option<f64>, String> {
        h.map(|v| req(what, v)).transpose()
    };
    let (clearance, top, bottom, face_top, retract) = match params {
        OpParams::Face { clearance_z, cut_z, top_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            None,
            Some(req("Cut Z", *cut_z)?),
            opt("Top Z", *top_z)?,
            None,
        ),
        OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            Some(req("Retract Z", *retract_z)?),
        ),
        OpParams::Pocket { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Slot { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Pocket3D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Finish3D { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            opt("Retract Z", *retract_z)?,
        ),
        // The drill bottom is an override of the whole pattern, not a
        // requirement: each hole's own floor is the default depth, so an
        // unresolved (None) bottom is the normal shape, not a failure.
        OpParams::Drill { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            opt("Bottom Z", *bottom_z)?,
            None,
            opt("Retract Z", *retract_z)?,
        ),
    };
    Ok(ResolvedHeights { clearance, top, bottom, face_top, retract })
}

/// The resolved depth pair for the verify context and the level list:
/// (top, bottom), Face folded to (cut, cut-or-top) - None parts stay
/// None, never guessed.
#[must_use]
pub fn resolve_map(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> (Option<f64>, Option<f64>) {
    match resolve_params(world, setup, params) {
        Ok(r) => {
            if matches!(params, OpParams::Face { .. }) {
                let cut = r.bottom;
                (cut, cut)
            } else {
                (r.top, r.bottom)
            }
        }
        Err(_) => (None, None),
    }
}

/// Ordering sanity on resolved values: an operation whose top is not
/// above its bottom (or whose finished cut is not below its first stage)
/// is refused at command creation. A height that cannot resolve is not
/// here's business: `resolve_params` refuses it at generation, by name.
#[must_use]
pub fn depth_sane(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    params: &OpParams,
) -> bool {
    let Ok(r) = resolve_params(world, setup, params) else {
        return true; // unresolved is refused elsewhere, never a false lie here
    };
    match params {
        OpParams::Face { .. } => match (r.face_top, r.bottom) {
            (Some(t), Some(c)) => t >= c,
            _ => true,
        },
        _ => match (r.top, r.bottom) {
            (Some(t), Some(b)) => t > b,
            _ => true,
        },
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
    /// heights and runs in [`depth_sane`] wherever a command holds the
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

    /// The digest-relevant scalars, written to a hasher in a fixed
    /// order. A height contributes its reference (tag, plus slot for a
    /// work offset) and its offset bits; a non-Fixed reference ALSO
    /// contributes its resolved Z (or an unresolved marker). The source
    /// digests in `current_stamp` only cover the operation's own source
    /// entity - a stock-sourced Face with a `TopOfModel` cut height must
    /// go Stale when a model moves, and nothing else feeds that world in.
    /// Resolved Zs are not double-counted for Fixed heights: their
    /// offset bits are the whole truth.
    fn stamp_with(
        &self,
        world: &World,
        setup: Option<&str>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| {
            let (tag, slot) = match v.reference {
                HeightReference::Fixed => (0u8, 0u8),
                HeightReference::TopOfStock => (1, 0),
                HeightReference::BottomOfStock => (2, 0),
                HeightReference::TopOfModel => (3, 0),
                HeightReference::BottomOfModel => (4, 0),
                HeightReference::WorkOffset(i) => (5, i.min(255) as u8),
            };
            h.write_u8(tag);
            h.write_u8(slot);
            w(h, v.offset);
            if tag != 0 {
                match resolve_height(world, setup, v) {
                    Some(z) => {
                        h.write_u8(1);
                        w(h, z);
                    }
                    None => h.write_u8(0),
                }
            }
        };
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| match v {
            Some(v) => {
                h.write_u8(1);
                wh(h, v);
            }
            None => h.write_u8(0),
        };
        let wp2 = |h: &mut std::hash::DefaultHasher, p: Point2| {
            w(h, p.x);
            w(h, p.y);
        };
        match self {
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
            } => {
                h.write_u8(0);
                w(h, *stepover);
                wh(h, *clearance_z);
                wh(h, *cut_z);
                w(h, *feed);
                w(h, *plunge_feed);
                h.write_u8(u8::from(*climb));
                // Appended at v5-era staging: presence first, then the
                // value. Even a legacy (None, None) face digest differs
                // from a pre-stamp face op - the mechanism saying so.
                wh_opt(h, *top_z);
                match *stepdown {
                    Some(v) => {
                        h.write_u8(1);
                        w(h, v);
                    }
                    None => h.write_u8(0),
                }
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
                side,
                tolerance,
                tab_count,
                tab_width,
                tab_height,
                climb,
            } => {
                h.write_u8(1);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *retract_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *tolerance, *tab_width, *tab_height] {
                    w(h, v);
                }
                h.write_u32(*tab_count);
                h.write_u8(u8::from(*climb));
                match lead {
                    Lead::None => h.write_u8(0),
                    Lead::Linear { length } => {
                        h.write_u8(1);
                        w(h, *length);
                    }
                    Lead::Arc { radius } => {
                        h.write_u8(2);
                        w(h, *radius);
                    }
                    Lead::Tangential { radius } => {
                        h.write_u8(3);
                        w(h, *radius);
                    }
                }
                // Appended at the v7-era side field: even a pre-Side
                // contour op digests differently - the mechanism saying
                // so, so nothing quietly reinterprets a committed path
                // under the new Auto law.
                h.write_u8(match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                });
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
                climb,
                strategy,
            } => {
                h.write_u8(2);
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    PocketStrategy::Adaptive => 3,
                });
                // v5-era hop plane: presence, then the value (same shape
                // as Face's staging fields appended above).
                wh_opt(h, *retract_z);
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
                climb,
                strategy,
                entry,
                helix_radius,
                rest_rough,
                max_engagement,
                feed_opt,
            } => {
                h.write_u8(6);
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *helix_radius, *max_engagement] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    PocketStrategy::Adaptive => 3,
                });
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                });
                wh_opt(h, *retract_z);
                // v12-era rest pass: the flag is the generator's
                // switch, so it belongs in the digest (a model edit
                // must mark a rest path Stale like any other).
                h.write_u8(u8::from(*rest_rough));
                // Same law for the v14 feed modulation: flipping it
                // changes every F word, so it changes the stamp.
                h.write_u8(u8::from(*feed_opt));
            }
            OpParams::Finish3D {
                pattern,
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                ramp,
                min_step,
                ramp_angle,
            } => {
                h.write_u8(7);
                // v16: the pattern chooses a different kinematic (and
                // the raster fields only answer to Parallel), so they
                // belong in the digest; the Some/None split keeps a
                // derived stepover from digesting as the number 0.
                h.write_u8(*pattern as u8);
                match stepover {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                // v17 (M2.2e): ramp changes the chain between lines,
                // so it belongs in the digest; the one-time staleness
                // it hands old ops is the step-up law, same as ever.
                h.write_u8(u8::from(*ramp));
                // v18 (M2.2e): the surface-finish pair. min_step
                // floors the march's refinement (same Some/None
                // split as stepover above - None must not digest as
                // the number 0), ramp_angle caps the chain's slope
                // - both change moves, both belong in the digest.
                match min_step {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                w(h, *ramp_angle);
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
                climb,
            } => {
                h.write_u8(3);
                wp2(h, *a);
                wp2(h, *b);
                w(h, *width);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *stepover, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
            } => {
                h.write_u8(4);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                cycle,
                peck,
                dwell,
                feed,
                plunge_feed,
                holes,
            } => {
                h.write_u8(5);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh_opt(h, *bottom_z);
                wh_opt(h, *retract_z);
                for v in [*feed, *plunge_feed, *peck, *dwell] {
                    w(h, v);
                }
                h.write_u8(match cycle {
                    CycleChoice::Auto => 0,
                    CycleChoice::Spot => 1,
                    CycleChoice::Drill => 2,
                    CycleChoice::Peck => 3,
                    CycleChoice::Tap => 4,
                    CycleChoice::Bore => 5,
                    CycleChoice::BackBore => 6,
                });
                // The hole list is params data, so it digests: recognition
                // output, a typed edit, or a moved model (whose re-
                // detect changes the numbers) all mark the op Stale
                // through here or through the source arm.
                h.write_u32(holes.len() as u32);
                for hole in holes {
                    wp2(h, hole.xy);
                    w(h, hole.radius);
                    w(h, hole.entry_z);
                    w(h, hole.floor_z);
                    h.write_u8(u8::from(hole.through));
                }
            }
        }
    }
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

/// Inputs an operation depends on: tool (name and numerics), parameters,
/// and the source's placement and shape. `None` when a referenced entity
/// is gone - an unresolvable digest is never equal to a stored one, which
/// is exactly the "Stale" reading a deleted tool should give.
/// The digest of an operation's inputs as they stand right now.
/// `op_name` is the operation's own name: re-machining digests the
/// program AHEAD of it (M3.1 D), and program order is name order, so
/// the name is part of the question. Callers that build a stamp for an
/// operation that has no entity yet (or none that matters) pass `""`,
/// which reads as "no predecessors" and writes no rest term.
#[must_use]
/// `setup` is the operation's declared setup (M3.2 A): the stock entity,
/// the resolved heights and the re-machining mask all read it, so an
/// operation that switches setups stamps like the world it would
/// machine. `None` is the first setup - bit-for-bit the old stamp in a
/// document without setups (the implicit-setup law).
pub fn current_stamp(
    world: &World,
    tool_name: &str,
    src: &SrcRef,
    params: &OpParams,
    op_name: &str,
    setup: Option<&str>,
) -> Option<GenStamp> {
    let mut h = std::hash::DefaultHasher::new();
    // v2: heights became From+Offset expressions and their resolved
    // sources feed the digest - every op cached under v1 reads Stale
    // once and clears on regenerate (same one-time churn as the
    // b3eaebc stamp move, the mechanism working, not broken).
    h.write(b"hsmw-op-stamp-v2");
    let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
    h.write(tool_name.as_bytes());
    h.write_u8(0xff);
    let tool = model::find_by_name(world, tool_name)
        .and_then(|e| model::tool_of(world, &e))?;
    h.write_u8(match tool.kind {
        crate::tools::ToolKind::EndMill => 0,
        crate::tools::ToolKind::BallNose => 1,
        crate::tools::ToolKind::BullNose => 2,
        crate::tools::ToolKind::Drill => 3,
        crate::tools::ToolKind::Tap => 4,
        crate::tools::ToolKind::Bore => 5,
    });
    for v in [
        tool.diameter,
        tool.corner_r,
        tool.flute_len,
        tool.shank_d,
        tool.shank_len,
        tool.holder_d,
        tool.holder_len,
    ] {
        w(&mut h, v);
    }
    // Kind-scoped numbers: a tool digests only what its kind reads. A
    // tip_angle parked on a ball nose is inert data, and inert data must
    // never make an operation look stale - the same law the profile and
    // the cycles draw by. It also keeps every pre-M1.6 stamp value
    // unchanged: no one-time churn for the milling kinds.
    match tool.kind {
        crate::tools::ToolKind::Drill => w(&mut h, tool.tip_angle),
        crate::tools::ToolKind::Tap => w(&mut h, tool.pitch),
        _ => {}
    }
    params.stamp_with(world, setup, &mut h);
    match src {
        SrcRef::Stock => {
            h.write_u8(b'S');
            let stock_name = crate::setup::stock_entity_name(world, setup);
            let entity = model::find_by_name(world, &stock_name)?;
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]]
            {
                w(&mut h, v);
            }
            match model::stock_def_of(world, &entity)? {
                StockShape::Box { dx, dy, dz } => {
                    h.write_u8(0);
                    for v in [dx, dy, dz] {
                        w(&mut h, v);
                    }
                }
                StockShape::Cylinder { diameter, height } => {
                    h.write_u8(1);
                    w(&mut h, diameter);
                    w(&mut h, height);
                }
                StockShape::FromModel { source } => {
                    h.write_u8(2);
                    h.write(source.as_bytes());
                    // M3.1 B: the name is provenance; what gets milled is
                    // the stock's own copy, so the copy is what the digest
                    // reads. UpdateModel re-derives the copy inside the
                    // update command (the propagation law), so an update to
                    // the source lands here as a moved signature - and a
                    // stock whose source was deleted stays honestly Ok,
                    // because the material it describes never changed.
                    let tris = model::facets_of(world, &entity)?;
                    h.write_u64(revision::signature(&model::features_of_local(&tris)));
                }
            }
        }
        SrcRef::Entity(name) => {
            h.write_u8(b'E');
            h.write(name.as_bytes());
            let entity = model::find_by_name(world, name)?;
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]]
            {
                w(&mut h, v);
            }
            // M3.1 A: the revision signature, not count + bounds. Facet
            // count and AABB are coincidences waiting to happen - a solid
            // that keeps both while moving material (the diamond where the
            // cube was) must read Stale, and a re-tessellation of the same
            // faces must not. Face ids are local-frame content hashes, so
            // the transform hashed above carries all the placement.
            let tris = model::facets_of(world, &entity)?;
            h.write_u64(revision::signature(&model::features_of_local(&tris)));
        }
        SrcRef::Selection(items) => {
            // b'L', not b'S': the Stock arm already owns that byte, and a
            // digest that collides across source kinds is a staleness bug.
            h.write_u8(b'L');
            h.write_usize(items.len());
            let mut digested: Vec<&str> = Vec::new();
            for g in items {
                h.write_u8(match g.feature {
                    FeatureSel::Face(_) => 0,
                    FeatureSel::Chain(_) => 1,
                    FeatureSel::Patch(_) => 2,
                });
                h.write_usize(g.entity.len());
                h.write(g.entity.as_bytes());
                let id = match g.feature {
                    FeatureSel::Face(id) | FeatureSel::Chain(id) | FeatureSel::Patch(id) => id,
                };
                h.write_u64(id);
                // The pick resolves against this entity's LOCAL features
                // (ids are transform-free) and projects through its
                // transform, so both belong in the digest. The face-id
                // signature - M3.1's ids-as-signatures law, the same one
                // the Entity arm reads - keeps a re-import whose faces
                // survive fresh (a re-tessellation cannot move a pick),
                // and any real change moves ids, so it goes stale. Face
                // ids are content hashes of their facet positions, so a
                // different mesh that repeats the facet count cannot hide.
                let entity = model::find_by_name(world, &g.entity)?;
                if !digested.contains(&g.entity.as_str()) {
                    digested.push(g.entity.as_str());
                    let loc = model::transform_of(world, &entity).unwrap_or_default();
                    for v in [
                        loc.pos.x,
                        loc.pos.y,
                        loc.pos.z,
                        loc.rot[0],
                        loc.rot[1],
                        loc.rot[2],
                        loc.rot[3],
                    ] {
                        w(&mut h, v);
                    }
                    let tris = model::facets_of(world, &entity)?;
                    h.write_u64(revision::signature(&model::features_of_local(&tris)));
                }
            }
        }
    }
    // M3.1 D re-machining: the program AHEAD is an input too - a 3D
    // pocket consumes what earlier operations leave standing, so when
    // their recorded PATHS or their order move, its cached path no
    // longer describes the same cut. With nothing ahead of it nothing
    // is written: first operations' and other kinds' stamps are
    // bit-for-bit what they were before stage D (the inert-data law).
    if params.kind() == OpType::Pocket3D
        && let Some(digest) = crate::rest::prior_digest(world, op_name, setup)
    {
        h.write_u8(b'R');
        h.write_u64(digest);
    }
    // M3.2 B what-the-program-clamps-to is deliberately NOT here.
    // Workholding - user fixtures and the machine-derived table and
    // spindle bands - rides the post's LIVE verification gate, exactly
    // like the M2.3 machine limits: "load the roomy one and the same
    // program still posts" is the user-facing law, and the gate
    // re-checks the steel here and now, so a moved vise can never post
    // silently without churning a single stamp. Generation still
    // reports fixture warnings early (the verify call below reads the
    // boxes live); staleness from workholding is the failure mode this
    // keeps out (see RECOMMENDED.md, the M2.3-law extension).
    Some(GenStamp(h.finish()))
}

/// Derived status of one entity: never stored, always recomputed from the
/// cache, the stamp and the live world (AGENTS.md: commands own fields).
#[must_use]
pub fn status_of(world: &World, entity: &flecs_ecs::prelude::Entity) -> OpStatus {
    let Some(op) = model::operation_of(world, entity) else {
        return OpStatus::Never;
    };
    let Some(cache) = model::toolpath_of(world, entity) else {
        return OpStatus::Never;
    };
    if cache.path.is_empty() {
        return OpStatus::Never; // a cache with no moves is no cache
    }
    let name = model::name_of(world, entity).unwrap_or_default();
    match (
        op.stamp,
        current_stamp(
            world,
            &op.tool,
            &op.src,
            &op.params,
            &name,
            op.setup.as_deref(),
        ),
    ) {
        (Some(stored), Some(now)) if stored == now => OpStatus::Ok,
        _ => OpStatus::Stale,
    }
}

/// Why an operation reads Stale, phrased for a user who can act on it
/// (M3.1 C). The stamp is a single digest, so it cannot itself say what
/// moved; this asks the world the cheap live questions instead - the
/// ones whose answer is true independent of history: is the tool still
/// there, does each referenced entity still exist, does each picked
/// feature id still exist in its entity's live geometry, can every
/// height still resolve. When none of them fires, the honest answer is
/// the general one: something the generation read has changed. None
/// when the operation is not Stale.
#[must_use]
pub fn stale_reason(
    world: &flecs_ecs::prelude::World,
    entity: &flecs_ecs::prelude::Entity,
) -> Option<String> {
    if status_of(world, entity) != OpStatus::Stale {
        return None;
    }
    let op = model::operation_of(world, entity)?;
    match model::find_by_name(world, &op.tool) {
        None => return Some(format!("its tool '{}' no longer exists", op.tool)),
        Some(t) if !model::is_tool(world, &t) => {
            return Some(format!("'{}' is no longer a tool", op.tool));
        }
        _ => {}
    }
    match &op.src {
        SrcRef::Stock => {
            let stock_name = crate::setup::stock_entity_name(world, op.setup.as_deref());
            if model::find_by_name(world, &stock_name).is_none() {
                return Some("the stock no longer exists - set it up again".into());
            }
        }
        SrcRef::Entity(name) => {
            if model::find_by_name(world, name).is_none() {
                return Some(format!("its geometry source '{name}' was deleted"));
            }
        }
        SrcRef::Selection(items) => {
            for g in items {
                let Some(e) = model::find_by_name(world, &g.entity) else {
                    return Some(format!("'{}' was deleted", g.entity));
                };
                let Some(tris) = model::facets_of(world, &e) else {
                    return Some(format!("'{}' no longer carries geometry", g.entity));
                };
                let features = model::features_of_local(&tris);
                let (present, what) = match g.feature {
                    FeatureSel::Face(id) => (
                        revision::face_ids(&features).binary_search(&id).is_ok(),
                        "face",
                    ),
                    FeatureSel::Patch(id) => (
                        revision::patch_ids(&features).binary_search(&id).is_ok(),
                        "welded face",
                    ),
                    FeatureSel::Chain(id) => (
                        revision::chain_ids(&features).binary_search(&id).is_ok(),
                        "edge chain",
                    ),
                };
                if !present {
                    return Some(format!(
                        "the picked {what} of '{}' no longer exists - the geometry moved",
                        g.entity
                    ));
                }
            }
        }
    }
    for h in collect_heights(&op.params) {
        if let Some(missing) = missing_source(world, op.setup.as_deref(), h.reference) {
            return Some(format!("a height is measured from {missing}"));
        }
    }
    // M3.1 D: the program ahead is the LAST question, and reaching it
    // forces the answer: every own-input question above came back
    // clean while the stamp still mismatches, so a cutting
    // predecessor's recorded path is what moved - whatever its own
    // status reads.
    if op.params.kind() == OpType::Pocket3D
        && let Some(name) = model::name_of(world, entity)
        && let Some(why) = crate::rest::prior_stale(world, &name, op.setup.as_deref())
    {
        return Some(why);
    }
    Some("the geometry or tool inputs changed since it was generated".into())
}

/// The height expressions of a params set (the `for_each_height` soup
/// collected once, for scanning).
fn collect_heights(params: &OpParams) -> Vec<Height> {
    let mut out = Vec::new();
    params.for_each_height(|h| out.push(h));
    out
}

/// Gouge law's radial slack: half the geo2d round-join chord error plus
/// room for float chains; below this, a "bite" is an artifact, above it a
/// real overrun of the finish surface (see `tests/verify.rs` rationale).
pub(crate) const GOUGE_TOL: f64 = 0.01;
/// "Below the clearance plane" / "moved in XY" margin for the rapid law.
const Z_TOL: f64 = 1e-6;
/// Z samples for a silhouette footprint; convergence needs far fewer.
const FOOTPRINT_SAMPLES: usize = 16;
/// One micron under a grazing section plane, for the slice retry.
const SLICE_NUDGE: f64 = 1e-3;
/// Points per semicircular slot cap when building the stadium region.
const CAP_SEGMENTS: usize = 16;

/// The source geometry in world coordinates, plus its top.
struct Material {
    facets: Vec<Triangle>,
    top: f64,
}

fn material_of(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<Material, GenError> {
    match src {
        SrcRef::Stock => {
            let name = crate::setup::stock_entity_name(world, setup);
            named_material(world, &name, "no stock: set it up in Setup first")
        }
        SrcRef::Entity(name) => {
            named_material(world, name, &format!("no entity named '{name}'"))
        }
        SrcRef::Selection(items) => {
            // Union of the distinct selected entities (stock is a legal
            // pick): what the generators stand in and what verification
            // sweeps. The region itself comes from the features.
            let mut facets = Vec::new();
            let mut top = f64::NEG_INFINITY;
            let mut seen: Vec<&str> = Vec::new();
            for g in items {
                if seen.contains(&g.entity.as_str()) {
                    continue;
                }
                seen.push(g.entity.as_str());
                let m = named_material(
                    world,
                    &g.entity,
                    &format!("the selection names '{}', which is gone", g.entity),
                )?;
                facets.extend(m.facets);
                top = top.max(m.top);
            }
            usable(facets, top, "the selection")
        }
    }
}

/// The hole list of one source, for the panel's Detect holes button:
/// the SAME world-transformed facets generation machines, so what is
/// detected is what will be drilled - a local-frame list would drift
/// from a moved part. A pure read; the caller (the UI, through a
/// command) decides what to do with the list.
pub fn holes_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Vec<crate::drilling::Hole>, GenError> {
    let m = material_of(world, setup, src)?;
    Ok(crate::drilling::recognize(&m.facets))
}

fn named_material(world: &World, name: &str, gone: &str) -> Result<Material, GenError> {
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(GenError::Invalid(gone.to_string()));
    };
    let facets = model::world_facets(world, &entity).unwrap_or_default();
    let top = model::entity_world_aabb(world, &entity)
        .map(|b| b.max.z)
        .unwrap_or(f64::NAN);
    usable(facets, top, name)
}

/// The shared sanity gate: every vertex finite, a real top.
fn usable(facets: Vec<Triangle>, top: f64, label: &str) -> Result<Material, GenError> {
    if !facets.iter().all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        || !top.is_finite()
    {
        return Err(GenError::Invalid(format!(
            "'{label}' has no usable geometry to machine"
        )));
    }
    Ok(Material { facets, top })
}

fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The section at `z`, retried a micron below when the plane grazes a
/// vertex or edge (a tangential slice is an error by contract, and a
/// 1 um shift cannot change what a cutter sees).
fn section(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    match slice::slice(facets, z) {
        Ok(r) if !r.is_empty() => Ok(r),
        Ok(_) => Err(GenError::Empty(format!("no material at z = {z:.3}"))),
        Err(e) => match slice::slice(facets, z - SLICE_NUDGE) {
            Ok(r) if !r.is_empty() => Ok(r),
            Ok(_) => Err(GenError::Empty(format!("no material at z = {z:.3}"))),
            // Both the plane and the nudged plane are unusable: the
            // original error describes the level the user asked for.
            Err(_) => Err(from_slice(e)),
        },
    }
}

/// Closed-form stadium: `a`/`b` centers, half-width `h`, semicircles of
/// `CAP_SEGMENTS` points at each end. The region only feeds verification
/// (the slot generator computes the path in closed form), and any finite
/// inputs build a simple closed ring.
fn stadium(a: Point2, b: Point2, h: f64) -> Result<Region, GenError> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-9 { (dx / len, dy / len) } else { (1.0, 0.0) };
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::with_capacity(2 * CAP_SEGMENTS + 2);
    for (c, t0, t1) in [(b, -90.0f64, 90.0), (a, 90.0, 270.0)] {
        for i in 0..=CAP_SEGMENTS {
            let th = (t0 + (t1 - t0) * i as f64 / CAP_SEGMENTS as f64).to_radians();
            let (ca, sa) = (th.cos(), th.sin());
            pts.push(Point2::new(c.x + h * (ca * ux + sa * nx), c.y + h * (ca * uy + sa * ny)));
        }
    }
    let loop_ = Loop::new(pts)?;
    Ok(Region::new(vec![loop_])?)
}

/// The cutting region of an operation as the world stands now - the same
/// construction `generate` runs, without the path (and without the tool,
/// which the region never consults). This is the viewport's "what gets
/// cut" outline. None when there is nothing honest to draw: a vanished
/// source or an unsliceable level shows nothing, never a stale loop
/// (Prime Directive: a wrong preview is as bad as a wrong toolpath).
#[must_use]
pub fn region_of(world: &World, op: &Operation) -> Option<Region> {
    if let SrcRef::Selection(items) = &op.src
        && !matches!(
            op.params,
            OpParams::Slot { .. }
                | OpParams::Drill { .. }
                | OpParams::Pocket3D { .. }
                | OpParams::Finish3D { .. }
        )
    {
        return selection_region(world, op.setup.as_deref(), items, &op.params).ok();
    }
    let material = material_of(world, op.setup.as_deref(), &op.src).ok()?;
    region_for(world, op.setup.as_deref(), &material, &op.params).ok()
}

/// The centerline runs `generate` would add to `region_of`'s outline for
/// this operation as the world stands now - the open chains of a Contour
/// selection, in the order they were picked. Drawn and generated through
/// this one call, so the viewport can never show a run the path does not
/// cut (or hide one it does). Unresolvable selections yield nothing, like
/// a vanished source in `region_of`.
#[must_use]
pub fn chains_of(world: &World, op: &Operation) -> Vec<Vec<Point2>> {
    let SrcRef::Selection(items) = &op.src else {
        return Vec::new();
    };
    match selection_geometry(world, op.setup.as_deref(), items, &op.params) {
        Ok((_, chains)) => chains,
        Err(_) => Vec::new(),
    }
}

/// The source dispatch shared by `generate` and `region_of`: a selection
/// resolves its picked features, anything else slices its material, and
/// Slot keeps its closed-form stadium regardless of source (its boundary
/// is the two endpoints, not the model). Drill likewise ignores a
/// selection source: its geometry is the hole list in the params, so
/// picking features could only disagree with it. Pocket3D too: its
/// footprint is the model's own silhouette - the level law reads the
/// mesh at every Z, so a picked loop could only disagree with it.
fn region_for_src(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
    material: &Material,
) -> Result<Region, GenError> {
    match src {
        SrcRef::Selection(items)
            if !matches!(
                params,
                OpParams::Slot { .. }
                    | OpParams::Drill { .. }
                    | OpParams::Pocket3D { .. }
                    | OpParams::Finish3D { .. }
            ) =>
        {
            selection_region(world, setup, items, params)
        }
        _ => region_for(world, setup, material, params),
    }
}

/// The per-kind region build, shared by `generate` and `region_of` so the
/// preview outline cannot disagree with what the generators cut. The
/// section height resolves through the world like every other height in
/// the params: a preview with an unresolvable `top_z` shows nothing (the
/// `Err` folds to `None` in `region_of`), and a generation refuses by
/// name.
fn region_for(
    world: &World,
    setup: Option<&str>,
    material: &Material,
    params: &OpParams,
) -> Result<Region, GenError> {
    match params {
        OpParams::Face { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // The 3D pocket's machine area is a silhouette, not a section:
        // columns whose surface sits below the first level are still
        // cut, down to that surface - a section would miss them. The
        // per-level islands come from the mesh, in the generator and in
        // the verify pass, never from this region.
        OpParams::Pocket3D { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // Same silhouette law for the finish: the rings chase the mesh
        // at every plane, the region only says where the model shadows.
        OpParams::Finish3D { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Trace { top_z, .. } => {
            let z = resolve_height(world, setup, *top_z)
                .ok_or_else(|| unresolved_err("Top of Part Z", *top_z, world, setup))?;
            section(&material.facets, z)
        }
        OpParams::Slot { a, b, width, .. } => stadium(*a, *b, width * 0.5),
        OpParams::Drill { holes, .. } => {
            // The silhouette, plus a rim loop for every hole (a blind
            // hole's mouth sits inside the material's shadow and the
            // projection never sees it; a through wall does show up,
            // but as the mesh's own coarse polygon). The rims are what
            // make the gouge law's tangency test mean something at a
            // plunge: d(center, rim) = hole radius, so a matched drill
            // passes silent and an oversized tool is honestly flagged.
            // A silhouette loop tracing a hole is REPLACED, not kept:
            // a 16-facet wall's apothem sits a chord-sag inside its own
            // radius - far more than the gouge law's slack at drill
            // diameters - and the law would read that tessellation as
            // tool bite. The hole's parameters are the truth here.
            let base = slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)?;
            if holes.is_empty() {
                return Ok(base);
            }
            let mut loops: Vec<Loop> = base
                .loops()
                .iter()
                .filter(|l| !holes.iter().any(|h| rim_is(l, *h)))
                .cloned()
                .collect();
            for hole in holes {
                loops.push(rim_loop(*hole)?);
            }
            Ok(Region::new(loops)?)
        }
    }
}

/// Rim loop segments: enough that the chord sag below the circle,
/// r·(1−cos(π/n)), stays under RIM_SAG_BUDGET - half the tolerance the
/// gouge law grants, so a matched tool clears its own rim by the other
/// half, never by an accident of the segment count. That is 48 sides at
/// drill diameters and ~110 at a 12 mm bore radius; the floor keeps
/// tiny holes from degenerating and the ceiling keeps huge ones cheap.
const RIM_SAG_BUDGET: f64 = 0.005;
const DRILL_RIM_SEGMENTS: usize = 48;
const DRILL_RIM_MAX_SEGMENTS: usize = 4096;

fn rim_segments(r: f64) -> usize {
    let x = (RIM_SAG_BUDGET / r).clamp(0.0, 2.0);
    let n = (std::f64::consts::PI / (1.0 - x).acos()).ceil() as usize;
    n.clamp(DRILL_RIM_SEGMENTS, DRILL_RIM_MAX_SEGMENTS)
}

/// A polygonal circle at a hole's XY - the loop the rim of `h` adds to a
/// drill region. `Region::new` orients by nesting, so the winding here
/// is free.
fn rim_loop(h: Hole) -> Result<Loop, GenError> {
    let n = rim_segments(h.radius);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let th = std::f64::consts::TAU * i as f64 / n as f64;
        pts.push(Point2::new(h.xy.x + h.radius * th.cos(), h.xy.y + h.radius * th.sin()));
    }
    Ok(Loop::new(pts)?)
}

/// Whether `l` is `h`'s rim already: enough vertices, and every one of
/// them on the hole's circle within a band that forgives tessellation
/// (the fitted radius and the sliced rim share the mesh's own vertices
/// for a through wall; only quantization separates them). A square, a
/// slot end, or a different concentric circle fails.
fn rim_is(l: &Loop, h: Hole) -> bool {
    let pts = l.points();
    if pts.len() < 8 {
        return false;
    }
    let tol = (0.02 * h.radius).max(0.1);
    pts.iter().all(|p| ((p.x - h.xy.x).hypot(p.y - h.xy.y) - h.radius).abs() <= tol)
}

/// The shared refusal phrasing for an unresolved height, named by field.
/// One source so `resolve_params`, the region builders and every command
/// error read identically.
pub fn unresolved_msg(
    what: &str,
    h: Height,
    world: &World,
    setup: Option<&str>,
) -> String {
    format!(
        "{what} (From: {ref}, offset {off}) is unresolved: {why}",
        ref = h.reference,
        off = h.offset,
        why = missing_source(world, setup, h.reference)
            .unwrap_or("the source has no finite height"),
    )
}

fn unresolved_err(what: &str, h: Height, world: &World, setup: Option<&str>) -> GenError {
    GenError::Invalid(unresolved_msg(what, h, world, setup))
}

/// A required height slot of an already-`Ok` `ResolvedHeights` is `Some`
/// by that function's contract (per kind); the error arm exists so no
/// call site needs an unwrap.
fn req_z(v: Option<f64>, what: &str) -> Result<f64, GenError> {
    v.ok_or_else(|| GenError::Invalid(format!("{what} is unresolved")))
}

/// Largest tilt from horizontal, in degrees, a Face operation accepts for
/// a selected planar face: past it the facing footprint would lie about
/// the area being cleared.
const FACE_TILT_MAX_DEG: f64 = 60.0;
/// Shoelace area below which a projected loop is a traced line rather
/// than a boundary. 1e-9 mm² is far under weld-lattice noise and far
/// above an actual face.
const DEGENERATE_AREA: f64 = 1e-9;
/// The material-side rule's threshold (M1.5.1 issue C): when the area
/// inside a chain-only Face region holds less than this fraction of the
/// material the region disagrees with - inside vs outside - the outside
/// is taken instead. 0.5 means "strictly less material inside than
/// outside": a bore rim rings air (inside ≈ 0) and machines the annulus;
/// a box-top ring holds all its material inside (ratio ≈ 1) and stays.
const OUTSIDE_BIAS: f64 = 0.5;

/// What a selection resolves to: the boundary loops and centerline runs
/// it spells, plus Auto's inside verdict for a Contour (the fill law;
/// `false` for every other kind and for a side the user named).
type SelectionGeometry = (Vec<Loop>, Vec<Vec<Point2>>, bool);

/// The region loops and centerline runs a feature selection resolves to,
/// as the world stands now: each picked face gives its closed boundary
/// loops, each closed chain its ring, projected to XY, coincident rims
/// counted once. An open chain is no boundary at all: for a Contour it
/// comes back in the second half - the one kind that machines a run -
/// and for a Face it runs out to the stock edge, the run acting as the
/// limit (M1.5.1 issue C). Refusals are the Prime Directive's kind - a
/// tilted face for a facing pass, an open chain for a Pocket or a Trace,
/// an open sheet, a run-out whose side cannot be told, each named in the
/// message, each with the fix stated. Never a silent drop: an unconsumed
/// run either bounds something or the message says why it cannot.
///
/// Picked open runs are stitched before this routing (M1.5.1 issue C):
/// several runs that meet end to end spell one ring, and a ring is a
/// ring however it is spelled - see `stitch_open_runs`. A facing region
/// spelled only by chains is then clipped to the material that actually
/// stands there - and taken on the far side when the picked rings bound
/// air, which is what "machine out to the stock, the run is the limit"
/// means on a bore rim (the material-side rule, [`OUTSIDE_BIAS`]). A
/// contour with `Side::Auto` asks the same rings a narrower question -
/// what fraction of the area THEY bound is material - and answers it in
/// the third slot of [`selection_geometry_impl`]'s tuple; this wrapper
/// hands out only the halves the region-shaped kinds want.
fn selection_geometry(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<(Vec<Loop>, Vec<Vec<Point2>>), GenError> {
    let (loops, chains, _) = selection_geometry_impl(world, setup, items, params, None)?;
    Ok((loops, chains))
}

/// [`side_sane`]'s question, answered for one concrete source.
pub(crate) fn side_sane(params: &OpParams, src: &SrcRef) -> Option<String> {
    let OpParams::Contour { side, .. } = params else {
        return None;
    };
    if *side != Side::Inside || matches!(src, SrcRef::Selection(_)) {
        return None;
    }
    Some(
        "Side 'Inside' needs picked rings: this source machines the whole model section, whose \
         hole walls are already cut from inside - switch Side to Auto, or pick the wall's edges"
            .into(),
    )
}

/// The side [`Side::Auto`] resolves to for this operation's contour,
/// asked of the very walk generation runs ([`selection_geometry_impl`]) -
/// the same law [`machining2d::PocketStrategy::resolve`] is for the
/// pocket's Auto, so a panel note and a generated path can never
/// disagree. `None` when there is no verdict to quote yet: picks that
/// resolve to open runs only (the side is inert there), or geometry that
/// refuses. A whole-section source always answers Outside - its `+` law
/// is already parity-correct over holes, and Inside there is a named
/// refusal, never an Auto verdict.
pub fn contour_auto_side(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Option<bool> {
    match src {
        SrcRef::Stock | SrcRef::Entity(_) => Some(false),
        SrcRef::Selection(items) => {
            let (loops, _runs, inside) =
                selection_geometry_impl(world, setup, items, params, None).ok()?;
            (!loops.is_empty()).then_some(inside)
        }
    }
}

/// [`selection_geometry`] with a human-readable account of what the
/// selection resolved to (`notes`) accumulated on the side, so the
/// account can never disagree with the geometry - they are one walk.
/// The third return value is Auto's verdict for a Contour (material-fill
/// on the picked rings); it is `false` for every other kind and for a
/// contour whose side the user already named.
fn selection_geometry_impl(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
    mut notes: Option<&mut Vec<String>>,
) -> Result<SelectionGeometry, GenError> {
    let facing = matches!(params, OpParams::Face { .. });
    let pocket = matches!(params, OpParams::Pocket { .. });
    let contour = matches!(params, OpParams::Contour { .. });
    let contour_side = match params {
        OpParams::Contour { side, .. } => *side,
        _ => Side::Auto,
    };
    // The two silhouettes a run-out consults, computed once and shared
    // with the assembly below: the source's own footprint (the Face arm
    // of `region_for` and the verification project, so preview and path
    // agree) and the stock around it (what a run runs out to, where
    // stock exists).
    let mut material_sil = None;
    let mut envelope = None;
    let mut chain_only = false;
    if facing {
        let material = material_of(world, setup, &SrcRef::Selection(items.to_vec()))?;
        chain_only = items.iter().all(|g| !matches!(g.feature, FeatureSel::Face(_)));
        let sil = silhouette(&material.facets);
        let stock_name = crate::setup::stock_entity_name(world, setup);
        envelope = model::find_by_name(world, &stock_name)
            .and_then(|e| model::world_facets(world, &e))
            .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
            .filter(|r| !r.is_empty())
            .or_else(|| sil.clone());
        material_sil = sil;
    }
    let mut loops: Vec<Loop> = Vec::new();
    let mut chains: Vec<Vec<Point2>> = Vec::new();
    let mut strips: Vec<Loop> = Vec::new();
    let mut rims: Vec<Vec<(i64, i64)>> = Vec::new();
    let (rings, stitched) = stitch_open_runs(world, items);
    let mut stitched_runs = 0usize;
    let mut stitched_rings = 0usize;
    for (label, pts, runs) in rings {
        if runs > 1 {
            stitched_runs += runs;
            stitched_rings += 1;
        }
        push_loop(&label, pts, &mut loops, &mut rims)?;
    }
    if let Some(n) = &mut notes
        && stitched_runs > 0
    {
        n.push(format!(
            "{stitched_runs} open run{} stitched into {stitched_rings} ring{}",
            if stitched_runs == 1 { "" } else { "s" },
            if stitched_rings == 1 { "" } else { "s" },
        ));
    }
    for (gi, g) in items.iter().enumerate() {
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(GenError::Invalid(format!(
                "the selection names '{}', which is gone",
                g.entity
            )));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(GenError::Invalid(format!(
                "'{}' has no geometry to select from",
                g.entity
            )));
        };
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let label = format!("{} of '{}'", g.feature, g.entity);
        match g.feature {
            FeatureSel::Face(fid) => {
                let Some(f) = features.faces.iter().find(|f| f.id == fid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let n = transform_point(
                    Point3::new(f.normal[0], f.normal[1], f.normal[2]),
                    Point3::ZERO,
                    loc.rot,
                );
                if facing && n.z.abs() < FACE_TILT_MAX_DEG.to_radians().cos() {
                    return Err(GenError::Invalid(format!(
                        "{label} is not horizontal - a facing pass needs a floor; machine its boundary with a 2D Contour instead"
                    )));
                }
                if f.loops.iter().all(|l| matches!(l.kind, LoopKind::Open)) {
                    return Err(GenError::Invalid(format!(
                        "{label} is an open sheet; select its edge chain instead"
                    )));
                }
                for l in &f.loops {
                    if matches!(l.kind, LoopKind::Open) {
                        continue;
                    }
                    let pts: Vec<Point2> = l
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize))
                        .map(|p| {
                            let w =
                                transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                            Point2::new(w.x, w.y)
                        })
                        .collect();
                    push_loop(&label, pts, &mut loops, &mut rims)?;
                }
            }
            FeatureSel::Chain(cid) => {
                let cs = feature::chains(&features);
                let Some(c) = cs.iter().find(|c| c.id == cid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let xy: Vec<Point2> = feature::chain_points(&features, c)
                    .iter()
                    .map(|p| {
                        let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                        Point2::new(w.x, w.y)
                    })
                    .collect();
                if !c.closed {
                    if stitched.contains_key(&gi) {
                        // The stitch pass closed this run's ends into a
                        // ring with its siblings; the ring is already
                        // in `loops`, routed like any closed chain.
                        continue;
                    }
                    if pocket {
                        return Err(GenError::Invalid(format!(
                            "{label} is an open run - a 2D Pocket cannot end mid-edge; select a closed chain"
                        )));
                    }
                    if contour {
                        // The run's consumer: a centerline pass along
                        // these exact points, XY like everything else
                        // here - the Z ladder is the operation's.
                        chains.push(xy);
                    } else if facing {
                        // M1.5.1 issue C: the run is the limit - face
                        // from it out to the stock edge. No stock and
                        // no source footprint means nowhere to run to,
                        // said by name rather than dropped.
                        let Some(env) = &envelope else {
                            return Err(GenError::Invalid(format!(
                                "{label} is an open run and nothing bounds the plane around it - set up stock or close the pick into a chain"
                            )));
                        };
                        strips.push(extend_run_to_footprint(
                            &label,
                            &xy,
                            env,
                            material_sil.as_ref(),
                        )?);
                    } else {
                        // Trace used to drop the run in silence; the
                        // Prime Directive's answer is a named refusal.
                        return Err(GenError::Invalid(format!(
                            "{label} is an open run - Trace machines a closed boundary; use a 2D Contour or close the pick into a chain"
                        )));
                    }
                    continue;
                }
                push_loop(&label, xy, &mut loops, &mut rims)?;
            }
            FeatureSel::Patch(pid) => {
                let Some(rings) = feature::patch_rings(&features, pid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                // The Face arm's horizontal-tilt rule deliberately does not
                // apply: a patch is exactly how one asks for a wall. The
                // wall's rims go through the same seams as any closed
                // boundary - `push_loop`'s canonical-rim dedupe drops the
                // bottom rim of a vertical wall as the XY twin of the top,
                // so the region is the footprint the weld encloses, never
                // an even-odd sliver of its own skin - and the facing
                // material-side rule below turns a bore wall's rim around
                // the air it rings into the annulus the cut needs.
                let mut drew = false;
                for r in rings.iter().filter(|r| r.closed) {
                    let pts: Vec<Point2> = r
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize))
                        .map(|p| {
                            let w =
                                transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                            Point2::new(w.x, w.y)
                        })
                        .collect();
                    push_loop(&label, pts, &mut loops, &mut rims)?;
                    drew = true;
                }
                if !drew {
                    return Err(GenError::Invalid(format!(
                        "{label} bounds no closed ring - re-pick it"
                    )));
                }
            }
        }
    }
    if facing && (chain_only || !strips.is_empty()) {
        // The material-side rule, assembled once at the end so the
        // stitching, the run-outs and the clip all report through the
        // same walk: rings (or nothing), then the run-out strips, then
        // one test against the metal - if what the selection bounds
        // holds less material than what it does not, the answer is the
        // complement inside the material. A bore rim rings air, and a
        // convex rim's outward side is all air; a Face region that
        // named the void as its own answer would machine nothing,
        // which is gouging by another route.
        let mut region = if loops.is_empty() {
            Region::empty()
        } else {
            Region::new(loops.clone())?
        };
        let extended = strips.len();
        for s in strips {
            region = region.union(&Region::new(vec![s])?)?;
        }
        let mut flipped = false;
        if chain_only && let Some(mat) = &material_sil {
            let keep = region.intersect(mat)?.area().abs();
            let whole = mat.area().abs();
            if keep < OUTSIDE_BIAS * (whole - keep) {
                region = mat.difference(&region)?;
                flipped = true;
            } else {
                region = region.intersect(mat)?;
            }
        }
        if let Some(n) = &mut notes {
            if extended > 0 {
                n.push(format!(
                    "{extended} open run{} extended to the stock boundary",
                    if extended == 1 { "" } else { "s" },
                ));
            }
            if flipped {
                n.push("machining outside the run - the selection bounds air".into());
            }
        }
        loops = region.loops().to_vec();
    }
    let mut contour_inside = false;
    if contour && contour_side == Side::Auto && !loops.is_empty() {
        // Auto asks the rings themselves what they bound: how much of the
        // area they enclose is actually material. A boss or OD ring sits
        // in its own metal (fill ≈ 1) - Outside, today's law. A bore rim
        // rings air (fill ≈ 0) - the wall's metal is OUTSIDE the ring,
        // and the Inside law is the one that grazes it. This is NOT the
        // Face complement rule (that one compares the region to the rest
        // of the part and would flip a small boss ring inside one
        // entity's footprint); the loop count and winding are untouched
        // either way, so the preview of a picked bore wall stays the wall
        // the picks spelled - one walk, sign and traversal carrying the
        // side (M1.5.4 issue 2).
        let region = Region::new(loops.clone())?;
        let whole = region.area().abs();
        if whole > 0.0 {
            let sil = match &material_sil {
                Some(s) => Some(s.clone()),
                None => material_of(world, setup, &SrcRef::Selection(items.to_vec()))
                    .ok()
                    .and_then(|m| silhouette(&m.facets)),
            };
            if let Some(mat) = sil {
                let fill = region.intersect(&mat)?.area().abs() / whole;
                if fill < OUTSIDE_BIAS {
                    contour_inside = true;
                    if let Some(n) = &mut notes {
                        n.push("machining inside the ring - the selection bounds air".into());
                    }
                }
            }
        }
    }
    if contour && contour_side != Side::Auto && loops.is_empty() && !chains.is_empty() {
        // An explicit Side on a pure open-run selection changes nothing -
        // the cutter rides the run's own points - so say that plainly
        // instead of letting the combo imply an effect.
        if let Some(n) = &mut notes {
            n.push("Side does not apply to open runs - the cutter rides the run's line".into());
        }
    }
    Ok((loops, chains, contour_inside))
}

/// The region half of `selection_geometry` in the shape `region_of` and
/// every non-contour generator wants: bounds, or a named refusal - never
/// a silently empty area. A Contour of pure open runs is the case that
/// skips this and reads `selection_geometry` directly.
fn selection_region(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<Region, GenError> {
    let (loops, _) = selection_geometry(world, setup, items, params)?;
    if loops.is_empty() {
        return Err(GenError::Empty(
            "nothing in the selection bounds an area".into(),
        ));
    }
    Region::new(loops).map_err(Into::into)
}

/// Which stitched ring (0-based, the stitcher's own order) each pick took
/// part in: one entry per item, `None` for picks that closed nothing or
/// were themselves closed chains. The chips' `· → ring k` note reads
/// through the very stitch walk the region takes, so a row can never say
/// it joins a ring the preview does not build.
#[must_use]
pub fn stitched_rings(world: &World, items: &[GeoSel]) -> Vec<Option<usize>> {
    let (_, consumed) = stitch_open_runs(world, items);
    let mut out: Vec<Option<usize>> = vec![None; items.len()];
    for (it, k) in consumed {
        if let Some(slot) = out.get_mut(it) {
            *slot = Some(k);
        }
    }
    out
}

/// A human-readable account of what a feature selection machines beyond
/// the bare pick list: runs stitched into rings, runs extended out to
/// the stock boundary, and a region taken on the far side of its rings
/// because the rings bound air. Computed on read from the very walk the
/// region takes - the account cannot disagree with the geometry, and a
/// selection that refuses says so by owning nothing.
///
/// Deliberately *not* a persisted `verify::Warning` variant: those names
/// are the v4 project-file contract, and this is derived state, not a
/// verification finding.
#[must_use]
pub fn selection_notes(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Vec<String> {
    let mut notes = Vec::new();
    match selection_geometry_impl(world, setup, items, params, Some(&mut notes)) {
        Ok(_) => notes,
        // The refusal itself is `region_of`'s and generation's to make,
        // with detail; notes stay empty rather than contradict it.
        Err(_) => notes,
    }
}

/// The footprint a piece of material projects to, XY: the same
/// `slice::project` the Face arm of `region_for` and the generation
/// verifier run, so a preview that consults it and the path it previews
/// agree by construction (M1.5.1: a run-out strip is clipped to this).
/// A model with height sweeps to a silhouette with its through-holes
/// intact; a zero-thickness sheet has no Z span to section, so its
/// facets' planar union is its footprint instead.
fn silhouette(facets: &[Triangle]) -> Option<Region> {
    if let Ok(r) = slice::project(facets, FOOTPRINT_SAMPLES)
        && !r.is_empty()
    {
        return Some(r);
    }
    let loops: Vec<Loop> = facets
        .iter()
        .filter_map(|t| {
            Loop::new(vec![
                Point2::new(t.a.x, t.a.y),
                Point2::new(t.b.x, t.b.y),
                Point2::new(t.c.x, t.c.y),
            ])
            .ok()
            .filter(|l| l.area().abs() > DEGENERATE_AREA)
        })
        .collect();
    if loops.is_empty() {
        return None;
    }
    Region::new(loops)
        .ok()
        .filter(|r| !r.is_empty())
}

/// The first point and edge index where ray `o + t·d` (t > 0) hits a
/// polygon ring; `None` when it misses (or runs along the ring without
/// reaching it). The run-out's geometric question, verbatim.
fn ray_hit(ring: &[Point2], o: Point2, d: Point2) -> Option<(Point2, usize)> {
    let mut best: Option<(f64, usize)> = None;
    let n = ring.len();
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let e = Point2::new(b.x - a.x, b.y - a.y);
        let den = d.x * e.y - d.y * e.x;
        if den.abs() < 1e-12 {
            continue; // parallel; an overlapping collinear hit is the ambiguity's own job to name
        }
        let oa = Point2::new(a.x - o.x, a.y - o.y);
        let t = (oa.x * e.y - oa.y * e.x) / den;
        let s = (oa.x * d.y - oa.y * d.x) / den;
        if t > 1e-9 && (0.0..=1.0).contains(&s) && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, i));
        }
    }
    let (t, i) = best?;
    Some((Point2::new(o.x + t * d.x, o.y + t * d.y), i))
}

/// Walk `ring` from the edge index+hit of one point to that of another,
/// forward along the ring's own order or back against it. Returns the
/// strict interior vertices, then the destination point - what a
/// closure needs between the two ray hits. `None` on a degenerate ring.
fn boundary_path(
    ring: &[Point2],
    from: (usize, Point2),
    to: (usize, Point2),
    forward: bool,
) -> Option<Vec<Point2>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut out = Vec::new();
    if from.0 == to.0 {
        // Same edge: forward = the whole ring from here and back to the
        // destination; reverse = nothing between (the direct hop).
        if forward {
            for k in 1..=n {
                out.push(ring[(from.0 + k) % n]);
            }
        }
        out.push(to.1);
        return Some(out);
    }
    if forward {
        let mut i = (from.0 + 1) % n;
        loop {
            out.push(ring[i]);
            if i == to.0 {
                break;
            }
            i = (i + 1) % n;
        }
    } else {
        // Backward: the destination sits mid-edge, entered against the
        // ring's order from that edge's END vertex - walking to the one
        // before it would step past the hit and force the closing edge
        // to jump back over the interior.
        let mut i = from.0;
        loop {
            out.push(ring[i]);
            if i == (to.0 + 1) % n {
                break;
            }
            i = (i + n - 1) % n;
        }
    }
    out.push(to.1);
    Some(out)
}

/// Close one open run into a boundary by casting each free end straight
/// out to the envelope's outer ring and walking that ring between the
/// hits (M1.5.1 issue C: "an open loop should just let us generate to
/// the outside of the stock box - the open loop selected should act as
/// a limit"). The run is first oriented so the chord-closing shoelace is
/// positive, which pins the outward normal (a 90° clockwise turn of the
/// travel direction) to the material-free side at each end; the walk
/// direction that keeps the strip on that side is chosen by material
/// overlap when a footprint is known, shortest boundary otherwise, with
/// the forward walk winning ties - and candidates that cross into the
/// run's own chord lens are out of the race, because the run is a limit.
/// Every failure mode - no stock ahead of an end, a self-crossing strip,
/// a zero-area strip - is the named ambiguity, never a silent skip.
fn extend_run_to_footprint(
    label: &str,
    run: &[Point2],
    envelope: &Region,
    material: Option<&Region>,
) -> Result<Loop, GenError> {
    let ambiguous = || {
        GenError::Invalid(format!(
            "{label} and the selection geometry is ambiguous; use a closed chain or a Face pick"
        ))
    };
    let Some(outer) = envelope
        .loops()
        .iter()
        .max_by(|a, b| total_cmp_signed(a.area(), b.area()))
    else {
        return Err(ambiguous());
    };
    let ring = outer.points();
    if ring.len() < 3 || run.len() < 2 {
        return Err(ambiguous());
    }
    // Orient: shoelace over run + chord. A straight run shoelaces to ~0
    // and keeps its order; the normal choice is still deterministic.
    let mut twice = 0.0f64;
    let m = run.len();
    for i in 0..m {
        let a = run[i];
        let b = run[(i + 1) % m];
        twice += a.x * b.y - b.x * a.y;
    }
    let pts: Vec<Point2> = if twice < 0.0 {
        run.iter().rev().copied().collect()
    } else {
        run.to_vec()
    };
    // Outward = clockwise turn of travel at each free end: (d.y, -d.x).
    let d0 = Point2::new(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
    let d1 = Point2::new(pts[m - 1].x - pts[m - 2].x, pts[m - 1].y - pts[m - 2].y);
    let n0 = Point2::new(d0.y, -d0.x);
    let n1 = Point2::new(d1.y, -d1.x);
    // A straight run laces to nothing, so "away from the bulge" names
    // no side - both normals are tried and the overlap score picks the
    // material side. A curved run's chord-CCW orientation already fixed
    // one honest outward direction; trying the other would let a run
    // that rings material (a bore rim) claim the far half of itself.
    let straight = twice.abs() < DEGENERATE_AREA;
    // The lens the run closes against its own chord: material or air on
    // the run's concave side. A strip that reaches into it crosses the
    // run - and "the open loop acts as a limit" forbids crossing. One
    // probe per segment, a hair left of travel (the chord-CCW
    // orientation above puts lens interior on the left); any candidate
    // containing one is out of the race, so overlap can no longer be
    // gamed by wrapping the whole envelope behind the run's back. A
    // straight run closes no lens: pure overlap decides, as scribbled.
    let mut probes: Vec<Point2> = Vec::new();
    if !straight {
        for i in 0..m - 1 {
            let (a, b) = (pts[i], pts[i + 1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if len <= f64::EPSILON {
                continue;
            }
            let eps = len * 1e-3;
            probes.push(Point2::new(
                (a.x + b.x) * 0.5 - dy / len * eps,
                (a.y + b.y) * 0.5 + dx / len * eps,
            ));
        }
    }
    let mut candidates: Vec<(f64, f64, usize, Loop)> = Vec::new();
    let sides: &[f64] = if straight { &[1.0, -1.0] } else { &[1.0] };
    for (si, &sgn) in sides.iter().enumerate() {
        let Some(hit_start) = ray_hit(ring, pts[0], Point2::new(n0.x * sgn, n0.y * sgn)) else {
            continue;
        };
        let Some(hit_end) = ray_hit(ring, pts[m - 1], Point2::new(n1.x * sgn, n1.y * sgn)) else {
            continue;
        };
        for &forward in [true, false].iter() {
            let Some(between) = boundary_path(
                ring,
                (hit_end.1, hit_end.0),
                (hit_start.1, hit_start.0),
                forward,
            ) else {
                continue;
            };
            let mut closed = pts.clone();
            closed.push(hit_end.0);
            closed.extend(between.iter().copied());
            let Ok(loop_) = Loop::new(closed) else {
                continue;
            };
            if loop_.area().abs() < DEGENERATE_AREA {
                continue;
            }
            // Self-crossing is the ambiguity guard: a crossing loop is
            // no boundary, and `Region::new` is the one that says so.
            let Ok(region) = Region::new(vec![loop_.clone()]) else {
                continue;
            };
            if probes.iter().any(|p| region.contains(*p)) {
                continue;
            }
            // More material inside the strip than less - the side that
            // machines metal, not air.
            let overlap = match material {
                Some(mat) => match region.intersect(mat) {
                    Ok(r) => r.area().abs(),
                    Err(_) => continue,
                },
                None => 0.0,
            };
            // Then the shorter boundary walk: the corridor hugging the
            // run's normal side beats the candidate that wraps the
            // whole envelope, and with no footprint to consult this is
            // the whole decision.
            let mut walk = 0.0;
            let mut prev = hit_end.0;
            for &p in &between {
                walk += ((p.x - prev.x).powi(2) + (p.y - prev.y).powi(2)).sqrt();
                prev = p;
            }
            candidates.push((overlap, -walk, si, loop_));
        }
    }
    // Highest score wins; ties break to the shorter walk, then to the
    // first side tried, so the answer is a pure function of the pick.
    match candidates
        .into_iter()
        .max_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.cmp(&b.2))
        })
    {
        Some((_, _, _, loop_)) => Ok(loop_),
        None => Err(ambiguous()),
    }
}

/// Order two signed areas by magnitude - the outer loops of a region
/// are the big positive ones; a stray negative inside an `outer` pick
/// should never win.
fn total_cmp_signed(a: f64, b: f64) -> std::cmp::Ordering {
    a.abs().total_cmp(&b.abs())
}

/// The 1e6 lattice `push_loop` keys rims on, in one place so the
/// stitcher's endpoint matches and the dedupe agree to the nanometre.
fn qkey(p: &Point2) -> (i64, i64) {
    (
        (p.x * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
        (p.y * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
    )
}

/// One distinct picked open run in the shape the stitcher works in:
/// world XY points with the entity's transform applied (where
/// `push_loop` expects every boundary to live), its quantized endpoints,
/// and every item index that picked it - the same chain twice is one
/// run picked twice.
struct Run {
    items: Vec<usize>,
    entity: String,
    cid: u64,
    pts: Vec<Point2>,
    start: (i64, i64),
    end: (i64, i64),
}

/// What the stitcher hands back: rings composed from picked runs (label,
/// points, run count), and for every item the rings consumed, which ring
/// (index into that vec) consumed it - the set form answers "was this
/// stitched?", the map form lets the UI's chips say "→ ring k" from the
/// very walk the region takes.
type Stitched = (Vec<(String, Vec<Point2>, usize)>, HashMap<usize, usize>);

/// M1.5.1 issue C: tessellation - STEP adaptive chords especially - can
/// break a geometrically closed rim into several chains, each stopping
/// where a chord's turn exceeds the merge angle. The user picks them
/// all, and to them it is obviously one loop. So before routing, stitch
/// the picked open runs of each entity end to end and return every walk
/// that closes back onto its start as one ring, with its contributing
/// picks marked consumed so routing treats them as the closed chain
/// they describe.
///
/// Junctions match either endpoint of a candidate because a loop's runs
/// are stored with whatever direction their edge orientations allow -
/// across the rim of a split circle the two halves meet end-to-end on
/// one side and start-to-start on the other, so a flipped continuation
/// IS the ring, not a corruption of it. Committed rings are shoelace-
/// canonicalized to CCW so pick order can never flip the winding. A
/// doubled pick is deduped by chain id - the same segments twice close
/// no ring, they close a zero-area spike.
///
/// Deliberately conservative: only picked runs take part (nothing
/// unpicked is pulled in), cross-entity rings are not attempted (a
/// recorded limit), a greedy fork that dead-ends commits nothing, and
/// runs that do not close route exactly as they always did. Failed
/// lookups are not errors here; the routing loop owns those refusals
/// and will name them.
fn stitch_open_runs(world: &World, items: &[GeoSel]) -> Stitched {
    let mut runs: Vec<Run> = Vec::new();
    for (i, g) in items.iter().enumerate() {
        let FeatureSel::Chain(cid) = g.feature else {
            continue;
        };
        if let Some(r) = runs.iter_mut().find(|r| r.entity == g.entity && r.cid == cid) {
            r.items.push(i);
            continue;
        }
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            continue;
        };
        let Some(features) = model::features_of(world, &entity) else {
            continue;
        };
        let Some(c) = feature::chains(&features).into_iter().find(|c| c.id == cid) else {
            continue;
        };
        if c.closed {
            continue;
        }
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let pts: Vec<Point2> = feature::chain_points(&features, &c)
            .iter()
            .map(|p| {
                let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                Point2::new(w.x, w.y)
            })
            .collect();
        let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else {
            continue;
        };
        runs.push(Run {
            items: vec![i],
            entity: g.entity.clone(),
            cid,
            pts,
            start: qkey(&first),
            end: qkey(&last),
        });
    }
    let used = |r: &Run, consumed: &HashMap<usize, usize>| {
        r.items.iter().any(|it| consumed.contains_key(it))
    };
    // Each committed ring carries the count of runs that spelled it, so
    // `selection_notes` can say what the stitching did.
    let mut rings: Vec<(String, Vec<Point2>, usize)> = Vec::new();
    let mut consumed: HashMap<usize, usize> = HashMap::new();
    for s in 0..runs.len() {
        if used(&runs[s], &consumed) {
            continue;
        }
        // Walk the cycle through run `s` forward: `anchor` is where we
        // started, `far` the junction to extend from. `in_seq` is why a
        // walk can never loop.
        let anchor = runs[s].start;
        let mut far = runs[s].end;
        let mut seq: Vec<(usize, bool)> = vec![(s, false)];
        let mut in_seq = vec![false; runs.len()];
        in_seq[s] = true;
        let closed = loop {
            if far == anchor {
                break true;
            }
            let mut next: Option<(usize, bool)> = None;
            for (j, r) in runs.iter().enumerate() {
                if in_seq[j] || used(r, &consumed) || r.entity != runs[s].entity {
                    continue;
                }
                if r.start == far {
                    next = Some((j, false));
                    break;
                }
                if r.end == far {
                    next = Some((j, true));
                    break;
                }
            }
            let Some((j, rev)) = next else { break false };
            in_seq[j] = true;
            seq.push((j, rev));
            far = if rev { runs[j].start } else { runs[j].end };
        };
        if !closed {
            continue;
        }
        // Concatenate the walk, flipping what the junctions asked for.
        // Shared seam vertices arrive as consecutive repeats and
        // closing wraps - `push_loop` and `Loop::new` drop exactly
        // those, so the ring needs no special handling here.
        let mut pts: Vec<Point2> = Vec::new();
        for (j, rev) in &seq {
            for &it in &runs[*j].items {
                consumed.insert(it, rings.len());
            }
            if *rev {
                pts.extend(runs[*j].pts.iter().rev().copied());
            } else {
                pts.extend(runs[*j].pts.iter().copied());
            }
        }
        let mut twice = 0.0;
        let n = pts.len();
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (pts[i], pts[i + 1]);
            twice += a.x * b.y - a.y * b.x;
        }
        if twice < 0.0 {
            pts.reverse();
        }
        rings.push((
            format!("the stitched rim of '{}'", runs[s].entity),
            pts,
            seq.len(),
        ));
    }
    (rings, consumed)
}

/// Accept one selected boundary: refuse what collapses to a line seen
/// from above, and count a rim already in the region once - two faces
/// meeting at an edge share it, and a coincident double loop would be a
/// zero-wide sliver.
fn push_loop(
    label: &str,
    pts: Vec<Point2>,
    loops: &mut Vec<Loop>,
    rims: &mut Vec<Vec<(i64, i64)>>,
) -> Result<(), GenError> {
    let mut key: Vec<(i64, i64)> = pts.iter().map(qkey).collect();
    // Same drop Loop::new does - consecutive repeats and the closing
    // wrap - so the collapse check below sees the ring the loop would
    // actually be.
    key.dedup();
    if key.len() > 1 && key.first() == key.last() {
        key.pop();
    }
    if key.len() < 3 {
        // A vertical wall seen from above, or a chain that runs up one:
        // nothing encloses.
        return Err(GenError::Invalid(format!(
            "{label} projects to a line from above and bounds no area; select a horizontal face or a closed edge chain around the part"
        )));
    }
    let canon = canon_ring(&key);
    if rims.contains(&canon) {
        return Ok(());
    }
    let loop_ = Loop::new(pts)?;
    if loop_.area().abs() < DEGENERATE_AREA {
        return Err(GenError::Invalid(format!(
            "{label} projects to a line from above and bounds no area; select a horizontal face or a closed edge chain around the part"
        )));
    }
    rims.push(canon);
    loops.push(loop_);
    Ok(())
}

/// Rotate- and mirror-canonical form of a quantized ring: the
/// lexicographic minimum over its 2n walks. Compared exactly, never by
/// hash - a colliding dedupe key would silently drop a real boundary.
fn canon_ring(ring: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let n = ring.len();
    if n == 0 {
        return Vec::new();
    }
    let mut best: Option<Vec<(i64, i64)>> = None;
    for start in 0..n {
        for step in [1usize, 2] {
            let walk: Vec<(i64, i64)> = (0..n).map(|k| ring[(start + k * step) % n]).collect();
            if best.as_ref().is_none_or(|b| walk < *b) {
                best = Some(walk);
            }
        }
    }
    best.unwrap_or_default()
}

/// Run one generated path after another. Both start and end on the
/// clearance plane, so the joint is a clearance rapid: append and re-anchor
/// the added path's opening `from` to the previous move's `to` - a stale
/// `(0, 0)` there would read as a teleport to the origin.
fn stitch(mut a: Toolpath, mut b: Toolpath) -> Toolpath {
    if let (Some(to), Some(first)) = (a.moves.last().map(|m| m.to), b.moves.first_mut()) {
        first.from = to;
    }
    a.moves.append(&mut b.moves);
    a.cycles.append(&mut b.cycles);
    a
}

/// Generate the path for an operation as the world stands now, and verify
/// it. Reads only; the caller stores whatever comes back. The error class
/// is `GenError` for both setup problems (missing tool/stock, unusable
/// geometry) and generator refusals - commands surface them at
/// construction, so nothing half-generated reaches the world.
/// `op_name` is the operation's own name, needed by re-machining to find
/// the program ahead of it (M3.1 D); callers generating a params preview
/// for an operation that has no entity (or none that matters) pass `""`,
/// which reads as "nothing ahead" and generates against the bare stock.
pub fn generate(
    world: &World,
    op: &Operation,
    op_name: &str,
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    // Tool: the diameter drives the path, the body drives the reach law.
    let Some(tool_entity) = model::find_by_name(world, &op.tool) else {
        return Err(GenError::Invalid(format!("no tool named '{}'", op.tool)));
    };
    let Some(tool) = model::tool_of(world, &tool_entity) else {
        return Err(GenError::Invalid(format!("'{}' is not a tool", op.tool)));
    };
    let d = tool.diameter;

    // Source material (the slot reads its own endpoints instead, but the
    // verification still wants to know what stands around it).
    let setup = op.setup.as_deref();
    let material = material_of(world, setup, &op.src)?;
    // Setup stock, when it is not the source itself: the swarf column
    // around a smaller model. Island-vs-swarf in the reach law keys on
    // `region.contains`, so this only widens what counts as material -
    // and it is THIS operation's setup's stock (M3.2 A), never a
    // neighbor setup's block.
    let stock_region = match &op.src {
        SrcRef::Stock => None,
        SrcRef::Entity(_) | SrcRef::Selection(_) => {
            model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
                .and_then(|e| model::world_facets(world, &e))
                .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
                .filter(|r| !r.is_empty())
        }
    };
    let stock_top_z = material.top
       .max(
            model::stock_world_aabb_for(world, setup)
                .map(|b| b.max.z)
                .unwrap_or(f64::NEG_INFINITY),
        );

    let mode = match op.params.kind() {
        // Drill verifies as Boundary on purpose: its region's rim loops
        // put the gouge law's tangency test at the hole wall, where a
        // matched drill passes and an oversized tool is flagged. What
        // the move-by-move laws cannot see for a straight deep plunge
        // (it clears its own kerf, so the band sampling never fires)
        // the cycle reach law in `verify` checks over `path.cycles`.
        OpType::Face | OpType::Contour | OpType::Drill => Mode::Boundary,
        // The 3-D pocket verifies as Area with the mesh in hand (the
        // `model` field below): the region is its silhouette, the
        // per-level islands come from the facets.
        OpType::Pocket | OpType::Slot | OpType::Pocket3D => Mode::Area,
        OpType::Trace => Mode::Engrave,
        // The waterline's verdict is measured, not deduced from
        // tangency: law (f) of `verify` scans the swept ball against
        // the mesh (the `finish_tol` field below carries the budget).
        OpType::Finish3D => Mode::Envelope,
    };

    // The height seam: the params' From+offset expressions become the
    // generators' plain f64s right here. Every unresolved reference is
    // refused by name before a single move is computed - never a
    // guessed z. `machining2d`'s params keep taking resolved values, so
    // the generators (and the goldens) know nothing of references.
    let hs = resolve_params(world, setup, &op.params).map_err(GenError::Invalid)?;

    // Region + path per kind.
    let (region, path) = match &op.params {
        OpParams::Face { stepover, stepdown, feed, plunge_feed, climb, .. } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = machining2d::face(
                &region,
                &FaceParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    clearance_z: hs.clearance,
                    cut_z: req_z(hs.bottom, "Cut Z")?,
                    top_z: hs.face_top,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        OpParams::Contour {
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            side,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
            ..
        } => {
            // Backstop for a committed file that carries Inside on a
            // whole-model source (commands refuse it at creation): the
            // mirror-gouge law must never reach a path.
            if let Some(msg) = side_sane(&op.params, &op.src) {
                return Err(GenError::Invalid(msg));
            }
            let cp = ContourParams {
                tool_diameter: d,
                stock_to_leave: *stock_to_leave,
                clearance_z: hs.clearance,
                retract_z: req_z(hs.retract, "Retract Z")?,
                top_z: req_z(hs.top, "Top of Part Z")?,
                bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                side: *side,
                lead: *lead,
                tolerance: *tolerance,
                tab_count: *tab_count,
                tab_width: *tab_width,
                tab_height: *tab_height,
            };
            let (region, path) = match &op.src {
                SrcRef::Selection(items) => {
                    // The boundary path cuts the loops the picks bound;
                    // each open run in the picks adds a centerline pass
                    // along its own points. Either half alone is enough;
                    // together they machine the whole pick list.
                    let (loops, runs, auto_inside) =
                        selection_geometry_impl(world, setup, items, &op.params, None)?;
                    if loops.is_empty() && runs.is_empty() {
                        return Err(GenError::Empty(
                            "nothing in the selection bounds an area or a run".into(),
                        ));
                    }
                    let mut cp = cp;
                    cp.side = match side {
                        Side::Auto if auto_inside => Side::Inside,
                        Side::Auto => Side::Outside,
                        s => *s,
                    };
                    let region = if loops.is_empty() {
                        Region::empty()
                    } else {
                        Region::new(loops)?
                    };
                    let mut path = if region.is_empty() {
                        Toolpath::new(Vec::new())
                    } else {
                        machining2d::contour(&region, &cp)?
                    };
                    for run in &runs {
                        path = stitch(path, machining2d::contour_along_chain(run, &cp)?);
                    }
                    (region, path)
                }
                _ => {
                    let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
                    let path = machining2d::contour(&region, &cp)?;
                    (region, path)
                }
            };
            (region, path)
        }
        OpParams::Pocket {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            ..
        } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hs.clearance,
                    retract_z: hs.retract,
                    top_z: req_z(hs.top, "Top of Part Z")?,
                    bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    strategy: *strategy,
                },
            )?;
            (region, path)
        }
        OpParams::Pocket3D {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            rest_rough,
            max_engagement,
            feed_opt,
            ..
        } => {
            // The region is the model's own silhouette (see
            // `region_for`); the level law sections the facets
            // themselves at every plane, so there is nothing here for a
            // picked loop to add - and nothing to erode by hand either:
            // one erosion per level, from that level's target.
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let pp = Pocket3DParams {
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hs.clearance,
                top_z: req_z(hs.top, "Top of Part Z")?,
                bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hs.retract,
                entry: *entry,
                helix_radius: *helix_radius,
                max_engagement: *max_engagement,
            };
            // Re-machining (M3.1 D): what the program ahead of this
            // pocket left standing bounds the coarse run, so it cuts
            // the shelves its predecessors already cleared only if the
            // mask says material still stands there. Nothing ahead (or
            // nothing cut) is `None`, and the path is what it was.
            let prior = crate::rest::prior_state(world, op_name, setup, d * 0.5)?;
            let mask = prior.grid.as_ref();
            // The rest pass's verdict rides the path: what the sim
            // still reports after the closing ladder is the closing
            // report's business (the simulation view), not a silent
            // extra here.
            let generated = if *rest_rough {
                machining3d::pocket3d_with_rest_prev(&material.facets, &region, &pp, mask)
                    .map(|(p, _)| p)
            } else {
                machining3d::pocket3d_prev(&material.facets, &region, &pp, mask)
            };
            let path = match generated {
                Ok(path) => path,
                // The ladder's words are "the model fills the pocket"
                // - true of an unmasked nothing-to-cut. With a mask
                // the truer sentence is this one: the program already
                // machined here, and the mask law only ever removes
                // cuts, so the whole path is air.
                Err(GenError::Empty(_)) if mask.is_some() => {
                    return Err(GenError::Empty(
                        "nothing left to cut - the operations ahead in the program already \
                         machined this area"
                            .into(),
                    ));
                }
                Err(e) => return Err(e),
            };
            // The feed modulation is a re-read of the finished path
            // against the sim (M2.1d): same moves, new F words - it
            // runs after both ladder variants, never instead of one.
            let path = if *feed_opt {
                machining3d::optimize_feed(&path, &material.facets, &region, &pp)?
            } else {
                path
            };
            (region, path)
        }
        OpParams::Finish3D {
            pattern,
            stepover,
            tolerance,
            feed,
            plunge_feed,
            climb,
            ramp,
            min_step,
            ramp_angle,
            ..
        } => {
            // Ball jurisdiction: the rings are the tangent locus of a
            // sphere. A flat-bottomed or torus-bottomed cutter's
            // forbidden set is not this facetwise support scan (the
            // Minkowski sum differs), so an end mill here refuses by
            // name - never a ball path dressed as something else.
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "3D Finish needs a ball-nose tool, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let fp = Finish3DParams {
                pattern: *pattern,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hs.clearance,
                retract_z: hs.retract,
                top_z: req_z(hs.top, "Top of Part Z")?,
                bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: *ramp,
                min_step: *min_step,
                ramp_angle: *ramp_angle,
            };
            // The pattern picks the generator; Waterline never
            // reaches the raster code at all.
            let path = match pattern {
                finishing3d::FinishPattern::Waterline => {
                    finishing3d::waterline(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Parallel
                | finishing3d::FinishPattern::ParallelMulti
                | finishing3d::FinishPattern::Scallop => {
                    finishing3d::parallel(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Radial => {
                    finishing3d::radial(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Spiral => {
                    finishing3d::spiral(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Contour => {
                    finishing3d::contour(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Project => {
                    finishing3d::project(&material.facets, &fp)?
                }
            };
            (region, path)
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
            ..
        } => {
            let path = machining2d::slot(
                *a,
                *b,
                &SlotParams {
                    tool_diameter: d,
                    width: *width,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hs.clearance,
                    retract_z: hs.retract,
                    top_z: req_z(hs.top, "Top of Part Z")?,
                    bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stepover: *stepover,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            (region, path)
        }
        OpParams::Trace { stepdown, feed, plunge_feed, climb, .. } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = machining2d::trace(
                &region,
                &TraceParams {
                    clearance_z: hs.clearance,
                    retract_z: hs.retract,
                    top_z: req_z(hs.top, "Top of Part Z")?,
                    bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        OpParams::Drill { cycle, peck, dwell, feed, plunge_feed, holes, .. } => {
            // No region feeds the generator - the hole list is the
            // geometry - but the verify pass below needs one, and it is
            // built the same shared way as every other kind's.
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = drilling::drill(&drilling::CycleParams {
                clearance_z: hs.clearance,
                r_z: req_z(hs.top, "Top of Part Z")?,
                hop_z: hs.retract,
                bottom_z: hs.bottom,
                cycle: *cycle,
                peck: *peck,
                dwell: *dwell,
                feed: *feed,
                plunge_feed: *plunge_feed,
                tool: &tool,
                holes,
            })?;
            (region, path)
        }
    };

    // M3.2 stage B: what the program clamps to. The op's setup context
    // decides - user fixtures bound to it (tagged models, implicit-
    // setup resolution on both sides) plus the machine table when the
    // .mch states a travel the table can sit in.
    let fixture_boxes = crate::fixture::boxes_for(world, op.setup.as_deref());
    let geom = tool.geom();
    // The machine's own steel, if it states any (M3.2 stage B): the
    // unstated-limit law means a file that names no spindle body adds
    // no band, exactly as it adds no feed or travel check.
    let spindle = crate::machine::of(world)
        .1
        .and_then(|m| m.spindle_d.zip(m.spindle_len))
        .map(|(d, l)| (d * 0.5, l));
    let warns = verify::verify(
        &path,
        &VerifyContext {
            region: &region,
            stock: stock_region.as_ref(),
            // Only the mesh-driven kinds' laws are depth-dependent;
            // every other op verifies against its static region,
            // untouched.
            model: matches!(
                &op.params,
                OpParams::Pocket3D { .. } | OpParams::Finish3D { .. }
            )
            .then_some(&material.facets[..]),
            tool: &geom,
            mode,
            stock_top_z,
            clearance_z: hs.clearance,
            stock_to_leave: match &op.params {
                OpParams::Face { .. }
                | OpParams::Trace { .. }
                | OpParams::Drill { .. }
                | OpParams::Finish3D { .. } => 0.0,
                OpParams::Contour { stock_to_leave, .. }
                | OpParams::Pocket { stock_to_leave, .. }
                | OpParams::Slot { stock_to_leave, .. }
                | OpParams::Pocket3D { stock_to_leave, .. } => *stock_to_leave,
            },
            gouge_tol: GOUGE_TOL,
            z_tol: Z_TOL,
            // The finish op's envelope budget is its own tolerance; no
            // other kind runs the envelope law.
            finish_tol: match &op.params {
                OpParams::Finish3D { tolerance, .. } => Some(*tolerance),
                _ => None,
            },
            fixtures: &fixture_boxes,
            spindle,
        },
    );
    Ok((path, warns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;

    #[test]
    fn stadium_is_a_closed_simple_ring() {
        let r = stadium(Point2::new(0.0, 0.0), Point2::new(50.0, 0.0), 10.0).expect("finite");
        assert_eq!(r.loops().len(), 1);
        // The center and both caps' extents are inside; a point beyond the
        // cap is not.
        assert!(r.contains(Point2::new(25.0, 0.0)));
        assert!(r.contains(Point2::new(59.9, 0.0)));
        assert!(!r.contains(Point2::new(60.5, 0.0)));
        // The caps reach 10 mm past each endpoint, so just inside a cap is
        // in, and a point past its arc is out.
        assert!(r.contains(Point2::new(-9.5, 0.0)));
        assert!(!r.contains(Point2::new(-10.5, 0.0)));
    }

    #[test]
    fn zero_length_stadium_is_a_circle() {
        let r = stadium(Point2::new(1.0, 2.0), Point2::new(1.0, 2.0), 5.0).expect("finite");
        assert!(r.contains(Point2::new(1.0, 2.0)));
        assert!(!r.contains(Point2::new(1.0 + 5.5, 2.0)));
    }

    #[test]
    fn params_sanity_gates() {
        // `is_sane` is structural - finite numbers, positive increments,
        // in-range slots; ordering needs resolved heights and lives in
        // `depth_sane`, which commands run with the world in hand. An
        // unresolved reference is never a lie there: refusing happens at
        // generate, by name.
        let world = crate::world::new_world();
        let ok = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z: None,
            stepdown: None,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(ok.is_sane() && depth_sane(&world, None, &ok));
        // Staged facing is sane when it descends: a top below the
        // finished height, or a zero stepdown, is not.
        let staged = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z: Some(Height::fixed(1.0)),
            stepdown: Some(0.5),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(staged.is_sane() && depth_sane(&world, None, &staged));
        let low = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z: Some(Height::fixed(-0.6)),
            stepdown: Some(0.5),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(low.is_sane(), "structurally fine - the order is the flaw");
        assert!(
            !depth_sane(&world, None, &low),
            "staged top below the finished height"
        );
        let no_step = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z: Some(Height::fixed(1.0)),
            stepdown: Some(0.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(!no_step.is_sane(), "stepdown must be positive when set");
        let bad = OpParams::Pocket {
            stepover: f64::NAN,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(5.0),
            retract_z: None,
            top_z: Height::fixed(0.0),
            bottom_z: Height::fixed(-4.0),
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
        };
        assert!(!bad.is_sane());
        // A depth range that runs upward is nonsense, refused at command
        // creation - not by the shape of the numbers but by their z's.
        let up = OpParams::Trace {
            clearance_z: Height::fixed(5.0),
            retract_z: None,
            top_z: Height::fixed(-4.0),
            bottom_z: Height::fixed(0.0),
            stepdown: 1.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(up.is_sane());
        assert!(!depth_sane(&world, None, &up));
        // An out-of-range slot is structural: there is no seventh offset.
        let ghost = OpParams::Trace {
            clearance_z: Height::fixed(5.0),
            retract_z: None,
            top_z: Height::fixed(5.0),
            bottom_z: Height::of(HeightReference::WorkOffset(6), 0.0),
            stepdown: 1.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(!ghost.is_sane(), "work-offset references must name a real slot");
    }

    #[test]
    fn stamp_follows_every_input() {
        // Tool numerics, params and source must each move the digest; the
        // world pieces are exercised in tests/operations.rs, the pure
        // param path is deterministic here (Fixed heights resolve without
        // touching the world).
        let world = crate::world::new_world();
        let a = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z: None,
            stepdown: None,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        // (No struct-update syntax on enum variants: write it out.)
        let b = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.6),
            top_z: None,
            stepdown: None,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        let c = OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z: Some(Height::fixed(1.0)),
            stepdown: Some(0.5),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        let mut ha = std::hash::DefaultHasher::new();
        let mut hb = std::hash::DefaultHasher::new();
        let mut hc = std::hash::DefaultHasher::new();
        a.stamp_with(&world, None, &mut ha);
        b.stamp_with(&world, None, &mut hb);
        c.stamp_with(&world, None, &mut hc);
        assert_ne!(ha.finish(), hb.finish());
        assert_ne!(ha.finish(), hc.finish(), "staging moves the stamp");
    }

    #[test]
    fn face_stamp_moves_with_stepdown() {
        // A cached single-level facing must never be replayed as the
        // answer for a staged one - and two stagings that cut different
        // stacks must not claim each other's cache either.
        let world = crate::world::new_world();
        let face_with = |top_z: Option<Height>, stepdown: Option<f64>| OpParams::Face {
            stepover: 8.0,
            clearance_z: Height::fixed(5.0),
            cut_z: Height::fixed(-0.5),
            top_z,
            stepdown,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        let digest = |p: &OpParams| {
            let mut h = std::hash::DefaultHasher::new();
            p.stamp_with(&world, None, &mut h);
            h.finish()
        };
        let legacy = face_with(None, None);
        let staged = face_with(Some(Height::fixed(1.0)), Some(0.5));
        let deeper = face_with(Some(Height::fixed(1.0)), Some(0.4));
        assert_ne!(digest(&legacy), digest(&staged), "staging invalidates");
        assert_ne!(digest(&staged), digest(&deeper), "step size is geometry");
        assert_eq!(
            digest(&staged),
            digest(&face_with(Some(Height::fixed(1.0)), Some(0.5)))
        );
    }
}
