//! All Flecs component declarations live here (AGENTS.md layering rule).
//!
//! Provenance: our own domain model; no HSMWorks internals involved.
//!
//! Components are native Rust types owned by the ECS: `Vec`/`String` fields
//! are freed by flecs_ecs's registered drop hooks when an entity is deleted
//! or a component removed. Per-type arenas are a later (M4.3-era) perf
//! optimization, not current policy.

use flecs_ecs::prelude::*;

use crate::cl::Toolpath as ClPath;
use crate::geometry::{IDENTITY_Q, Point3, Triangle};
use crate::operation::{GenStamp, OpParams, SrcRef};
use crate::stock::StockShape;
use crate::tools::Tool;
use crate::verify::Warning;

/// Human-unique name of a persistent entity. The project serializer and
/// name-based command lookup both key on this.
#[derive(Component, Debug)]
pub struct Name(pub String);

/// Rigid placement of a model entity: world position plus orientation as a
/// unit quaternion (x, y, z, w). The UI exposes yaw only for now; the data
/// model carries full orientation so stock/fixture alignment later is an
/// editor change, not a schema change. Write it through `model::set_transform`,
/// which guarantees the quaternion is unit.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub pos: Point3,
    pub rot: [f64; 4],
}

impl Default for Location {
    /// Manual: a derived `Default` would store the zero quaternion, which is
    /// not a rotation.
    fn default() -> Self {
        Location {
            pos: Point3::ZERO,
            rot: IDENTITY_Q,
        }
    }
}

/// Triangulated geometry of a model. Rust-owned storage: freed on delete.
#[derive(Component, Debug)]
pub struct Facets {
    pub tris: Vec<Triangle>,
}

/// Selection marker (tag). Only `SelectEntity` writes it (AGENTS.md: changes
/// go through commands); the tree and viewport read it to highlight.
#[derive(Component, Debug)]
pub struct Selected;

/// The setup's raw material, stored as parameters (`stock` module). Only
/// `SetStock` writes it, and it always sits on the entity named `STOCK_NAME`,
/// whose `Facets` mirror the meshed shape (render and pick need no special
/// stock paths, only the kind test).
#[derive(Component, Debug)]
pub struct StockDef {
    pub shape: StockShape,
}

/// Unit tag marking an entity as a work coordinate system origin. The entity
/// NAME is the slot identity (`G54`..`G59`), its `Location` the transform;
/// there is no payload to drift. Never has facets.
#[derive(Component, Debug)]
pub struct Wcs;

/// A cutting tool definition, on the entity named by the catalog or the
/// user. Tools never have facets; operations and the verifier read their
/// numbers through `model::tool_of`.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub tool: Tool,
}

/// A machining operation's definition: which tool, which geometry source
/// (by name), the parameters, and the stamp of the world state the last
/// successful generation saw (`None` until `GenerateOperation` runs).
/// Status is derived from `stamp` vs the live world - nothing here stores
/// "stale". (`gen` would be the obvious name, but it is a reserved keyword
/// in this Rust edition.)
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Operation {
    pub tool: String,
    pub src: SrcRef,
    pub params: OpParams,
    pub stamp: Option<GenStamp>,
}

/// The cached result of generating an operation: the path plus the
/// warnings `verify` attached to it. Deleting the operation entity frees
/// both through the Rust-owned drop hooks (AGENTS.md resource invariant).
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Toolpath {
    pub path: ClPath,
    pub warnings: Vec<Warning>,
}

/// The document's display unit, stored as a flecs singleton (M1.1): on
/// the world, not on any entity - so it can never leak into a browser
/// row, a name lookup, or the saved entity list. Millimeters until a
/// `SetUnits` command or a v8 project file says otherwise; every stored
/// number stays millimeters whatever this says (see `units` module law).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Units(pub crate::units::Unit);

/// The machine this document posts to (M2.3 stage C), stored as a flecs
/// singleton like `Units`: setup furniture, never a browser row, a name
/// lookup, or a saved entity. `machine: None` means no definition was
/// loaded and the limit checker checks nothing; `path` names the `.mch`
/// it was read from (empty when not read from disk) for display only.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct MachineDef {
    pub path: String,
    pub machine: Option<crate::machine::Machine>,
}

/// The single stock entity's name. Identity by name, like everything else:
/// tree, undo, and save all reach stock through this.
pub const STOCK_NAME: &str = "Stock";

/// Work coordinate system slots - the machine offsets posted as G54..G59.
pub const WCS_SLOTS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];

/// Registers every schema component with a fresh world. Called by
/// `world::new_world()` so every binary (and every test) gets identical
/// registration.
pub fn register_schema(world: &World) {
    world.component::<Name>();
    world.component::<Location>();
    world.component::<Facets>();
    world.component::<Selected>();
    world.component::<StockDef>();
    world.component::<Wcs>();
    world.component::<ToolDef>();
    world.component::<Operation>();
    world.component::<Toolpath>();
    world.component::<Units>();
    world.component::<MachineDef>();
}
