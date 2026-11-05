//! 2D machining generators: region + parameters in, CL path out.
//!
//! Provenance: serpentine facing, offset contouring, zigzag and waterline
//! pocketing, stadium slotting rows and engraved trace paths are textbook
//! CAM strategies as described in machine-tool handbooks and published CAM
//! literature; the arc fitter is the classic three-point circle with a
//! two-sided sagitta bound (geometry handbooks, e.g. Farin's curves and
//! surfaces primer); turning-point blends are RS-274 / ISO 6983 G2/G3
//! path filleting (general machining knowledge), and the contour /
//! pass-link corner fillets the classical circle tangent to two lines. Every number (parameter model, entry/exit overruns,
//! snap rules) is ours. Pure module: no flecs, no
//! world access - the command layer resolves geometry into regions, this
//! module only cuts them.
//!
//! Prime Directive: parameters are validated before any geometry is
//! touched; impossible shapes return `GenError` (or a clean empty result
//! where the plan says collapse means empty), never a partially linked
//! path. Every returned path is chained, finite, and posts real motion.




mod arcs;
mod blend;
mod bore;
mod chamfer;
mod circular;
mod thread_mill;
mod contour;
mod contour_entry;
mod face;
mod fillet;
mod fillet_tally;
mod lead_fit;
mod levels;
mod pocket;
mod ring;
mod slots;
mod waterscan;

#[cfg(test)]
#[allow(unused_imports)]
use {
    arcs::*, bore::*, chamfer::*, circular::*, contour::*, face::*, levels::*, pocket::*,
    ring::*, slots::*, thread_mill::*, waterscan::*,
};
#[cfg(test)]
mod tests;

pub use arcs::{fit_arcs, Seg, ARC_FIT_TOL};
pub use blend::MIN_BLEND_RADIUS;
pub use bore::{bore, BoreHole, BoreParams};
pub use chamfer::{chamfer, ChamferParams};
pub use circular::{circular, CircularParams};
pub use contour::{contour, contour_along_chain, ContourParams, Lead, Side};
pub use face::{face, FaceParams, GenError};
pub use fillet::{fillet_corners, Legal};
pub use fillet_tally::{fillet_floor, FilletTally};
pub(crate) use levels::cut_levels;
pub use pocket::{pocket, PocketParams, PocketStrategy};
pub use ring::{on_boundary, ring_is_circular};
pub use slots::{chord_clears, slot, trace, SlotParams, TraceParams};
pub use thread_mill::{thread_mill, ThreadHole, ThreadParams, THREAD_DEPTH_PER_PITCH};
pub(crate) use pocket::plan_passes;
pub(crate) use waterscan::chord_crosses_path;
