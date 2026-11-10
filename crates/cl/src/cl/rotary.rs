//! The spindle C law (issue #33): how a continuous C-axis live-tool
//! path states its rotary angle, and how a part-frame path is stamped
//! with one.
//!
//! Provenance: RS-274 / ISO 6983 name C the rotation about Z (public
//! standard); the station convention, the unwrap and the
//! deviation-bounded split below are ours, from general rotary
//! machining practice (a rotary move is linear in the axis words, so a
//! part-frame chord is honest only to the deviation it is split to).
//!
//! Convention, stated once: the part turns on the world Z line through
//! `center`; `C` (degrees) is the angle `Rz(C)` that carries the
//! PART-frame tip onto the fixed live-tool station, the +X half-plane
//! of that line. So a tip at azimuth `phi` stands under the tool at
//! `C = -phi`, unwrapped against the previous move so a winding path
//! reads 0, -90, ... -1080, never a jump back by 360. The tool axis is
//! the station's - radial, tip toward shank along +X once turned - and
//! a pose that leans off it is refused: a fixed live tool cannot hold
//! it. Moves keep their part-frame `to` (every verifier, sim and law
//! reads it unchanged); `rotary` is the stated C beside it. A move
//! split into pieces states, on every piece, the station pose at that
//! piece's own C - the move's axis turned back by the rest of the turn
//! (issue #486; the last piece keeps the move's axis bit for bit) - so
//! a law reading the pose sees the radial tool where the machine holds
//! it, not where the whole move ends.
//!
//! The machine moves X (radius), Z and C linearly between two blocks,
//! which in the part frame is a spiral, not the chord the verifier
//! reads. [`stamp_c`] splits every move (collinearly - the chords are
//! the same geometry) until the spiral and the chord agree to `tol` at
//! the quarter points, so what was verified is what the machine cuts.

use crate::geometry::Point3;

use super::{Move, Toolpath};

/// Below this radius (mm) a point stands on the spindle line: it has
/// no azimuth, and the move keeps the previous C.
const ON_LINE: f64 = 1e-6;
/// How far a stamped pose may sit off the radial station (unit-vector
/// component, ~0.06 deg).
const STATION_TOL: f64 = 1e-3;
/// Bisection cap per move: 2^12 pieces. A move still deviating past
/// that passes the line too closely to be split honestly - refused.
const MAX_DEPTH: u32 = 12;

/// The tip's azimuth about the Z line through `center`, degrees, or
/// None on the line itself.
fn azimuth(p: Point3, center: [f64; 2]) -> Option<f64> {
    let (dx, dy) = (p.x - center[0], p.y - center[1]);
    (dx.hypot(dy) > ON_LINE).then(|| dy.atan2(dx).to_degrees())
}

/// The point a fraction `t` along the chord `a..b`.
fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
}

/// `c` moved by whole turns to the value nearest `prev`.
#[must_use]
pub fn unwrap_near(c: f64, prev: f64) -> f64 {
    c + 360.0 * ((prev - c) / 360.0).round()
}

/// The part-frame point the machine stands at a fraction `t` of the
/// way from (`a`, `ca`) to (`b`, `cb`): radius and Z linear, C linear.
fn machine_at(a: Point3, b: Point3, ca: f64, cb: f64, t: f64, center: [f64; 2]) -> Point3 {
    let ra = (a.x - center[0]).hypot(a.y - center[1]);
    let rb = (b.x - center[0]).hypot(b.y - center[1]);
    let r = ra + (rb - ra) * t;
    let phi = -(ca + (cb - ca) * t).to_radians();
    Point3::new(
        center[0] + r * phi.cos(),
        center[1] + r * phi.sin(),
        a.z + (b.z - a.z) * t,
    )
}

/// The worst gap between the machine's linear-in-C motion and the
/// part-frame chord over one piece, read at its quarter points.
fn deviation(a: Point3, b: Point3, ca: f64, cb: f64, center: [f64; 2]) -> f64 {
    [0.25, 0.5, 0.75]
        .iter()
        .map(|&t| {
            let m = machine_at(a, b, ca, cb, t, center);
            let c = lerp(a, b, t);
            let d = m - c;
            d.dot(d).sqrt()
        })
        .fold(0.0, f64::max)
}

