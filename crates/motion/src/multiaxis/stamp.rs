//! The STAMP: the pass that reads a finished ball-center path back
//! into an oriented one (M5.1 stage C). A sphere's envelope is
//! rotation-invariant, so the CL ladder a 5-axis finish cuts is
//! exactly the waterline/raster path of stage A/B - and what makes it
//! 5-axis is the pose each move holds: at every move that ends in
//! contact, the nearest surface point gives the normal
//! (`d = |c - q| = r` is tangency), the move's own travel gives the
//! tangent, and [`super::tilt::command_axis`] folds lead, lag and the
//! clamp into the direction the spindle must point. Moves that end in
//! air - the hops, the clearances - hold no angle, and a pose change
//! where the tip stands still is a position intent the post commands
//! on its own axis words (M5.1 A's law).
//!
//! Provenance: the ball-center/axis-pose split is standard multi-axis
//! CL practice (tool-axis vectors ride the location words,
//! RS-274/ISO 10791 tradition); written from those published
//! descriptions and our own design notes only.

use crate::cl::{Move, MoveKind, PathBuilder, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::avoid::{Shank, escape_axis, shank_hits};
use super::tilt::{command_axis, AxisControl};

/// The search ceiling shank avoidance tilts within when the control
/// states no cone (`max_tilt_deg: None` = "as far as the geometry
/// asks"): a horizontal spindle is the last pose anyone means, and
/// the cone's own law is a cone, not a plane.
pub(super) const ESCAPE_CAP: f64 = 85.0;

/// Read a ball-center path back as an oriented one: same positions,
/// same order, same arc intents - only the pose each move holds is
/// new. `r` is the ball radius the path was generated with; contact
/// is decided by the tangent law `|c - q| = r`, with a band that
/// forgives the project's micron lattice and the arc fit, never a
/// whole radius of slack. Facets are scanned with an AABB shell
/// reject; these paths have thousands of moves and models have tens
/// of thousands of facets, so the cheap box answer must gate the
/// Voronoi solve. `shank` (M5.1 stage F) is the tool above the ball:
/// where `Some`, a pose whose shank column touches the part tilts to
/// the nearest clear angle inside the cone, and a move no cone angle
/// clears refuses the path by name.
pub(super) fn stamp_axes(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    stamp_inner(path, facets, r, c, None, shank)
}

/// The same pass with the swarf law: on the facets the ruling
/// analysis marked ruled, the tool lies WITH the blade - its axis is
/// the ruling direction projected into the surface (tangent to the
/// flank, the side-machining contact) - and everywhere else the
/// lean law of stage C holds. `dir` is the region's unit XY
/// direction, `ruled` is the per-facet verdict of
/// [`super::ruling::analyze`].
pub(super) fn stamp_axes_ruled(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    dir: Point3,
    ruled: &[bool],
) -> Result<Toolpath, GenError> {
    // The ruled reads stay out of stage F: on a ruled facet the tool
    // lies WITH the blade - that direction is surface truth, not a
    // free axis to dodge with (RECOMMENDED.md).
    stamp_inner(path, facets, r, c, Some((dir, ruled)), None)
}

fn stamp_inner(
    path: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    ruling: Option<(Point3, &[bool])>,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    if !path.cycles.is_empty() {
        return Err(GenError::Invalid(
            "a 5-axis stamp cannot re-pose drill cycles".into(),
        ));
    }
    let band = 0.01 + 1e-6 * r; // micron lattice + float dust, not a radius
    let boxes: Vec<[f64; 6]> = facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect();
    let mesh = Mesh { facets, boxes: &boxes };
    let mut b = PathBuilder::new(path.start().unwrap_or(Point3::ZERO));
    // The smoothing memory (stage F): the axis the machine actually
    // holds entering a move. It starts vertical (the home the post
    // wakes at), keeps the last pose stated through rapids (air moves
    // command no angle), and drops back to vertical on a cut that
    // states none - which is what verify's law (c) already reads an
    // unposed move as.
    let mut held = [0.0, 0.0, 1.0];
    for (i, mv) in path.moves.iter().enumerate() {
        let axis = match mv.kind {
            MoveKind::Rapid => None,
            _ => cut_axis(mv, &mesh, r, band, &path.moves[i + 1..], c, ruling, shank, held, i)?,
        };
        if axis.is_some() || mv.kind != MoveKind::Rapid {
            held = axis.unwrap_or([0.0, 0.0, 1.0]);
        }
        b.set_axis(axis);
        match &mv.arc {
            Some(a) => b.push_arc(mv.kind, mv.to, mv.feed, a.center, a.ccw),
            None => b.push(mv.kind, mv.to, mv.feed),
        }
    }
    let stamped = b.finish();
    if !stamped.all_finite() {
        return Err(GenError::Invalid("a 5-axis stamp left the finite horizon".into()));
    }
    Ok(stamped)
}

/// The facet list and its AABB shell, carried as one argument.
struct Mesh<'a> {
    facets: &'a [Triangle],
    boxes: &'a [[f64; 6]],
}

