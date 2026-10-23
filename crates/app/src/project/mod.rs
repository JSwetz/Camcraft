//! Project save/load: `.hsmproj`, JSON, version 20.
//!
//! Serialization goes through plain DTO structs (deliberately not flecs
//! reflection): the file format is our public persistence contract and must
//! stay stable regardless of ECS storage decisions. Entity order is by name,
//! so a save -> load -> save cycle is byte-identical.
//!
//! v2 added the rotation quaternion to every entity (rigid transforms, M1.2)
//! and the `kind`/`stock` fields for setup furniture; both are defaulted, so
//! a v2 file written before stock existed still parses. Parametric stock is
//! persisted as PARAMETERS only - `stock::stock_mesh` regenerates identical
//! facets in any build (see `stock.rs`), keeping files small and round-trips
//! exact. v3 adds `kind` values `tool`/`operation` with a `tool`/`operation`
//! payload; CAM data is persisted as definition plus the cached path (paths
//! are expensive to recompute, and staleness recomputes anyway). v4 makes
//! feature picks a contractual source: the `selection` variant of `src`
//! already existed in v3 files (it arrived with the domain type), so every
//! v3 file reads unchanged - the bump records that picks are now covered
//! by the save/load contract rather than an experiment in transit. v5 is
//! the M1.5.2 landing shape: welded-face picks (`patch`), arc moves
//! (`arc`), facing stepdowns (`top_z`/`stepdown` on face) and pocket
//! retract hops (`retract_z` on pocket) all appear in the file grammar at
//! once, each with a serde default that reads as exact legacy behavior -
//! a v4 file changes nothing in meaning, and a v5 build that has not
//! wired one of these yet round-trips files that carry it without
//! breaking it. Later steps wire the fields to the generators; the
//! format does not move again. v6 is the heights landing shape: every
//! operation height field (`clearance_z`, `top_z`, `bottom_z`,
//! `retract_z`, `cut_z`) is now a From+Offset expression
//! (`{"reference": ..., "offset": ...}`, see `HeightDto`), and a bare
//! number - the whole v5 grammar for a height - still reads, as
//! `{"reference": "fixed", "offset": n}` with the saved z preserved to
//! the bit. Work-offset references name a `G54..G59` slot; the slots'
//! placements were already persisted in every entity's
//! `translation`/`rotation`, so nothing else about the file moved. v7
//! adds the contour side choice (`side` on contour params, see
//! `SideDto`): an absent key reads as `auto` - today's Outside law until
//! the selection walk resolves it - so a v6 file changes nothing in
//! meaning. v8 adds document units (a top-level `units` key, see
//! `crate::units`): an absent key reads as `mm`, which is exactly what
//! every v7 file displayed. v9 widens the contour `lead` vocabulary with
//! `tangential` (see `LeadDto`); `lead` has been a required key since
//! v1, so no older file contains the new tag and none needs a content
//! change. v10 is the drilling landing shape: the `drill` operation
//! params (with its hole list, see `HoleDto`), the `tip_angle`/`pitch`
//! tool fields, and the cached cycle list (`PathDto.cycles`) all arrive
//! at once, each with a serde default that reads as exact legacy
//! behavior - no pre-v10 file could contain a drill-kind tool or a
//! drill operation (the tags did not exist), and no pre-v10 path ever
//! had cycles, so absent means precisely what it behaved as. v11 is the
//! 3-D pocket landing shape: the `pocket3d` operation params arrive with
//! a serde default on every new key (`retract_z` absent is
//! hop-to-clearance, `entry` absent is Plunge), and no pre-v11 file could
//! contain a 3-D pocket operation at all - the tag did not exist - so the
//! step-up is the stamp itself. v12-v18 added the later operation-family
//! grammars (corner/flow profiles, surface finish, ramp), each the same
//! pure step-up, one default per new key; v19 added the top-level `machine`
//! key (absent means no machine, the mute checker). v20 is M3.1: the model
//! `rev` key (the geometry-revision counter, see `schema::Revision`) - no
//! pre-v20 file could carry it, and absent means 0, which is exactly what
//! every pre-v20 document was. The signatures that make staleness work
//! recompute from the persisted facets; nothing else moved. v21 is
//! M3.2 stage A: setups arrive as a `setup` kind with its `SetupDto`
//! payload, an optional `setup` name inside `OperationDto`, and a
//! top-level `active_setup` key. No pre-v21 file can contain any of them:
//! the kind tag did not exist, and every default (no setup rows, no
//! setup names, no explicit active choice) is exactly the legacy global
//! behavior the implicit-setup law already guarantees. Step-up is the
//! stamp itself, like every bump since v6. v22 is M3.2 stage B: a
//! fixture is a model entity wearing a `fixture` marker (`FixtureDto` -
//! one key, the setup binding; the solid itself is the model's own
//! persisted geometry, and the machine's table is never stored, it is
//! derived from the `machine` key). No pre-v22 file can contain the
//! marker - the component did not exist - and the default (absent means
//! no fixture) is exactly the pre-B world: collision checked against
//! stock and model alone. Step-up is the stamp itself, like every bump
//! since v6. v23 is M3.3 stages A-B: the top-level `post` key (a user
//! `.cps` path, or a shipped registry name once stage B grew the
//! registry; absent means the default post - exactly what every
//! pre-v23 file ran, since the registry then held one script). No
//! pre-v23 file can contain the key and its default is the shipping
//! behavior verbatim. Step-up is the stamp itself, like every bump
//! since v6. v1-v22 files migrate on load, one version step at a time.

use std::path::Path;

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;
use serde::{Deserialize, Deserializer, Serialize};

use crate::cl::{Cycle, CycleKind, Move, MoveKind, Toolpath, lattice};
use crate::drilling::{CycleChoice, Hole};
use crate::geo2d::Point2;
use crate::geometry::{IDENTITY_Q, Point3, Triangle};
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::machining3d::Entry3D;
use crate::model;
use crate::operation::{FeatureSel, GenStamp, GeoSel, OpParams, SrcRef};
use crate::schema::{
    FixtureDef, Location, Operation, SetupDef, StockDef, ToolDef, Toolpath as ToolpathCache, Wcs,
    STOCK_NAME, WCS_SLOTS,
};
use crate::stock::{self, StockShape};
use crate::tools::{Tool, ToolKind};
use crate::units::{self, Unit};
use crate::verify::Warning;

pub const PROJECT_VERSION: u32 = 23;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported project version {0} (this build writes {PROJECT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("malformed project file: {0}")]
    Corrupt(String),
}

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    /// The document's display unit ("mm" | "in"), v8+. Absent means
    /// millimeters - exactly what every pre-v8 file displayed, so the
    /// serde default is itself the whole migration. A millimeter document
    /// omits the field (byte-shape of the pre-v8 files, plus the stamp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// The document's machine (M2.3 C), v19+. An absent key means no
    /// machine was ever loaded - exactly what every v18 file meant,
    /// since the checker did not exist to read one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<MachineDto>,
    /// The document's explicit active-setup choice (M3.2 A), v21+. An
    /// absent key means no choice was ever made, which resolves to the
    /// first setup by name - what every v20 document did. A name that
    /// matches no setup row is legal, not corrupt: it reads through the
    /// same existence fallback the live singleton does (the fallback that
    /// also protects an undo of `DeleteSetup` mid-history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_setup: Option<String>,
    /// The document's post script (M3.3 A/B), v23+. An absent key
    /// means the default post - what every v22 file ran, since the
    /// registry then held one script. `{"file": path}` names a user
    /// `.cps`; `{"named": name}` picks a shipped registry entry BY
    /// NAME (stage B grew the registry to five, and an index into it
    /// would make a re-order silently change a saved document's post -
    /// names survive growth, indices don't). Chosen values are stored
    /// as chosen: opening does not re-validate (the `SelectPost`
    /// constructor did), and a script that went missing - file or
    /// registry name - refuses by name at post time, never silently
    /// falling back (`script_source`'s law).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<PostDto>,
    pub entities: Vec<EntityDto>,
}

/// The file mirror of the `PostRef` singleton (M3.3): exactly the two
/// choices a document can carry beyond the default, each spelled for
/// what it is - a registry name or a path, never one string standing
/// ambiguously for both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PostDto {
    Named(String),
    File(String),
}

/// The file mirror of the `MachineDef` singleton: the parsed machine
/// and the `.mch` path it came from, kept for display so opening a
/// project restates where the setup's numbers live. `Machine` is
/// serde-transparent (our own struct, our own field semantics).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MachineDto {
    pub path: String,
    pub machine: crate::machine::Machine,
}

/// What an entity is, in file terms. Absent means `Model` (the only kind
/// v1 and early v2 files could hold).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Model,
    Stock,
    Wcs,
    Tool,
    Operation,
    Setup,
}

/// The file mirror of `StockShape`, kept separate so the domain enum can
/// evolve without editing the persistence contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StockDto {
    Box { dx: f64, dy: f64, dz: f64 },
    Cylinder { diameter: f64, height: f64 },
    FromModel { source: String },
}

