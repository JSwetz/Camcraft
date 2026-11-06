//! The STAMP: the pass that reads a finished upright-ball path back
//! into an oriented one (M5.1 stage C). A sphere's envelope is
//! rotation-invariant about its own CENTRE, so the ball ladder a
//! 5-axis finish cuts is exactly the waterline/raster ladder of stage
//! A/B - and what makes it 5-axis is the pose each move holds: at
//! every move whose ball rests on the part, the nearest surface point
//! gives the normal (`d = |c - q| = r` is tangency, read at the BALL
//! CENTRE), the move's own travel gives the tangent, and
//! [`super::tilt::command_axis`] folds lead, lag and the clamp into
//! the direction the spindle must point. Moves whose ball is in air
//! (the hops, the clearances) and level floors hold no lean.
//!
//! The pivot law ([`super::retip`]): the base ladder stores TIPS of an
//! upright ball (finishing3d's convention, and the stream's - the post
//! locates the tool tip), so its centres stand `r` up Z from every
//! point. A pose turns the tool about that centre, never about the
//! tip, so every emitted point is the centre minus `r` along the pose
//! it holds. Keeping the tip and swapping the axis (what this pass did
//! before M13's follow-up) carried every posed ball `r·(a - up)` off
//! the skin it was planned on: most of a radius of air off a wall,
//! and a bite wherever a later upright move inherited the tilt.
//! Two consequences ride along:
//!
//! - **The post keeps the tilt modal.** A move stating nothing runs
//!   under the last pose on the machine, so once a lean has been
//!   stated every later move states its own: a cut the lean the law
//!   gives, or upright `Z` (which the post commands as explicit zero
//!   angles); a rapid the pose it inherits, so a retract lifts the
//!   ball straight off its last contact - the plan's own air path -
//!   instead of swinging the tool upright through that surface.
//! - **Posed arcs are chords.** A tip backed off a turning pose leaves
//!   any circle a G2/G3 could state, so an arc that leans anywhere is
//!   emitted as its chords at `ARC_FIT_TOL`, each end posed fresh; an
//!   arc that stays upright end to end keeps its exact intent.
//!
//! Provenance: the ball-center/axis-pose split and tool-tip (RTCP)
//! programming are standard multi-axis CL practice (tool-axis vectors
//! ride the location words, RS-274/ISO 10791 tradition); written from
//! those published descriptions and our own design notes only.

