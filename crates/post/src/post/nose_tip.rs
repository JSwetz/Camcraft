//! Nose centre to control point (#630): the one place a turning path's
//! nose-centre CL becomes the point a lathe control drives under G40.
//!
//! The turning kernels state the insert's nose CIRCLE CENTRE (the
//! envelope law the verifier and the stock sim judge). A control with
//! nose compensation off drives the tool's CONTROL POINT - the
//! imaginary tip the operator touched off on (LinuxCNC Lathe User
//! Information section 7.1; `camcraft_tools::orientation` for the 1-9
//! chart). The insert is rigid, so the control point stands at one
//! fixed offset from the nose centre for the whole cut: the program
//! that drives the nose centre along the CL drives the control point
//! along the CL translated by `r * tip_offset(q)`. That translation is
//! the EXACT compensated path - what a control's own nose-radius
//! compensation would compute from the part profile - on diameters,
//! faces, tapers and arcs alike: the nose-centre path is already the
//! profile's offset by r, and a rigid shift of the tool moves every
//! point of it the same. (Programming the PART profile as the tip
//! path is what leaves the r(1 - cos) error on tapers and radii; this
//! is not that.)
//!
//! The frame is the lathe posts' own: the spindle axis is world Z
//! through the WCS origin, and a move's radius is its signed X or Y
//! offset from the origin - one meridian half-plane per path. Points
//! on the axis take the path's meridian. A path that is not one
//! meridian's (both X and Y off the axis, or two meridians), carries a
//! C angle, an XY arc, a cycle, a thread record or a control-comp span
//! is refused by name: none of those is a nose-centre turning cut.
//!
//! Provenance: LinuxCNC Lathe User Information (public docs) for the
//! control point; nose-radius compensation as machine-tool handbook
//! practice. The translation argument is our own. No vendor material.

use crate::cl::{lattice, ArcPlane, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// The lathe posts' half-micron lattice tolerance for "on the axis".
const EPS: f64 = 0.0005;

/// Which world axis a path's radius rides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meridian {
    X,
    Y,
}

/// The nose-centre path `t` as the control-point path: every point
/// moved by `shift` = (radius, Z) mm, i.e. `r * tip_offset(q)`. The
/// radius part moves along the path's meridian (+ away from the axis
/// on the side the path stands), the Z part along world Z. `origin` is
/// the WCS origin the post reads radii from. Everything but the
/// geometry (feeds, kinds, notes) is kept as it was.
pub fn control_point_path(t: &Toolpath, shift: (f64, f64), origin: [f64; 3]) -> Result<Toolpath, String> {
    let (dr, dz) = shift;
    if !dr.is_finite() || !dz.is_finite() {
        return Err("the nose offset is not a number".into());
    }
    if !t.cycles.is_empty() || !t.threads.is_empty() || !t.comps.is_empty() || !t.probes.is_empty() {
        return Err("a cycle, thread, compensation or probing record rides this path - not a nose-centre turning cut".into());
    }
    let meridian = meridian_of(t, origin)?;
    let mut out = t.clone();
    if dr == 0.0 && dz == 0.0 {
        return Ok(out);
    }
    let (sx, sy) = match meridian {
        Meridian::X => (dr, 0.0),
        Meridian::Y => (0.0, dr),
    };
    // The side of the axis a point stands on decides which way "+
    // radius" points; the posts' signed radius is the world coordinate
    // itself, so the shift is in that coordinate, sign and all.
    let mv = |p: Point3| Point3::new(lattice(p.x + sx), lattice(p.y + sy), lattice(p.z + dz));
    for (i, m) in out.moves.iter_mut().enumerate() {
        if m.rotary.is_some() {
            return Err(format!("move {i} states a C angle - live-tool work, not a nose-centre turning cut"));
        }
        m.from = mv(m.from);
        m.to = mv(m.to);
        if let Some(a) = m.arc.as_mut() {
            a.center = match (a.plane, meridian) {
                (ArcPlane::Xz, Meridian::X) => Point2::new(lattice(a.center.x + dr), lattice(a.center.y + dz)),
                (ArcPlane::Yz, Meridian::Y) => Point2::new(lattice(a.center.x + dr), lattice(a.center.y + dz)),
                _ => {
                    return Err(format!(
                        "move {i} is an arc off the path's meridian plane - not a nose-centre turning cut"
                    ))
                }
            };
        }
    }
    Ok(out)
}

/// The one meridian a path's off-axis points stand on.
fn meridian_of(t: &Toolpath, origin: [f64; 3]) -> Result<Meridian, String> {
    let mut seen: Option<Meridian> = None;
    let pts = t.moves.iter().flat_map(|m| [m.from, m.to]);
    for p in pts {
        let (dx, dy) = (p.x - origin[0], p.y - origin[1]);
        if !dx.is_finite() || !dy.is_finite() {
            return Err("a move is not a number".into());
        }
        let here = match (dx.abs() > EPS, dy.abs() > EPS) {
            (false, false) => continue,
            (true, false) => Meridian::X,
            (false, true) => Meridian::Y,
            (true, true) => {
                return Err("a move stands off the spindle axis meridian (both X and Y off the axis) - milling work, not turning".into())
            }
        };
        match seen {
            None => seen = Some(here),
            Some(s) if s != here => {
                return Err("the path stands on two meridians of the spindle axis - not one turning cut".into())
            }
            Some(_) => {}
        }
    }
    seen.ok_or_else(|| "every move stands on the spindle axis - no radius to read the meridian from".into())
}

#[cfg(test)]
#[path = "nose_tip_tests.rs"]
mod tests;