impl StockDto {
    fn from_shape(shape: &StockShape) -> Self {
        match shape {
            StockShape::Box { dx, dy, dz } => StockDto::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockShape::Cylinder { diameter, height } => StockDto::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockShape::FromModel { source } => StockDto::FromModel {
                source: source.clone(),
            },
        }
    }

    fn to_shape(&self) -> StockShape {
        match self {
            StockDto::Box { dx, dy, dz } => StockShape::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockDto::Cylinder { diameter, height } => StockShape::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockDto::FromModel { source } => StockShape::FromModel {
                source: source.clone(),
            },
        }
    }
}

/// The file mirror of `tools::Tool`; same reason as `StockDto`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolDto {
    pub kind: ToolKindDto,
    pub diameter: f64,
    pub corner_r: f64,
    /// M1.6 additions: absent from a v9 file, serde-defaults to the
    /// inert 0.0 both fields were behaviorally equal to before.
    #[serde(default)]
    pub tip_angle: f64,
    #[serde(default)]
    pub pitch: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    pub material_group: String,
    pub rpm: f64,
    pub feed: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolKindDto {
    EndMill,
    BallNose,
    BullNose,
    Drill,
    Tap,
    Bore,
}

impl ToolDto {
    fn from_tool(t: &Tool) -> Self {
        ToolDto {
            kind: match t.kind {
                ToolKind::EndMill => ToolKindDto::EndMill,
                ToolKind::BallNose => ToolKindDto::BallNose,
                ToolKind::BullNose => ToolKindDto::BullNose,
                ToolKind::Drill => ToolKindDto::Drill,
                ToolKind::Tap => ToolKindDto::Tap,
                ToolKind::Bore => ToolKindDto::Bore,
            },
            diameter: t.diameter,
            corner_r: t.corner_r,
            tip_angle: t.tip_angle,
            pitch: t.pitch,
            flute_len: t.flute_len,
            shank_d: t.shank_d,
            shank_len: t.shank_len,
            holder_d: t.holder_d,
            holder_len: t.holder_len,
            material_group: t.material_group.clone(),
            rpm: t.rpm,
            feed: t.feed,
        }
    }

    fn to_tool(&self) -> Tool {
        Tool {
            kind: match self.kind {
                ToolKindDto::EndMill => ToolKind::EndMill,
                ToolKindDto::BallNose => ToolKind::BallNose,
                ToolKindDto::BullNose => ToolKind::BullNose,
                ToolKindDto::Drill => ToolKind::Drill,
                ToolKindDto::Tap => ToolKind::Tap,
                ToolKindDto::Bore => ToolKind::Bore,
            },
            diameter: self.diameter,
            corner_r: self.corner_r,
            tip_angle: self.tip_angle,
            pitch: self.pitch,
            flute_len: self.flute_len,
            shank_d: self.shank_d,
            shank_len: self.shank_len,
            holder_d: self.holder_d,
            holder_len: self.holder_len,
            material_group: self.material_group.clone(),
            rpm: self.rpm,
            feed: self.feed,
        }
    }
}

/// The file mirror of `operation::SrcRef` (names, like everywhere else).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SrcDto {
    Stock,
    Entity { name: String },
    Selection { items: Vec<GeoSelDto> },
}

/// The file mirror of `operation::GeoSel`: a named entity and one of its
/// feature ids. Ids are geometry hashes (stable ids), not positions, so a
/// saved pick survives save/load and facet reorder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GeoSelDto {
    pub entity: String,
    pub feature: FeatureSelDto,
}

/// The file mirror of `operation::FeatureSel`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureSelDto {
    Face { id: u64 },
    Chain { id: u64 },
    Patch { id: u64 },
}

/// The file mirror of `operation::OpParams`, tool-free by the same rule as
/// the domain type: the diameter lives in the tool, never a parameter.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParamsDto {
    Face {
        stepover: f64,
        clearance_z: HeightDto,
        cut_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Declared at v5, wired in step 4: absent (or the defaults) is
        /// exactly the legacy single-level face.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_z: Option<HeightDto>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepdown: Option<f64>,
    },
    Contour {
        stock_to_leave: f64,
        clearance_z: HeightDto,
        retract_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead: LeadDto,
        tolerance: f64,
        tab_count: u32,
        tab_width: f64,
        tab_height: f64,
        /// Declared at v7, wired with the side feature: absent is exactly
        /// `auto`, the choice every pre-v7 contour file silently had -
        /// Auto resolves at generate, so the stored path never changes.
        #[serde(default)]
        side: SideDto,
    },
    Pocket {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        /// Declared at v5, wired in step 5: absent is exactly "hop to
        /// clearance", the behavior every pre-v5 pocket had.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
    },
    Slot {
        a: [f64; 2],
        b: [f64; 2],
        width: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stepover: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
    },
    Trace {
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
    },
    /// Declared at v10 with the drilling operation. The hole list is
    /// params data, not a live query: what recognition (or a typed edit)
    /// produced is what the file holds, and the source digest makes a
    /// moved model mark the op Stale beside it.
    Drill {
        clearance_z: HeightDto,
        /// Absent means the hop-to-clearance every drilling default
        /// uses, like the pocket family's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        /// The whole-pattern depth override; absent is each hole's own
        /// floor - the normal shape, so `skip_serializing_if` keeps the
        /// common file lean.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom_z: Option<HeightDto>,
        cycle: CycleChoiceDto,
        peck: f64,
        dwell: f64,
        feed: f64,
        plunge_feed: f64,
        holes: Vec<HoleDto>,
    },
    /// Declared at v11 with the 3-D pocket. No footprint on the wire:
    /// a 3-D op's machine area is the model's silhouette at generation
    /// time (the level law reads the mesh, not picks), and the source
    /// digest is what marks a moved model Stale beside it. The rename
    /// states the tag - the automatic snake_case of `Pocket3D` would be
    /// `pocket3_d`, and a file contract deserves the cleaner word.
    #[serde(rename = "pocket3d")]
    Pocket3D {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; a stored number is the
        /// user's choice, never a stale derivation from the tool.
        helix_radius: f64,
        /// Absent means off: the v11 files predate the rest pass, and
        /// the plain level ladder is exactly what they generated.
        #[serde(default)]
        rest_rough: bool,
        /// Max engagement angle, degrees; read only when `strategy` is
        /// `Adaptive`. Absent means the law's 45° - v12 files can carry
        /// no Adaptive ring family to feed it.
        #[serde(default = "default_engagement_deg")]
        max_engagement: f64,
        /// Absent means off: v13 files posted every F word at the
        /// programmed feed, which is exactly what "no modulation"
        /// means for them.
        #[serde(default)]
        feed_opt: bool,
    },
    /// Declared at v15 with the waterline. The rename states the tag
    /// for the same reason `pocket3d` does - the automatic snake_case
    /// of `Finish3D` would be `finish3_d`, and a file contract
    /// deserves the cleaner word. No stepdown key: the scallop law
    /// derives it from tolerance and tool, so storing one could only
    /// disagree with the two numbers that author it. v16 added the
    /// pattern (absent means `waterline`, what every v15 file ran)
    /// and the raster stepover - the one number the law does not
    /// author, because a stored override states a machinist's
    /// decision, not the tolerance's derivation.
    #[serde(rename = "finish3d")]
    Finish3D {
        /// Absent means Waterline (the v15 truth).
        #[serde(default)]
        pattern: FinishPatternDto,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        /// Absent means no ramping - the hop, what every pre-v17 file
        /// ran (M2.2e). Written always, like `climb`.
        #[serde(default)]
        ramp: bool,
        /// Absent means the march's own floor (delta0/4096), what
        /// every pre-v18 file ran (M2.2e surface finish). Absent
        /// spells None exactly like `stepover`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_step: Option<f64>,
        /// Absent means 45 degrees - the slope limit the raster's
        /// shallow jurisdiction already grants, hence exactly what
        /// v17's chain certified (M2.2e surface finish). Written
        /// always, like `ramp`.
        #[serde(default = "default_ramp_angle")]
        ramp_angle: f64,
    },
}

/// The angle an absent `ramp_angle` means: 45°, the ground the
/// raster's own law admits - every certifyable v17 chain already
/// descended no faster.
#[must_use]
pub fn default_ramp_angle() -> f64 {
    45.0
}

/// The file mirror of `finishing3d::FinishPattern`; `default` is
/// Waterline, the only pattern a v15 file can have run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishPatternDto {
    #[default]
    Waterline,
    Parallel,
    ParallelMulti,
    /// M2.2c: a new word in an existing grammar - v16 files never
    /// carried it and still read unchanged, so no version step-up.
    Scallop,
    /// M2.2d, same law: chords through the footprint center.
    Radial,
    /// M2.2d, same law: one Archimedean curve in from the rim.
    Spiral,
    /// M2.2e, same law: concentric shadows of the terrain, lifted.
    Contour,
    /// M2.2e, same law: the silhouette lifted onto the surface.
    Project,
}

