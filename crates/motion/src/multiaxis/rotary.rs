//! The rotary generators (M5.1 stage E): the turned part's two
//! native path ladders, each riding a fitted axis [`super::axis`] and
//! the pose law of [`super::tilt`] with the surface normal replaced
//! by the RADIAL direction - the line from the axis to the tool,
//! which is what a turned work actually offers the spindle. Both
//! kinds are world-in, world-out: the path is planned in an axis
//! frame (an exact rotation of the work), posed, and carried back,
//! so the app branch is as thin as the 5-axis ones.
//!
//! `rotary_contour` is the waterline rung ladder about the axis -
//! the same rings `contour5d` cuts, posed radially: on a cylinder
//! the tool stands square to the wall and its axis MEETS the
//! rotation axis, the defining posture of rotary work. Heights read
//! as axial stations: a world height H maps to the station
//! `(H - o.z) / u.z` along the axis, the exact stage-B identity when
//! the part stands on its axis (the lathe case) and the honest
//! station reading when it leans.
//!
//! `rotary_parallel` lays the bar down in its planning frame -
//! passes along the axis, stepping around it - and runs the swarf
//! raster there: on a laid bar the raster's shallow terrain is the
//! crown of the work and the steep flanks its sides, where the
//! rings' level loops degenerate into exactly the straight
//! generatrices the kind is named for. No new pattern kernel; the
//! same two ladders of stages A-D, composed. A bar standing nearly
//! on end refuses (its parallel passes are the contour's rings) and
//! a bar lying nearly flat refuses the contour's height law (its
//! rings are the parallel passes) - each kind says so by name
//! rather than invent a frame the parameters cannot speak in.
//!
//! The world exit is spelled here, not through `world_path`: that
//! stamps the frame's CONSTANT pose where these paths carry a
//! per-move radial one. Arcs survive verbatim only in an upright
//! frame (the bit-exact golden law, with the center carried through
//! the frame's own translation); a tilted frame's XY circle is no
//! world XY circle, so its chords ride at `ARC_FIT_TOL` with the
//! pose computed fresh at every chord end - never interpolated,
//! always the law's own answer at the point the move ends.
//!
//! Provenance: axis-of-revolution detection and the radial tool-axis
//! convention of 4/5-axis rotary machining from public CAM
//! literature and turning practice; composed on this codebase's own
//! ring, raster and lean laws. Nothing of any vendor's consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::cl::TiltedFrame;
use crate::finishing3d::{self, Finish3DParams};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::axis::{self, Axis};
use super::tilt::{command_axis, AxisControl};

/// The axis' tilt below which "standing" and "laid" trade places:
/// one sine for both refusals, so the pair cannot disagree about
/// where the boundary sits (sin 1 degree).
const LEAN_MIN: f64 = 0.0175;
/// World up, spelled once (Point3::new is not a const fn, so the
/// literal stands in for it).
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};

/// The rotary contour: the axis read off the mesh, the waterline
/// rung ladder in the axis frame, the radial pose on every cut move.
pub fn rotary_contour(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let ax = axis::fit(facets)?;
    let lean = ax.dir.dot(UP).abs();
    if lean < LEAN_MIN {
        return Err(GenError::Invalid(
            "the work's axis lies flat on the table - a laid bar is finished with passes along it: Rotary Parallel".into(),
        ));
    }
    let f = frame(&ax)?;
    let local = f.local_facets(facets);
    let uz = if ax.dir.dot(UP) < 0.0 { -lean } else { lean };
    let q = stations(&local, p, &f, &|h| (h - ax.origin.z) / uz);
    let base = finishing3d::waterline(&local, &q)?;
    posed(&f, &base, &ax, c)
}

