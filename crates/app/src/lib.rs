//! CamCraft app layer - a clean-room implementation.
//!
//! Provenance: independent implementation specced solely from public
//! documentation of commercial CAM workflows and published technical
//! literature. No HSMWorks source, binaries, or assets were consulted.

pub use camcraft_cl::cl;
pub mod cli;
pub mod commands;
pub mod cutting;
pub use camcraft_motion::drilling;
pub mod fixture;
pub use camcraft_motion::finishing3d;
pub use camcraft_geometry::geo2d;
pub mod geometry;
pub use camcraft_motion::links;
pub use camcraft_motion::load;
pub use camcraft_motion::machining2d;
pub use camcraft_motion::machining3d;
pub use camcraft_motion::multiaxis;
pub use camcraft_motion::turning;
pub mod machine;
pub mod model;
pub mod operation;
pub mod pick;
pub use camcraft_post::post;
pub mod posting;
pub use camcraft_motion::probing;
pub mod project;
pub mod rest;
pub mod ribbon;
pub mod schema;
pub mod script;
pub mod setup;
pub mod setup_sheet;
pub use camcraft_sim::solid_rest;
pub use camcraft_sim::sim;
pub use camcraft_geometry::geometry::stock;
pub mod stock_sim;
pub mod stock_export;
pub mod stock_play;
pub mod stock_play_worker;
pub mod stock_view;
pub mod tool_list;
pub use camcraft_tools::tools;
pub mod units;
pub use camcraft_sim::verify;
pub mod world;