impl From<FinishPatternDto> for crate::finishing3d::FinishPattern {
    fn from(v: FinishPatternDto) -> Self {
        match v {
            FinishPatternDto::Waterline => crate::finishing3d::FinishPattern::Waterline,
            FinishPatternDto::Parallel => crate::finishing3d::FinishPattern::Parallel,
            FinishPatternDto::ParallelMulti => crate::finishing3d::FinishPattern::ParallelMulti,
            FinishPatternDto::Scallop => crate::finishing3d::FinishPattern::Scallop,
            FinishPatternDto::Radial => crate::finishing3d::FinishPattern::Radial,
            FinishPatternDto::Spiral => crate::finishing3d::FinishPattern::Spiral,
            FinishPatternDto::Contour => crate::finishing3d::FinishPattern::Contour,
            FinishPatternDto::Project => crate::finishing3d::FinishPattern::Project,
        }
    }
}

impl From<crate::finishing3d::FinishPattern> for FinishPatternDto {
    fn from(v: crate::finishing3d::FinishPattern) -> Self {
        match v {
            crate::finishing3d::FinishPattern::Waterline => FinishPatternDto::Waterline,
            crate::finishing3d::FinishPattern::Parallel => FinishPatternDto::Parallel,
            crate::finishing3d::FinishPattern::ParallelMulti => FinishPatternDto::ParallelMulti,
            crate::finishing3d::FinishPattern::Scallop => FinishPatternDto::Scallop,
            crate::finishing3d::FinishPattern::Radial => FinishPatternDto::Radial,
            crate::finishing3d::FinishPattern::Spiral => FinishPatternDto::Spiral,
            crate::finishing3d::FinishPattern::Contour => FinishPatternDto::Contour,
            crate::finishing3d::FinishPattern::Project => FinishPatternDto::Project,
        }
    }
}

fn default_engagement_deg() -> f64 {
    45.0
}

/// The file mirror of `machining3d::Entry3D`; `default` is Plunge, the
/// engagement every pre-3-D op had and the one no legality gate can
/// refuse.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryDto {
    #[default]
    Plunge,
    Ramp,
    Helix,
}

/// One hole of a drill op's list, on the wire: world XY, the model's
/// hole radius, the entry surface and floor it was recognized between,
/// and `through` - stored, because it selects the exit allowance at
/// generation and a live re-query of the mesh would let a model edit
/// silently redefine a hole nobody re-detected.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct HoleDto {
    pub xy: [f64; 2],
    pub radius: f64,
    pub entry_z: f64,
    pub floor_z: f64,
    pub through: bool,
}

impl HoleDto {
    fn from_hole(h: &Hole) -> Self {
        HoleDto {
            // The lattice snap is idempotent, and recognition's output is
            // already snapped; this only levels a typed value onto the
            // file's fixed-point law (see MoveDto).
            xy: [lattice(h.xy.x), lattice(h.xy.y)],
            radius: lattice(h.radius),
            entry_z: lattice(h.entry_z),
            floor_z: lattice(h.floor_z),
            through: h.through,
        }
    }

    fn to_hole(self) -> Hole {
        Hole {
            xy: Point2::new(self.xy[0], self.xy[1]),
            radius: self.radius,
            entry_z: self.entry_z,
            floor_z: self.floor_z,
            through: self.through,
        }
    }
}

/// Which canned cycle a drill op runs; the file mirror of
/// `drilling::CycleChoice`. `Auto` resolves at generate time (tool kind
/// first, then the depth-vs-diameter law), so the file stores only the
/// user's ask, like `StrategyDto::Auto` and `SideDto::Auto`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CycleChoiceDto {
    #[default]
    Auto,
    Spot,
    Drill,
    Peck,
    Tap,
    Bore,
    BackBore,
}

/// The reference part of a stored height. Serialized as a plain string;
/// `work_offset` carries its slot number.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeightRefDto {
    Fixed,
    TopOfStock,
    BottomOfStock,
    TopOfModel,
    BottomOfModel,
    WorkOffset { slot: usize },
}

/// One stored height: `{"reference": ..., "offset": z}`. A bare number
/// still deserializes (`HeightWire`) as a `Fixed` expression carrying
/// exactly that number - the exact v5 value, so every pre-v6 file reads
/// unchanged with its saved z preserved to the bit. Writing always
/// produces the expression form.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct HeightDto {
    pub reference: HeightRefDto,
    pub offset: f64,
}

impl<'de> Deserialize<'de> for HeightDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        HeightWire::deserialize(deserializer).map(HeightDto::from)
    }
}

/// The two shapes a height takes on the wire. `Expr`'s fields are spelled
/// inline rather than nesting `HeightDto`, whose own `Deserialize` routes
/// back through here - the indirection must stay one-way, or parsing an
/// expression form recurses until the stack is gone.
#[derive(Deserialize)]
#[serde(untagged)]
enum HeightWire {
    Legacy(f64),
    Expr {
        reference: HeightRefDto,
        #[serde(default)]
        offset: f64,
    },
}

impl From<HeightWire> for HeightDto {
    fn from(v: HeightWire) -> Self {
        match v {
            HeightWire::Legacy(z) => HeightDto { reference: HeightRefDto::Fixed, offset: z },
            HeightWire::Expr { reference, offset } => HeightDto { reference, offset },
        }
    }
}

impl From<crate::operation::Height> for HeightDto {
    fn from(h: crate::operation::Height) -> Self {
        HeightDto { reference: h.reference.into(), offset: h.offset }
    }
}

impl From<HeightDto> for crate::operation::Height {
    fn from(d: HeightDto) -> Self {
        crate::operation::Height { reference: d.reference.into(), offset: d.offset }
    }
}

impl From<crate::operation::HeightReference> for HeightRefDto {
    fn from(r: crate::operation::HeightReference) -> Self {
        use crate::operation::HeightReference as R;
        match r {
            R::Fixed => HeightRefDto::Fixed,
            R::TopOfStock => HeightRefDto::TopOfStock,
            R::BottomOfStock => HeightRefDto::BottomOfStock,
            R::TopOfModel => HeightRefDto::TopOfModel,
            R::BottomOfModel => HeightRefDto::BottomOfModel,
            R::WorkOffset(slot) => HeightRefDto::WorkOffset { slot },
        }
    }
}

