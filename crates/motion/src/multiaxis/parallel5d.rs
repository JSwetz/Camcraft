//! 5-axis parallel (M5.1 stage C): the Z-following raster of
//! [`crate::finishing3d::parallel`'], with the spindle oriented at
//! every contact by the lead/lag law. The raster lines are the
//! pattern's "parallel" half and the scallop cos-law `s = √(8·r·tol)`
//! (or the explicit stepover) their side step - the stepdown rides
//! the same ring ladder's law, and steep flanks still finish with
//! the rings' own level changes inside `parallel`. What stage C adds
//! is again the pose: each line's tangent direction is the raster's,
//! the normal the facet's, and the lean keeps the shank off the
//! walls the raster follows.
//!
//! Provenance: parallel-path 5-axis finishing with tilt control is
//! standard CAM practice; built on this codebase's own support-scan
//! raster. No vendor material consulted.

use crate::cl::Toolpath;
use crate::finishing3d::{self, Finish3DParams};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::avoid::Shank;
use super::stamp::stamp_axes;
use super::tilt::AxisControl;

/// The raster with the tool leaning where the law says - and, with a
/// `shank` (M5.1 stage F), dodging the part inside the cone where the
/// law's pose would touch.
pub fn parallel5d(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    let base = finishing3d::parallel(facets, p)?;
    stamp_axes(&base, facets, p.tool_diameter * 0.5, c, shank)
}
