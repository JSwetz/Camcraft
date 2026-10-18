//! Project save/load: `.hsmproj`, JSON, version 9.
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
//! change. v1-v8 files migrate on load, one version step at a time.

use std::path::Path;

use flecs_ecs::prelude::World;
use serde::{Deserialize, Deserializer, Serialize};

use crate::cl::{Move, MoveKind, Toolpath, lattice};
use crate::geo2d::Point2;
use crate::geometry::{IDENTITY_Q, Point3, Triangle};
use crate::machining2d::{Lead, PocketStrategy, Side};
use crate::model;
use crate::operation::{FeatureSel, GenStamp, GeoSel, OpParams, SrcRef};
use crate::schema::{
    Location, Operation, StockDef, ToolDef, Toolpath as ToolpathCache, Wcs, STOCK_NAME, WCS_SLOTS,
};
use crate::stock::{self, StockShape};
use crate::tools::{Tool, ToolKind};
use crate::units::{self, Unit};
use crate::verify::Warning;

pub const PROJECT_VERSION: u32 = 9;

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
    pub entities: Vec<EntityDto>,
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
}

impl ToolDto {
    fn from_tool(t: &Tool) -> Self {
        ToolDto {
            kind: match t.kind {
                ToolKind::EndMill => ToolKindDto::EndMill,
                ToolKind::BallNose => ToolKindDto::BallNose,
                ToolKind::BullNose => ToolKindDto::BullNose,
            },
            diameter: t.diameter,
            corner_r: t.corner_r,
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
            },
            diameter: self.diameter,
            corner_r: self.corner_r,
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
                },
                // Absent means the hop-to-clearance every pre-step-5
                // pocket performed; a set plane round-trips exactly.
                retract_z: (*retract_z).map(Into::into),
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
            }
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
                },
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
            }
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

/// The cached generation output: path plus the warnings that came with it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PathDto {
    pub moves: Vec<MoveDto>,
    pub warnings: Vec<Warning>,
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
        }
    }

    fn to_cache(&self) -> ToolpathCache {
        ToolpathCache {
            path: Toolpath::new(self.moves.iter().map(|m| m.to_move()).collect()),
            warnings: self.warnings.clone(),
        }
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
}

fn identity_rotation() -> [f64; 4] {
    IDENTITY_Q
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
            } else {
                None
            };
            let stock = model::stock_def_of(world, &entity).map(|s| StockDto::from_shape(&s));
            let tool = model::tool_of(world, &entity).map(|t| ToolDto::from_tool(&t));
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
            });
            // Parametric stock regenerates its facets on load; offsets, tools
            // and operations never have geometry, so none writes a facet
            // array.
            let facets = match &kind {
                Some(EntityKind::Wcs) | Some(EntityKind::Tool) | Some(EntityKind::Operation) => None,
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
                kind,
                stock,
                tool,
                operation,
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
    units::set(world, doc_units);
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
                });
                if let Some(cache) = &odto.cache {
                    entity.entity_view(world).set(cache.to_cache());
                }
            }
        }
    }
    Ok(())
}

/// Structural checks that JSON typing cannot express. All run before this
/// entity reaches the world; a corrupt field never half-loads (Prime
/// Directive: validate early, fail cleanly).
fn validate_kind(dto: &EntityDto, kind: EntityKind) -> Result<(), ProjectError> {
    match kind {
        EntityKind::Model => {
            if dto.stock.is_some() || dto.tool.is_some() || dto.operation.is_some() {
                return Err(corrupt(format!(
                    "'{}' is a model carrying setup or CAM parameters",
                    dto.name
                )));
            }
        }
        EntityKind::Stock => {
            if dto.tool.is_some() || dto.operation.is_some() {
                return Err(corrupt(format!(
                    "'{}' is stock carrying tool or operation parameters",
                    dto.name
                )));
            }
            if dto.name != STOCK_NAME {
                return Err(corrupt(format!(
                    "a stock entity must be named '{STOCK_NAME}', found '{}'",
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
            if !WCS_SLOTS.contains(&dto.name.as_str()) {
                return Err(corrupt(format!(
                    "'{}' is not a work offset slot (expected one of {})",
                    dto.name,
                    WCS_SLOTS.join(", ")
                )));
            }
            if dto.facets.is_some()
                || dto.stock.is_some()
                || dto.tool.is_some()
                || dto.operation.is_some()
            {
                return Err(corrupt(format!("'{}' is an offset, not a solid", dto.name)));
            }
        }
        EntityKind::Tool => {
            if dto.name == STOCK_NAME || WCS_SLOTS.contains(&dto.name.as_str()) {
                return Err(corrupt(format!(
                    "'{}' is reserved for setup furniture, not a tool",
                    dto.name
                )));
            }
            if dto.name.is_empty() {
                return Err(corrupt("a tool entity must be named"));
            }
            if dto.facets.is_some() || dto.stock.is_some() || dto.operation.is_some() {
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
            if dto.facets.is_some() || dto.stock.is_some() || dto.tool.is_some() {
                return Err(corrupt(format!(
                    "'{}' is an operation, not a solid or a tool",
                    dto.name
                )));
            }
            let Some(op) = &dto.operation else {
                return Err(corrupt("operation entity without a definition"));
            };
            if op.tool.is_empty() {
                return Err(corrupt(format!("'{}' names no tool", dto.name)));
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
                let path = cl_path(&cache.moves);
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
            }
        }
    }
    Ok(())
}

/// Moves only, for the cache's structural checks (the warnings ride along
/// untouched; `Warning`'s serde shape is its own contract).
fn cl_path(moves: &[MoveDto]) -> Toolpath {
    Toolpath::new(moves.iter().map(|m| m.to_move()).collect())
}