impl From<HeightRefDto> for crate::operation::HeightReference {
    fn from(d: HeightRefDto) -> Self {
        use crate::operation::HeightReference as R;
        match d {
            HeightRefDto::Fixed => R::Fixed,
            HeightRefDto::TopOfStock => R::TopOfStock,
            HeightRefDto::BottomOfStock => R::BottomOfStock,
            HeightRefDto::TopOfModel => R::TopOfModel,
            HeightRefDto::BottomOfModel => R::BottomOfModel,
            HeightRefDto::WorkOffset { slot } => R::WorkOffset(slot),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LeadDto {
    None,
    Linear { length: f64 },
    Arc { radius: f64 },
    /// Added at v9 with the tangential lead: older files carry no key
    /// change at all (`lead` has been required since v1), only this new
    /// tag can appear in newer ones.
    Tangential { radius: f64 },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyDto {
    Zigzag,
    Waterline,
    /// Added at v5 with the auto strategy: the choice itself is not
    /// stored - resolution runs at generate time - only that the user
    /// asked for it.
    Auto,
    /// Added at v13 with the constant-engagement ring strategy (the 3-D
    /// pocket's `Adaptive`; in the 2-D pocket it is the documented
    /// waterline alias - the geometry family has no engagement field to
    /// feed it).
    Adaptive,
}

/// Which side of its geometry a contour runs, on the wire. Added at v7
/// with the contour side choice; like `StrategyDto::Auto` the stored
/// value is the user's ask, not the resolution - an absent key reads as
/// `auto` and the selection layer decides at generate time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SideDto {
    /// Resolved by the selection layer: a lone ring bounding air
    /// machines inside, everything else outside.
    #[default]
    Auto,
    Inside,
    Outside,
}

impl ParamsDto {
    fn from_params(p: &OpParams) -> Self {
        match p {
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
                ParamsDto::Face {
                    stepover: *stepover,
                    clearance_z: (*clearance_z).into(),
                    cut_z: (*cut_z).into(),
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    top_z: (*top_z).map(Into::into),
                    stepdown: *stepdown,
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
                climb,
                lead,
                tolerance,
                tab_count,
                tab_width,
                tab_height,
                side,
            } => ParamsDto::Contour {
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).into(),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                lead: match lead {
                    Lead::None => LeadDto::None,
                    Lead::Linear { length } => LeadDto::Linear { length: *length },
                    Lead::Arc { radius } => LeadDto::Arc { radius: *radius },
                    Lead::Tangential { radius } => LeadDto::Tangential { radius: *radius },
                },
                tolerance: *tolerance,
                tab_count: *tab_count,
                tab_width: *tab_width,
                tab_height: *tab_height,
                side: match side {
                    Side::Auto => SideDto::Auto,
                    Side::Inside => SideDto::Inside,
                    Side::Outside => SideDto::Outside,
                },
            },
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
            } => ParamsDto::Pocket {
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: match strategy {
                    PocketStrategy::Zigzag => StrategyDto::Zigzag,
                    PocketStrategy::Waterline => StrategyDto::Waterline,
                    PocketStrategy::Auto => StrategyDto::Auto,
                    PocketStrategy::Adaptive => StrategyDto::Adaptive,
                },
                // Absent means the hop-to-clearance every pre-step-5
                // pocket performed; a set plane round-trips exactly.
                retract_z: (*retract_z).map(Into::into),
            },
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
            } => ParamsDto::Pocket3D {
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: match strategy {
                    PocketStrategy::Zigzag => StrategyDto::Zigzag,
                    PocketStrategy::Waterline => StrategyDto::Waterline,
                    PocketStrategy::Auto => StrategyDto::Auto,
                    PocketStrategy::Adaptive => StrategyDto::Adaptive,
                },
                retract_z: (*retract_z).map(Into::into),
                entry: match entry {
                    Entry3D::Plunge => EntryDto::Plunge,
                    Entry3D::Ramp => EntryDto::Ramp,
                    Entry3D::Helix => EntryDto::Helix,
                },
                helix_radius: *helix_radius,
                rest_rough: *rest_rough,
                max_engagement: *max_engagement,
                feed_opt: *feed_opt,
            },
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
            } => ParamsDto::Finish3D {
                pattern: (*pattern).into(),
                stepover: *stepover,
                tolerance: *tolerance,
                clearance_z: (*clearance_z).into(),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                retract_z: (*retract_z).map(Into::into),
                ramp: *ramp,
                min_step: *min_step,
                ramp_angle: *ramp_angle,
            },
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
            } => ParamsDto::Slot {
                a: [a.x, a.y],
                b: [b.x, b.y],
                width: *width,
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                stepover: *stepover,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            },
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
            } => ParamsDto::Trace {
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            },
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
            } => ParamsDto::Drill {
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).map(Into::into),
                cycle: match cycle {
                    CycleChoice::Auto => CycleChoiceDto::Auto,
                    CycleChoice::Spot => CycleChoiceDto::Spot,
                    CycleChoice::Drill => CycleChoiceDto::Drill,
                    CycleChoice::Peck => CycleChoiceDto::Peck,
                    CycleChoice::Tap => CycleChoiceDto::Tap,
                    CycleChoice::Bore => CycleChoiceDto::Bore,
                    CycleChoice::BackBore => CycleChoiceDto::BackBore,
                },
                peck: *peck,
                dwell: *dwell,
                feed: *feed,
                plunge_feed: *plunge_feed,
                holes: holes.iter().map(HoleDto::from_hole).collect(),
            },
        }
    }

    fn to_params(&self) -> OpParams {
        match self {
            // Absent v5 keys decode (via serde defaults) to the legacy
            // single-level face, byte-exact; present ones land on the
            // domain as staged facing.
            ParamsDto::Face { stepover, clearance_z, cut_z, top_z, stepdown, feed, plunge_feed, climb } => {
                OpParams::Face {
                    stepover: *stepover,
                    clearance_z: (*clearance_z).into(),
                    cut_z: (*cut_z).into(),
                    top_z: (*top_z).map(Into::into),
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                }
            }
            ParamsDto::Contour {
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                lead,
                tolerance,
                tab_count,
                tab_width,
                tab_height,
                side,
            } => OpParams::Contour {
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).into(),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                lead: match lead {
                    LeadDto::None => Lead::None,
                    LeadDto::Linear { length } => Lead::Linear { length: *length },
                    LeadDto::Arc { radius } => Lead::Arc { radius: *radius },
                    LeadDto::Tangential { radius } => Lead::Tangential { radius: *radius },
                },
                tolerance: *tolerance,
                tab_count: *tab_count,
                tab_width: *tab_width,
                tab_height: *tab_height,
                side: match side {
                    SideDto::Auto => Side::Auto,
                    SideDto::Inside => Side::Inside,
                    SideDto::Outside => Side::Outside,
                },
            },
            ParamsDto::Pocket {
                stepover,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                strategy,
                retract_z,
            } => OpParams::Pocket {
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: match strategy {
                    StrategyDto::Zigzag => PocketStrategy::Zigzag,
                    StrategyDto::Waterline => PocketStrategy::Waterline,
                    StrategyDto::Auto => PocketStrategy::Auto,
                    StrategyDto::Adaptive => PocketStrategy::Adaptive,
                },
            },
            ParamsDto::Pocket3D {
                stepover,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                strategy,
                retract_z,
                entry,
                helix_radius,
                rest_rough,
                max_engagement,
                feed_opt,
            } => OpParams::Pocket3D {
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: match strategy {
                    StrategyDto::Zigzag => PocketStrategy::Zigzag,
                    StrategyDto::Waterline => PocketStrategy::Waterline,
                    StrategyDto::Auto => PocketStrategy::Auto,
                    StrategyDto::Adaptive => PocketStrategy::Adaptive,
                },
                entry: match entry {
                    EntryDto::Plunge => Entry3D::Plunge,
                    EntryDto::Ramp => Entry3D::Ramp,
                    EntryDto::Helix => Entry3D::Helix,
                },
                helix_radius: *helix_radius,
                rest_rough: *rest_rough,
                max_engagement: *max_engagement,
                feed_opt: *feed_opt,
            },
            ParamsDto::Finish3D {
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
            } => OpParams::Finish3D {
                pattern: (*pattern).into(),
                stepover: *stepover,
                tolerance: *tolerance,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: *ramp,
                min_step: *min_step,
                ramp_angle: *ramp_angle,
            },
            ParamsDto::Slot {
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
            } => OpParams::Slot {
                a: Point2::new(a[0], a[1]),
                b: Point2::new(b[0], b[1]),
                width: *width,
                stock_to_leave: *stock_to_leave,
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                stepover: *stepover,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            },
            ParamsDto::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
            } => OpParams::Trace {
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).into(),
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            },
            ParamsDto::Drill {
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
            } => OpParams::Drill {
                clearance_z: (*clearance_z).into(),
                retract_z: (*retract_z).map(Into::into),
                top_z: (*top_z).into(),
                bottom_z: (*bottom_z).map(Into::into),
                cycle: match cycle {
                    CycleChoiceDto::Auto => CycleChoice::Auto,
                    CycleChoiceDto::Spot => CycleChoice::Spot,
                    CycleChoiceDto::Drill => CycleChoice::Drill,
                    CycleChoiceDto::Peck => CycleChoice::Peck,
                    CycleChoiceDto::Tap => CycleChoice::Tap,
                    CycleChoiceDto::Bore => CycleChoice::Bore,
                    CycleChoiceDto::BackBore => CycleChoice::BackBore,
                },
                peck: *peck,
                dwell: *dwell,
                feed: *feed,
                plunge_feed: *plunge_feed,
                holes: holes.iter().copied().map(HoleDto::to_hole).collect(),
            },
        }
    }
}

/// One CL move: kind, endpoints, contact feed (absent for rapids).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MoveDto {
    pub kind: MoveKindDto,
    pub from: [f64; 3],
    pub to: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<f64>,
    /// Added at v5: the circular segment this move traverses, absent for
    /// the straight-line moves every pre-v5 path held (and every path
    /// with no arcs still writes - the field skips serialization).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<ArcDto>,
}

/// An arc move's center, in the XY plane, with direction. `kind` stays
/// the motion class (rapid/feed/plunge); the arc is geometry.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ArcDto {
    pub center: [f64; 2],
    pub ccw: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MoveKindDto {
    Rapid,
    Feed,
    Plunge,
}

/// The cached generation output: path plus the warnings that came with it,
/// plus - from v10 - the canned-cycle list that rides alongside the moves
/// of a drilling path (see `crate::cl::Cycle`). Absent from any pre-v10
/// cache, which is exactly right: no pre-v10 build ever generated a cycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PathDto {
    pub moves: Vec<MoveDto>,
    pub warnings: Vec<Warning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<CycleDto>,
}

/// One cached cycle: the post's source of truth for a hole, stored beside
/// (not instead of) the moves it expands to. Floats take the same lattice
/// snap `MoveDto` gives its coordinates, so a cycle round-trips
/// byte-identically and stays bit-exact with its moves' depths - what the
/// agreement law pins.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CycleDto {
    pub kind: CycleKindDto,
    pub xy: [f64; 2],
    pub r_z: f64,
    pub bottom_z: f64,
    pub entry_z: f64,
    pub peck: f64,
    pub dwell: f64,
    pub first_move: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CycleKindDto {
    Spot,
    Drill,
    Peck,
    Deep,
    Tap,
    Bore,
    BackBore,
}

