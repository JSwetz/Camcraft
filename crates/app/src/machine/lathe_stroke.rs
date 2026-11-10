//! The lathe's slide stroke (#647): what a lathe-kind machine's X and
//! Z slides must reach for one operation, read off the path the post
//! writes - not off the cached path's world X.
//!
//! The turning kernels state their radius on one MERIDIAN of the
//! spindle axis (world Z through the WCS origin): a cached facing pass
//! is (0, y, z), so world X never sees its radius, and a radial
//! over-travel on the Y meridian went unjudged. And the cache is the
//! nose centre; since #630 the post writes the tool's control point.
//! So a lathe op is judged here on [`crate::posting::nose::posted_path`],
//! the very stream the post is handed, with each point's X read as
//! the lathe posts read it: the signed offset from the axis on the
//! path's meridian (X if Y is on the axis, else Y), so a point past
//! centre on the meridian is negative and meets the band's floor.
//! Ops the post does not convert (grooving, parting, threading,
//! drilling) are judged on their own path, read the same way.
//!
//! Two readings are not a meridian's: an arc about the spindle axis
//! (a thread's turn; #631) is the spindle turning, so the slide
//! reaches its RADIUS at every extreme; any other point off every
//! meridian is not a lathe position at all (the post refuses it), and
//! is judged both on its signed world X (the pre-#647 reading) and on
//! its radius, so no reading acquits it. Over-reporting a real stroke
//! is the honest direction of error; acquitting one never is.
//!
//! Provenance: general lathe practice (X is the radius from the
//! spindle centreline, Z along it); the meridian reading is our own
//! lathe posts' (`assets/posts/fanuc-lathe.cps`, `radiusOf`). Our own
//! design otherwise.

use std::borrow::Cow;

use super::{kind, move_extremes, say, Kind, Machine, MachineCheck};
use crate::cl::Toolpath;
use crate::geometry::Point3;
use crate::operation::OpParams;
use crate::tools::Tool;

/// The lathe posts' "on the axis" tolerance (half a micron).
const EPS: f64 = 0.0005;

/// One operation's slide stroke on a lathe-kind machine. Returns false
/// (and judges nothing) on any other machine, whose travel the generic
/// per-axis walk keeps; true when this judged the op's X, Y and Z.
#[allow(clippy::too_many_arguments)]
pub(super) fn judge(
    out: &mut MachineCheck,
    name: &str,
    params: &OpParams,
    tool_name: &str,
    tool: &Tool,
    path: &Toolpath,
    machine: &Machine,
    origin: [f64; 3],
) -> bool {
    if kind(Some(machine)) != Kind::Lathe {
        return false;
    }
    // An orientation the nose cannot cut with is refused by name at
    // the post (`posting::nose`); until it is fixed, its nose centre
    // is judged - a path is never left unjudged.
    let posted = crate::posting::nose::posted_path(params, tool_name, tool, path, origin)
        .map_or(Cow::Borrowed(path), |(p, _)| p);
    for (i, mv) in posted.moves.iter().enumerate() {
        let spindle_arc = mv.arc.is_some_and(|a| {
            a.plane.is_xy() && (a.center.x - origin[0]).abs() <= EPS && (a.center.y - origin[1]).abs() <= EPS
        });
        for p in move_extremes(mv) {
            for (k, v) in readings(p, origin, spindle_arc) {
                let Some(ax) = machine.axis(k) else { continue };
                let Some((lo, hi)) = ax.travel else { continue };
                let v = v * ax.direction.sign();
                if !(lo..=hi).contains(&v) {
                    let inv = if ax.direction.is_standard() { "" } else { " (inverted axis)" };
                    say(
                        out,
                        format!("{name}: move {i} reaches {v:.3} in {}{inv}, the stroke is {lo:.3}..{hi:.3}", ax.name),
                    );
                }
            }
        }
    }
    true
}

/// The program-frame values a point asks of the slides, as (axis
/// index, value): X the meridian's signed radius, Z along the spindle.
/// A NaN reading fails every band (`contains` is false), so a broken
/// number is refused, never acquitted.
fn readings(p: Point3, origin: [f64; 3], spindle_arc: bool) -> Vec<(usize, f64)> {
    let (dx, dy, dz) = (p.x - origin[0], p.y - origin[1], p.z - origin[2]);
    let mut v = vec![(2, dz)];
    if spindle_arc {
        v.push((0, dx.hypot(dy)));
    } else if dy.abs() <= EPS {
        v.push((0, dx));
    } else if dx.abs() <= EPS {
        v.push((0, dy));
    } else {
        v.extend([(0, dx), (0, dx.hypot(dy)), (1, dy)]);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meridian_radius_is_read_signed() {
        let o = [0.0; 3];
        assert_eq!(readings(Point3::new(0.0, -3.0, 1.0), o, false), vec![(2, 1.0), (0, -3.0)]);
        assert_eq!(readings(Point3::new(4.0, 0.0, 0.0), o, false), vec![(2, 0.0), (0, 4.0)]);
        assert_eq!(readings(Point3::new(-3.0, 4.0, 0.0), o, true), vec![(2, 0.0), (0, 5.0)]);
        let off = readings(Point3::new(-3.0, 4.0, 0.0), o, false);
        assert!(off.contains(&(0, -3.0)) && off.contains(&(0, 5.0)) && off.contains(&(1, 4.0)));
    }
}
