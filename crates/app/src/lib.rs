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
pub mod fixture_carried;
pub mod fixture_struck;
pub mod fixture_view;
pub use camcraft_motion::finishing3d;
pub use camcraft_geometry::geo2d;
pub mod geometry;
pub use camcraft_motion::links;
pub use camcraft_motion::load;
pub use camcraft_motion::machining2d;
pub use camcraft_motion::machining3d;
pub use camcraft_motion::multiaxis;
pub use camcraft_motion::turning;
pub mod logging;
pub mod machine;
// #25: deviation classes on the machined mesh.
pub mod machined_classes;
pub mod mcp;
pub mod model;
pub mod operation;
pub mod pick;
pub use camcraft_post::post;
pub mod path_sig;
pub mod posting;
// #155: standalone Pause / Home entries in the operation list.
pub mod program_entry;
pub use camcraft_motion::probing;
pub mod project;
// #182: the CPU rasteriser behind the MCP stock render.
pub mod raster;
pub mod rest;
pub mod ribbon;
pub mod schema;
pub mod script;
pub mod setup;
// #326: the part's alignment per setup.
pub mod setup_align;
// #343: each setup's own machine and post, and the one-mount law.
pub mod setup_mount;
// #316: transforms between setups and the setup-relative link.
pub mod setup_relative;
pub mod setup_sheet;
// #320: a setup's stock source - its own, or what the setup before leaves.
pub mod setup_stock;
pub use camcraft_sim::solid_rest;
pub use camcraft_sim::sim;
pub use camcraft_geometry::geometry::stock;
pub mod stock_sim;
pub mod stock_export;
pub mod stock_play;
pub mod stock_play_worker;
pub mod stock_view;
pub mod tool_library;
pub mod tool_list;
// #221: the number a tool posts as, and the clash law.
pub mod tool_numbers;
pub use camcraft_tools::tools;
// #93: the drawn spun profile of a milling holder.
pub use camcraft_tools::spun;
// #368: a tool's modelled body and the reads the judging laws take from it.
pub mod tool_body;
// #371: the editor's placement preview and picks, the sim's posed body.
pub mod tool_body_view;
// #369: lathe-held tool bodies (the turning tool frame).
pub mod tool_body_lathe;
pub mod units;
pub use camcraft_sim::verify;
// #261: the moves a cached path's warnings name.
pub mod warned_moves;
pub mod world;
