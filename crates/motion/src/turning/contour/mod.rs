//! Contour turning (M5.2 E): chamfering, taper turning and
//! multi-pass turning - the rides that state a FACE and swear the
//! model already has it.
//!
//! The law is the threading kind's, one step sideways (M5.2 D): a
//! contour op is a promise over a MODELED contour. The chamfer's
//! angle and the taper's two diameters do not author a surface to
//! chase - the model is the surface the verdict measures against,
//! and a pass that followed an authored cone BELOW it would cut
//! past the drawing and call it output. So the ride is the profile
//! finish's, plain, along the model's own offset inside the window,
//! and the authored numbers spend themselves at the door: every
//! silhouette segment strictly between the window ends must stand
//! within the tolerance of the authored line (radius vs. station,
//! the meridian's own reading), or the kernel refuses by name.
//! A model the door cannot match is a model the pass was not told
//! the truth about, and silence about that is the Prime Directive's
//! own worst case.
//!
//! Multi-pass turning states no new surface at all: it is the
//! roughing ladder and a sequence of finishing rides, each the
//! last's midpoint toward the profile - the spring-pass law of the
//! handbooks (a finishing cut removes about half of what the
//! previous one sprang back). The leave `stock` is shared out over
//! the rides: ride k of n stands at d = nose + stock * (n - k) / n,
//! so the last ride is the profile itself and every earlier one is
//! a light skim inside the rung ladder's floor.
//!
//! Provenance: lathe chamfering, taper turning and multi-pass
//! finishing as machine-tool handbook practice; the promise-at-the-
//! door law and the spring-pass midpoint formula are our own,
//! composed on stage B's silhouette and offset machinery. No vendor
//! strategy or post output was inspected.

mod door;
mod rides;

pub use rides::{chamfer, chamfer_in, multi_pass, multi_pass_in, taper, taper_in};

/// Chamfer turning as the kernel reads it: the window that bounds
/// the face, the face's authored cone angle off the cylinder, and
/// the profile ride's own numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChamferParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub angle_deg: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
    pub tolerance: f64,
}

/// Taper turning: the same window, and the two diameters that state
/// the cone - `bottom_dia` at the window's low station, `top_dia`
/// at its high one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaperParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub bottom_dia: f64,
    pub top_dia: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
    pub tolerance: f64,
}

/// Multi-pass turning: the roughing ladder's numbers and the count
/// of finishing rides that share the leave out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MultiPassParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub stock_to_leave: f64,
    pub passes: u32,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
    pub tolerance: f64,
}
