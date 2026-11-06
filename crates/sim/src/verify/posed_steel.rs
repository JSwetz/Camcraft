//! The posed sweep of the fixture laws (c2)/(d2) (M13.8): every move
//! the machine runs with its tool off the upright - a radial retract,
//! an index on the retract cylinder, a plunge back in, a tilted cut -
//! swept with its REAL pose against the fixture boxes, the whole stack
//! (flute, shank, holder, the spindle housing when stated) as tilted
//! capsules along the pose, not a vertical column standing above the
//! tip. A tool pointing down under a bar's equator reaches toward the
//! chuck, the tailstock, the table; the column enclosures of
//! [`super::laws`] look upward from the move's deepest point and the
//! rapid enclosure reads only the flute, so a holder swept through a
//! jaw on an index passed silent. This law closes that.
//!
//! The pose along a move is the controller's: the move starts on the
//! pose held coming in (the tilt is modal - a move stating nothing
//! keeps it) and ends on the one it states, the letters lerped
//! between ([`crate::cl::pose`]), the tip on its straight line (an
//! arc on its own 3-D chords). Each move is cut into intervals no
//! longer than [`STEP`] of travel at the stack's far end; per interval
//! the stack is placed at the midpoint and every capsule's radius
//! grows by the farthest any of its points travels to either interval
//! end (tip travel plus reach times the pose's turn) - so the capsules
//! ENCLOSE the swept volume: an over-report of a graze, never a hidden
//! crash. A capsule encloses its cylinder (its caps reach one radius
//! past each end), the same err-toward-HIT direction. The segment-box
//! distance is exact: the distance to a convex box along a segment is
//! convex in the segment parameter, minimized by ternary search.
//!
//! Upright moves (held and stated pose both upright, or unstated) are
//! not this law's: the prismatic laws keep them byte-identical.
//!
//! Provenance: swept-volume enclosure by bounded sampling and
//! capsule/AABB distance are textbook computational geometry (Ericson,
//! "Real-Time Collision Detection", public material); the modal pose
//! law is RS-274's. Our own design; no vendor source consulted.

use crate::cl::{angles_to_axis, axis_to_angles, is_vertical, MoveKind, Toolpath};
use crate::fixture::{FixtureBox, FIXTURE_HIT_TOL};
use crate::geometry::Point3;
use crate::tools::ToolGeom;

use super::types::FixturePart;

/// Interval travel cap at the stack's far end, mm.
const STEP: f64 = 0.5;
/// Intervals per piece cap (a longer piece is cut coarser - the
/// growth term still encloses it - never an unbounded loop).
const MAX_STEPS: usize = 4096;
/// Ternary-search rounds for the segment-box distance: (2/3)^90 of
/// the segment, far below the hit tolerance on any real move.
const ROUNDS: usize = 90;