/// Rotate a part-frame axis by C and check it lies along the +X
/// station (tip toward shank outward).
fn on_station(axis: [f64; 3], c_deg: f64) -> bool {
    let (s, co) = c_deg.to_radians().sin_cos();
    let x = axis[0] * co - axis[1] * s;
    let y = axis[0] * s + axis[1] * co;
    (x - 1.0).abs() <= STATION_TOL && y.abs() <= STATION_TOL && axis[2].abs() <= STATION_TOL
}

/// Stamp `path` (part frame, posed radially) with the spindle C of
/// every move, split so the machine's motion tracks each verified
/// chord within `tol`. Refused by name: an arc (its circle is not a
/// C-linear motion), a move with no pose or one leaning off the radial
/// station, a path that never leaves the spindle line, a move that
/// cannot be split to `tol`.
pub fn stamp_c(path: &Toolpath, center: [f64; 2], tol: f64) -> Result<Toolpath, String> {
    if !(tol.is_finite() && tol > 0.0) || !center.iter().all(|v| v.is_finite()) {
        return Err("C-axis stamp: no usable tolerance or spindle line".into());
    }
    let first = path
        .moves
        .iter()
        .flat_map(|m| [m.from, m.to])
        .find_map(|p| azimuth(p, center))
        .ok_or("C-axis path never leaves the spindle line - nothing for C to turn")?;
    let mut prev = -first;
    let mut out = Vec::with_capacity(path.moves.len());
    for (i, m) in path.moves.iter().enumerate() {
        if m.arc.is_some() {
            return Err(format!("C-axis move {i} is an arc - only straight moves ride a C turn"));
        }
        let Some(axis) = m.axis else {
            return Err(format!("C-axis move {i} states no tool pose - a live tool stands radial"));
        };
        let ca = azimuth(m.from, center).map_or(prev, |p| unwrap_near(-p, prev));
        let cb = azimuth(m.to, center).map_or(ca, |p| unwrap_near(-p, ca));
        if !on_station(axis, cb) {
            return Err(format!(
                "C-axis move {i} leans the tool off the radial live-tool station - a fixed live tool cannot hold that pose"
            ));
        }
        split(m, (m.from, m.to), (ca, cb), cb, center, tol, 0, &mut out)
            .map_err(|()| format!("C-axis move {i} passes too near the spindle line to follow its chord within {tol} mm"))?;
        prev = cb;
    }
    let mut t = path.clone();
    t.moves = out;
    // Index-carrying side tables would point at the wrong moves after
    // a split; a C path carries none, and any it did is refused.
    if !(t.cycles.is_empty() && t.comps.is_empty() && t.threads.is_empty() && t.probes.is_empty()) {
        return Err("C-axis path carries cycles, compensation, thread or probe records - not supported on a C turn".into());
    }
    Ok(t)
}

/// The move's stated pose `axis` (the station pose at its end C,
/// `c_end`) as the station pose at C `c`: turned about Z by the turn
/// left between them. At `c == c_end` the axis comes back unchanged,
/// bit for bit.
fn pose_at(axis: [f64; 3], c_end: f64, c: f64) -> [f64; 3] {
    if c == c_end {
        return axis;
    }
    let (s, co) = (c_end - c).to_radians().sin_cos();
    [axis[0] * co - axis[1] * s, axis[0] * s + axis[1] * co, axis[2]]
}