/// The rotary parallel: generatrix passes along the axis, stepping
/// around the bar - the swarf raster in a frame that lays the axis
/// down, with the same radial pose law.
pub fn rotary_parallel(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let ax = axis::fit(facets)?;
    let flat = 1.0 - ax.dir.dot(UP) * ax.dir.dot(UP);
    if flat < LEAN_MIN * LEAN_MIN {
        return Err(GenError::Invalid(
            "the work stands on its axis - passes along a standing bar are its rings: Rotary Contour".into(),
        ));
    }
    // The laid-down frame: local x along the axis, local z as close
    // to world up as the axis allows (exact for a bar lying flat),
    // right-handed the way TiltedFrame stores its basis.
    let uz = ax.dir.dot(UP);
    let up = Point3::new(-ax.dir.x * uz, -ax.dir.y * uz, 1.0 - uz * uz);
    let up = match normalize(&up) {
        Some(v) => v,
        None => {
            return Err(GenError::Invalid(
                "rotary could not lay the axis down - the fit is degenerate".into(),
            ))
        }
    };
    let v = up.cross(ax.dir); // u x v = axis, by construction
    let f = TiltedFrame {
        origin: ax.origin,
        u: [ax.dir.x, ax.dir.y, ax.dir.z],
        v: [v.x, v.y, v.z],
        axis: [up.x, up.y, up.z],
    };
    let local = f.local_facets(facets);
    let q = stations(&local, p, &f, &|h| h - ax.origin.z);
    let base = finishing3d::swarf(&local, &q, 1.0, 0.0)?;
    posed(&f, &base, &ax, c)
}

/// The contour frame: the axis as local z, the canonical foot as
/// origin. `TiltedFrame::new` refuses a zero axis; a fitted unit
/// direction never is one, and the mapping names the impossibility.
fn frame(ax: &Axis) -> Result<TiltedFrame, GenError> {
    TiltedFrame::new(ax.origin, [ax.dir.x, ax.dir.y, ax.dir.z]).ok_or_else(|| {
        GenError::Invalid("the fitted axis is not a direction - the mesh is corrupt".into())
    })
}

/// The params as their planning frame reads them: the pattern
/// geometry untouched, the heights through `hmap` (axial stations
/// for the contour, plain local z for the laid raster - both exact
/// when the part stands on the table's z), and the ladder's top
/// starting where the local work actually tops out.
fn stations(
    local: &[Triangle],
    p: &Finish3DParams,
    f: &TiltedFrame,
    hmap: &dyn Fn(f64) -> f64,
) -> Finish3DParams {
    let mut q = *p;
    q.clearance_z = hmap(p.clearance_z);
    q.retract_z = p.retract_z.map(hmap);
    q.bottom_z = hmap(p.bottom_z);
    let top = hmap(p.top_z);
    q.top_z = local.iter().fold(top, |x, t| {
        x.max(f.to_local(t.a).z).max(f.to_local(t.b).z).max(f.to_local(t.c).z)
    });
    q
}

