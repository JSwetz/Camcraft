//! Multi-axis roughing (M5.1 stage E3): the level ladder of
//! [`crate::machining3d::pocket3d`] with the stage-C stamp read over
//! it, so every move that ends in tangency poses along the surface it
//! is cutting - steep flanks take the tool OFF vertical (within the
//! max-tilt cone, lead and lag folded as in finishing), floors and
//! air keep it honest. The positions are the prismatic ladder's,
//! move for move and bit for bit: a sphere's envelope is
//! rotation-invariant about its own center, so leaning the spindle
//! cannot gouge a micrometre that the vertical cut did not already
//! own - which is why the rough rides law (f) verify like its
//! finishing siblings, and why collision-aware auto-tilt (stage F)
//! is a refinement of THIS pose law, never a precondition of it.
//!
//! What separates the kind from a 3+2 tilt (stage B) is the same
//! thing that separates contour5d from a tilted waterline: the part
//! stands still and the tool axis moves per move; one index cannot
//! hold the poses this ladder's flanks ask for.
//!
//! Provenance: 5-axis roughing with tool-axis control over a
//! z-level staircase is standard practice in the public multi-axis
//! CAM manuals; built from this codebase's own pocket ladder and
//! stage-C stamp. No vendor material consulted.

use crate::cl::Toolpath;
use crate::geo2d::Region;
use crate::geometry::Triangle;
use crate::machining2d::GenError;
use crate::machining3d::Pocket3DParams;
use crate::stock_sim::Dexel;

use super::avoid::Shank;
use super::stamp::stamp_axes;
use super::tilt::AxisControl;

/// The ladder, posed: pocket3d's path, move for move, with the
/// spindle leaned wherever the tangency law finds a surface to lean
/// off. `r` is read from the params' diameter; the stamp's tangency
/// band is the same micron-lattice law the finishing trio rides.
/// With a `shank` (stage F), the pose law gets its refinement here:
/// moves whose shank would touch tilt to the nearest clear angle in
/// the cone - and floors pose nothing until the shank speaks.
pub fn multiaxis_rough(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    multiaxis_rough_prev(facets, footprint, p, c, None, shank)
}

/// The re-machining sibling, same law as [`crate::machining3d::pocket3d_prev`]:
/// the mask bounds the coarse ladder, and the stamp reads whatever
/// path that ladder produced. Nothing ahead in the program cut means
/// the identical path, mask or no mask.
pub fn multiaxis_rough_prev(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    c: &AxisControl,
    mask: Option<&Dexel>,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    let base = crate::machining3d::pocket3d_prev(facets, footprint, p, mask)?;
    stamp_axes(&base, facets, p.tool_diameter * 0.5, c, shank)
}