impl CycleDto {
    fn from_cycle(c: &Cycle) -> Self {
        CycleDto {
            kind: match c.kind {
                CycleKind::Spot => CycleKindDto::Spot,
                CycleKind::Drill => CycleKindDto::Drill,
                CycleKind::Peck => CycleKindDto::Peck,
                CycleKind::Deep => CycleKindDto::Deep,
                CycleKind::Tap => CycleKindDto::Tap,
                CycleKind::Bore => CycleKindDto::Bore,
                CycleKind::BackBore => CycleKindDto::BackBore,
            },
            xy: [lattice(c.xy.x), lattice(c.xy.y)],
            r_z: lattice(c.r_z),
            bottom_z: lattice(c.bottom_z),
            entry_z: lattice(c.entry_z),
            peck: lattice(c.peck),
            dwell: lattice(c.dwell),
            first_move: c.first_move as u64,
        }
    }

    fn to_cycle(self) -> Cycle {
        Cycle {
            kind: match self.kind {
                CycleKindDto::Spot => CycleKind::Spot,
                CycleKindDto::Drill => CycleKind::Drill,
                CycleKindDto::Peck => CycleKind::Peck,
                CycleKindDto::Deep => CycleKind::Deep,
                CycleKindDto::Tap => CycleKind::Tap,
                CycleKindDto::Bore => CycleKind::Bore,
                CycleKindDto::BackBore => CycleKind::BackBore,
            },
            xy: Point2::new(self.xy[0], self.xy[1]),
            r_z: self.r_z,
            bottom_z: self.bottom_z,
            entry_z: self.entry_z,
            peck: self.peck,
            dwell: self.dwell,
            first_move: usize::try_from(self.first_move).unwrap_or(usize::MAX),
        }
    }
}

/// A setup as files hold it (M3.2 A): the furniture it owns, by name.
/// `wcs` is indexed by [`WCS_SLOTS`]; an entry may name an entity the
/// file does not contain - that slot is simply not placed in this setup,
/// exactly what a missing `G55` entity has always meant. `datum_from` is
/// clone provenance for the setup sheet (stage D); every stamp ignores
/// it, so it defaults and rides along for the reader, not the machinery.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct SetupDto {
    pub stock: String,
    #[serde(default)]
    pub wcs: Vec<String>,
    #[serde(default)]
    pub datum_from: String,
}

/// The fixture marker as files hold it (M3.2 stage B): a tag on a model
/// entity - the solid itself is that model's own `facets`, its placement
/// the entity's `translation`/`rotation`, and the machine's table is
/// never stored here (it derives from the `machine` key). `setup` is the
/// DECLARED binding, same grammar as `OperationDto::setup`: absent
/// means the first setup by name (the implicit-setup law), so a tagged
/// model in a setup-free document saves as one key and nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FixtureDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
}

/// An operation as files hold it: definition, the stamp the last generate
/// saw (raw digest; its meaning lives in `operation::current_stamp`), and
/// the cache that stamp was taken for. `stamp` present without a cache (or
/// the reverse, e.g. after an edit clears the stamp but keeps the old path
/// on screen) is legitimate and reads Stale.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OperationDto {
    pub tool: String,
    pub src: SrcDto,
    pub params: ParamsDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PathDto>,
    /// The setup this operation machines in (M3.2 A). Absent means the
    /// first setup by name - exactly what every pre-v21 operation was
    /// (the implicit-setup law), so a v21 save of a setup-free document
    /// is byte-identical to its v20 save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
}

impl MoveDto {
    /// Coordinates stand on the 1 um lattice ([`crate::cl::lattice`]):
    /// every float a file carries is then a fixed point of write-parse,
    /// so save-load-save is byte-identical - an off-lattice double's
    /// shortest decimal can parse back one ulp over from what was
    /// written, and a cache that drifts a ulp per reopen is a file
    /// that never repeats itself. Generated coordinates are already
    /// lattice values (clipper's grid, user-typed feeds); the snap
    /// only ever moves anything computed - a lead tangent point - and
    /// does so by at most half a micron.
    fn from_move(m: &Move) -> Self {
        let lat = |p: Point3| [lattice(p.x), lattice(p.y), lattice(p.z)];
        MoveDto {
            kind: match m.kind {
                MoveKind::Rapid => MoveKindDto::Rapid,
                MoveKind::Feed => MoveKindDto::Feed,
                MoveKind::Plunge => MoveKindDto::Plunge,
            },
            from: lat(m.from),
            to: lat(m.to),
            feed: m.feed.map(lattice),
            arc: m.arc.map(|a| ArcDto {
                center: [lattice(a.center.x), lattice(a.center.y)],
                ccw: a.ccw,
            }),
        }
    }

    fn to_move(self) -> Move {
        let m = Move::new(
            match self.kind {
                MoveKindDto::Rapid => MoveKind::Rapid,
                MoveKindDto::Feed => MoveKind::Feed,
                MoveKindDto::Plunge => MoveKind::Plunge,
            },
            Point3::new(self.from[0], self.from[1], self.from[2]),
            Point3::new(self.to[0], self.to[1], self.to[2]),
            self.feed,
        );
        match self.arc {
            Some(a) => m.with_arc(Point2::new(a.center[0], a.center[1]), a.ccw),
            None => m,
        }
    }
}

impl PathDto {
    fn from_cache(t: &ToolpathCache) -> Self {
        PathDto {
            moves: t.path.moves.iter().map(MoveDto::from_move).collect(),
            warnings: t.warnings.clone(),
            cycles: t.path.cycles.iter().map(CycleDto::from_cycle).collect(),
        }
    }

    fn to_cache(&self) -> ToolpathCache {
        let mut path = Toolpath::new(self.moves.iter().map(|m| m.to_move()).collect());
        path.cycles = self.cycles.iter().copied().map(CycleDto::to_cycle).collect();
        ToolpathCache { path, warnings: self.warnings.clone() }
    }
}

#[derive(Serialize, Deserialize)]
pub struct EntityDto {
    pub name: String,
    pub translation: [f64; 3],
    /// Unit quaternion (x, y, z, w). Defaulted so v1 files parse; the v1 -> v2
    /// migration below makes that choice explicit rather than implicit.
    #[serde(default = "identity_rotation")]
    pub rotation: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<[[f64; 3]; 3]>>, // a, b, c per triangle
    /// Geometry-revision counter (M3.1, `schema::Revision`). Absent means 0,
    /// which is what every pre-v20 file held; a model always carries >= 1
    /// (import stamps the first landing), so this key never appears on
    /// setup furniture and a v20 save of a pre-M3.1 document is byte-shape
    /// identical to its v19 save.
    #[serde(default, skip_serializing_if = "is_unrevised")]
    pub rev: u64,
    /// Absent means Model; only non-model entities carry the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntityKind>,
    /// Stock parameters; present exactly when `kind == Some(Stock)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<StockDto>,
    /// Tool definition; present exactly when `kind == Some(Tool)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolDto>,
    /// Operation definition (+ cached path); present exactly when
    /// `kind == Some(Operation)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationDto>,
    /// Setup definition; present exactly when `kind == Some(Setup)`
    /// (M3.2 A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<SetupDto>,
    /// Fixture marker (M3.2 B); present exactly on an UNTAGGED-kind
    /// (Model) entity - a fixture is a model that collision checks
    /// respect, so it borrows the model row's geometry, never a kind
    /// of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture: Option<FixtureDto>,
}

fn identity_rotation() -> [f64; 4] {
    IDENTITY_Q
}

fn is_unrevised(rev: &u64) -> bool {
    *rev == 0
}

fn corrupt(msg: impl Into<String>) -> ProjectError {
    ProjectError::Corrupt(msg.into())
}

fn facets_to_dto(tris: Vec<Triangle>) -> Vec<[[f64; 3]; 3]> {
    tris.into_iter()
        .map(|t| {
            [
                [t.a.x, t.a.y, t.a.z],
                [t.b.x, t.b.y, t.b.z],
                [t.c.x, t.c.y, t.c.z],
            ]
        })
        .collect()
}

fn dto_to_facets(v: Vec<[[f64; 3]; 3]>) -> Vec<Triangle> {
    v.into_iter()
        .map(|t| {
            Triangle::new(
                Point3::new(t[0][0], t[0][1], t[0][2]),
                Point3::new(t[1][0], t[1][1], t[1][2]),
                Point3::new(t[2][0], t[2][1], t[2][2]),
            )
        })
        .collect()
}

