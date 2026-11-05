//! camcraft-kernel - the B-rep kernel seam of M12.1, stage 1 (evaluation +
//! agreement bench). A thin [`Kernel`] trait carries only the operations both
//! candidate engines can be asked for: primitive construction (box, Z
//! cylinder), boolean subtract, whole-solid offset, and a [`Measure`]
//! (volume + bounding box) the bench compares. Nothing else in the workspace
//! depends on this crate yet; truck stays the live engine.
//!
//! Why `measure` and not `tessellate`: solidcore 1.0.1 does not port SMLib's
//! tessellation (its README lists it among the unported SM_API entry points),
//! so a triangle-mesh method would have no solidcore impl. The port measures
//! exactly through its mass-property integrator; the truck impl measures its
//! tessellation. The bench compares those numbers.
//!
//! Provenance: our own seam design; solidcore (tangency-labs, Apache-2.0
//! port of NVIDIA's SMLib) and truck (ricosjp, Apache-2.0) are linked as
//! published crates; the mesh volume is the divergence theorem (any
//! computational-geometry text). Notices in assets/ui/LICENSES.md.

#![forbid(unsafe_code)]

mod error;
mod measure;
pub mod solidcore_impl;
#[cfg(feature = "truck")]
pub mod truck_impl;

pub use error::KernelError;
pub use measure::{Measure, mesh_signed_volume};

/// A point or extent in model units, `[x, y, z]`.
pub type P3 = [f64; 3];

/// The kernel seam. `Solid` is the engine's own solid type; the seam never
/// looks inside it. Every method validates its numeric input before the
/// engine sees it (Prime Directive: degenerate input is a clean `Err`).
pub trait Kernel {
    /// The engine's solid representation.
    type Solid;

    /// Short engine name for diagnostics (`"solidcore"`, `"truck"`).
    fn name(&self) -> &'static str;

    /// Axis-aligned box with its minimum corner at `origin`.
    fn make_box(&mut self, origin: P3, size: P3) -> Result<Self::Solid, KernelError>;

    /// Cylinder along +Z, base circle centered at `base`.
    fn make_cylinder(
        &mut self,
        base: P3,
        radius: f64,
        height: f64,
    ) -> Result<Self::Solid, KernelError>;

    /// `a` minus `b`. An empty result is an `Err`, not an empty solid.
    fn boolean_subtract(
        &mut self,
        a: Self::Solid,
        b: Self::Solid,
    ) -> Result<Self::Solid, KernelError>;

    /// Whole-solid offset by `distance` (negative = inset), convex edges
    /// rounded. Engines without an offset return [`KernelError::Unsupported`].
    fn offset(&mut self, s: &mut Self::Solid, distance: f64) -> Result<Self::Solid, KernelError>;

    /// Volume and bounding box of a closed solid.
    fn measure(&mut self, s: &mut Self::Solid) -> Result<Measure, KernelError>;
}

/// Rejects non-finite coordinates (shared input guard of both impls).
pub(crate) fn finite_point(op: &'static str, p: P3) -> Result<(), KernelError> {
    if p.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(KernelError::InvalidInput {
            op,
            detail: format!("non-finite coordinate {p:?}"),
        })
    }
}

/// Rejects a length that is not finite and strictly positive.
pub(crate) fn positive_len(op: &'static str, what: &str, v: f64) -> Result<(), KernelError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(KernelError::InvalidInput {
            op,
            detail: format!("{what} must be finite and > 0, got {v}"),
        })
    }
}

/// Rejects an offset distance that is not finite or is zero.
pub(crate) fn offset_distance(v: f64) -> Result<(), KernelError> {
    if v.is_finite() && v != 0.0 {
        Ok(())
    } else {
        Err(KernelError::InvalidInput {
            op: "offset",
            detail: format!("distance must be finite and non-zero, got {v}"),
        })
    }
}
