//! The fixture mesh law for POSED and TURNED moves (#339 slice 3): a
//! 4/5-axis index, a tilted cut, a rotary's index under fixed steel,
//! a spindle-C live-tool cut. Slice 1 ([`super::mesh_steel`]) reads
//! upright moves against a fixture's closed mesh; here the stack leans
//! along a pose that changes across the move, so the swept volume is
//! no longer one convex sum - it is enclosed, piece by piece, in convex
//! sums GJK can prove clear.
//!
//! ## The enclosure (why a clear proof is a real clearance)
//!
//! A move is walked in stretches (the posed sweep's own intervals,
//! [`super::midpose::turn_intervals`], cut by the pose's turn only).
//! Over one stretch the tip stays on the straight segment `p0..p1`
//! (within `chord` on an arc's chord piece) and the pose `a(t)` within
//! `swing` radians of the stretch's `axis` (midpose's proven letter-rate
//! bound). Write a band point as `p(t) + Q(t) y`, `y` in the band's
//! cylinder about `axis` and `Q(t)` the least rotation carrying `axis`
//! onto `a(t)`: `|Q(t) y - y| <= swing |y|` (a turn by θ moves a point
//! at most θ times its distance from the turn's centre, arc >= chord),
//! and `|y| <= hypot(max(|lo|, |hi|), r)`. So every true band point
//! lies in `segment ⊕ cylinder(axis) ⊕ ball(pad)`, pad = `chord +
//! swing * hypot(max(|lo|, |hi|), r)` - a convex set whose segment and
//! cylinder are swept EXACTLY by [`FixtureMesh::band_meets_along`] and
//! whose ball rides on its margin. No capsule: the flute's end stays a
//! flat disc, so a tilted tool over a vise floor is not convicted by a
//! ball reaching below its tip (slice 1's reason for the swept band).
//!
//! **A rigid turn on top** (steel in another frame): a stretch carried
//! by a turn `T(φ)` about a line, where the true turn strays at most
//! `dth` from the stretch's `φm`: `T(φ) x = T(φm) T(φ - φm) x` and
//! `|T(δ) x - x| <= |δ| ρ(x)`, ρ the distance from the line. Distance
//! to a line is convex, so over the parallelogram (segment ⊕ axial
//! segment) it peaks at one of its four corners; the disc and the ball
//! add at most `r + pad`. The turned set is `T(φm)` of the stretch's
//! set (a rigid map of a Minkowski sum is the sum of the maps) grown by
//! `dth * ρmax` - again segment ⊕ cylinder ⊕ ball.
//!
//! **Inside the solid.** Each stretch's set is convex; proven clear of
//! every facet it lies wholly in or wholly out of the closed solid, and
//! consecutive stretches overlap (both hold the band at their shared
//! instant), so one point decides the whole band - tested per band.
//!
//! ## The frames (which steel is read where)
//!
//! - **Posed, the part not turned by C** - [`judge_posed`]: steel that
//!   rides the part, or any steel with no rotary axis stated, in the
//!   path frame (as the box laws read it). Steel declared FIXED under a
//!   rotary op's axis: in the machine frame, each stretch carried by
//!   the turn standing its pose upright ([`super::posed_steel`]'s own
//!   `upright_turn`, with its `dth` = the end turns' spread, never under
//!   the swing) - the very frame [`super::workholding`] judges those
//!   boxes in. A pose the rotary alone cannot stand upright: unjudged,
//!   the box verdict stands (the box law falls back the same way).
//! - **Spindle C turning the part** - [`judge_c`]: by `cl::rotary`'s
//!   stated convention the part turns on the world Z line so the tip
//!   lands on the fixed live-tool station (+X of that line, tool axis
//!   +X), X/Z/C linear between blocks. The line is not in the path, so
//!   it is recovered per move from the two stated C angles: the tip
//!   stands at azimuth `-C` about it at both ends, two rays that meet
//!   at the line when C turns (by more than [`MIN_TURN`]). The machine
//!   frame motion is then exact (a straight tip run, the station
//!   pose), and fixed steel (stated at C = 0) is judged there in one sweep;
//!   steel riding the part is judged in the part frame, the machine
//!   stretches carried by `-C` about the line (the spiral the machine
//!   really traces, not the chord the CL states). The station pose is
//!   the convention's, not the stated axis: `cl::stamp_c` splits a
//!   move into pieces that keep the whole move's axis, off the station
//!   by the rest of the turn. A C held without turning: steel riding
//!   the part sees a straight run at the station pose turned by `-C`;
//!   fixed steel needs the line and is unjudged. A C move whose
//!   starting C is not the previous move's stated one (a section's
//!   first move, a jump), a pose that is not horizontal (not the
//!   radial station tool), an arc, or a turn too small to place the
//!   line: unjudged.
//!
//! Provenance: GJK and the swept band are [`crate::fixture_mesh`]'s
//! (Gilbert-Johnson-Keerthi 1988; van den Bergen 1999); the enclosure
//! of a swept rigid body by bounded rotation is textbook (Ericson,
//! "Real-Time Collision Detection", 2005; swept-volume bounds as in
//! Abdel-Malek et al., "Swept volumes: foundations, perspectives, and
//! applications", 2006); rigid turns by Rodrigues' formula; the spindle
//! C convention is `cl::rotary`'s (RS-274 C about Z). Our own design;
//! no vendor source consulted.

