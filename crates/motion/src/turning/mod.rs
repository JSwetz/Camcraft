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
//! radial pass of the insert's nose circle THROUGH the axis. Profile
//! roughing/finishing, grooving, threading join this module as their
//! own files (stages B..); the axis frame below is what they share.
//!
//! Provenance: lathe facing as a radial-pass ladder at axial
//! stations - standard turning practice from machine-tool handbooks
//! and the RS-274 linear/circular move vocabulary; the silhouette
//! and station mathematics are our own, composed on this codebase's
//! axis-fit law. No vendor strategy or post output was inspected.

pub mod face;
pub mod profile;

pub use face::{facing, FacingParams};
pub use profile::{profile_of, Profile};
