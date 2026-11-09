//! The fixture laws (c2)/(d2) under the spindle C (issue #486): a
//! live-tool path turns the part under a FIXED radial tool, so what
//! the machine sweeps is not what the part-frame path says once the
//! steel is read in the wrong frame or at the wrong pose.
//!
//! By `cl::rotary`'s convention the part turns on the world Z line
//! through `c`; C carries the part-frame tip onto the station, the +X
//! half-plane of that line, the tool radial along +X (tip to shank
//! outward); X (radius), Z and C run linearly between blocks, Y stays
//! on the line. The post writes the C and nothing of the tool axis, so
//! the machine always holds the station pose. Two readings follow:
//!
//! - **Steel riding the part** (a chuck jaw) is read in the part frame,
//!   but at the pose the machine holds there: the station pose turned
//!   back by C, `Rz(-C) x = (cos C, -sin C, 0)` - stated by the C
//!   alone, whatever axis the move carries ([`station_poses`]). A path
//!   stamped before #486 states the WHOLE move's axis on every piece,
//!   off the station by the rest of the turn; reading the C keeps those
//!   cached paths honest too.
//! - **Steel declared fixed** (a tailstock, a steady, the table) never
//!   turns, so it is read in the MACHINE frame ([`fixed_hits`]): there
//!   the tip runs exactly straight from `(c + rho0, z0)` to
//!   `(c + rho1, z1)` on the station ray and the pose is +X throughout -
//!   one constant-pose posed sweep ([`super::posed_steel`]), no turn to
//!   enclose. The part-frame reading of fixed steel (the box laws'
//!   verdict before #486) is kept: the machine-frame reading only ADDS
//!   hits, so no verdict any test or project relied on is loosened.
//!
//! The line is recovered from the path itself - two stamped tips whose
//! C differ stand on two rays from the line, which meet at it - and
//! else taken from the rotary axis stated beside the path, and is
//! accepted only when EVERY stamped tip stands at azimuth `-C` about it
//! (within [`LINE_TOL`]): a line the C words do not state is no frame.
//! With no line, or a C move this law cannot place (an arc, a
//! non-finite point), a fixed-steel verdict fails closed - a hit, never
//! silence.
//!
//! Provenance: rigid rotation about the spindle line and the RS-274 C
//! address (rotation about Z) - public standard and textbook machine
//! kinematics; the station convention is `cl::rotary`'s. Our own
//! design; no vendor source consulted.

use std::borrow::Cow;

use crate::cl::{Move, Toolpath};
use crate::fixture::FixtureBox;
use crate::geometry::Point3;
use crate::tools::ToolGeom;
use camcraft_tools::holder_profile::HolderStack;

use super::posed_steel::{sweep, Verdict};
use super::rotary::RotaryAxis;
use super::types::{FixturePart, Warning};

/// The live-tool station's axis in the machine frame.
const STATION: [f64; 3] = [1.0, 0.0, 0.0];
/// How far (mm) a stamped tip may stand off the ray `-C` of a line and
/// still be on it. The stamp states C exactly for its line (float
/// dust); the post refuses a tip off its station by 0.005 mm.
const LINE_TOL: f64 = 1e-4;
/// The least |sin| between two tips' rays the line is recovered from
/// (~5.7 deg): nearer parallel, the crossing is poorly placed.
const MIN_CROSS: f64 = 0.1;
/// A tip this close to the line (mm) has no azimuth to check.
const ON_LINE: f64 = 1e-6;
/// Two unit directions this aligned (cosine) are one line - the rotary
/// gate's own tolerance for reading a fitted axis as the spindle's Z.
const SAME_LINE: f64 = 0.999;

/// The path with every move under the spindle C posed at the station
/// pose its C states (module header). Borrowed untouched when no move
/// states a C - every other path keeps its reading bit for bit.
pub(super) fn station_poses(path: &Toolpath) -> Cow<'_, Toolpath> {
    if path.moves.iter().all(|m| m.rotary.is_none()) {
        return Cow::Borrowed(path);
    }
    let mut out = path.clone();
    for m in &mut out.moves {
        if let Some(c) = m.rotary {
            let (s, co) = c.to_radians().sin_cos();
            // A non-finite C states no pose; the move keeps its own,
            // and the fixed reading fails closed on it.
            if s.is_finite() && co.is_finite() {
                m.axis = Some([co, -s, 0.0]);
            }
        }
    }
    Cow::Owned(out)
}