/// The pose stamp and the world exit in one pass: every cutting
/// move ends with the radial pose the lean law commands about the
/// fitted axis (computed in world coordinates straight off the
/// axis line, so both frames pose identically), rapids hold nothing,
/// and positions ride the frame. A cycle would ask the spindle to
/// stand still inside a moving-axis path: refused upstream, named
/// here.
fn posed(
    f: &TiltedFrame,
    base: &Toolpath,
    ax: &Axis,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    if !base.cycles.is_empty() {
        return Err(GenError::Invalid(
            "rotary cannot pose drill cycles - the spindle never stands still here".into(),
        ));
    }
    let upright = f.is_upright();
    let mut b = PathBuilder::new(f.to_world(base.start().unwrap_or(Point3::ZERO)));
    for mv in &base.moves {
        let air = mv.kind == MoveKind::Rapid;
        let pose = |q: Point3, t: Point3, b: &mut PathBuilder| {
            if air {
                b.set_axis(None);
            } else {
                match radial_pose(f, ax, c, q, t) {
                    Some(a) => b.set_axis(Some(a)),
                    None => {
                        eprintln!(
                            "multiaxis: no radial exists at {:?}; the move holds vertical",
                            f.to_world(q)
                        );
                        b.set_axis(Some([0.0, 0.0, 1.0]));
                    }
                }
            }
        };
        match &mv.arc {
            // Upright frame: the local XY circle IS a world XY
            // circle - its center rides the frame's translation and
            // the arc intent survives to the post verbatim.
            // XZ / YZ (M11.2 A), upright: the identity basis keeps the
            // plane; the center rides the translation in its own pair.
            Some(arc) if upright && !arc.plane.is_xy() => {
                let (cu, cv) = arc.plane.center_uv(arc.center);
                let c3 = f.to_world(arc.plane.point(cu, cv, 0.0));
                pose(mv.to, mv.to - mv.from, &mut b);
                b.push_arc_in(
                    mv.kind,
                    f.to_world(mv.to),
                    mv.feed,
                    arc.plane.words(c3),
                    arc.ccw,
                    arc.plane,
                );
            }
            // XZ / YZ, tilted: the arc's own 3-D chords, posed per chord.
            Some(arc) if !arc.plane.is_xy() => {
                let mut from = mv.from;
                for (_a, to) in mv.segments3(ARC_FIT_TOL) {
                    pose(to, to - from, &mut b);
                    b.push(mv.kind, f.to_world(to), mv.feed);
                    from = to;
                }
            }
            Some(arc) if upright => {
                let center = f.to_world(Point3::new(arc.center.x, arc.center.y, mv.to.z));
                pose(mv.to, mv.to - mv.from, &mut b);
                b.push_arc(
                    mv.kind,
                    f.to_world(mv.to),
                    mv.feed,
                    Point2::new(center.x, center.y),
                    arc.ccw,
                );
            }
            // Tilted: the circle is no world XY arc the G2/G3
            // contract can state; its chords replace it, motion for
            // motion, Z linear between the endpoints (endpoints
            // exact, their bits kept), the pose computed at each
            // chord's own end.
            Some(_) => {
                let ch = mv.chords(ARC_FIT_TOL);
                let n = ch.len().max(1) as f64;
                let mut from = mv.from;
                for (i, (_a, e)) in ch.iter().enumerate() {
                    let zb = mv.from.z + (mv.to.z - mv.from.z) * ((i + 1) as f64 / n);
                    let to = Point3::new(e.x, e.y, zb);
                    pose(to, to - from, &mut b);
                    b.push(mv.kind, f.to_world(to), mv.feed);
                    from = to;
                }
                if ch.is_empty() {
                    pose(mv.to, mv.to - mv.from, &mut b);
                    b.push(mv.kind, f.to_world(mv.to), mv.feed);
                }
            }
            None => {
                pose(mv.to, mv.to - mv.from, &mut b);
                b.push(mv.kind, f.to_world(mv.to), mv.feed);
            }
        }
    }
    let out = b.finish();
    if !out.all_finite() {
        return Err(GenError::Invalid("a rotary path left the finite horizon".into()));
    }
    Ok(out)
}

/// The radial pose at a world-carried endpoint `q` (a LOCAL point
/// of the planning path, mapped here): out of the axis line to the
/// tool, along the move's travel through the lean law, and the
/// whole answer turned into the world. Where the point lies ON the
/// axis no radial exists - the vertical stands in and says so,
/// never silently.
fn radial_pose(
    f: &TiltedFrame,
    ax: &Axis,
    c: &AxisControl,
    q: Point3,
    t_local: Point3,
) -> Option<[f64; 3]> {
    let w = f.to_world(q);
    let d = w - ax.origin;
    let s = d.dot(ax.dir);
    let n = Point3::new(d.x - ax.dir.x * s, d.y - ax.dir.y * s, d.z - ax.dir.z * s);
    let n = normalize(&n)?;
    let tw = f.to_world(Point3::new(q.x + t_local.x, q.y + t_local.y, q.z + t_local.z)) - w;
    let a = match normalize(&tw) {
        Some(t) => command_axis(n, t, c)?,
        None => n, // nowhere to lead: hold the radial
    };
    Some([a.x, a.y, a.z])
}

fn normalize(v: &Point3) -> Option<Point3> {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if !l.is_finite() || l <= 1e-12 {
        return None;
    }
    Some(Point3::new(v.x / l, v.y / l, v.z / l))
}
