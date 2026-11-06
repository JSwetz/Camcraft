//! The wrap space's world exit (M5.1 stage E2): a planned toolpath
//! living in developed coordinates (s, v, ρ) comes back onto and
//! around the fitted axis of [`super::axis`], posed by the same lean
//! law the rotary finishing kinds use.
//!
//! Ball centers map back at the plan's own ρ plus one tool radius (a
//! radial ball's CENTER stands r outside its tangent cylinder), and
//! the emitted point is the TIP under that center along the pose it
//! holds - on the cylinder itself for a pure radial (the M13 pivot
//! law, [`super::retip`]: the stream locates the tip, and a center
//! posted as a tip stood every level one radius off its skin). Every
//! wrap arc is chorded (no wrap circle is a world circle), and - the
//! law this exit adds to the twins' - no s-sweep is a world LINE
//! either: a wrap straight laps the axis, so each resampled chord
//! spans at most ARC_FIT_TOL of sagitta on the position circle.
//! Rapids state the pure radial (the post's tilt is modal, and a hop
//! lapping the bar with the last cut's fixed direction would swing
//! the tool through it); cut moves stand RADIAL with the lean, both
//! computed in world coordinates straight off the axis line from the
//! very center being emitted
//! (so the stamped axis and the developed point share bits and the
//! two frames pose identically), with [`super::tilt`]'s lean applied
//! about it. A radial feed - a dive leg running straight at the axis
//! - has no tangent plane to lead in; on a surface of revolution the
//!   plane is not missing, the azimuth supplies it, and the lead
//!   rides the constant-`s` plane the wrap itself lives in.
//!
//! Provenance: unwrapped-to-developed toolpath reconstruction from
//! public multi-axis turning-milling literature; composed on this
//! codebase's own axis fit and lean laws. Nothing of any vendor's
//! consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::axis::Axis;
use super::retip::tip_of;
use super::tilt::{command_axis, AxisControl};
use crate::cl::normalize_axis;