pub fn collect(world: &World) -> ProjectFile {
    let entities = model::named_entities(world)
        .into_iter()
        .map(|(name, entity)| {
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            let kind = if model::is_stock(world, &entity) {
                Some(EntityKind::Stock)
            } else if model::is_wcs(world, &entity) {
                Some(EntityKind::Wcs)
            } else if model::is_tool(world, &entity) {
                Some(EntityKind::Tool)
            } else if model::is_operation(world, &entity) {
                Some(EntityKind::Operation)
            } else if model::is_setup(world, &entity) {
                Some(EntityKind::Setup)
            } else {
                None
            };
            let stock = model::stock_def_of(world, &entity).map(|s| StockDto::from_shape(&s));
            let tool = model::tool_of(world, &entity).map(|t| ToolDto::from_tool(&t));
            // The setup payload only ever rides a setup row (the kind test
            // above and the component below are the same SetupDef).
            let setup = if matches!(kind, Some(EntityKind::Setup)) {
                entity
                    .entity_view(world)
                    .try_get::<&SetupDef>(|d| SetupDto {
                        stock: d.stock.clone(),
                        wcs: d.wcs.clone(),
                        datum_from: d.datum_from.clone(),
                    })
            } else {
                None
            };
            // The fixture marker rides whatever model it tags; the kind
            // test above deliberately does NOT look at it (a fixture is
            // a model, not a kind).
            let fixture = entity
                .entity_view(world)
                .try_get::<&FixtureDef>(|d| FixtureDto {
                    setup: d.setup.clone(),
                });
            let operation = model::operation_of(world, &entity).map(|op| OperationDto {
                tool: op.tool,
                src: match &op.src {
                    SrcRef::Stock => SrcDto::Stock,
                    SrcRef::Entity(name) => SrcDto::Entity {
                        name: name.clone(),
                    },
                    SrcRef::Selection(items) => SrcDto::Selection {
                        items: items
                            .iter()
                            .map(|g| GeoSelDto {
                                entity: g.entity.clone(),
                                feature: match g.feature {
                                    FeatureSel::Face(id) => FeatureSelDto::Face { id },
                                    FeatureSel::Chain(id) => FeatureSelDto::Chain { id },
                                    FeatureSel::Patch(id) => FeatureSelDto::Patch { id },
                                },
                            })
                            .collect(),
                    },
                },
                params: ParamsDto::from_params(&op.params),
                stamp: op.stamp.map(|g| g.0),
                cache: model::toolpath_of(world, &entity).map(|c| PathDto::from_cache(&c)),
                setup: op.setup,
            });
            // Parametric stock regenerates its facets on load; offsets, tools,
            // operations and setups never have geometry, so none writes a
            // facet array.
            let facets = match &kind {
                Some(EntityKind::Wcs)
                | Some(EntityKind::Tool)
                | Some(EntityKind::Operation)
                | Some(EntityKind::Setup) => None,
                Some(EntityKind::Stock)
                    if matches!(
                        stock,
                        Some(StockDto::Box { .. }) | Some(StockDto::Cylinder { .. })
                    ) =>
                {
                    None
                }
                _ => model::facets_of(world, &entity).map(facets_to_dto),
            };
            EntityDto {
                name,
                translation: [loc.pos.x, loc.pos.y, loc.pos.z],
                rotation: loc.rot,
                facets,
                // The counter is only ever set on models (import/update
                // commands); everywhere else it reads 0 and omits itself.
                rev: model::rev_of(world, &entity),
                kind,
                stock,
                tool,
                operation,
                setup,
                fixture,
            }
        })
        .collect();
    ProjectFile {
        version: PROJECT_VERSION,
        // mm omits the field (the default); only an inch document pays
        // for saying so.
        units: match units::of(world) {
            Unit::Millimeters => None,
            Unit::Inches => Some("in".into()),
        },
        // The mute singleton (no path, no machine) omits the field -
        // the mm/units law again: only a loaded machine pays for
        // saying so, and an absent key is the mute state verbatim.
        machine: {
            let (path, m) = crate::machine::of(world);
            m.map(|machine| MachineDto { path, machine })
        },
        // The raw singleton, not the resolved one: a document where the
        // user never chose a setup writes no key, exactly like the mm
        // units and mute machine laws - and a name left dangling by a
        // deleted setup saves as it reads (the fallback is the law, not
        // a defect to hide).
        active_setup: crate::setup::stored_active(world),
        // The default post matches every pre-v23 document, so it
        // writes no key (the mute-machine law, worn in the post's clothes).
        post: match crate::posting::post_of(world) {
            crate::schema::PostRef::Builtin => None,
            crate::schema::PostRef::Named(name) => Some(PostDto::Named(name)),
            crate::schema::PostRef::File(path) => Some(PostDto::File(path)),
        },
        entities,
    }
}

