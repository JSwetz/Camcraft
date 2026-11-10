//! The rotary wrap's seam closure (issue #54): the curtain that closes
//! the developed sheet's two free seam edges, and the closed-solid law
//! the wrap demands of its work.
//!
//! [`super::rotary_offset`] develops a closed solid lap by lap: a facet
//! whose columns straddle the ±π branch rides one turn low, so each mesh
//! edge between such a facet and a plain one is developed TWICE, one
//! period apart - a cut. Two copies of the sheet, the second one period
//! up, weld the first copy's high side of every cut to the second's low
//! side; what is left free is the first copy's low side and the second
//! copy's high side: the cover's two ends. Each free seam edge gets a
//! CURTAIN - the vertical quad hanging from the edge straight down to
//! the floor radius - so every level's chord chain runs down the curtain
//! and along the next one, and closes. On a revolve the seam is one
//! straight column and the curtain is exactly the meridian wall at the
//! cover end; on a bar with a flat (a D-flat, a keyway) the seam column
//! is ragged - its vertices sit at their own azimuths - and a wall in the
//! plane of the cover's extreme column met none of those edges (the
//! "mesh is open" #9 round 2 hit).
//!
//! The curtain never closes a real hole: only an edge the closed mesh
//! shares between two facets, developed one period apart, is a seam
//! edge. An edge only one facet holds is a hole in the skin, and the wrap
//! refuses the work by name before developing it ([`refuse_open`]) - a
//! level that never crosses the hole would otherwise rough on as if the
//! skin were whole.
//!
//! Provenance: closed 2-manifold edge topology (every edge shared by
//! exactly two facets) and the development of a cylinder cut along one
//! meridian, from standard solid-modelling and differential-geometry
//! texts; composed on this crate's own wrap map. No vendor material.

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// World weld grid: 1 unit = 1 um, the slicer's own quantization.
const WELD: f64 = 1e6;
/// Seam vertices whose azimuths differ by less than this (radians)
/// stand on one meridian column (#228): the weld grid's grain read as
/// an angle - 1 um of arc per mm of radius, the same relative quantum
/// the column profiles are compared at, and coarser than a float32
/// STL's own azimuth noise. A flat's or keyway's ragged vertices stand
/// apart by whole degrees. It was 1e-9, inside the axis fit's rounding.
const SEAM_TH_TOL: f64 = 1.0 / WELD;

type Key = (i64, i64, i64);

fn key(p: Point3) -> Key {
    let q = |v: f64| (v * WELD).round() as i64;
    (q(p.x), q(p.y), q(p.z))
}

fn edge_key(a: Key, b: Key) -> (Key, Key) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// One developed edge of an original facet: its world endpoints' weld
/// keys and the developed points standing for them, in that order.
pub(super) struct DevEdge {
    pub(super) world: (Key, Key),
    pub(super) a: Point3,
    pub(super) b: Point3,
}

impl DevEdge {
    pub(super) fn new(wa: Point3, wb: Point3, a: Point3, b: Point3) -> Self {
        DevEdge { world: (key(wa), key(wb)), a, b }
    }
}

/// Refuse a work whose mesh is open: an edge held by one facet only is a
/// hole in the skin, and no wrap of it is the solid it stands for.
pub(super) fn refuse_open(facets: &[Triangle]) -> Result<(), GenError> {
    let mut count: HashMap<(Key, Key), usize> = HashMap::new();
    for t in facets {
        let (a, b, c) = (key(t.a), key(t.b), key(t.c));
        if a == b || b == c || c == a {
            continue; // a degenerate sliver bounds nothing
        }
        for e in [edge_key(a, b), edge_key(b, c), edge_key(c, a)] {
            *count.entry(e).or_insert(0) += 1;
        }
    }
    let open = count.values().filter(|n| **n == 1).count();
    if open > 0 {
        return Err(GenError::Invalid(format!(
            "the rotary wrap needs a closed solid, but the mesh is open: {open} edges bound one facet only (a hole in the skin)"
        )));
    }
    Ok(())
}

/// Is the seam turned? `cols` are the off-axis vertices `(azimuth,
/// station, radius)` of every facet straddling the wrap's ±π branch.
/// On a revolve those stand on exactly two meridian columns, one each
/// side of the branch, with one profile: `None`. A flat, a keyway or a
/// cross hole crossing the seam breaks that - its vertices sit at their
/// own azimuths (a ragged seam), or the columns either side carry
/// different radii - and so does a turned mesh whose diameters were
/// tessellated with different column counts (#228): `Some(columns)`,
/// the number of meridian columns meeting the seam. Such a seam is not
/// wrapped - the cover's ends would read the metal another lap holds as
/// air (PR #210's review: cut tips down to the axis).
pub(super) fn seam_columns(cols: &[(f64, f64, f64)], scale: f64) -> Option<usize> {
    let q = |x: f64| (x * WELD / scale.max(1.0)).round() as i64;
    let mut columns: Vec<(f64, Vec<(i64, i64)>)> = Vec::new();
    for &(th, v, rho) in cols {
        let a = th.rem_euclid(std::f64::consts::TAU);
        let at = (q(v), q(rho));
        match columns.iter_mut().find(|(c, _)| (c - a).abs() <= SEAM_TH_TOL) {
            Some((_, prof)) => prof.push(at),
            None => columns.push((a, vec![at])),
        }
    }
    for (_, prof) in columns.iter_mut() {
        prof.sort_unstable();
        prof.dedup();
    }
    match columns.as_slice() {
        [] => None,
        [(_, a), (_, b)] if a == b => None,
        _ => Some(columns.len()),
    }
}