use crate::cl::Move;
use crate::fixture::FIXTURE_HIT_TOL;
use crate::fixture_mesh::FixtureMesh;
use crate::geometry::Point3;

use super::mesh_steel::{chords, label, MeshSteel, Read};
use super::midpose;
use super::posed_steel::{off_axis, turn_about, upright_turn, wrap};
use super::rotary::RotaryAxis;

/// Far-end travel per C stretch, mm (the posed sweep's interval step).
const STEP: f64 = 0.5;
/// Stretches per C move cap: past it each stretch's pad grows instead.
const MAX_STEPS: usize = 4096;
/// The least C turn (radians) the spindle line is recovered from; a
/// smaller one places it too poorly to bound the spiral.
const MIN_TURN: f64 = 1e-4;
/// The live-tool station's axis in the machine frame (`cl::rotary`).
const STATION: [f64; 3] = [1.0, 0.0, 0.0];
/// How far off horizontal a C move's stated pose may sit and still be
/// the radial station tool (`cl::rotary`'s own station tolerance).
const RADIAL_TOL: f64 = 1e-3;

/// A stretch of a move: the tip on `p0..p1` (the true path within
/// `chord`), the pose within `swing` radians of the unit `axis`.
#[derive(Clone, Copy, Debug)]
struct Stretch {
    p0: Point3,
    p1: Point3,
    axis: [f64; 3],
    swing: f64,
    chord: f64,
}

/// The rigid turn a stretch is carried by into the steel's frame: `th`
/// about `ax`, the true turn within `dth` of it over the stretch.
#[derive(Clone, Copy, Debug)]
struct Turn {
    ax: RotaryAxis,
    th: f64,
    dth: f64,
}

/// A posed move not turned by C: path frame, or the machine frame of a
/// rotary turning the part about `frame` (steel declared fixed).
pub(super) fn judge_posed(
    mesh: &FixtureMesh,
    m: &Move,
    (from, to): (Option<[f64; 3]>, Option<[f64; 3]>),
    steel: &MeshSteel<'_>,
    rapids: bool,
    frame: Option<&RotaryAxis>,
) -> Read {
    let up = [0.0, 0.0, 1.0];
    let (Some(a), Some(b)) = (unit(from.unwrap_or(up)), unit(to.unwrap_or(up))) else {
        return Read::Unjudged;
    };
    let tol = tol_of(steel);
    let Some(segs) = chords(m, tol) else { return Read::Unjudged };
    let chord = segs.iter().map(|s| s.2).fold(0.0f64, f64::max);
    let ivs = midpose::turn_intervals(m, a, b, far_reach(steel), tol);
    if ivs.is_empty() {
        return Read::Unjudged;
    }
    let mut out = Vec::with_capacity(ivs.len());
    for iv in &ivs {
        let st = Stretch { p0: iv.p0, p1: iv.p1, axis: iv.am, swing: iv.swing, chord };
        let turn = match frame {
            None => None,
            Some(ax) => match (upright_turn(ax, iv.am), upright_turn(ax, iv.a0), upright_turn(ax, iv.a1)) {
                (Some(th), Some(t0), Some(t1)) => {
                    Some(Turn { ax: *ax, th, dth: wrap(th - t0).abs().max(wrap(th - t1).abs()).max(iv.swing) })
                }
                _ => return Read::Unjudged,
            },
        };
        out.push((st, turn));
    }
    judge_stretches(mesh, &out, steel, rapids)
}

