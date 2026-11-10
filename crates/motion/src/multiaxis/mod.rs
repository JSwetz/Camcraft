//! Simultaneous-axis finishing kinds - the 5-axis freeform trio of
//! stage C (contour, parallel, swarf, all posing from the SURFACE
//! NORMAL through `tilt`), the ruled swarf reads of stage D
//! (`ruling`), and the rotary family of stage E (`axis` + `rotary`
//! for the finishing pair, `rotary_offset` for the rough ladder, all
//! posing from the RADIAL of a fitted axis of revolution), and the
//! stamped pocket ladder of stage E3 (`rough`: the prismatic rough in
//! freeform ground, posed by the same normal law as the finishing
//! trio), and the drive-curve contour of stage J (`curve`: the pass
//! that walks the caller's picked chains, resting the ball on the
//! vertex's own fan of planes).
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

mod avoid;
mod curv;
mod curve_walk;
mod entry_ramp;
mod linalg;
mod stamp;
mod stamp_pose;
mod stamp_rise;
#[cfg(test)]
mod stamp_tests;
pub mod axis;
mod axis_consensus;
mod axis_point;
mod axis_poles;
mod axis_refine;
mod axis_robust;
pub mod contour5d;
pub mod curve;
pub mod flow;
pub mod parallel5d;
pub mod rough;
pub mod rotary;
pub mod rotary_offset;
mod rotary_seam;
mod rotary_seam_angle;
mod rotary_sheet;
pub mod rotary_bar;
mod rotary_links;
mod wrap_exit;
mod retip;
pub mod ruling;
pub mod stamp_link;
pub mod swarf;
pub mod tilt;

pub use avoid::Shank;
pub use contour5d::contour5d;
pub use curve::{contour_curve, CurveParams};
pub use flow::{flow3d, flow5d};
pub use parallel5d::parallel5d;
pub use rough::{multiaxis_rough, multiaxis_rough_prev};
pub use rotary::{rotary_contour, rotary_parallel};
pub use rotary_bar::revolved_bar;
pub use rotary_offset::{rotary_offset, rotary_offset_developed, rotary_offset_in};
pub use stamp_link::{guard_turns, link_swings, LINK_TURN_DEG};
pub use swarf::{swarf5d, swarf5d_advanced};
pub use tilt::{command_axis, AxisControl};