/// The pose one cutting move holds, before and after the shank speaks:
/// None in air, the surface normal with lead/lag/clamp folded in
/// wherever the move's endpoint rests on a facet at the ball's tangent
/// distance. With a `shank` (stage F) a pose that touches is re-aimed
/// by the escape search; a move nothing inside the cone clears
/// refuses the whole path by name. `seed` is the cone's axis the
/// search tilts around: the contact normal where there is one,
/// vertical wherever the pose is an answer to air or a level floor.
#[allow(clippy::too_many_arguments)] // the stamp's flat read of one move
fn cut_axis(
    mv: &Move,
    mesh: &Mesh,
    r: f64,
    band: f64,
    rest: &[Move],
    c: &AxisControl,
    ruling: Option<(Point3, &[bool])>,
    shank: Option<&Shank>,
    held: [f64; 3],
    i: usize,
) -> Result<Option<[f64; 3]>, GenError> {
    let (a0, seed, skip) = pose_law(mv, mesh, r, band, rest, c, ruling);
    let Some(s) = shank.filter(|_| !skip) else {
        return Ok(a0);
    };
    let p = mv.to;
    let a = a0.unwrap_or([0.0, 0.0, 1.0]);
    if !shank_hits(p, a, r, s, mesh.facets, mesh.boxes) {
        return Ok(a0);
    }
    let cone = c.max_tilt_deg.unwrap_or(ESCAPE_CAP).min(ESCAPE_CAP);
    match escape_axis(p, a, seed, held, cone, r, s, mesh.facets, mesh.boxes) {
        Some(b) => Ok(Some(b)),
        None => Err(GenError::Invalid(format!(
            "the shank of the tool touches the part at move {i} of the posed path, and no \
             pose inside the max-tilt cone ({cone:.0} deg) clears it - widen the cone, \
             shorten the tool, or turn shank avoidance off"
        ))),
    }
}