/// The world exit: `base` is the ladder's plan in wrap coordinates
/// (x = s, y = v, z = ρ), mapped and posed about `ax`; `rho_ref` is
/// the development's arc-length radius and `r` the tool radius the
/// plan was offset by.
pub(super) fn wrap_exit(
    base: &Toolpath,
    ax: &Axis,
    e1: &Point3,
    e2: &Point3,
    rho_ref: f64,
    r: f64,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    if !base.cycles.is_empty() {
        return Err(GenError::Invalid(
            "rotary offset cannot pose drill cycles - the spindle never stands still here".into(),
        ));
    }
    let radial = |s: f64| -> Point3 {
        let (sin, cos) = (s / rho_ref).sin_cos();
        Point3::new(
            e1.x * cos + e2.x * sin,
            e1.y * cos + e2.y * sin,
            e1.z * cos + e2.z * sin,
        )
    };
    let map = |pt: Point3| -> Point3 {
        let rc = pt.z + r;
        let n = radial(pt.x);
        Point3::new(
            ax.origin.x + pt.y * ax.dir.x + rc * n.x,
            ax.origin.y + pt.y * ax.dir.y + rc * n.y,
            ax.origin.z + pt.y * ax.dir.z + rc * n.z,
        )
    };
    // `map` names the BALL CENTER (the plan's level plus one radius,
    // out along the radial); the emitted point is the tip that center
    // stands on under the pose it holds ([`super::retip`]).
    let c0 = map(base.start().unwrap_or(Point3::ZERO));
    let mut b = PathBuilder::new(tip_of(c0, None, r));
    let mut held: Option<[f64; 3]> = None;
    for mv in &base.moves {
        let air = mv.kind == MoveKind::Rapid;
        let pose = |w: Point3, t: Point3, held: Option<[f64; 3]>| -> Option<[f64; 3]> {
            // The radial computed in world coordinates straight off
            // the axis line, from the very center being emitted - so
            // the stamped axis and the developed point share bits and
            // agree to the last ulp (the twins' pose law).
            let d = w - ax.origin;
            let s = d.dot(ax.dir);
            let foot = Point3::new(d.x - ax.dir.x * s, d.y - ax.dir.y * s, d.z - ax.dir.z * s);
            // On the axis (or worse) there is no radial to stand on:
            // the 1e-24 squared bound is set_axis's own refusal
            // threshold, so a held pose is always a direction.
            if !foot.is_finite() || foot.dot(foot) <= 1e-24 {
                if air {
                    return held; // a rapid keeps the tilt it has
                }
                eprintln!(
                    "multiaxis: a rotary offset cut stands ON the axis at {w:?}; the move holds vertical"
                );
                return Some([0.0, 0.0, 1.0]);
            }
            // A rapid laps the work with the head turning WITH it: the
            // post keeps the tilt modal, so a rapid that stated nothing
            // would sweep the last cut's fixed direction round the
            // bar. It states the pure radial - no travel to lead in.
            if air || (c.lead_deg == 0.0 && c.lag_deg == 0.0 && c.max_tilt_deg.is_none()) {
                // No lean asked: the radial stands, and it is stamped
                // from the RAW foot so the one normalization is the
                // vector's only division - the pose then matches the
                // position's own radial to the bit, not the ulp.
                return normalize_axis([foot.x, foot.y, foot.z]);
            }
            let n = match normalize(&foot) {
                Some(n) => n,
                None => return Some([0.0, 0.0, 1.0]),
            };
            // A radial feed (a dive leg runs straight at the axis) has
            // no tangent plane to lead in: the law would hold the
            // radial. On a surface of revolution the plane is not
            // missing - the azimuth supplies it: lead within the
            // constant-`s` plane, the same plane the wrap lives in.
            let hinge = n.cross(t);
            let t = if hinge.dot(hinge) <= 1e-18 * t.dot(t) {
                match normalize(&ax.dir.cross(n)) {
                    Some(a) => a,
                    None => t,
                }
            } else {
                t
            };
            match command_axis(n, t, c) {
                Some(a) => Some([a.x, a.y, a.z]),
                None => {
                    eprintln!(
                        "multiaxis: no lean exists at world point {w:?}; the move holds radial"
                    );
                    Some([n.x, n.y, n.z])
                }
            }
        };
        // The move's wrap polyline: an (s,v) arc chords at the arc-fit
        // tolerance with rho carried linearly; a line is its endpoints.
        let mut poly: Vec<Point3> = Vec::new();
        match &mv.arc {
            // XZ / YZ (M11.2 A): the arc's own 3-D chords.
            Some(a) if !a.plane.is_xy() => {
                poly.push(mv.from);
                poly.extend(mv.segments3(ARC_FIT_TOL).into_iter().map(|(_, e)| e));
            }
            Some(_) => {
                let ch = mv.chords(ARC_FIT_TOL);
                let n = ch.len().max(1) as f64;
                poly.push(mv.from);
                for (i, (_a, e)) in ch.iter().enumerate() {
                    let zb = mv.from.z + (mv.to.z - mv.from.z) * ((i + 1) as f64 / n);
                    poly.push(Point3::new(e.x, e.y, zb));
                }
                if ch.is_empty() {
                    poly.push(mv.to);
                }
            }
            None => {
                poly.push(mv.from);
                poly.push(mv.to);
            }
        }
        for seg in poly.windows(2) {
            // No s-sweep is a world line: a wrap straight is a lap of
            // the fitted axis. Resample each piece so its world chord
            // spans the position circle within ARC_FIT_TOL of sagitta
            // (a full circle is TAU·rc/(2·cap)-ish chords); pure
            // v-steps, plunges and radial rapids have no s-run and
            // pass through as the single line they are.
            let (pa, pb) = (seg[0], seg[1]);
            let rc = (pa.z.max(pb.z) + r).max(f64::MIN_POSITIVE);
            let cap = 2.0 * (1.0 - (ARC_FIT_TOL / rc).min(2.0)).clamp(-1.0, 1.0).acos();
            let dth = (pb.x - pa.x).abs() / rho_ref.max(f64::MIN_POSITIVE);
            let n = if cap > 1e-12 && dth > cap {
                (dth / cap).ceil() as usize
            } else {
                1
            };
            let n = n.clamp(1, 8192);
            for i in 0..n {
                let at = |t: f64| {
                    Point3::new(
                        pa.x + (pb.x - pa.x) * t,
                        pa.y + (pb.y - pa.y) * t,
                        pa.z + (pb.z - pa.z) * t,
                    )
                };
                let (qa, qb) = (at(i as f64 / n as f64), at((i + 1) as f64 / n as f64));
                let (wf, wt) = (map(qa), map(qb));
                let a = pose(wt, wt - wf, held);
                b.set_axis(a);
                held = a;
                b.push(mv.kind, tip_of(wt, a, r), mv.feed);
            }
        }
    }
    let mut path = b.finish();
    // The plan's notes ride the wrap (M8.1's channel, this leg): an
    // entry substitution said on the developed path must still be
    // readable on the revolved one - the map re-emits every move, and
    // rebuilding the path without the notes would silence the very
    // verdict the entry law posts loudly.
    path.notes.extend(base.notes.iter().cloned());
    if !path.all_finite() {
        return Err(GenError::Invalid(
            "a rotary offset path left the finite horizon".into(),
        ));
    }
    Ok(path)
}

fn normalize(v: &Point3) -> Option<Point3> {
    let l = (v.dot(*v)).sqrt();
    if !l.is_finite() || l <= f64::EPSILON {
        None
    } else {
        Some(Point3::new(v.x / l, v.y / l, v.z / l))
    }
}
