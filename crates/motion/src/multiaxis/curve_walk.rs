//! The drive-curve WALK (M5.1 J, pivot-law follow-up): the ball
//! centres a picked chain is cut through, dense enough that the
//! straight chords between them keep the ball on the skin. Resting
//! the ball at the chain's vertices alone is not enough: on a convex
//! edge between two corner rests the straight chord of the centres
//! sags `r·(1 - √(2/3))` into the edge (0.55 mm at r = 3 on a box's
//! top edge), and round a convex corner the rest direction jumps
//! between the two edges' bisectors. So:
//!
//! - **Segments subdivide** at their midpoints until the midpoint's
//!   own rest stands within `EPS` of the chord between its
//!   neighbours (curved chains, face changes) - bounded depth.
//! - **Convex vertices roll.** Where the ball rests ON the vertex
//!   point itself (touch height `r` - the convex case), the segments
//!   end on their own edge rests a hair short of the vertex, and the
//!   join is an arc of centres on the radius-`r` sphere about the
//!   vertex, stepped so the chord sag stays inside `EPS`: the ball
//!   rolls round the corner touching it, never cutting across.
//! - **Concave vertices keep their own rest** (the groove bisector,
//!   `r / cos` out): there the faces run on toward the ball, a roll
//!   would bury it, and straight chords to the vertex rest stand off
//!   every face they approach.
//!
//! Provenance: ball-centre offset loci of polyhedral edges and
//! vertices (the Minkowski sum of a polyhedron and a ball: planes,
//! cylinders about edges, spheres about vertices) from public
//! computational-geometry and CAM offset literature; chord-sag
//! stepping is the standard arc-fit law. Our own composition.

use crate::geometry::Point3;
use crate::machining2d::GenError;

/// The walk's chord budget, mm: the centre chord between two stations
/// stands within this of the rest locus (a quarter of the house arc
/// fit, so the envelope verdict's own budgets dwarf it).
const EPS: f64 = 0.0025;

/// Recursion depth per segment: 2^12 stations at most.
const DEPTH: u32 = 12;

