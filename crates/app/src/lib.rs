//! HSMWorks clean-room app layer.
//!
//! Provenance: independent implementation specced solely from public
//! documentation of commercial CAM workflows and published technical
//! literature. No HSMWorks source, binaries, or assets were consulted.

pub use camcraft_cl::cl;
pub mod commands;
pub use camcraft_motion::drilling;
pub mod fixture;
pub use camcraft_motion::finishing3d;
pub use camcraft_geometry::geo2d;
pub mod geometry;
pub use camcraft_motion::machining2d;
pub use camcraft_motion::machining3d;
pub mod machine;
pub mod model;
pub mod operation;
pub mod pick;
pub use camcraft_post::post;
pub mod posting;
pub mod project;
pub mod rest;
pub mod schema;
pub mod setup;
pub mod setup_sheet;
pub use camcraft_sim::solid_rest;
pub use camcraft_sim::sim;
pub use camcraft_geometry::geometry::stock;
pub mod stock_sim;
pub mod tool_list;
pub use camcraft_tools::tools;
pub mod units;
pub use camcraft_sim::verify;
pub mod world;