pub fn save(world: &World, path: &Path) -> Result<(), ProjectError> {
    let file = collect(world);
    let text = serde_json::to_string_pretty(&file).map_err(|e| corrupt(e.to_string()))?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Rebuild world content from a project file. Not a command: opening a
/// project replaces the document and clears the history (M1.2 wires that).
pub fn load(world: &World, path: &Path) -> Result<(), ProjectError> {
    let text = std::fs::read_to_string(path)?;
    open(world, &text)
}

pub fn open(world: &World, text: &str) -> Result<(), ProjectError> {
    let file: ProjectFile = serde_json::from_str(text).map_err(|e| corrupt(e.to_string()))?;
    check_and_apply(world, file)
}

fn check_and_apply(world: &World, mut file: ProjectFile) -> Result<(), ProjectError> {
    // Migration hook: step UP one version at a time, each migration exact,
    // before any entity touches the world. Anything newer than this build
    // remains refused. The chain must run its steps sequentially - a v1
    // file passes through v2's effects before v3's are applied.
    if file.version == 1 {
        // v1 -> v2: entities gained `rotation`; v1 had translation-only
        // placements, so identity is the exact migration. Kind and stock
        // fields default (absent = model), so nothing else changes.
        for dto in &mut file.entities {
            dto.rotation = IDENTITY_Q;
        }
        file.version = 2;
    }
    if file.version == 2 {
        // v2 -> v3: `kind` gained tool/operation and their payload fields
        // appeared; every existing entity keeps its fields and defaults
        // the new ones (absent), which is exactly what a v2 file meant.
        file.version = 3;
    }
    if file.version == 3 {
        // v3 -> v4: feature picks graduate from "the src enum can carry
        // them" to "they persist". The shape is already right - a v3 file
        // with a selection reads verbatim, one without gains nothing -
        // so the step-up is the stamp itself.
        file.version = 4;
    }
    if file.version == 4 {
        // v4 -> v5: the M1.5.2 landing shape (patch picks, arc moves,
        // face stepdowns, pocket retract) is declared in the DTOs with
        // serde defaults that mean exactly the v4 behavior, so a v4
        // file needs no content change to live in v5 - the step-up is
        // the stamp itself, like v3 -> v4.
        file.version = 5;
    }
    if file.version == 5 {
        // v5 -> v6: heights became From+Offset expressions, and the
        // wire (`HeightWire`) already read a bare number as the exact
        // `Fixed` expression - every v5 height keeps its z to the bit.
        // Like v4 -> v5, the step-up is the stamp itself.
        file.version = 6;
    }
    if file.version == 6 {
        // v6 -> v7: contour gained the side choice, and the wire already
        // reads an absent `side` as `auto` - which is exactly what a v6
        // contour meant, since Auto resolves at generate time and the
        // stored path is unchanged. Step-up is the stamp itself.
        file.version = 7;
    }
    if file.version == 7 {
        // v7 -> v8: document units arrive as a top-level `units` field.
        // Every v7 file displayed millimeters (its numbers always were
        // millimeters), and the field's default - absent means mm - is
        // exactly that. Step-up is the stamp itself, like v6 -> v7.
        file.version = 8;
    }
    if file.version == 8 {
        // v8 -> v9: the Lead vocabulary grows a tangential member, and
        // `lead` has been a required key since v1 - no v8 file can
        // contain the new tag, and no v8 file needs a content change to
        // live in v9. Step-up is the stamp itself, like v7 -> v8.
        file.version = 9;
    }
    if file.version == 9 {
        // v9 -> v10: drilling arrives - the `drill` params variant, the
        // `drill`/`tap`/`bore` tool-kind tags, `tip_angle`/`pitch`, and
        // cached `cycles`. A v9 file can contain none of them (the tags
        // and keys did not exist), and the serde defaults for the two
        // optional-shaped additions (`cycles` -> empty, tool floats ->
        // the inert 0.0) are exactly v9's behavior. No content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 10;
    }
    if file.version == 10 {
        // v10 -> v11: the 3-D pocket arrives as one new params variant
        // (`pocket3d`), which no v10 file can contain - the tag did not
        // exist. Nothing existing changed shape, so no content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 11;
    }
    if file.version == 11 {
        // v11 -> v12: the 3-D pocket's rest pass arrives as one new
        // params key (`rest_rough`), absent from every v11 file - the
        // serde default is false, which is exactly what v11 generated.
        // Nothing existing changed shape; step-up is the stamp itself,
        // like every bump since v6.
        file.version = 12;
    }
    if file.version == 12 {
        // v12 -> v13: the constant-engagement strategy arrives as one
        // new params key (`max_engagement`) and one new `strategy` tag
        // (`adaptive`), neither of which any v12 file can contain - the
        // serde default of 45° is exactly what a v12 file meant, since
        // its rings were stepover-spaced and the angle reads only under
        // Adaptive. No content change; step-up is the stamp itself, like
        // every bump since v6.
        file.version = 13;
    }
    if file.version == 13 {
        // v13 -> v14: the feed modulation arrives as one new params key
        // (`feed_opt`), absent from every v13 file - the serde default
        // is off, which is exactly what v13 generated (every F word at
        // the programmed feed). No content change; step-up is the stamp
        // itself, like every bump since v6.
        file.version = 14;
    }
    if file.version == 14 {
        // v14 -> v15: the 3-D finish arrives as one new params variant
        // (`finish3d`), which no v14 file can contain - the tag did not
        // exist. Nothing existing changed shape; step-up is the stamp
        // itself, like every bump since v6.
        file.version = 15;
    }
    if file.version == 15 {
        // v15 -> v16: the parallel finish arrives as two new keys
        // inside `finish3d` (`pattern`, `stepover`), absent from every
        // v15 file - the serde defaults (`waterline`, derived
        // stepover) are exactly what v15 ran. No content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 16;
    }
    if file.version == 16 {
        // v16 -> v17: one more new key inside `finish3d` - `ramp`,
        // absent from every v16 file, whose serde default (false: the
        // hop) is exactly what v16 ran. Same pure step-up, same law.
        file.version = 17;
    }
    if file.version == 17 {
        // v17 -> v18: the surface-finish pair inside `finish3d` -
        // `min_step` (absent: None, the march's own floor) and
        // `ramp_angle` (absent: 45°, the slope limit the raster's
        // jurisdiction already granted). Both defaults are exactly
        // what v17 ran; no content change. Same pure step-up, same
        // law.
        file.version = 18;
    }
    if file.version == 18 {
        // v18 -> v19: the machine arrives as a top-level `machine` key.
        // No v18 file can contain it - the checker did not exist - and
        // the field's default, absent means no machine, is exactly what
        // every v18 document was: a program judged against nothing,
        // which is what the unstated-limit law already does with an
        // empty machine. Step-up is the stamp itself, like v7 -> v8.
        file.version = 19;
    }
    if file.version == 19 {
        // v19 -> v20: the model `rev` key (M3.1's revision counter). No
        // v19 file can contain it - the component did not exist - and the
        // field's default, absent means 0, is exactly what every v19
        // document was: geometry with no remembered landings. The
        // staleness machinery needs nothing migrated - it recomputes the
        // signature from the persisted facets. Step-up is the stamp
        // itself, like v7 -> v8 and v18 -> v19.
        file.version = 20;
    }
    if file.version == 20 {
        // v20 -> v21: setups arrive (M3.2 stage A) as a `setup` kind, a
        // `setup` name inside each operation, and a top-level
        // `active_setup` key. No v20 file can contain any of them - the
        // kind tag did not exist - and the defaults read as exactly the
        // v20 world: no setup rows and no setup names is the implicit-
        // setup law's legacy mode verbatim, and an absent active choice
        // resolves to the first setup by name (nothing). No content
        // change; step-up is the stamp itself, like every bump since v6.
        file.version = 21;
    }
    if file.version == 21 {
        // v21 -> v22: fixtures arrive (M3.2 stage B) as an optional
        // `fixture` marker on model entities - a tag, not a new kind,
        // because a fixture IS the model the user loaded. No v21 file
        // can contain it: the component did not exist, and its default
        // (no marker) is exactly the v21 collision world. Step-up is
        // the stamp itself, like every bump since v6.
        file.version = 22;
    }
    if file.version == 22 {
        // v22 -> v23: the post choice arrives as one optional top-level
        // `post` key (M3.3 stage A). No v22 file can contain it - the
        // component did not exist, and its default (absent, the
        // built-in) is exactly the v22 posting world: one embedded
        // script, no choices. Step-up is the stamp itself, like every
        // bump since v6.
        file.version = 23;
    }
    if file.version != PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(file.version));
    }
    // The document's unit first (it belongs to the whole file, not to an
    // entity): an unspellable value is a corrupt file, refused before any
    // world write. Opening always lands the file's value - including mm,
    // so a stale singleton from the previous document can never leak in.
    let doc_units = match file.units.as_deref() {
        None | Some("mm") => Unit::Millimeters,
        Some("in") => Unit::Inches,
        Some(other) => return Err(corrupt(format!("unknown units {other:?}"))),
    };
    // Whole-file validation BEFORE any world write: a corrupt entity deeper
    // in the list must not leave a half-loaded document behind.
    for dto in &file.entities {
        validate_kind(dto, dto.kind.unwrap_or(EntityKind::Model))?;
    }
    if let Some(m) = &file.machine
        && !m.machine.is_sane()
    {
        return Err(corrupt("machine definition states a limit that cannot be meant"));
    }
    if file.active_setup.as_deref().is_some_and(str::is_empty) {
        return Err(corrupt("the active setup names nothing - an empty setup name"));
    }
    units::set(world, doc_units);
    // Opening lands the machine too - including the mute state, so a
    // stale singleton from the previous document can never leak in
    // (the units law, restated for the checker's input).
    match file.machine.as_ref() {
        Some(m) => crate::machine::set(world, &m.path, Some(&m.machine)),
        None => crate::machine::set(world, "", None),
    }
    // Opening lands the active-setup choice too - including None, so a
    // stale singleton from the previous document can never leak in (the
    // units law, restated for setups). A name matching no setup row is
    // legal: `setup::active`'s existence fallback is the same law the
    // live world uses mid-history, so the file reads as it was saved.
    crate::setup::set_active(world, file.active_setup);
    // Opening lands the post choice too - including the default, so a
    // stale singleton from the previous document can never leak in (the
    // units law again). Paths and registry names are landed as written,
    // unvalidated: each names a script, not a limit, and the post-time
    // refusal is the net (`script_source` reads both, and both refuse
    // by name when gone).
    crate::posting::post_set(
        world,
        &match file.post {
            Some(PostDto::Named(name)) => crate::schema::PostRef::Named(name),
            Some(PostDto::File(path)) => crate::schema::PostRef::File(path),
            None => crate::schema::PostRef::Builtin,
        },
    );
    for dto in file.entities {
        let kind = dto.kind.unwrap_or(EntityKind::Model);
        let entity = model::spawn(world, &dto.name);
        model::set_transform(
            world,
            &entity,
            Location {
                pos: Point3::new(dto.translation[0], dto.translation[1], dto.translation[2]),
                rot: dto.rotation,
            },
        );
        match kind {
            EntityKind::Model => {
                if let Some(tris) = dto.facets {
                    model::set_facets(world, &entity, dto_to_facets(tris));
                }
                // 0 means the key was absent, i.e. a pre-v20 document: no
                // counter component is the world a v19 loader would have
                // built, and `rev_of` reads it as 0 anyway. A model first
                // updated after such a load lands rev 1 - the counter
                // counts landings this document remembers, not the file's
                // whole history. Staleness never reads it; the stamp's
                // signature does the work.
                if dto.rev > 0 {
                    model::set_rev(world, &entity, dto.rev);
                }
            }
            EntityKind::Stock => {
                // validate_kind already refused these; Result, never unwrap.
                let shape = dto
                    .stock
                    .ok_or_else(|| corrupt("stock entity without parameters"))?
                    .to_shape();
                let facets = match &shape {
                    StockShape::FromModel { .. } => {
                        dto_to_facets(dto.facets.ok_or_else(|| {
                            corrupt("from-model stock is missing its facets")
                        })?)
                    }
                    _ => stock::stock_mesh(&shape)
                        .filter(|f| !f.is_empty())
                        .ok_or_else(|| corrupt("stock mesh regeneration failed"))?,
                };
                model::set_facets(world, &entity, facets);
                entity
                    .entity_view(world)
                    .set(StockDef { shape: shape.clone() });
            }
            EntityKind::Wcs => {
                entity.entity_view(world).add(Wcs);
            }
            EntityKind::Tool => {
                // validate_kind already refused a missing or insane one.
                let tool = dto
                    .tool
                    .ok_or_else(|| corrupt("tool entity without a definition"))?
                    .to_tool();
                entity.entity_view(world).set(ToolDef { tool });
            }
            EntityKind::Operation => {
                let odto = dto
                    .operation
                    .ok_or_else(|| corrupt("operation entity without a definition"))?;
                let src = match &odto.src {
                    SrcDto::Stock => SrcRef::Stock,
                    SrcDto::Entity { name } => SrcRef::Entity(name.clone()),
                    SrcDto::Selection { items } => SrcRef::Selection(
                        items
                            .iter()
                            .map(|g| GeoSel {
                                entity: g.entity.clone(),
                                feature: match g.feature {
                                    FeatureSelDto::Face { id } => FeatureSel::Face(id),
                                    FeatureSelDto::Chain { id } => FeatureSel::Chain(id),
                                    FeatureSelDto::Patch { id } => FeatureSel::Patch(id),
                                },
                            })
                            .collect(),
                    ),
                };
                entity.entity_view(world).set(Operation {
                    tool: odto.tool,
                    src,
                    params: odto.params.to_params(),
                    stamp: odto.stamp.map(GenStamp),
                    setup: odto.setup,
                });
                if let Some(cache) = &odto.cache {
                    entity.entity_view(world).set(cache.to_cache());
                }
            }
            EntityKind::Setup => {
                // validate_kind already refused a missing one.
                let sdto = dto
                    .setup
                    .ok_or_else(|| corrupt("setup entity without a definition"))?;
                entity.entity_view(world).set(SetupDef {
                    stock: sdto.stock,
                    wcs: sdto.wcs,
                    datum_from: sdto.datum_from,
                });
            }
        }
        // v22: the fixture marker rides a finished model row (the kind
        // match above built the solid; validate_kind refused the tag on
        // anything else). Setting it after keeps the geometry-first
        // order of every other load.
        if let Some(f) = &dto.fixture {
            entity.entity_view(world).set(FixtureDef {
                setup: f.setup.clone(),
            });
        }
    }
    Ok(())
}

