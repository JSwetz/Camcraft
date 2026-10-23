//! Geometry: the camcraft-geometry crate re-exported whole (M4.1 R1,
//! docs/M4.1-CRATE-SPLIT.md), plus the truck-bound mesh importers, which
//! stay in app by design (the crate must not carry a CAD-kernel dep).
//! Every `crate::geometry::...` path in app and UI resolves through the
//! glob exactly as before the move.

pub use camcraft_geometry::geometry::*;

pub mod import;

pub use import::{ImportError, load_model};