/// A move under the spindle C (module header): `c0` the C the previous
/// move ended on (None: not known), `pose` the tool pose it holds, and
/// `fixed` whether the steel stays put while the part turns.
pub(super) fn judge_c(
    mesh: &FixtureMesh,
    m: &Move,
    (c0, pose): (Option<f64>, Option<[f64; 3]>),
    steel: &MeshSteel<'_>,
    rapids: bool,
    fixed: bool,
) -> Read {
    let (Some(c0), Some(c1)) = (c0, m.rotary) else { return Read::Unjudged };
    if m.arc.is_some() || !(c0.is_finite() && c1.is_finite() && m.from.is_finite() && m.to.is_finite()) {
        return Read::Unjudged;
    }
    // The station tool lies radial - horizontal. A pose that is not
    // (an axial live tool, a lean) is not the tool this law places.
    if !pose.and_then(unit).is_some_and(|a| a[2].abs() <= RADIAL_TOL) {
        return Read::Unjudged;
    }
    let (r0, r1) = (-c0.to_radians(), -c1.to_radians());
    if c0 == c1 {
        if fixed {
            return Read::Unjudged; // the line is needed, and nothing places it
        }
        let axis = [r0.cos(), r0.sin(), 0.0];
        let st = Stretch { p0: m.from, p1: m.to, axis, swing: 0.0, chord: 0.0 };
        return judge_stretches(mesh, &[(st, None)], steel, rapids);
    }
    // The line: from = c + rho0 u0, to = c + rho1 u1 (XY), u = the tip's
    // azimuth -C. Solved by 2-D cross products; both rays forward.
    let (u0, u1) = ([r0.cos(), r0.sin()], [r1.cos(), r1.sin()]);
    let cr = |p: [f64; 2], q: [f64; 2]| p[0] * q[1] - p[1] * q[0];
    let den = cr(u0, u1);
    if den.is_nan() || den.abs() <= MIN_TURN.sin() || (c1 - c0).abs().to_radians() < MIN_TURN {
        return Read::Unjudged;
    }
    let w = [m.from.x - m.to.x, m.from.y - m.to.y];
    let (rho0, rho1) = (cr(w, u1) / den, cr(w, u0) / den);
    let dust = 1e-9 * (1.0 + m.from.x.abs().max(m.from.y.abs()).max(m.to.x.abs()).max(m.to.y.abs()));
    if !(rho0.is_finite() && rho1.is_finite() && rho0 >= -dust && rho1 >= -dust) {
        return Read::Unjudged;
    }
    let (rho0, rho1) = (rho0.max(0.0), rho1.max(0.0));
    let (cx, cy) = (m.from.x - rho0 * u0[0], m.from.y - rho0 * u0[1]);
    // The machine frame: the tip on the station's +X ray of the line.
    let m0 = Point3::new(cx + rho0, cy, m.from.z);
    let m1 = Point3::new(cx + rho1, cy, m.to.z);
    if fixed {
        let st = Stretch { p0: m0, p1: m1, axis: STATION, swing: 0.0, chord: 0.0 };
        return judge_stretches(mesh, &[(st, None)], steel, rapids);
    }
    let ax = RotaryAxis { origin: Point3::new(cx, cy, 0.0), dir: [0.0, 0.0, 1.0] };
    let turn = (c1 - c0).to_radians();
    let far = rho0.max(rho1) + far_reach(steel);
    let span = turn.abs() * far;
    let n = if span.is_finite() { ((span / STEP).ceil() as usize).clamp(1, MAX_STEPS) } else { 1 };
    let mut out = Vec::with_capacity(n);
    for j in 0..n {
        let (s0, s1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
        let cm = c0 + (c1 - c0) * (0.5 * (s0 + s1));
        let st = Stretch { p0: lerp(m0, m1, s0), p1: lerp(m0, m1, s1), axis: STATION, swing: 0.0, chord: 0.0 };
        out.push((st, Some(Turn { ax, th: -cm.to_radians(), dth: 0.5 * turn.abs() / n as f64 })));
    }
    judge_stretches(mesh, &out, steel, rapids)
}

/// Every band (the stack's order) along every stretch: the first band
/// whose enclosure is not proven clear is the hit; then one inside test
/// per band. Clear only on proof.
fn judge_stretches(mesh: &FixtureMesh, stretches: &[(Stretch, Option<Turn>)], steel: &MeshSteel<'_>, rapids: bool) -> Read {
    let Some(first) = stretches.first() else { return Read::Unjudged };
    for b in steel.stack {
        if !(b.lo.is_finite() && b.hi.is_finite() && b.r.is_finite()) {
            return label(b.part, rapids);
        }
        if b.hi - b.lo <= 0.0 {
            continue;
        }
        for (st, turn) in stretches {
            if enclosure_meets(mesh, st, turn.as_ref(), b.lo, b.hi, b.r) {
                return label(b.part, rapids);
            }
        }
    }
    let (st, turn) = first;
    for b in steel.stack.iter().filter(|b| b.hi - b.lo > 0.0) {
        let mid = 0.5 * (b.lo + b.hi);
        let probe = add(lerp(st.p0, st.p1, 0.5), st.axis, mid);
        let probe = turn.map_or(probe, |t| turn_about(&t.ax, probe, t.th));
        if mesh.contains(probe) {
            return label(b.part, rapids);
        }
    }
    Read::Clear
}

/// Does one band's enclosure over one stretch (module header) come
/// within the hit tolerance of the mesh? Non-finite reads as a hit.
fn enclosure_meets(mesh: &FixtureMesh, st: &Stretch, turn: Option<&Turn>, lo: f64, hi: f64, r: f64) -> bool {
    let pad = st.chord + st.swing * lo.abs().max(hi.abs()).hypot(r);
    let Some(t) = turn else {
        return mesh.band_meets_along(st.p0, st.p1, st.axis, lo, hi, r, FIXTURE_HIT_TOL + pad);
    };
    let corners = [add(st.p0, st.axis, lo), add(st.p0, st.axis, hi), add(st.p1, st.axis, lo), add(st.p1, st.axis, hi)];
    let rhos = corners.map(|c| off_axis(&t.ax, c));
    if !rhos.iter().all(|d| d.is_finite()) {
        return true;
    }
    let rho = rhos.into_iter().fold(0.0f64, f64::max);
    let grow = t.dth.abs() * (rho + r + pad);
    let (p0, p1) = (turn_about(&t.ax, st.p0, t.th), turn_about(&t.ax, st.p1, t.th));
    let tip = turn_about(&t.ax, add(st.p0, st.axis, 1.0), t.th);
    let axis = [tip.x - p0.x, tip.y - p0.y, tip.z - p0.z];
    mesh.band_meets_along(p0, p1, axis, lo, hi, r, FIXTURE_HIT_TOL + pad + grow)
}

fn tol_of(steel: &MeshSteel<'_>) -> f64 {
    if steel.chord_tol.is_finite() && steel.chord_tol > 0.0 { steel.chord_tol } else { 0.01 }
}

/// The farthest any band point stands from the tip (finite bands).
fn far_reach(steel: &MeshSteel<'_>) -> f64 {
    steel
        .stack
        .iter()
        .map(|b| b.lo.abs().max(b.hi.abs()).hypot(b.r))
        .filter(|d| d.is_finite())
        .fold(0.0, f64::max)
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

fn add(p: Point3, a: [f64; 3], s: f64) -> Point3 {
    Point3::new(p.x + a[0] * s, p.y + a[1] * s, p.z + a[2] * s)
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}