/// Per move, the element a POSED move strikes a fixture with (first in
/// the order shank, holder, spindle, flute), None where it strikes
/// nothing or is not posed. `rapids` picks the move kinds judged: the
/// rapids (law d2, reported as [`FixturePart::Rapid`]) or every other
/// move (law c2).
pub(super) fn posed_hits(
    path: &Toolpath,
    tool: &ToolGeom,
    fixtures: &[FixtureBox],
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
    rapids: bool,
) -> Vec<Option<FixturePart>> {
    let mut out = vec![None; path.moves.len()];
    if fixtures.is_empty() {
        return out;
    }
    let (fl, sl, hl) = (tool.flute_len, tool.shank_len, tool.holder_len);
    let mut stack: Vec<(FixturePart, f64, f64, f64)> = vec![
        (FixturePart::Shank, fl, fl + sl, tool.shank_radius),
        (FixturePart::Holder, fl + sl, fl + sl + hl, tool.holder_radius),
    ];
    if let Some((sr, slen)) = spindle {
        stack.push((FixturePart::Spindle, fl + sl + hl, fl + sl + hl + slen, sr));
    }
    stack.push((FixturePart::Flute, 0.0, fl, tool.radius));
    // A band of no length is no steel; a non-finite one fails closed
    // (kept, and the distance law reads NaN as a hit).
    stack.retain(|&(_, lo, hi, r)| hi - lo > 0.0 || !(lo.is_finite() && hi.is_finite() && r.is_finite()));
    let reach = stack.iter().map(|e| e.2).fold(0.0f64, |a, b| if b.is_finite() { a.max(b) } else { a });
    let tol = if chord_tol.is_finite() && chord_tol > 0.0 { chord_tol } else { 0.01 };
    let upright = |a: Option<[f64; 3]>| a.is_none_or(|a| is_vertical(&a));
    let mut held: Option<[f64; 3]> = path.moves.first().and_then(|m| m.axis);
    for (i, m) in path.moves.iter().enumerate() {
        let from_pose = held;
        let to_pose = m.axis.or(held);
        held = to_pose;
        if (m.kind == MoveKind::Rapid) != rapids || (upright(from_pose) && upright(to_pose)) {
            continue;
        }
        let la = axis_to_angles(from_pose.unwrap_or([0.0, 0.0, 1.0]));
        let lb = axis_to_angles(to_pose.unwrap_or([0.0, 0.0, 1.0]));
        let pieces = m.segments3(tol);
        let total: f64 = pieces.iter().map(|(a, b)| dist(*a, *b)).sum();
        let mut done = 0.0f64;
        let mut struck: Option<FixturePart> = None;
        'pieces: for (pa, pb) in &pieces {
            let len = dist(*pa, *pb);
            let frac = |s: f64| if total > 0.0 { (done + len * s) / total } else { s };
            let turn = angle(pose(la, lb, frac(0.0)), pose(la, lb, frac(1.0)));
            let span = len + reach * turn;
            let n = if span.is_finite() { ((span / STEP).ceil() as usize).clamp(1, MAX_STEPS) } else { 1 };
            for j in 0..n {
                let (t0, t1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
                let tm = 0.5 * (t0 + t1);
                let (p0, p1, pm) = (lerp(*pa, *pb, t0), lerp(*pa, *pb, t1), lerp(*pa, *pb, tm));
                let (a0, a1, am) = (pose(la, lb, frac(t0)), pose(la, lb, frac(t1)), pose(la, lb, frac(tm)));
                let travel = dist(pm, p0).max(dist(pm, p1));
                let swing = angle(am, a0).max(angle(am, a1));
                for &(part, lo, hi, r) in &stack {
                    let grow = travel + hi.abs() * swing;
                    let c0 = add(pm, am, lo);
                    let c1 = add(pm, am, hi);
                    if fixtures.iter().any(|b| capsule_hits(c0, c1, r + grow, b)) {
                        struck = Some(if rapids { FixturePart::Rapid } else { part });
                        break 'pieces;
                    }
                }
            }
            done += len;
        }
        out[i] = struck;
    }
    out
}

/// Does the capsule `c0`-`c1` of radius `r` meet the box? Distance
/// within `r` (plus the hit tolerance) counts; anything non-finite
/// fails closed.
fn capsule_hits(c0: Point3, c1: Point3, r: f64, b: &FixtureBox) -> bool {
    if !(c0.is_finite() && c1.is_finite() && r.is_finite() && b.min.is_finite() && b.max.is_finite()) {
        return true;
    }
    let lo = [b.min.x.min(b.max.x), b.min.y.min(b.max.y), b.min.z.min(b.max.z)];
    let hi = [b.max.x.max(b.min.x), b.max.y.max(b.min.y), b.max.z.max(b.min.z)];
    let (p, q) = ([c0.x, c0.y, c0.z], [c1.x, c1.y, c1.z]);
    let lim = r.max(0.0) + FIXTURE_HIT_TOL;
    // Quick reject: the segment's box grown by the radius misses.
    for k in 0..3 {
        if p[k].min(q[k]) - lim > hi[k] || p[k].max(q[k]) + lim < lo[k] {
            return false;
        }
    }
    let d2 = |t: f64| {
        (0..3)
            .map(|k| {
                let x = p[k] + (q[k] - p[k]) * t;
                let e = if x < lo[k] { lo[k] - x } else if x > hi[k] { x - hi[k] } else { 0.0 };
                e * e
            })
            .sum::<f64>()
    };
    let (mut a, mut z) = (0.0f64, 1.0f64);
    for _ in 0..ROUNDS {
        let m1 = a + (z - a) / 3.0;
        let m2 = z - (z - a) / 3.0;
        if d2(m1) <= d2(m2) {
            z = m2;
        } else {
            a = m1;
        }
    }
    let best = d2(0.0).min(d2(1.0)).min(d2(0.5 * (a + z)));
    best <= lim * lim
}

/// The axis the letters hold `s` of the way from `a` to `b` (each
/// letter on its own straight line); upright when no vector holds them.
fn pose(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    let l = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s, a[2] + (b[2] - a[2]) * s];
    angles_to_axis(l).unwrap_or([0.0, 0.0, 1.0])
}

/// The angle between two unit axes, radians (the chord bound of a
/// point's travel at unit reach).
fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    let t = d.acos();
    if t.is_finite() { t } else { std::f64::consts::PI }
}

fn add(p: Point3, a: [f64; 3], s: f64) -> Point3 {
    Point3::new(p.x + a[0] * s, p.y + a[1] * s, p.z + a[2] * s)
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}

fn dist(a: Point3, b: Point3) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt()
}