/// Per move, the element a move under the spindle C strikes FIXED
/// steel with in the machine frame (first in the posed sweep's order),
/// None where it strikes nothing or states no C. `rapids` picks the
/// move kinds judged, as [`sweep`]. Every C move the frame cannot be
/// stated for fails closed.
#[allow(clippy::too_many_arguments)]
pub(super) fn fixed_hits(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    fixed: &[FixtureBox],
    axis: Option<&RotaryAxis>,
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
    rapids: bool,
) -> Vec<Option<FixturePart>> {
    let mut out = vec![None; path.moves.len()];
    if fixed.is_empty() || path.moves.iter().all(|m| m.rotary.is_none()) {
        return out;
    }
    let line = line_of(path, axis);
    let fail = if rapids { FixturePart::Rapid } else { FixturePart::Flute };
    for (i, m) in path.moves.iter().enumerate() {
        if m.rotary.is_none() || (m.kind == crate::cl::MoveKind::Rapid) != rapids {
            continue;
        }
        let machine = line.filter(|_| m.arc.is_none()).and_then(|c| {
            let (m0, m1) = (on_station(m.from, c), on_station(m.to, c));
            (m0.is_finite() && m1.is_finite()).then_some(Move { from: m0, to: m1, axis: Some(STATION), ..*m })
        });
        let Some(mm) = machine else {
            out[i] = Some(fail);
            continue;
        };
        let one = Toolpath::new(vec![mm]);
        out[i] = match sweep(&one, tool, holder, fixed, spindle, chord_tol, rapids, None).first() {
            Some(Verdict::Clear) => None,
            Some(Verdict::Hit(p)) => Some(*p),
            // A constant +X pose is always posed and always judged; any
            // other answer is not a reading - fail closed.
            _ => Some(fail),
        };
    }
    out
}

/// `warns` (one fixture hit per move at most) with `add`'s hits on the
/// moves it left clear, in move order.
pub(super) fn merge(warns: Vec<Warning>, add: &[Option<FixturePart>]) -> Vec<Warning> {
    if add.iter().all(Option::is_none) {
        return warns;
    }
    let mut slot: Vec<Option<FixturePart>> = add.to_vec();
    let mut other = Vec::new();
    for w in warns {
        match w {
            Warning::FixtureHit { at, fixture_part } if at < slot.len() => slot[at] = Some(fixture_part),
            w => other.push(w),
        }
    }
    other.extend(
        slot.into_iter()
            .enumerate()
            .filter_map(|(at, p)| p.map(|fixture_part| Warning::FixtureHit { at, fixture_part })),
    );
    other
}

/// The tip `p` (part frame) where the machine stands it: on the
/// station ray of the line through `c`, at its own radius and height.
fn on_station(p: Point3, c: [f64; 2]) -> Point3 {
    Point3::new(c[0] + (p.x - c[0]).hypot(p.y - c[1]), c[1], p.z)
}

/// The spindle line's XY (module header): recovered from the path,
/// else the stated axis's crossing of z = 0 (where `cl::stamp_c`'s
/// caller places it); either only when every stamped tip agrees.
fn line_of(path: &Toolpath, axis: Option<&RotaryAxis>) -> Option<[f64; 2]> {
    [from_path(path), axis.and_then(from_axis)]
        .into_iter()
        .flatten()
        .find(|&c| agrees(path, c))
}

/// The unit ray a tip stands on from the line at C `c` (degrees).
fn ray(c: f64) -> [f64; 2] {
    let r = -c.to_radians();
    [r.cos(), r.sin()]
}

fn cross(p: [f64; 2], q: [f64; 2]) -> f64 {
    p[0] * q[1] - p[1] * q[0]
}

/// The line where the first stamped tip's ray meets the ray most
/// across it.
fn from_path(path: &Toolpath) -> Option<[f64; 2]> {
    let tips: Vec<(Point3, [f64; 2])> = path
        .moves
        .iter()
        .filter_map(|m| m.rotary.map(|c| (m.to, ray(c))))
        .filter(|(p, u)| p.is_finite() && u.iter().all(|v| v.is_finite()))
        .collect();
    let &(p0, u0) = tips.first()?;
    let &(p1, u1) = tips.iter().max_by(|a, b| cross(u0, a.1).abs().total_cmp(&cross(u0, b.1).abs()))?;
    let den = cross(u0, u1);
    if den.abs() < MIN_CROSS {
        return None;
    }
    // p0 - rho0 u0 = p1 - rho1 u1: both on rays from one point.
    let w = [p0.x - p1.x, p0.y - p1.y];
    let rho0 = cross(w, u1) / den;
    let c = [p0.x - rho0 * u0[0], p0.y - rho0 * u0[1]];
    c.iter().all(|v| v.is_finite()).then_some(c)
}

