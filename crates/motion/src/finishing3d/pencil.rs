//! Pencil & corner finishing (M2.5 B): the corner locus a ball can
//! never reach, ridden by the ball that reaches deepest.
//!
//! Provenance: corner rest is textbook machining geometry - a sphere
//! in a concave crease stands `r / sin(theta/2)` off the edge line
//! and grazes neither face nearer than `r / tan(theta/2)`: a band
//! along every concave edge that no pass of that ball can clean.
//! Riding the simultaneous tangent to both faces (the bisector
//! locus) is published corner-finishing kinematics; the construction
//! below - welded edge pairs, the two-plane tangent test, the
//! longest-valid-run shrink and the deterministic chain walk - is
//! our own, written from that geometry, not from any vendor's code,
//! output forensics, or documentation text. The point arithmetic
//! (rests, shrink, chain) lives beside it in [`super::crease`].
//!
//! ## The laws here
//!
//! - **Concave by both faces' say-so.** An edge is a corner only
//!   when each face's opposite vertex stands above the other face's
//!   plane (the pocket floor-and-wall test), and the two triangles
//!   must run the shared edge opposite ways - a same-way pair is a
//!   flipped winding: skipped by note, never guessed through.
//! - **The locus is the deepest legal center.** Center = edge point +
//!   `2r(nA + nB) / |nA + nB|^2`: tangent to both planes at once,
//!   closed form, no angle ever taken (`|nA + nB| = 2 sin(theta/2)`).
//!   Emitted only where the ball RESTS, not floats: clear of every
//!   facet, touching the solid at exactly `r`, and both contact feet
//!   standing on real facets - the Prime Directive's three
//!   questions, asked of every sample. The center's tip also keeps
//!   the programmed window: nothing here cuts below `bottom_z`.
//! - **The band decides who cuts.** `Corner` (self-reference) cuts
//!   where the corner's own band `r / tan(theta/2)` beats the
//!   tolerance - a near-coplanar meeting has nothing to clean; where
//!   a level ladder already rode this deepest position the pass is
//!   idempotent: it grazes, never gouges, and the envelope verdict
//!   watches. `Pencil` cuts where a stated reference ball's band
//!   reaches deeper than this tool's by more than `tol`:
//!   `(R - r) / tan > tol` - the big ball's hidden rest at
//!   single-op scale: the program-order mask (the app's `rest`
//!   module) answers that question across OPERATIONS; the band
//!   answers it for one tool against another in one program, no
//!   grid needed. Reference absent says "clean every corner"; a
//!   reference not bigger than the op's ball is refused by name.
//! - **Chains weld at equal locus ends, loops close.** A tessellated
//!   corner (one edge per triangle pair along a pocket floor) joins
//!   into polylines; the weld skin is float, so loci that merely MEET
//!   at a corner vertex (their ends stand a fraction of `r` apart
//!   there) stay the separate runs they are. Segment, candidate and
//!   run order are all fixed by sorting: the path is byte-identical
//!   run to run.
//! - **Ladder law.** Each run: rapid over its start, vertical plunge
//!   down the tangent column, feeds along the locus, hop to
//!   clearance. Corner runs are different features, not levels of
//!   one ladder, so between runs the hop is to clearance z. Moves
//!   speak TIPS, as everywhere else in finishing3d: the locus is a
//!   center line, so every emitted point is its center lowered by
//!   `r` - a path that posted centers would run the ball a radius
//!   clear of the crease it was made for, and the envelope scan's
//!   `+r` lift would read that silence as a clean bill.

use std::collections::HashMap;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::crease::{add3, chain, cmp_pt, scale3, shrink, sub3, third, vkey, vkey_scale, Seg};
use super::params::{validate, Finish3DParams};
use super::support::build_sups;

fn vid(pt: Point3, s: f64, verts: &mut Vec<Point3>, ids: &mut HashMap<(i64, i64, i64), usize>) -> usize {
    let k = vkey(pt, s);
    if let Some(&i) = ids.get(&k) {
        return i;
    }
    let i = verts.len();
    verts.push(pt);
    ids.insert(k, i);
    i
}

/// Pencil: the corner bands the reference ball leaves standing,
/// ridden by this op's (smaller) ball. `pencil_reference` is the pen:
/// `Some(d)` says "what a ball of diameter d could not reach";
/// `None` says every concave corner; `d` not bigger than the op's
/// own tool is refused by name - a tool that is not bigger hides
/// under no band.
pub fn pencil(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    let reference = match p.pencil_reference {
        None => f64::INFINITY,
        Some(d) => {
            if !(d.is_finite() && d > p.tool_diameter) {
                return Err(GenError::Invalid(format!(
                    "pencil reference diameter {d} does not exceed the op's ball {} - a tool that is not bigger hides under no band",
                    p.tool_diameter
                )));
            }
            d * 0.5
        }
    };
    loci(facets, p, r, reference, "pencil")
}

/// Corner: the simultaneous two-face tangent pass with the
/// operation's own ball. `pencil_reference` is inert here, as if it
/// had never arrived.
pub fn corner(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    loci(facets, p, r, r, "corner")
}

/// The shared-edge index: unordered vertex pair -> the (face,
/// from-vertex, to-vertex) records of every facet running it.
type EdgeIndex = HashMap<(usize, usize), Vec<(usize, usize, usize)>>;