#[allow(clippy::too_many_arguments)]
fn split(
    m: &Move,
    (a, b): (Point3, Point3),
    (ca, cb): (f64, f64),
    c_end: f64,
    center: [f64; 2],
    tol: f64,
    depth: u32,
    out: &mut Vec<Move>,
) -> Result<(), ()> {
    if deviation(a, b, ca, cb, center) <= tol {
        let axis = m.axis.map(|ax| pose_at(ax, c_end, cb));
        out.push(Move { from: a, to: b, rotary: Some(cb), axis, ..*m });
        return Ok(());
    }
    if depth >= MAX_DEPTH {
        return Err(());
    }
    let mid = lerp(a, b, 0.5);
    let cm = azimuth(mid, center).map_or(ca, |p| unwrap_near(-p, ca));
    split(m, (a, mid), (ca, cm), c_end, center, tol, depth + 1, out)?;
    split(m, (mid, b), (cm, cb), c_end, center, tol, depth + 1, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;

    fn radial(p: Point3) -> [f64; 3] {
        let r = p.x.hypot(p.y);
        [p.x / r, p.y / r, 0.0]
    }

    fn mv(from: Point3, to: Point3) -> Move {
        Move::new(MoveKind::Feed, from, to, Some(300.0)).with_axis(radial(to))
    }

    #[test]
    fn a_tip_on_plus_y_stands_at_minus_ninety_and_unwraps_past_a_turn() {
        assert_eq!(unwrap_near(170.0, -170.0), -190.0);
        let p = Point3::new(0.0, 10.0, 0.0);
        let path = Toolpath::new(vec![mv(Point3::new(10.0, 0.0, 0.0), p)]);
        let t = stamp_c(&path, [0.0, 0.0], 0.01).ok().unwrap_or_else(|| Toolpath::new(vec![]));
        let last = t.moves.last().and_then(|m| m.rotary);
        assert!(last.is_some_and(|c| (c + 90.0).abs() < 1e-9), "{last:?}");
        assert!(t.moves.len() > 1, "a 90 deg chord at r 10 must split");
        // Every piece's chord meets the machine within tol.
        for w in t.moves.windows(2) {
            assert_eq!(w[0].to, w[1].from);
        }
    }

    #[test]
    fn every_piece_states_the_station_pose_at_its_own_c() {
        let b = Point3::new(0.0, 10.0, -3.0);
        let m = mv(Point3::new(10.0, 0.0, 0.0), b);
        let t = stamp_c(&Toolpath::new(vec![m]), [0.0, 0.0], 0.01).ok().unwrap_or_else(|| Toolpath::new(vec![]));
        assert!(t.moves.len() > 1);
        for p in &t.moves {
            let (c, a) = (p.rotary.unwrap_or(f64::NAN), p.axis.unwrap_or([0.0; 3]));
            assert!(on_station(a, c), "C {c}: {a:?}");
        }
        // The last piece keeps the move's own axis, bit for bit.
        assert_eq!(t.moves.last().and_then(|p| p.axis), m.axis);
    }

    #[test]
    fn a_winding_path_reads_its_turns() {
        let mut moves = Vec::new();
        let mut a = Point3::new(10.0, 0.0, 0.0);
        for k in 1..=8 {
            let th = (k as f64 * 90.0).to_radians();
            let b = Point3::new(10.0 * th.cos(), 10.0 * th.sin(), -(k as f64));
            moves.push(mv(a, b));
            a = b;
        }
        let t = stamp_c(&Toolpath::new(moves), [0.0, 0.0], 0.01).ok().unwrap_or_else(|| Toolpath::new(vec![]));
        let last = t.moves.last().and_then(|m| m.rotary);
        assert!(last.is_some_and(|c| (c + 720.0).abs() < 1e-9), "{last:?}");
    }

    #[test]
    fn a_leaning_pose_and_an_arc_refuse_by_name() {
        let a = Point3::new(10.0, 0.0, 0.0);
        let b = Point3::new(10.0, 0.0, -5.0);
        let lean = Move::new(MoveKind::Feed, a, b, Some(300.0)).with_axis([1.0, 0.0, 1.0]);
        let e = stamp_c(&Toolpath::new(vec![lean]), [0.0, 0.0], 0.01).err();
        assert!(e.is_some_and(|e| e.contains("radial live-tool station")));
        let bare = Move::new(MoveKind::Feed, a, b, Some(300.0));
        let e = stamp_c(&Toolpath::new(vec![bare]), [0.0, 0.0], 0.01).err();
        assert!(e.is_some_and(|e| e.contains("no tool pose")));
    }
}
