//! turning - the lathe branch of the generator seam (M5.2 A).
//!
//! The mill crates above spin the TOOL; here the WORK spins and the
//! insert traverses in the XZ half-plane about the axis of revolution.
//! That axis is not re-derived: it is the very line stage E's
//! [`crate::multiaxis::axis::fit`] reads off the facet normals, so
//! "where the spindle is" stays one truth across families, and a
//! model that says nothing of turning refuses here by the same words
//! it refuses with there.
//!
//! The stage-A kernel is Facing: a ladder of Z stations stepped from
//! the facing plane toward the work's top, each station cut by one
//! radial pass of the insert's nose circle THROUGH the axis. Stage B
//! adds the profile kinds: [`profile`]'s silhouette of the bar in
//! (station, radius), [`offset`]'s ride of the nose on it, and
//! [`prof`]'s finish and concentric roughing ladders built from that
//! pair. Stage C adds the blade: [`revolve`]'s meridian verdict of
//! what a banded cut leaves, and [`blade`]'s grooving, groove-
//! finish and parting sweeps built on the same silhouette. Stage D
//! added [`thread`]'s helical ladder; stage E adds [`contour`]'s
//! promised-face rides - chamfer, taper, multi-pass. The rest join
//! as their own files; the axis frame below is what they all share.
//!
//! Provenance: lathe facing as a radial-pass ladder at axial
//! stations - standard turning practice from machine-tool handbooks
//! and the RS-274 linear/circular move vocabulary; the silhouette
//! and station mathematics are our own, composed on this codebase's
//! axis-fit law. No vendor strategy or post output was inspected.

pub mod blade;
pub mod contour;
pub mod face;
pub mod offset;
pub mod prof;
pub mod profile;
pub mod revolve;
pub mod thread;

pub use blade::{
    groove_finish, groove_rough, part, GrooveFinishParams, GrooveRoughParams, PartParams,
};
pub use contour::{
    chamfer, multi_pass, taper, ChamferParams, MultiPassParams, TaperParams,
};
pub use face::{facing, FacingParams};
pub use offset::profile_offset;
pub use prof::{profile_finish, profile_rough, ProfileFinishParams, ProfileRoughParams};
pub use profile::{profile_of, silhouette, Profile};
pub use revolve::{blade_warnings, BandMove};
pub use thread::{thread, thread_warnings, ThreadParams};
