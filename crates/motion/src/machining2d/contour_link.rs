//! How a multi-depth contour moves from one level to the next - the
//! user's choice, never a premade one. Provenance: the two standard
//! multi-depth profile transitions of published CAM practice - lead
//! out, retract, traverse and re-enter by the chosen entry; or stay at
//! depth and ramp down along the profile itself at a stated maximum
//! ramp angle (the cutter walks the ring it just finished, so the
//! center never leaves the proven ring). RS-274 / ISO 6983 linear
//! interpolation for the ramp's 3-axis feeds. No vendor material.

use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::contour::ContourParams;
use super::contour_depth::Ladder;
use super::contour_entry::say;
use super::ring::dist;

/// The level-to-level transition of a contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LevelLink {
    /// Lead out, lift to the retract (hop) plane, move over, and enter
    /// the next level by the chosen Entry - every pre-option file.
    #[default]
    RetractReenter,
    /// Stay at depth: skip the lead-out, ramp down along the ring just
    /// cut at no more than the ramp angle, and walk the next level from
    /// the seam the ramp lands on. Where it cannot be proved (tabbed
    /// levels, a ring too short for the angle, another ring cut in
    /// between) that level retracts and re-enters instead, by name.
    StayDownRamp,
}

/// Default maximum ramp angle (degrees) of the stay-down ramp: a gentle
/// helical-along-the-wall descent a flat end mill takes without a
/// center-cutting plunge.
pub const DEFAULT_LEVEL_RAMP_ANGLE: f64 = 3.0;
/// Laps of the ring a single ramp may take; past this the ring is too
/// short for the angle and the level retracts instead.
const MAX_RAMP_LAPS: usize = 64;

/// Laps the ramp needs to descend `dz` along a ring of length `len` at
/// no more than `angle_deg`; `None` when the ring cannot hold it.
pub(super) fn ramp_laps(len: f64, dz: f64, angle_deg: f64) -> Option<usize> {
    let tan = angle_deg.to_radians().tan();
    if !(len.is_finite() && len > 1e-6 && dz.is_finite() && dz >= 0.0 && tan.is_finite() && tan > 0.0) {
        return None;
    }
    let run = dz / tan;
    let laps = (run / len).ceil().max(1.0);
    (laps <= MAX_RAMP_LAPS as f64).then_some(laps as usize)
}

/// Push the ramp: from the ring's seam `poly[0]` at `z_from`, `laps`
/// times around the closed polyline `poly`, descending linearly in arc
/// length to `z_to` exactly at the seam. Every feed point lies on the
/// ring, so the center never leaves the path the level law proved.
pub(super) fn push_ramp(b: &mut PathBuilder, poly: &[Point2], laps: usize, z_from: f64, z_to: f64, feed: f64) {
    let n = poly.len();
    if n < 2 {
        return;
    }
    let lap_len: f64 = poly.windows(2).map(|w| dist(w[0], w[1])).sum();
    let total = lap_len * laps as f64;
    if !(total.is_finite() && total > 0.0) {
        return;
    }
    let mut s = 0.0;
    for lap in 0..laps {
        for (k, w) in poly.windows(2).enumerate() {
            s += dist(w[0], w[1]);
            let last = lap + 1 == laps && k + 2 == n;
            let z = if last { z_to } else { z_from - (z_from - z_to) * (s / total) };
            b.push(MoveKind::Feed, Point3::new(w[1].x, w[1].y, z), Some(feed));
        }
    }
}

/// The cut order and its stay-down links. Level by level (every ring at
/// a level, then the next level) under retract-and-re-enter; ring by
/// ring under the stay-down ramp, each ring all the way down before the
/// next, so a ring's levels follow one another and the ramp can link
/// them. `ramp[i]` is the lap count of the ramp onto item `i`, `None`
/// where item `i` is entered from above; every stay-down level that
/// cannot ramp is named in `notes`.
pub(super) fn plan_links(
    p: &ContourParams,
    lad: &Ladder,
    plan: &[Vec<Point2>],
    notes: &mut Vec<String>,
) -> (Vec<(f64, usize)>, Vec<Option<usize>>) {
    let is_tabbed = |z: f64| p.tab_count > 0 && z < p.bottom_z + p.tab_height;
    let loops = lad.per_ring.len();
    // The cut order: level by level (every ring at a level, then the
    // next level) - or, staying down, ring by ring, each ring all the
    // way down before the next, so a ring's levels follow one another
    // and the ramp can link them.
    let sched: Vec<(f64, usize)> = if p.level_link == LevelLink::StayDownRamp {
        (0..loops).flat_map(|li| lad.ring(li).iter().map(move |&z| (z, li))).collect()
    } else {
        lad.zs.iter()
            .flat_map(|&z| (0..loops).filter(move |&li| lad.walks(li, z)).map(move |li| (z, li)))
            .collect()
    };
    // Where the ramp links item i to item i - 1: the same ring, its
    // previous level just cut, neither level tabbed, and a ring long
    // enough for the angle. Every other level change of a stay-down
    // contour retracts and re-enters, and says why.
    let mut ramp: Vec<Option<usize>> = vec![None; sched.len()];
    if p.level_link == LevelLink::StayDownRamp {
        for i in 1..sched.len() {
            let ((z, li), (zp, lp)) = (sched[i], sched[i - 1]);
            if li != lp {
                continue;
            }
            let at = plan[li].first().copied().unwrap_or(Point2::new(0.0, 0.0));
            if is_tabbed(z) || is_tabbed(zp) {
                say(
                    notes,
                    format!(
                        "level link: no stay-down ramp onto Z {z:.3} on the ring at ({:.3}, {:.3}) - \
                         a standing tab is material on the ring; retract and re-enter there",
                        at.x, at.y
                    ),
                );
                continue;
            }
            let len: f64 = plan[li].windows(2).map(|w| dist(w[0], w[1])).sum();
            match ramp_laps(len, zp - z, p.ramp_angle) {
                Some(n) => ramp[i] = Some(n),
                None => say(
                    notes,
                    format!(
                        "level link: no stay-down ramp onto Z {z:.3} on the ring at ({:.3}, {:.3}) - \
                         the ring is too short for a {:.1} deg ramp over {:.3} mm; retract and \
                         re-enter there",
                        at.x,
                        at.y,
                        p.ramp_angle,
                        zp - z
                    ),
                ),
            }
        }
    }
    (sched, ramp)
}
