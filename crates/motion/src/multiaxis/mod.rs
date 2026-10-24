//! Simultaneous-axis (true 5-axis) finishing kinds - M5.1 stage C.
//!
//! Where stage B kept the tool vertical and moved the PART (3+2
//! indexing through a tilted frame), these kinds keep the PART still
//! and move the TOOL AXIS: every CL move carries the direction the
//! spindle points, computed from the surface it is cutting. The two
//! path shapes - contour (level loops that follow the surface) and
//! parallel (straight raster lines swept over the freeform) - share
//! the axis law in `tilt`, the pass that stamps it onto a finished
//! path in `stamp`, and the ball-end contact geometry
//! (`crate::finishing3d::support`); both ride stage A's
//! axis-stamped path builder.
//!
//! Provenance: lead/lag/side-tilt angle definitions and level/parallel
//! 5-axis patterns from public multi-axis milling literature (ISO
//! 10791-10 test-part tradition, standard CAM manuals); written from
//! those published descriptions and our own design notes only.

mod stamp;
pub mod contour5d;
pub mod parallel5d;
pub mod tilt;

pub use contour5d::contour5d;
pub use parallel5d::parallel5d;
pub use tilt::{command_axis, AxisControl};
