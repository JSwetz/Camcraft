//! What the centre-cutting audit reads as solid for the kinds that carry
//! no approach (M11.3 owed (5): Face and the 3-D finishes): the setup's
//! stock in the generator's own frame - its XY footprint and its top -
//! or, with no separate stock, the material the op cuts. Read only for
//! a tool that is not centre-cutting, so no centre-cutting path pays.
//!
//! Provenance: the stock-as-solid reading of the centre-cutting plunge
//! law (`camcraft_motion::plunge_law`); this project's own design notes.

use super::{FOOTPRINT_SAMPLES, Material, Region, Triangle, slice};

/// The stock read as solid: `(top, footprint)`. A footprint that cannot
/// be projected is `None`, which the law reads as solid everywhere
/// under the top - unseen stock is solid.
pub(super) fn plunge_solid(
    stock_facets: Option<&[Triangle]>,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
) -> (f64, Option<Region>) {
    let local: Vec<Triangle> = match (stock_facets, frame) {
        (Some(s), Some(f)) => f.local_facets(s),
        (Some(s), None) => s.to_vec(),
        (None, _) => material.facets.clone(),
    };
    let top = local
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .filter(|z| z.is_finite())
        .fold(material.top, f64::max);
    let footprint = slice::project(&local, FOOTPRINT_SAMPLES).ok().filter(|r| !r.is_empty());
    (top, footprint)
}
