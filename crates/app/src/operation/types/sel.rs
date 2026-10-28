//! Split from `types.rs` by M5.0.1; structure only.

use crate::machining2d::Lead;

/// Work coordinate system slots - the machine offsets posted as G54..G59.
pub const WCS_SLOTS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];


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
    /// Simultaneous 5-axis level finishing: the waterline ring ladder
    /// with the TOOL AXIS moving per move - lead/lag lean against a
    /// tilt clamp, for shank clearance on steep walls (M5.1 stage C,
    /// see [`crate::multiaxis`]).
    Contour5D,
    /// Simultaneous 5-axis raster: the Z-following parallel pass with
    /// the same axis law (M5.1 stage C).
    Parallel5D,
    /// Ruled-surface (bladed) finishing: the raster run ALONG the
    /// surface's own straight direction, the tool lying with the
    /// flank instead of scalloping against it (M5.1 stage D, see
    /// [`crate::multiaxis`]).
    Swarf,
    /// The swarf family's tolerance ladder: the same path run
    /// coarse-first, finishing at the nominal tolerance (M5.1 stage D).
    AdvancedSwarf,
    /// Rotary 4-axis finishing of a turned work: the waterline ring
    /// ladder about the FITTED axis of revolution, the tool posed
    /// radially (M5.1 stage E, see [`crate::multiaxis::axis`]).
    RotaryContour,
    /// Rotary passes ALONG the fitted axis, stepping around the bar -
    /// the laid-down sibling of [`Self::RotaryContour`] (M5.1 stage E).
    RotaryParallel,
    /// The rotary family's ROUGH ladder (M5.1 stage E2): the pocket
    /// machinery run in wrap space about the fitted axis - the
    /// developed cylinder eroded level by level in radial infeed,
    /// every cut move standing RADIAL on the way back out.
    RotaryOffset,
    /// Multi-axis roughing (M5.1 stage E3): the 3-D pocket ladder,
    /// move for move, with the stage-C stamp read over it - floors
    /// and air keep the spindle vertical, steep flanks take it off
    /// vertical within the tilt cone. The part stands still and the
    /// TOOL AXIS moves; one 3+2 index cannot hold these poses (see
    /// [`crate::multiaxis`]).
    MultiAxisRough,
    /// Flowline finishing (M5.1 tail): the raster's laws with a
    /// direction that turns - each pass follows the surface's own
    /// least-curving direction field, so lines stay WITH a vane or
    /// channel instead of crossing it. Plain ground makes no
    /// directional claim and degrades to the parallel raster; the
    /// steep flanks are still the waterline rings' (see
    /// [`crate::multiaxis`]).
    Flow5D,
    /// Multi-axis contour on drive curves (M5.1 tail, stage J): the
    /// picked edge chains, machined WHERE THEY STAND - no pattern is
    /// asked of the model, the curve is the path. At every vertex the
    /// ball rests on the fan of planes the mesh shows there (the
    /// bisector of an edge, the seat of a corner) and the axis poses
    /// by the same lean law the family uses (see [`crate::multiaxis`]).
    MultiAxisContour,
    /// Turning facing (M5.2 stage A): the work spins, the insert
    /// travels - radial passes at Z stations stepping down the
    /// fitted axis toward the facing plane, the turned profile's
    /// own silhouette as the limit at every station (see
    /// [`crate::turning`]). The first kind of the TURN family: the
    /// family gate is what keeps it off a mill.
    Facing,
    /// Profile roughing (M5.2 stage B): the concentric rung ladder on
    /// the turned silhouette — the nose's ride at distance
    /// nose+stock+j·stepdown, rungs outermost first, each eating a
    /// radial shell down to the leave the finish takes off (see
    /// [`crate::turning`]).
    ProfileRough,
    /// Profile finishing (M5.2 stage B): one ride of the nose along
    /// the offset of the turned silhouette — the bar's own skin
    /// within the chord budget, lead in and slide out radially (see
    /// [`crate::turning`]).
    ProfileFinish,
    /// Grooving roughing (M5.2 stage C): the band ladder on a turned
    /// part — a full-width blade stepping down each center it tiles
    /// across the window, to the floor plus the leave the finish
    /// takes off (see [`crate::turning`]).
    GrooveRough,
    /// Grooving finishing (M5.2 stage C): one full-depth plunge per
    /// center to the band's floor — the model's own surface,
    /// tangentially, at every station the blade stands in.
    GrooveFinish,
    /// Parting (M5.2 stage C): the bar severed at one plane — a
    /// depth ladder at the parting station and the through-axis
    /// rung that lets the scrap go.
    Part,
    /// Threading (M5.2 stage D): the synchronized spindle walking a
    /// helix — a V-form insert's wave ladder in the meridian, one
    /// full turn per pitch of the modeled thread across the window
    /// of turns the helix imposes (see [`crate::turning`]). The
    /// verdict is [`Self::GrooveFinish`]'s meridian scan with the
    /// wave clause: the tool's V, not a boxcar, says what stands
    /// between the grooves.
    Threading,
    /// Chamfer turning (M5.2 stage E): one ride of the nose across
    /// a MODELED cone face — the authored angle is a promise the
    /// model must keep, and the ride chases the skin, not the
    /// number (see [`crate::turning`]). A face the model does not
    /// have is refused at the door by name.
    Chamfering,
    /// Taper turning (M5.2 stage E): the nose's ride of the cone
    /// the two authored diameters state — again a promise over a
    /// MODELED taper, so the pass and the verdict stand on the
    /// same skin. Equal diameters state the straight turn.
    TaperTurning,
    /// Multi-pass turning (M5.2 stage E): the roughing ladder down
    /// to the leave plus a sequence of finishing rides that share
    /// it out — the spring-pass law, each ride standing half the
    /// last one's shell in from it, the last on the profile
    /// itself. The moves are stages B's own, chained.
    MultiPassTurning,
}

impl core::fmt::Display for OpType {
    /// The panel label, read from the kind registry (M5.0.2) - the one
    /// list the tree, sheet, combo and ribbon tips all quote.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.info().label)
    }
}

pub(super) fn lead_sane(lead: &Lead) -> bool {
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