/// The refusal of a seam that is not turned, `n` columns meeting it;
/// `everywhere` when no seam angle the wrap could take is turned (issue
/// #226: a feature all the way round).
pub(super) fn seam_refusal(n: usize, everywhere: bool) -> GenError {
    let at = if everywhere {
        "at every angle the seam could take (a feature all the way round), "
    } else {
        ""
    };
    GenError::Invalid(format!(
        "a feature that is not turned (a flat, a keyway, a cross hole) crosses the rotary wrap's seam {at}or the mesh tessellates its diameters with different column counts ({n} meridian columns meet the seam, a turned seam has 2 alike): refused rather than read the seam's metal as air - leave the bar a turned band, or export the model with one column count"
    ))
}

/// The curtains closing the developed sheet's seam: `edges` are the
/// FIRST copy's developed edges (three per facet), `period` the lap's
/// arc length (the second copy rides one period up), `floor` the radius
/// the curtains hang down to. An edge the two facets sharing it develop
/// at the same place is interior; one developed a period apart is a seam
/// edge, and its lower version (first copy) and its upper version
/// (second copy, one period up) are the sheet's two free ends.
pub(super) fn curtains(edges: &[DevEdge], period: f64, floor: f64) -> Vec<Triangle> {
    let mut by: HashMap<(Key, Key), Vec<usize>> = HashMap::new();
    // Three edges per facet, in facet order; a sliver (two corners
    // welded together) takes no part, as in [`refuse_open`].
    for (k, f) in edges.chunks(3).enumerate() {
        if f.len() < 3 || f.iter().any(|e| e.world.0 == e.world.1) {
            continue;
        }
        for (i, e) in f.iter().enumerate() {
            by.entry(edge_key(e.world.0, e.world.1)).or_default().push(3 * k + i);
        }
    }
    let tol = 1e-6 * period.abs().max(1.0);
    let mut out = Vec::new();
    // Sorted for a program that does not hang on the hash order.
    let mut pairs: Vec<(&(Key, Key), &Vec<usize>)> = by.iter().collect();
    pairs.sort_by(|x, y| x.0.cmp(y.0));
    for (_, ids) in pairs {
        let (Some(&i), Some(&j), 2) = (ids.first(), ids.get(1), ids.len()) else {
            continue; // open or non-manifold: the slicer says which
        };
        let (Some(e), Some(f)) = (edges.get(i), edges.get(j)) else { continue };
        // f's points in e's world order.
        let (fa, fb) = if f.world.0 == e.world.0 { (f.a, f.b) } else { (f.b, f.a) };
        let (da, db) = (fa.x - e.a.x, fb.x - e.b.x);
        let (lo, hi) = if (da - period).abs() <= tol && (db - period).abs() <= tol {
            ((e.a, e.b), (fa, fb))
        } else if (da + period).abs() <= tol && (db + period).abs() <= tol {
            ((fa, fb), (e.a, e.b))
        } else {
            continue; // interior (or a lap mismatch the slicer reports)
        };
        let up = |p: Point3| Point3::new(p.x + period, p.y, p.z);
        hang(&mut out, lo.0, lo.1, floor);
        hang(&mut out, up(hi.0), up(hi.1), floor);
    }
    out
}

/// The vertical quad under the developed edge `p`-`q`, down to `floor`,
/// split on the diagonal from the low-station foot to the high-station
/// top (the meridian wall's own split, so a revolve slices as it did).
fn hang(out: &mut Vec<Triangle>, p: Point3, q: Point3, floor: f64) {
    let (p, q) = if p.y <= q.y { (p, q) } else { (q, p) };
    let (pf, qf) = (Point3::new(p.x, p.y, floor), Point3::new(q.x, q.y, floor));
    let real = |a: Point3, b: Point3, c: Point3| {
        let (ab, ac) = (b - a, c - a);
        let n = ab.cross(ac);
        n.dot(n) > 0.0
    };
    if real(pf, qf, q) {
        out.push(Triangle::new(pf, qf, q));
    }
    if real(pf, q, p) {
        out.push(Triangle::new(pf, q, p));
    }
}
