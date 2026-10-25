//! Split from `machining3d.rs` by M5.0.1; structure only.


use crate::geo2d::Region;
use crate::geometry::slice::{self, SliceError};
use crate::geometry::Triangle;
use crate::machining2d::GenError;
/// Half-open slices sit exactly on vertices for tessellated steps; this
/// is the same lattice the weld uses, so a grazing plane nudges rather
/// than lies.
pub(super) const EPS: f64 = 1e-6;

pub(super) fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The model's cross-section strictly above `z`, nudge-retried when a
/// plane grazes a tessellated shelf (reading an errored slice as
/// no-island would be a silent gouge - the retry lands above the shelf
/// instead, failing safe). Public because it is the islands law itself:
/// the generator reads it to shape each level, the verify pass to prove
/// them, and the tests to pin the monotonicity the ladder assumes -
/// one law, three readers (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    slice::material_above(facets, z).map_err(from_slice)
}

