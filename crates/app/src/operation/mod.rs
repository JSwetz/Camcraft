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
use crate::finishing3d::{self, DeburrParams, Finish3DParams};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::feature::{self, LoopKind};
use crate::geometry::revision;
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, transform_point};
use crate::machining2d::{
    self, ChamferParams, CircularParams, ContourParams, FaceParams, GenError, Lead, PocketParams,
    PocketStrategy, Side, SlotParams, TraceParams,
};
use crate::machining3d::{self, Entry3D, Pocket3DParams};
use crate::model;
use crate::schema::Operation;
use crate::stock::StockShape;
use crate::verify::{self, VerifyContext, Warning};

mod heights;
pub use heights::*;
mod kinds;
pub use kinds::*;
mod params;
mod stamp;
pub use stamp::*;
mod material;
pub use material::*;
mod probe;
pub(crate) use probe::*;
mod selection;
pub use selection::*;
mod picks;
pub use picks::*;
mod stitch;
use stitch::*;
mod job;
pub use job::*;
mod generate;
pub use generate::*;
#[cfg(test)]
mod tests;

pub mod types;
pub use types::*;
