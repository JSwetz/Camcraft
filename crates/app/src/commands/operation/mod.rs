//! Operation commands: create, edit, delete, and generate.
//!
//! `GenerateOperation` is where this layer's central design choice lives:
//! generation runs in the constructor, while the world is being read
//! anyway and errors can still refuse the command (validate early). The
//! resulting path, warnings and stamp become plain fields, so `apply` and
//! `invert` are pure component writes - an undo of an unrelated edit and
//! a redo of the generation replay the identical bytes, no re-run.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

mod create;
mod edit;
mod gates;
mod generate;
mod geometry;

pub use create::{CreateOperation, DeleteOperation};
pub use edit::{ChangeOperationKind, EditOperationLinks, EditOperationParams};
pub use generate::{GenerateOperation, RegenerateAll};
pub use geometry::SetOperationGeometry;
