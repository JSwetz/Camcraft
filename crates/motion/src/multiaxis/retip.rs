//! The pivot law of the radial kinds (the rotary twins and the wrap
//! rough): their ladders are planned as BALL CENTERS - a sphere's
//! envelope is invariant under any turn about its own center, which
//! is the whole argument for posing a finished ladder - but the
//! stream locates the TOOL TIP (the RTCP contract of `post::cnc`,
//! tool-tip programming as G43.4-class controls read it, and the
//! codebase's CL convention: `verify`'s probe frame stands its tip at
//! the move point). A pose turned about the tip instead of the center
//! carries the ball `r·(a - up)` off the skin it was planned against:
//! a radial pose about a standing bar stands one whole radius of air
//! off every wall (M13 rotary diagnosis). So every emitted point is
//! the center minus `r` along the pose it holds, and rapids pose too:
//! the post keeps the tilt MODAL across a prismatic record, so a
//! rapid that states nothing would not stand vertical on the machine:
//! it would hold whatever the last cut held, sweeping that fixed
//! direction around the work. Rapids therefore state the radial at
//! their own center (no lean - there is no travel to lead), and hold
//! the previous pose only where no radial exists (on the axis line).
//!
//! Provenance: tool-tip vs. ball-center CL points and RTCP tip
//! programming from public multi-axis CAM and controller literature
//! (ISO 6983 / RS-274 tool-length compensation practice); our own
//! composition. No vendor material consulted.

use crate::cl::normalize_axis;
use crate::geometry::Point3;

use super::axis::Axis;

/// World up: the pose a move without one stands in (the prismatic
/// reading of `axis: None`).
pub(super) const UP: [f64; 3] = [0.0, 0.0, 1.0];

/// `p + s·d`.
pub(super) fn along(p: Point3, d: [f64; 3], s: f64) -> Point3 {
    Point3::new(p.x + d[0] * s, p.y + d[1] * s, p.z + d[2] * s)
}

/// The tip a ball of radius `r` centered at `c` stands on while the
/// tool holds `pose` (None: upright).
pub(super) fn tip_of(c: Point3, pose: Option<[f64; 3]>, r: f64) -> Point3 {
    along(c, pose.unwrap_or(UP), -r)
}

/// The unit radial from the axis line to `w`, None on the line itself
/// (no direction to stand on) or for non-finite input.
pub(super) fn radial_at(ax: &Axis, w: Point3) -> Option<[f64; 3]> {
    let d = w - ax.origin;
    let s = d.dot(ax.dir);
    let n = [d.x - ax.dir.x * s, d.y - ax.dir.y * s, d.z - ax.dir.z * s];
    if !n.iter().all(|v| v.is_finite()) || n[0] * n[0] + n[1] * n[1] + n[2] * n[2] <= 1e-24 {
        return None;
    }
    normalize_axis(n)
}