/// A stated axis read as the spindle's Z line: its crossing of z = 0.
fn from_axis(ax: &RotaryAxis) -> Option<[f64; 2]> {
    let d = ax.dir;
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if !(len.is_finite() && len > 1e-12 && (d[2] / len).abs() >= SAME_LINE) {
        return None;
    }
    let t = -ax.origin.z / d[2];
    let c = [ax.origin.x + d[0] * t, ax.origin.y + d[1] * t];
    c.iter().all(|v| v.is_finite()).then_some(c)
}

/// Does every stamped tip stand on its ray `-C` from `c`?
fn agrees(path: &Toolpath, c: [f64; 2]) -> bool {
    path.moves.iter().all(|m| {
        let Some(cdeg) = m.rotary else { return true };
        let (dx, dy) = (m.to.x - c[0], m.to.y - c[1]);
        let rho = dx.hypot(dy);
        if !rho.is_finite() {
            return false;
        }
        if rho <= ON_LINE {
            return true;
        }
        let u = ray(cdeg);
        (dx - rho * u[0]).hypot(dy - rho * u[1]) <= LINE_TOL
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;

    fn c_move(to: Point3, c: f64) -> Move {
        let mut m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), to, Some(100.0)).with_axis([1.0, 0.0, 0.0]);
        m.rotary = Some(c);
        m
    }

    #[test]
    fn the_line_is_recovered_from_two_turned_tips_and_checked_against_all() {
        // Line at (3, -2): a tip at azimuth 0 (C 0) and one at azimuth
        // 90 (C -90), radius 10.
        let path = Toolpath::new(vec![c_move(Point3::new(13.0, -2.0, 0.0), 0.0), c_move(Point3::new(3.0, 8.0, 0.0), -90.0)]);
        let c = line_of(&path, None);
        assert!(c.is_some_and(|c| (c[0] - 3.0).abs() < 1e-9 && (c[1] + 2.0).abs() < 1e-9), "{c:?}");
        // A third tip off its ray: no line is stated.
        let mut bad = path.clone();
        bad.moves.push(c_move(Point3::new(3.0, -12.0, 0.0), 0.0));
        assert_eq!(line_of(&bad, None), None);
    }

    #[test]
    fn a_path_that_never_turns_takes_the_stated_axis_when_its_tips_agree() {
        let path = Toolpath::new(vec![c_move(Point3::new(10.0, 0.0, 0.0), 0.0), c_move(Point3::new(10.0, 0.0, -5.0), 0.0)]);
        assert_eq!(line_of(&path, None), None, "parallel rays place no line");
        let z = RotaryAxis { origin: Point3::new(0.0, 0.0, 7.0), dir: [0.0, 0.0, -1.0] };
        assert_eq!(line_of(&path, Some(&z)), Some([0.0, 0.0]));
        let off = RotaryAxis { origin: Point3::new(0.0, 4.0, 0.0), dir: [0.0, 0.0, 1.0] };
        assert_eq!(line_of(&path, Some(&off)), None, "a line the tips do not stand on");
        let x = RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [1.0, 0.0, 0.0] };
        assert_eq!(line_of(&path, Some(&x)), None, "an A line is no spindle C");
    }

    #[test]
    fn the_station_pose_is_read_from_c_and_other_paths_stay_borrowed() {
        let plain = Toolpath::new(vec![Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), None)]);
        assert!(matches!(station_poses(&plain), Cow::Borrowed(_)));
        let path = Toolpath::new(vec![c_move(Point3::new(0.0, 10.0, 0.0), -90.0)]);
        let a = station_poses(&path).moves[0].axis.unwrap_or([0.0; 3]);
        assert!(a[0].abs() < 1e-12 && (a[1] - 1.0).abs() < 1e-12 && a[2] == 0.0, "{a:?}");
    }
}
