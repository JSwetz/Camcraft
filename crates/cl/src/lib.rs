//! camcraft-cl - the motion-model seam of the M4.1 crate split (R2,
//! docs/M4.1-CRATE-SPLIT.md). The CL intent is the project's
//! intermediate format (docs/CNC-INTERMEDIATE.md): a lossless,
//! machine-neutral record of motion, geometry-independent enough that
//! posting and simulation both read it and no generator writes
//! machine code. Its only companions are the geometry crate's points
//! and unit - the lattice quantum lives here because it is a motion
//! law, not a display choice.
//!
//! Provenance: the APL/CLToolpath shape is general CNC practice,
//! specced from RS-274 motion blocks, ISO 6983 tool addressing and
//! published CAM post-processing documentation; the digest scheme of
//! GenStamp-era toolpath identity is our own. No flecs, no schema -
//! the app re-exports this module whole (`crate::cl`).

pub mod cl;
pub use camcraft_geometry::{geo2d, geometry};
