//! Simultaneous-axis finishing kinds - the 5-axis freeform trio of
//! stage C (contour, parallel, swarf, all posing from the SURFACE
//! NORMAL through `tilt`), the ruled swarf reads of stage D
//! (`ruling`), and the rotary family of stage E (`axis` + `rotary`
//! for the finishing pair, `rotary_offset` for the rough ladder, all
//! posing from the RADIAL of a fitted axis of revolution).
//!
//! Where stage B kept the tool vertical and moved the PART (3+2
//! indexing through a tilted frame), these kinds keep the PART still
//! and move the TOOL AXIS: every CL move carries the direction the
//! spindle points, computed from the surface it is cutting. They
//! share the lean law in `tilt`, the pass that stamps an axis onto a
//! finished path in `stamp`, and the ball-end contact geometry
//! (`crate::finishing3d::support`); all ride stage A's axis-stamped
//! path builder. `linalg` holds the eigen and solve kernels the
//! fits are written from.
//!
//! Provenance: lead/lag/side-tilt angle definitions and level/parallel
//! 5-axis patterns from public multi-axis milling literature (ISO
//! 10791-10 test-part tradition, standard CAM manuals); rotary
//! axis-of-revolution detection from the published normal-concurrency
//! law; written from those sources and our own design notes only.

mod linalg;
mod stamp;
#[cfg(test)]
mod stamp_tests;
pub mod axis;
pub mod contour5d;
pub mod parallel5d;
pub mod rotary;
pub mod rotary_offset;
mod wrap_exit;
pub mod ruling;
pub mod swarf;
pub mod tilt;

pub use contour5d::contour5d;
pub use parallel5d::parallel5d;
pub use rotary::{rotary_contour, rotary_parallel};
pub use rotary_offset::rotary_offset;
pub use swarf::{swarf5d, swarf5d_advanced};
pub use tilt::{command_axis, AxisControl};