/// Structural checks that JSON typing cannot express. All run before this
/// entity reaches the world; a corrupt field never half-loads (Prime
/// Directive: validate early, fail cleanly).
fn validate_kind(dto: &EntityDto, kind: EntityKind) -> Result<(), ProjectError> {
    // v22, once for every kind: the fixture marker rides a MODEL row
    // and nothing else - stock, offsets, tools, operations and setups
    // wearing it would be a file describing a CAM concept that has no
    // world representation.
    if dto.fixture.is_some() && !matches!(kind, EntityKind::Model) {
        return Err(corrupt(format!(
            "'{}' wears a fixture tag but is not a model",
            dto.name
        )));
    }
    match kind {
        EntityKind::Model => {
            if dto.stock.is_some() || dto.tool.is_some() || dto.operation.is_some() || dto.setup.is_some()
            {
                return Err(corrupt(format!(
                    "'{}' is a model carrying setup or CAM parameters",
                    dto.name
                )));
            }
            if dto.fixture.as_ref().is_some_and(|f| f.setup.as_deref().is_some_and(str::is_empty)) {
                return Err(corrupt(format!(
                    "'{}' is tagged a fixture of an empty setup name",
                    dto.name
                )));
            }
        }
        EntityKind::Stock => {
            if dto.tool.is_some() || dto.operation.is_some() || dto.setup.is_some() {
                return Err(corrupt(format!(
                    "'{}' is stock carrying tool, operation or setup parameters",
                    dto.name
                )));
            }
            // v21: setup furniture may live under a private name - the
            // two shapes are exactly what `setup::stock_entity_name`
            // resolves (the implicit-setup law's legacy name, or the
            // "{SetupN}/Stock" a declared setup points at by default).
            if dto.name != STOCK_NAME && !dto.name.ends_with("/Stock") {
                return Err(corrupt(format!(
                    "a stock entity must be named '{STOCK_NAME}' or a setup's \"<setup>/Stock\", found '{}'",
                    dto.name
                )));
            }
            let Some(stock) = &dto.stock else {
                return Err(corrupt("stock entity without parameters"));
            };
            let shape = stock.to_shape();
            if !shape.is_sane() || !shape.is_bounded() {
                return Err(corrupt(format!("stock parameters are not usable: {stock:?}")));
            }
            match (&shape, &dto.facets) {
                // Parametric stock is regenerated, never stored as floats;
                // FromModel stock has nothing to regenerate from.
                (StockShape::Box { .. } | StockShape::Cylinder { .. }, Some(_)) => {
                    return Err(corrupt("parametric stock must not store facets"));
                }
                (StockShape::FromModel { .. }, None) => {
                    return Err(corrupt("from-model stock is missing its facets"));
                }
                (StockShape::FromModel { .. }, Some(tris)) if tris.is_empty() => {
                    return Err(corrupt("from-model stock has no geometry"));
                }
                _ => {}
            }
        }
        EntityKind::Wcs => {
            // v21: the slot may live under a setup's private name; the
            // word it posts is still the G-number in the last segment.
            let slot_ok = WCS_SLOTS.contains(&dto.name.as_str())
                || dto
                    .name
                    .rsplit_once('/')
                    .is_some_and(|(_, slot)| WCS_SLOTS.contains(&slot));
            if !slot_ok {
                return Err(corrupt(format!(
                    "'{}' is not a work offset slot (expected one of {}, or a setup's \"<setup>/<slot>\")",
                    dto.name,
                    WCS_SLOTS.join(", ")
                )));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.tool.is_some()
                || dto.operation.is_some()
                || dto.setup.is_some()
            {
                return Err(corrupt(format!("'{}' is an offset, not a solid", dto.name)));
            }
        }
        EntityKind::Tool => {
            if is_furniture_name(&dto.name) {
                return Err(corrupt(format!(
                    "'{}' is reserved for setup furniture, not a tool",
                    dto.name
                )));
            }
            if dto.name.is_empty() {
                return Err(corrupt("a tool entity must be named"));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.operation.is_some()
                || dto.setup.is_some()
            {
                return Err(corrupt(format!("'{}' is a tool, not a solid", dto.name)));
            }
            let Some(tool) = &dto.tool else {
                return Err(corrupt("tool entity without a definition"));
            };
            if !tool.to_tool().is_sane() {
                return Err(corrupt(format!("tool parameters are not usable: {tool:?}")));
            }
        }
        EntityKind::Operation => {
            if dto.name.is_empty() {
                return Err(corrupt("an operation entity must be named"));
            }
            if dto.facets.is_some() || dto.stock.is_some() || dto.tool.is_some() || dto.setup.is_some()
            {
                return Err(corrupt(format!(
                    "'{}' is an operation, not a solid, tool or setup",
                    dto.name
                )));
            }
            let Some(op) = &dto.operation else {
                return Err(corrupt("operation entity without a definition"));
            };
            if op.tool.is_empty() {
                return Err(corrupt(format!("'{}' names no tool", dto.name)));
            }
            if op.setup.as_deref().is_some_and(str::is_empty) {
                return Err(corrupt(format!("'{}' names an empty setup", dto.name)));
            }
            if let SrcDto::Entity { name } = &op.src
                && name.is_empty()
            {
                return Err(corrupt(format!("'{}' has an empty geometry source", dto.name)));
            }
            if let SrcDto::Selection { items } = &op.src
                && (items.is_empty() || items.iter().any(|g| g.entity.is_empty()))
            {
                return Err(corrupt(format!(
                    "'{}' has an empty or half-named geometry selection",
                    dto.name
                )));
            }
            let params = op.params.to_params();
            if !params.is_sane() {
                return Err(corrupt(format!(
                    "operation parameters are not usable for a {} operation '{}'",
                    params.kind(),
                    dto.name
                )));
            }
            if let Some(cache) = &op.cache {
                let path = cl_path(&cache.moves, &cache.cycles);
                if path.is_empty() {
                    return Err(corrupt(format!(
                        "'{}' carries a cached path with no moves",
                        dto.name
                    )));
                }
                if !path.all_finite() {
                    return Err(corrupt(format!("'{}' cache has non-finite moves", dto.name)));
                }
                if !path.is_chained() {
                    return Err(corrupt(format!("'{}' cache is not a linked path", dto.name)));
                }
                if path.cycles.iter().any(|c| c.first_move >= path.moves.len()) {
                    return Err(corrupt(format!(
                        "'{}' cache has a cycle pointing past the end of its chain",
                        dto.name
                    )));
                }
            }
        }
        EntityKind::Setup => {
            if dto.name.is_empty() {
                return Err(corrupt("a setup entity must be named"));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.tool.is_some()
                || dto.operation.is_some()
            {
                return Err(corrupt(format!(
                    "'{}' is a setup, not a solid, tool or operation",
                    dto.name
                )));
            }
            let Some(s) = &dto.setup else {
                return Err(corrupt("setup entity without a definition"));
            };
            // A setup always owns a stock name; WCS entries may name
            // entities the file does not hold - that slot is simply not
            // placed in this setup (the schema's own law) - but an empty
            // string names nothing at all.
            if s.stock.is_empty() || s.wcs.iter().any(String::is_empty) {
                return Err(corrupt(format!("'{}' has an empty furniture name", dto.name)));
            }
        }
    }
    Ok(())
}

/// Does this name follow a setup-furniture shape - the legacy global
/// names, or a private "<setup>/Stock" / "<setup>/<slot>" (M3.2 A)? The
/// Tool arm's reserved-name test: a tool wearing a furniture name would
/// poison the by-name lookups in `setup::stock_entity` and friends.
fn is_furniture_name(name: &str) -> bool {
    name == STOCK_NAME || WCS_SLOTS.contains(&name) || crate::setup::is_furniture_shape(name)
}

/// Moves and cycles, for the cache's structural checks (the warnings ride
/// along untouched; `Warning`'s serde shape is its own contract). The
/// cycles join the checks on purpose: `all_finite` reads them, and the
/// `first_move` gate below makes sure a warning pointed at a cycle has
/// something in the chain to point at.
fn cl_path(moves: &[MoveDto], cycles: &[CycleDto]) -> Toolpath {
    let mut path = Toolpath::new(moves.iter().map(|m| m.to_move()).collect());
    path.cycles = cycles.iter().copied().map(CycleDto::to_cycle).collect();
    path
}