use crate::cl::{is_vertical, MoveKind, PathBuilder, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::avoid::{escape_axis, shank_hits_at, Shank};
use super::retip::{along, tip_of, UP};
use super::tilt::{command_axis, AxisControl};

/// The search ceiling shank avoidance tilts within when the control
/// states no cone (`max_tilt_deg: None` = "as far as the geometry
/// asks"): a horizontal spindle is the last pose anyone means, and
/// the cone's own law is a cone, not a plane.
pub(super) const ESCAPE_CAP: f64 = 85.0;

/// Read an upright ball's tip path back as an oriented one: the same
/// BALLS in the same order, each tip re-derived from its centre and
/// the pose it holds. `r` is the ball radius the path was generated
/// with; contact is decided at the centre by the tangent law
/// `|c - q| = r`, with a band that forgives the project's micron
/// lattice and the arc fit, never a whole radius of slack. Facets are
/// scanned with an AABB shell reject. `shank` (M5.1 stage F) is the
/// tool above the ball: where `Some`, a pose whose shank column
/// touches the part tilts to the nearest clear angle inside the cone
/// (about the same centre), and a move no cone angle clears refuses
/// the path by name.
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

/// One station of the expanded ladder: a move's end (an arc's chord
/// ends) as the BALL CENTRE the plan put there.
struct Station {
    kind: MoveKind,
    feed: Option<f64>,
    from: Point3,
    to: Point3,
    /// The base move this station belongs to.
    mv: usize,
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
    if !(r.is_finite() && r > 0.0) {
        return Err(GenError::Invalid(format!(
            "a 5-axis stamp needs a finite positive ball radius, got {r}"
        )));
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
    // The plan's balls: every tip lifted r up Z; arcs at their chords.
    let mut st: Vec<Station> = Vec::with_capacity(path.moves.len());
    for (i, mv) in path.moves.iter().enumerate() {
        let mut ends: Vec<Point3> = match mv.arc {
            Some(_) => mv.segments3(ARC_FIT_TOL).into_iter().map(|(_, e)| e).collect(),
            None => Vec::new(),
        };
        if ends.is_empty() {
            ends.push(mv.to);
        }
        let mut from = along(mv.from, UP, r);
        for e in ends {
            let to = along(e, UP, r);
            st.push(Station { kind: mv.kind, feed: mv.feed, from, to, mv: i });
            from = to;
        }
    }
    // The pose every station holds. `held` is the smoothing memory
    // (stage F): the axis the machine holds entering a station,
    // upright at the home the post wakes at. `leaned` turns true at
    // the first lean and never back: from there on every station
    // states its pose (the modal-tilt law above).
    let mut poses: Vec<Option<[f64; 3]>> = Vec::with_capacity(st.len());
    let mut held = UP;
    let mut leaned = false;
    for (k, s) in st.iter().enumerate() {
        let lean = match s.kind {
            MoveKind::Rapid => None,
            _ => cut_axis(s, &mesh, r, band, &st[k + 1..], c, ruling, shank, held)?,
        };
        leaned |= lean.is_some_and(|a| !is_vertical(&a));
        let pose = match (s.kind, leaned) {
            (_, false) => lean,
            // A rapid travels under the pose it inherits: the ball
            // leaves its last contact along the axis it cut with and
            // follows the plan's own air path, never swinging upright
            // through the surface it just left.
            (MoveKind::Rapid, true) => Some(held),
            (_, true) => lean.or(Some(UP)),
        };
        held = pose.unwrap_or(UP);
        poses.push(pose);
    }
    let mut b = PathBuilder::new(path.start().unwrap_or(Point3::ZERO));
    let mut k = 0usize;
    for (i, mv) in path.moves.iter().enumerate() {
        let k0 = k;
        while k < st.len() && st[k].mv == i {
            k += 1;
        }
        let group = &poses[k0..k];
        let upright = group.iter().all(|p| p.is_none_or(|a| is_vertical(&a)));
        match (mv.arc, upright, group.last()) {
            // A move that never leans keeps its exact intent: its
            // upright tips are the plan's own, bit for bit.
            (Some(a), true, Some(pose)) => {
                b.set_axis(*pose);
                b.push_arc_in(mv.kind, mv.to, mv.feed, a.center, a.ccw, a.plane);
            }
            (None, true, Some(pose)) => {
                b.set_axis(*pose);
                b.push(mv.kind, mv.to, mv.feed);
            }
            _ => {
                for (s, pose) in st[k0..k].iter().zip(group) {
                    b.set_axis(*pose);
                    b.push(s.kind, tip_of(s.to, *pose, r), s.feed);
                }
            }
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

/// The pose one cutting station holds, before and after the shank
/// speaks: None in air and on level floors, the surface normal with
/// lead/lag/clamp folded in wherever the station's ball rests on a
/// facet at its tangent distance. With a `shank` (stage F) a pose that
/// touches is re-aimed by the escape search about the same ball
/// centre; a station nothing inside the cone clears refuses the whole
/// path by name. `seed` is the cone's axis the search tilts around:
/// the contact normal where there is one, vertical wherever the pose
/// is an answer to air or a level floor.
#[allow(clippy::too_many_arguments)] // the stamp's flat read of one station
fn cut_axis(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    band: f64,
    rest: &[Station],
    c: &AxisControl,
    ruling: Option<(Point3, &[bool])>,
    shank: Option<&Shank>,
    held: [f64; 3],
) -> Result<Option<[f64; 3]>, GenError> {
    let (a0, seed, skip) = pose_law(s, mesh, r, band, rest, c, ruling);
    let Some(sh) = shank.filter(|_| !skip) else {
        return Ok(a0);
    };
    let a = a0.unwrap_or(UP);
    if !shank_hits_at(s.to, a, r, sh, mesh.facets, mesh.boxes) {
        return Ok(a0);
    }
    let cone = c.max_tilt_deg.unwrap_or(ESCAPE_CAP).min(ESCAPE_CAP);
    match escape_axis(s.to, a, seed, held, cone, r, sh, mesh.facets, mesh.boxes) {
        Some(b) => Ok(Some(b)),
        None => Err(GenError::Invalid(format!(
            "the shank of the tool touches the part at move {} of the posed path, and no \
             pose inside the max-tilt cone ({cone:.0} deg) clears it - widen the cone, \
             shorten the tool, or turn shank avoidance off",
            s.mv
        ))),
    }
}

/// Stage C's pose law alone - the angle the surface commands before
/// the shank has an opinion, read at the station's BALL CENTRE. The
/// `bool` says the law refused to state a direction (garbage in a
/// sane door's shadow): vertical it stays, and no escape invents an
/// angle the law itself would not compute.
fn pose_law(
    s: &Station,
    mesh: &Mesh,
    r: f64,
    band: f64,
    rest: &[Station],
    c: &AxisControl,
    ruling: Option<(Point3, &[bool])>,
) -> (Option<[f64; 3]>, Point3, bool) {
    let up = Point3::new(UP[0], UP[1], UP[2]);
    let p = s.to;
    let mut best: Option<(f64, Point3, usize)> = None;
    for (fi, (f, bx)) in mesh.facets.iter().zip(mesh.boxes.iter()).enumerate() {
        // Box distance: if the nearest gap already exceeds r + band,
        // no point of the box is in contact with the ball.
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
    // A centre ON the surface has no direction out of it at all.
    if d > r + band || d <= 1e-9 {
        return (None, up, false);
    }
    let n = Point3::new((p.x - q.x) / d, (p.y - q.y) / d, (p.z - q.z) / d);
    // A level floor: the contact stands straight under the centre,
    // there is no flank to lean off, and the honest pose for a floor
    // is the one it always had (upright).
    if 1.0 - n.z <= 1e-12 {
        return (None, up, false);
    }
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
    // The feed direction: this station's travel; a vertical entry has
    // none of its own, so the next station's states where the cut
    // goes. M8.3: a dive DEAD DOWN onto a flank has travel, but none
    // ALONG a cut that has not started - its own vector would lean the
    // lead inside the plunge plane. The cut the dive lands into is the
    // next station's, which is what the drive-curve family stamps its
    // first station with (curve.rs). A dive that lands on nothing (the
    // path ends here) keeps its own vector.
    let mut t = p - s.from;
    if tl(t) <= 1e-9 || (t.x * t.x + t.y * t.y <= 1e-18 && 1.0 - n.z * n.z > 1e-18) {
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
