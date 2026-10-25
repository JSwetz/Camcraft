//! Law (c3): the mid-turn sweep (M5.1 stage G).
//!
//! Provenance: the coordinated-motion reachable set - every axis
//! group's interpolation law is "each family runs its own clock and
//! holds at arrival", the controller model stated in public G-code
//! manuals' descriptions of simultaneous linear+rotary blocks
//! (behavior only, per the Clean-Room Protocol). The enclosure test
//! reuses law (c)'s column exactly; the grid is ours, and its limits
//! are stated where it runs.
//!
//! Law (c) judges the band stack at the poses each move STATES. That
//! was the whole story while every move stated home vertical. Since
//! stage F a move can command a new A/B/C pose - and a coordinated
//! block drives the table and the head AT ONCE, so between the start
//! and end poses the machine passes through every lerped letter
//! combination, at every point along the path, in EITHER family's
//! order of arrival. A wall both endpoint poses clear can still be
//! struck mid-turn: the column swings like a metronome while the tip
//! walks. And the endpoints themselves are not enough even at fixed
//! XY - a plunge that turns while it descends slides its lean column
//! through a Z window that is the union (a superset) of the two
//! endpoints' windows. This law names what meets steel in between.

use crate::cl::{Move, MoveKind, Toolpath, angles_to_axis, axis_to_angles, is_vertical, normalize_axis};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::laws::dist2_seg;
use super::types::Warning;

/// Path fractions sampled INSIDE the move (its endpoints are law (c)'s
/// domain - the to-point at the to-pose is (c) verbatim, and the
/// from-point was the previous move's to-point). Four points: the
/// same coarse-by-design sampling the rest of verify lives with, and
/// no fewer on a straight move, where (c) otherwise sees only the
/// endpoint. A pure-Z move keeps its XY and these become its Z
/// stations - the sliding-window case above.
const PATH_FRACS: [f64; 4] = [0.2, 0.4, 0.6, 0.8];
/// Pose fractions through the turn, endpoints INCLUDED: the head may
/// finish its arc before the table arrives (its leg is the short one
/// on a coordinated move), so "at the to-pose but not yet at the
/// to-point" is reachable, and on a straight move law (c) - which
/// samples the to-point only - would otherwise never see it.
const POSE_FRACS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// The reachable-set sweep of one path: for every cut move that
/// CHANGES the commanded pose (the modal law threaded through the
/// path itself - rapids hold, an unposed cut is home), test the band
/// stack's law-(c) column over a grid of (path position, turn
/// fraction). Moves law (c) already convicted are skipped: their
/// same-band verdict is spoken, and this law exists for what (c)
/// finds clean. One [`Warning::SweepHit`] per move; part material
/// outranks stock, like (c)'s worst-offender rule.
#[allow(clippy::too_many_arguments)]
pub(super) fn mid_turn_sweeps(
    path: &Toolpath,
    bands: &[(f64, f64, f64)],
    r: f64,
    gouge_tol: f64,
    chord_tol: f64,
    shanked: &[usize],
    samples: &[Point2],
    in_stock: &dyn Fn(Point2) -> bool,
    surface: &dyn Fn(Point2) -> f64,
    part_at: &dyn Fn(Point2) -> bool,
) -> Vec<Warning> {
    let mut warns = Vec::new();
    // The held letters, exactly as `sim::script` threads them (the
    // same modal law, the same canonical form): a path's first move
    // turns FROM home - the cross-operation swing belongs to the
    // document's replay timeline, not to one path's verdict, and the
    // connector rapids between operations command no angle anyway.
    let mut held = [0.0f64; 3];
    for (i, m) in path.moves.iter().enumerate() {
        let next = if m.kind == MoveKind::Rapid || !(m.from.is_finite() && m.to.is_finite()) {
            held
        } else {
            m.axis
                .and_then(normalize_axis)
                .map_or([0.0; 3], axis_to_angles)
        };
        if m.kind != MoveKind::Rapid && held != next && !shanked.contains(&i) {
            if let Some(part) = swing_hit(m, held, next, bands, r, gouge_tol, chord_tol, samples, in_stock, surface, part_at) {
                warns.push(Warning::SweepHit { at: i, part });
            }
        }
        held = next;
    }
    warns
}

