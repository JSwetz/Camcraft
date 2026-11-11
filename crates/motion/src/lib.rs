//! camcraft-motion - the generator seam of the M4.1 crate split (R4,
//! docs/M4.1-CRATE-SPLIT.md). Model facets and tool records in, CL
//! out; the one legal direction to the sim is the one used here:
//! machining3d asks the dexel engine for rest material
//! (motion -> sim, one-way as designed). `rest.rs` stayed in app -
//! it is remachining ORCHESTRATION (who counts as prior, by setup
//! traversal), not a generator; the design table's "rest in motion"
//! entry was wrong and R4 records it. The islands law itself sank to
//! `geometry::slice`, so generator and verifier read one module, not
//! each other: the generators `material_overhead` (section plus the
//! shadow of what hangs above), the verifier `material_above` (the
//! plain section at a move's plane).
//!
//! Provenance: offset/contour pocketing (clipper-backed island
//! logic), zigzag and waterline slicing, drill-cycle families
//! (RS-274 G73/G81/G83/G84/G85/G86/G89 semantics), scallop-height
//! finishing - all from public machining literature and G-code
//! standards; no vendor strategy was inspected. No flecs, no world.

pub mod approach;
pub mod drilling;
pub mod entry;
pub mod entry_arc;
pub mod links;
pub mod load;
pub mod finishing3d;
pub mod machining2d;
pub mod machining3d;
pub mod plunge_law;
pub mod plasma;
pub mod probing;
pub mod quality;
pub mod multiaxis;
pub mod turning;

pub use camcraft_cl::cl;
pub use camcraft_geometry::{geo2d, geometry};
pub use camcraft_geometry::geometry::{stock, units};
pub use camcraft_sim::{fixture, stock_sim, verify};
pub use camcraft_tools::tools;
