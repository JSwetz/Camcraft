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




mod arc_biarc;
mod arc_cross;
mod arc_proof;
mod arcs;
mod blend;
mod bore;
mod chamfer;
mod circular;
mod thread_mill;
mod contour;
mod contour_bore;
mod contour_chain;
mod contour_comp;
mod contour_comp_room;
mod contour_depth;
mod contour_entry;
mod contour_link;
mod contour_params;
mod contour_step;
mod descent;
mod descent_dive;
mod descent_xy;
mod face;
mod face_entry;
mod face_holes;
mod fillet;
mod gen_error;
mod fillet_arc;
mod fillet_tally;
mod lead_ext;
mod lead_fit;
mod ladder_links;
mod levels;
mod microlift;
mod pocket;
mod pocket_hsm;
mod pocket_troch;
mod adaptive;
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
pub use contour::{contour, contour_on_bodies, contour_on_part, ContourParams, Lead, Side};
pub use contour_chain::contour_along_chain;
pub use contour_link::DEFAULT_LEVEL_RAMP_ANGLE;
pub use crate::links::{Link, Links};
pub use face::{face, face_led, face_over, FaceParams, GenError};
pub use face_holes::FaceHoles;
pub use face_entry::{air_lead, AIR_MARGIN};
pub use fillet::{fillet_corners, Legal};
pub use fillet_tally::{fillet_floor, FilletTally};
pub(crate) use levels::cut_levels;
pub use pocket::{pocket, PocketParams, PocketStrategy};
pub use ring::{on_boundary, ring_is_circular};
pub use slots::{chord_clears, slot, trace, SlotParams, TraceParams};
pub use thread_mill::{thread_mill, ThreadHole, ThreadParams, THREAD_DEPTH_PER_PITCH};
pub(crate) use pocket::plan_passes;
pub(crate) use adaptive::PLAN_SAG;
// The HSM planner's door, its arc proofs and its microlift link, read
// by the 3-D ladder's per-level port (`machining3d::hsm`, M11.4).
pub(crate) use adaptive::Return;
pub(crate) use arc_cross::crossing_free;
pub(crate) use arc_proof::{ProofTally, fit_proved};
pub(crate) use arcs::push_segs;
pub(crate) use microlift::link_at as microlift_link;
pub(crate) use pocket_hsm::{adaptive_passes, choose_entry};
pub(crate) use waterscan::chord_crosses_path;