fn loci(
    facets: &[Triangle],
    p: &Finish3DParams,
    r: f64,
    reference: f64,
    what: &str,
) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let sups = build_sups(facets, r);
    let s = vkey_scale(facets);

    // Weld vertices; bucket directed edges by the unordered pair so
    // both faces of one edge meet in one entry.
    let mut verts: Vec<Point3> = Vec::new();
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut vix: Vec<[usize; 3]> = Vec::with_capacity(facets.len());
    for f in facets {
        vix.push([
            vid(f.a, s, &mut verts, &mut ids),
            vid(f.b, s, &mut verts, &mut ids),
            vid(f.c, s, &mut verts, &mut ids),
        ]);
    }
    let mut edges: EdgeIndex = HashMap::new();
    for (fi, tri) in vix.iter().enumerate() {
        for (from, to) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if from < to { (from, to) } else { (to, from) };
            edges.entry(key).or_default().push((fi, from, to));
        }
    }
    let mut keys: Vec<(usize, usize)> = edges.keys().copied().collect();
    keys.sort_unstable();

    let mut segs: Vec<Seg> = Vec::new();
    let mut flips = 0usize;
    let mut manifold = 0usize;
    for &(lo, hi) in &keys {
        let list = &edges[&(lo, hi)];
        if list.len() != 2 {
            if list.len() > 2 {
                manifold += 1;
            }
            continue; // a boundary edge of an open mesh cuts no corner
        }
        let (fi, a0, a1) = list[0];
        let (fj, b0, b1) = list[1];
        if (a0, a1) == (b0, b1) {
            flips += 1; // same direction in both faces: one winding is flipped
            continue;
        }
        if fi == fj {
            continue; // the same face twice is a degenerate triangle, not a crease
        }
        let (na, nb) = (facets[fi].normal(), facets[fj].normal());
        if !(na.dot(na) > 0.0 && nb.dot(nb) > 0.0) {
            continue; // a degenerate facet owns no corner
        }
        let (na, nb) = (
            scale3(na, 1.0 / na.dot(na).sqrt()),
            scale3(nb, 1.0 / nb.dot(nb).sqrt()),
        );
        let (e0, e1) = (verts[a0], verts[a1]);
        // Concave: each face's opposite vertex stands above the
        // other's plane.
        let qa = third(facets[fj], e0, e1); // inside face B
        let qb = third(facets[fi], e0, e1); // inside face A
        if sub3(qa, e0).dot(na) <= 0.0 || sub3(qb, e0).dot(nb) <= 0.0 {
            continue; // convex (or the mesh just has no floor here)
        }
        // The band test (module header). tan(theta/2) read from the
        // normals: theta = pi - phi, phi = angle(nA, nB).
        let ndot = na.dot(nb).clamp(-1.0, 1.0);
        let tan_half = if ndot > 1.0 - 1e-12 {
            continue; // flat meeting: no corner to clean
        } else if ndot < -1.0 + 1e-12 {
            0.0 // needle crack; the locus solve below has the last word
        } else {
            ((1.0 + ndot) / (1.0 - ndot)).sqrt()
        };
        let band_ok = if reference.is_infinite() {
            true
        } else if reference > r {
            (reference - r) / tan_half.max(1e-9) > p.tolerance
        } else {
            reference / tan_half.max(1e-9) > p.tolerance
        };
        if !band_ok {
            continue;
        }
        // Locus: center = edge point + 2r(nA + nB) / |nA + nB|^2.
        let sum = add3(na, nb);
        let len2 = sum.dot(sum);
        if len2 < 1e-8 {
            continue; // no bisector direction to stand on
        }
        let off = scale3(sum, 2.0 * r / len2);
        let Some((c0, c1)) = shrink(
            add3(e0, off),
            add3(e1, off),
            na,
            nb,
            p,
            r,
            &sups,
        ) else {
            continue; // this stretch floats; another edge's run covers it
        };
        segs.push(Seg {
            c0,
            c1,
            k0: vkey(c0, s),
            k1: vkey(c1, s),
        });
    }

    let mut runs = chain(&segs);
    if runs.is_empty() {
        return Err(GenError::Empty(format!(
            "{what} found no concave corner to clean inside the programmed depths - a corner is a bend past flat whose tangent locus rests on the solid"
        )));
    }
    runs.sort_by(|a, b| cmp_pt(a[0], b[0]));

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    if flips > 0 {
        b.note(format!("{what}: {flips} shared edges skipped for inconsistent winding"));
    }
    if manifold > 0 {
        b.note(format!(
            "{what}: {manifold} edges touch more than two facets and were skipped"
        ));
    }
    for run in &runs {
        let first = run[0];
        b.push(
            MoveKind::Rapid,
            Point3::new(first.x, first.y, p.clearance_z),
            None,
        );
        let tip = |q: Point3| Point3::new(q.x, q.y, q.z - r);
        b.push(MoveKind::Plunge, tip(first), Some(p.plunge_feed));
        for &pt in &run[1..] {
            b.push(MoveKind::Feed, tip(pt), Some(p.feed));
        }
    }
    let cur = b.at();
    b.push(
        MoveKind::Rapid,
        Point3::new(cur.x, cur.y, p.clearance_z),
        None,
    );
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid(format!(
            "{what} path left the finite horizon"
        )));
    }
    Ok(path)
}

