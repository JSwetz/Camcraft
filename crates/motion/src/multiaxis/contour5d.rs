//! 5-axis contour (M5.1 stage C): the waterline's Z-level ring ladder
//! over the freeform, every cut move holding the pose [`tilt`]
//! commands. The path is the ball-center tangent locus exactly as
//! [`crate::finishing3d::waterline`] builds it - a sphere cares which
//! way its shank points not at all - and the 5-axis content is the
//! orientation: on steep flanks a vertical spindle drags its shank
//! into the wall, and the lead/lag lean (plus the max-tilt clamp) is
//! the difference between a finishing pass and a crash. The levels
//! ride the scallop stepdown law `Δz = 2√(r·tol)`; the tolerance, not
//! a stepdown field, remains the author of depth.
//!
//! Provenance: 5-axis contouring over a Z-level pattern and the
//! lead/side-tilt axis law are published multi-axis milling practice
//! (ISO 10791-10 test-part tradition, standard CAM manuals); the
//! construction here reuses this codebase's own forbidden-center
//! rings and adds the pose stamp. No vendor source was consulted.

use crate::cl::Toolpath;
use crate::finishing3d::{self, Finish3DParams};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::avoid::Shank;
use super::stamp::stamp_axes;
use super::tilt::AxisControl;

/// The ring ladder with the tool leaning where the law says - and,
/// with a `shank` (M5.1 stage F), dodging the part inside the cone
/// where the law's pose would touch. The positions are the
/// waterline's either way: law (f) makes the dodge exact.
pub fn contour5d(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    let base = finishing3d::waterline(facets, p)?;
    stamp_axes(&base, facets, p.tool_diameter * 0.5, c, shank)
}