fn sub(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn add(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn scale(a: Point3, s: f64) -> Point3 {
    Point3::new(a.x * s, a.y * s, a.z * s)
}

fn len(a: Point3) -> f64 {
    a.dot(a).sqrt()
}

fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    add(a, scale(sub(b, a), t))
}

/// A rest the station law answered: the ball centre and its unit
/// direction out of the contact.
#[derive(Clone, Copy, Debug)]
pub(super) struct Rest {
    pub centre: Point3,
    pub dir: Point3,
}

/// The station law the walk asks: the rest at chain point `q` (the
/// `usize` names the chain point for a refusal's message).
pub(super) type RestFn<'a> = dyn FnMut(Point3, usize) -> Result<(Point3, f64), GenError> + 'a;

fn rest(law: &mut RestFn, q: Point3, at: usize) -> Result<Rest, GenError> {
    let (dir, h) = law(q, at)?;
    Ok(Rest { centre: add(q, scale(dir, h)), dir })
}

/// The dense centre walk of one chain: `pts` its distinct points (a
/// ring closes on its own first point, which is NOT repeated here),
/// `r` the ball radius. Returns the stations in walk order; a ring's
/// list ends where it began.
pub(super) fn walk(pts: &[Point3], ring: bool, r: f64, law: &mut RestFn) -> Result<Vec<Rest>, GenError> {
    let k = pts.len();
    if k < 2 {
        return Ok(Vec::new());
    }
    // Each vertex's own rest, and whether the ball touches the vertex
    // point itself there (the convex case: it rolls).
    let mut vert: Vec<(Rest, bool)> = Vec::with_capacity(k);
    for (i, q) in pts.iter().enumerate() {
        let v = rest(law, *q, i)?;
        let convex = len(sub(v.centre, *q)) <= r * (1.0 + 1e-9) + 1e-12;
        vert.push((v, convex));
    }
    let nseg = if ring { k } else { k - 1 };
    let mut out: Vec<Rest> = Vec::new();
    let mut first_in: Option<Rest> = None;
    for s in 0..nseg {
        let (ia, ib) = (s, (s + 1) % k);
        let (qa, qb) = (pts[ia], pts[ib]);
        let l = len(sub(qb, qa));
        if !(l.is_finite() && l > 0.0) {
            continue;
        }
        // A hair off a convex vertex, the segment's own edge/face rest:
        // far enough that the fan reads the edge, not the corner.
        let ta = if vert[ia].1 { 1e-3 } else { 0.0 };
        let tb = if vert[ib].1 { 1.0 - 1e-3 } else { 1.0 };
        let a = if vert[ia].1 { rest(law, lerp(qa, qb, ta), ia)? } else { vert[ia].0 };
        let b = if vert[ib].1 { rest(law, lerp(qa, qb, tb), ib)? } else { vert[ib].0 };
        // The join into this segment at its start vertex: a convex
        // vertex rolls from the last segment's end rest to this one's
        // start rest; a concave one already ended the last segment
        // ON its own rest, which is this segment's start.
        match out.last().copied() {
            None => {
                first_in = Some(a);
                out.push(a);
            }
            Some(prev) if vert[ia].1 => {
                roll(&mut out, qa, prev, a, r);
                out.push(a);
            }
            Some(_) => {}
        }
        subdivide(&mut out, law, qa, qb, (ta, a), (tb, b), ia, 0)?;
        out.push(b);
    }
    if ring && vert[0].1 {
        // Close a ring round a convex vertex 0: roll from the last
        // segment's end back to the first segment's start. (Concave:
        // the last segment already ended on vertex 0's own rest.)
        if let (Some(prev), Some(f)) = (out.last().copied(), first_in) {
            roll(&mut out, pts[0], prev, f, r);
            out.push(f);
        }
    }
    Ok(out)
}

/// Midpoint subdivision of the chain between parameters `ta`, `tb`
/// (on `qa -> qb`) whose rests are `a`, `b`: pushes the INTERIOR
/// stations in order.
#[allow(clippy::too_many_arguments)] // the recursion's flat frame
fn subdivide(
    out: &mut Vec<Rest>,
    law: &mut RestFn,
    qa: Point3,
    qb: Point3,
    (ta, a): (f64, Rest),
    (tb, b): (f64, Rest),
    at: usize,
    depth: u32,
) -> Result<(), GenError> {
    if depth >= DEPTH {
        return Ok(());
    }
    let tm = 0.5 * (ta + tb);
    let m = rest(law, lerp(qa, qb, tm), at)?;
    let chord = lerp(a.centre, b.centre, 0.5);
    if len(sub(m.centre, chord)) <= EPS {
        return Ok(());
    }
    subdivide(out, law, qa, qb, (ta, a), (tm, m), at, depth + 1)?;
    out.push(m);
    subdivide(out, law, qa, qb, (tm, m), (tb, b), at, depth + 1)
}

/// Roll the ball round convex vertex `q` from rest `a` to rest `b`:
/// centres on the radius-`r` sphere about `q`, stepped so the chord
/// sag stays inside `EPS`. Pushes the interior stations only; a turn
/// too small to sag is a straight chord already.
fn roll(out: &mut Vec<Rest>, q: Point3, a: Rest, b: Rest, r: f64) {
    let (ua, ub) = (sub(a.centre, q), sub(b.centre, q));
    let (la, lb) = (len(ua), len(ub));
    if !(la > 0.0 && lb > 0.0) {
        return;
    }
    let (ua, ub) = (scale(ua, 1.0 / la), scale(ub, 1.0 / lb));
    let cos = ua.dot(ub).clamp(-1.0, 1.0);
    let th = cos.acos();
    // Sag of a chord spanning angle s on radius r: r·(1 - cos(s/2)).
    let step = 2.0 * (1.0 - (EPS / r.max(EPS)).min(1.0)).acos();
    if !(th.is_finite() && step.is_finite() && step > 0.0) || th <= step {
        return;
    }
    let n = ((th / step).ceil() as usize).clamp(2, 4096);
    let sin = th.sin();
    for j in 1..n {
        let t = j as f64 / n as f64;
        let dir = if sin > 1e-12 {
            let (wa, wb) = (((1.0 - t) * th).sin() / sin, (t * th).sin() / sin);
            add(scale(ua, wa), scale(ub, wb))
        } else {
            let v = lerp(ua, ub, t);
            scale(v, 1.0 / len(v).max(1e-300))
        };
        out.push(Rest { centre: add(q, scale(dir, r)), dir });
    }
}