/// Stage C's pose law alone - the angle the surface commands before
/// the shank has an opinion. The `bool` says the law refused to state
/// a direction (garbage in a sane door's shadow): vertical it stays,
/// and no escape invents an angle the law itself would not compute.
fn pose_law(
    mv: &Move,
    mesh: &Mesh,
    r: f64,
    band: f64,
    rest: &[Move],
    c: &AxisControl,
    ruling: Option<(Point3, &[bool])>,
) -> (Option<[f64; 3]>, Point3, bool) {
    let up = Point3::new(0.0, 0.0, 1.0);
    let p = mv.to;
    let mut best: Option<(f64, Point3, usize)> = None;
    for (fi, (f, bx)) in mesh.facets.iter().zip(mesh.boxes.iter()).enumerate() {
        // Chebyshev-ish box distance: if the nearest corner-axis gap
        // already exceeds r + band, no point of the box is in contact.
        let gx = (bx[0] - p.x).max(0.0).max(p.x - bx[3]);
        let gy = (bx[1] - p.y).max(0.0).max(p.y - bx[4]);
        let gz = (bx[2] - p.z).max(0.0).max(p.z - bx[5]);
        if (gx * gx + gy * gy + gz * gz).sqrt() > r + band {
            continue;
        }
        let q = closest_pt_tri(p, &[f.a, f.b, f.c]);
        let d2 = {
            let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
            dx * dx + dy * dy + dz * dz
        };
        if best.is_none_or(|(bd, _, _)| d2 < bd) {
            best = Some((d2, q, fi));
        }
    }
    let Some((d2, q, fi)) = best else {
        return (None, up, false);
    };
    let d = d2.sqrt();
    // Contact (or a bite - verify is the law that judges a bite, the
    // normal here is still the honest direction out of the surface).
    // A floor reads d = 0: the ball's contact stands under the center
    // itself, there is no direction out of the surface to lean off,
    // and the honest pose for a floor is the one it always had.
    if d > r + band || d <= 1e-9 {
        return (None, up, false);
    }
    let n = Point3::new((p.x - q.x) / d, (p.y - q.y) / d, (p.z - q.z) / d);
    // The swarf law (M5.1 D): on a facet the ruling analysis marked
    // ruled, the tool lies WITH the blade - the pass direction
    // projected into the surface, tangent to the flank and normal to
    // nothing, which is what keeps the ball touching along a ruling.
    // A facet whose plane says almost nothing of the direction
    // (seen nearly head-on) is no flank at all, and the lean law of
    // stage C speaks for it instead.
    if let Some((dir, _)) = ruling.filter(|(_, f)| f.get(fi).copied().unwrap_or(false)) {
        let dn = dir.dot(n);
        let d3 = Point3::new(dir.x - n.x * dn, dir.y - n.y * dn, dir.z - n.z * dn);
        let l = tl(d3);
        if l > SIN_MIN {
            let mut a = Point3::new(d3.x / l, d3.y / l, d3.z / l);
            // One deterministic hemisphere: the first component
            // with a real opinion says which end the spindle sits
            // behind (z first - up on a floor, never down).
            for v in [a.z, a.x, a.y] {
                if v < -1e-9 {
                    a = Point3::new(-a.x, -a.y, -a.z);
                    break;
                }
                if v > 1e-9 {
                    break;
                }
            }
            return (Some([a.x, a.y, a.z]), n, false);
        }
    }
    // The feed direction: this move's travel; a vertical entry has
    // none of its own, so the next move's states where the cut goes.
    let mut t = p - mv.from;
    if tl(t) <= 1e-9 {
        t = rest
            .iter()
            .map(|m| m.to - p)
            .find(|&tt| tl(tt) > 1e-9)
            .unwrap_or(t);
    }
    if tl(t) <= 1e-9 {
        // Nowhere to lead: the path ends here; hold the normal.
        return (Some([n.x, n.y, n.z]), n, false);
    }
    match command_axis(n, t, c) {
        Some(a) => (Some([a.x, a.y, a.z]), n, false),
        None => {
            // A control that states no direction (a NaN the sane
            // door somehow missed): hold vertical, say so - never
            // invent an angle the law refused to compute, and never
            // let the escape speak for a law that stayed silent.
            eprintln!("multiaxis: the axis law stated no direction at {p:?}; the move holds vertical");
            (None, up, true)
        }
    }
}

/// A ruled facet seen within this of head-on (sin 10 degrees) says
/// nothing about the pass direction; the lean law takes it.
const SIN_MIN: f64 = 0.173_648_177_666_930_33; // sin(10 deg), frozen

/// The length of a difference vector, spelled once for `cut_axis`.
fn tl(v: Point3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

// The stamp-level law tests live in `super::stamp_tests` (the AGENTS
// 400-line seam, crossed by moving the test module, not by trimming a
// law comment).