/// Does the swing's reachable set meet steel? `Some(true)`: part
/// material (worst offender, search ended); `Some(false)`: stock
/// only; `None`: clean. The column stands at the sampled tip with
/// the lerped-pose lean - law (c)'s enclosure formula, pose by pose.
///
/// The 40 placements (4 path stations x 5 turn fractions x the live
/// bands) are enumerated once into a state list, and the list's own
/// bounding box filters the sample columns first: a band can only
/// touch a column its state's disc covers, and on a real fixture the
/// far-field majority of samples is outside every disc at once. The
/// grid's price is then 20x law (c)'s reach, not 20x its sample
/// count - which is what lets (c3) run for the Envelope programs,
/// where the posed kinds live.
#[allow(clippy::too_many_arguments)]
fn swing_hit(
    m: &Move,
    from_abc: [f64; 3],
    to_abc: [f64; 3],
    bands: &[(f64, f64, f64)],
    r: f64,
    gouge_tol: f64,
    chord_tol: f64,
    samples: &[Point2],
    in_stock: &dyn Fn(Point2) -> bool,
    surface: &dyn Fn(Point2) -> f64,
    part_at: &dyn Fn(Point2) -> bool,
) -> Option<bool> {
    // (pc, r2, h0): the band's column, its outer reach squared, its
    // bottom. Plus the union box of every disc, for the filter.
    let mut states: Vec<(Point2, f64, f64)> = Vec::new();
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &t in &PATH_FRACS {
        // XY walks the chord sequence (an arc's sweep is its chords,
        // the discretization every crossing/chord law lives with);
        // Z rides the move's own from-to lerp at the same parameter.
        let p = path_point(m, t, chord_tol);
        for &g in &POSE_FRACS {
            let abc = [
                from_abc[0] + (to_abc[0] - from_abc[0]) * g,
                from_abc[1] + (to_abc[1] - from_abc[1]) * g,
                from_abc[2] + (to_abc[2] - from_abc[2]) * g,
            ];
            let a = angles_to_axis(abc);
            for &(above, band_r, band_len) in bands {
                if band_r <= r || band_len <= 0.0 {
                    continue;
                }
                let (offx, offy, r_band, h0) = match a {
                    Some(a) if !is_vertical(&a) => {
                        let mid = above + band_len * 0.5;
                        let spread = (band_len * 0.5) * (a[0] * a[0] + a[1] * a[1]).sqrt();
                        let za = p.z + above * a[2];
                        let zb = p.z + (above + band_len) * a[2];
                        (mid * a[0], mid * a[1], band_r + spread, za.min(zb) - band_r)
                    }
                    _ => (0.0, 0.0, band_r, p.z + above),
                };
                let pc = Point2::new(p.x + offx, p.y + offy);
                states.push((pc, (r_band + 1e-9) * (r_band + 1e-9), h0));
                x0 = x0.min(pc.x - r_band);
                x1 = x1.max(pc.x + r_band);
                y0 = y0.min(pc.y - r_band);
                y1 = y1.max(pc.y + r_band);
            }
        }
    }
    if states.is_empty() {
        return None; // every band is the tool's own radius: nothing to sweep
    }
    let near: Vec<Point2> = samples
        .iter()
        .copied()
        .filter(|q| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1)
        .collect();
    let inner2 = (r + gouge_tol) * (r + gouge_tol);
    let mut worst = None;
    for &(pc, r2, h0) in &states {
        for q in &near {
            let dd = dist2_seg(*q, pc, pc);
            if dd > r2 || dd <= inner2 {
                continue; // outside this band's annulus
            }
            if !in_stock(*q) {
                continue;
            }
            if h0 < surface(*q) {
                if part_at(*q) {
                    return Some(true); // report the worst offender
                }
                worst = Some(false);
            }
        }
    }
    worst
}

/// The point `t` along the move: XY by cumulative chord walk, Z on
/// the move's own linear ramp. A zero-XY move keeps its column and
/// the Z lerp does the work - which is the whole point for a plunge
/// that turns while it descends.
fn path_point(m: &Move, t: f64, chord_tol: f64) -> Point3 {
    let chords = m.chords(chord_tol);
    let mut total = 0.0;
    for (a, b) in &chords {
        total += ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
    }
    let z = m.from.z + (m.to.z - m.from.z) * t;
    if !(total > 0.0) || chords.is_empty() {
        return Point3::new(m.from.x, m.from.y, z);
    }
    let target = t * total;
    let mut acc = 0.0;
    for (a, b) in &chords {
        let l = ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
        if acc + l >= target {
            let f = if l > 0.0 {
                ((target - acc) / l).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Point3::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, z);
        }
        acc += l;
    }
    Point3::new(m.to.x, m.to.y, z)
}
