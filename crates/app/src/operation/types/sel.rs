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
