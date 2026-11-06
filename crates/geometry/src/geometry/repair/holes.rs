//! Hole filling: the boundary loops of a shared-vertex mesh, each closed
//! by ear clipping in its best-fit plane (Liepa 2003's planar case;
//! Meisters 1975 / Eberly 2002 for the clipping, Newell's method for
//! the plane - see the module header of `repair`). Provenance: public
//! literature only, no vendor CAM material.
//!
//! Winding law: a boundary half-edge `a -> b` of the mesh is walked
//! `b -> a` by its hole loop, so every fill triangle traverses the shared
//! edge opposite to the facet it closes against - the orientation a
//! signed-volume check can prove.
//!
//! Refusals, by name: a non-manifold edge (three or more facets), an
//! inconsistently wound edge, a pinch vertex where two holes meet, a
//! loop over the caller's edge budget, a non-planar loop, a
//! self-intersecting loop, a loop no ear can be clipped from.

use std::collections::{HashMap, HashSet};

use super::{index_exact, Indexed, RepairReport};
use crate::geo2d::GeoError;
use crate::geometry::{Mesh, Point3, Triangle};

/// How far off its best-fit plane a loop may stray, as a fraction of its
/// bounding-box diagonal - a sagging seam fills, a crumpled one refuses.
const PLANAR_FRACTION: f64 = 0.05;

/// Find every boundary loop and close each one with `len - 2` triangles.
/// Loops longer than `max_edges` refuse by name; a mesh with no open
/// edges reports `holes_filled == 0` and hands the same facets back.
pub fn fill_holes(mesh: &Mesh, max_edges: usize) -> Result<RepairReport, GeoError> {
    if max_edges < 3 {
        return Err(GeoError::Invalid(format!(
            "hole budget must be at least 3 edges (got {max_edges})"
        )));
    }
    let ix = index_exact(mesh)?;
    if ix.tris.iter().any(|t| t[0] == t[1] || t[1] == t[2] || t[2] == t[0]) {
        return Err(GeoError::Invalid(
            "collapsed triangle in the mesh: weld before filling holes".into(),
        ));
    }
    let loops = boundary_loops(&ix)?;
    let mut added: Vec<Triangle> = Vec::new();
    for lp in &loops {
        if lp.len() > max_edges {
            return Err(GeoError::Invalid(format!(
                "hole of {} edges exceeds the {} edge budget",
                lp.len(),
                max_edges
            )));
        }
        triangulate_loop(&ix.pts, lp, &mut added)?;
    }
    let triangles_added = added.len();
    let mut facets: Vec<Triangle> = mesh.facets.to_vec();
    facets.extend(added);
    Ok(RepairReport {
        vertices_before: ix.pts.len(),
        vertices_after: ix.pts.len(),
        triangles_before: mesh.len(),
        triangles_after: facets.len(),
        welded: 0,
        dropped: 0,
        holes_filled: loops.len(),
        triangles_added,
        mesh: Mesh::from_facets(facets),
    })
}

/// How many facets use an undirected edge, and how many walk it low -> high.
struct EdgeUse {
    lo: u32,
    hi: u32,
    total: u32,
    forward: u32,
}

fn at(p: Point3) -> String {
    format!("({:.4}, {:.4}, {:.4})", p.x, p.y, p.z)
}

/// Every hole as a vertex ring in fill winding (reversed boundary
/// half-edges), ordered by lowest vertex id so output is deterministic.
fn boundary_loops(ix: &Indexed) -> Result<Vec<Vec<u32>>, GeoError> {
    // (total facets, facets walking low -> high) per undirected edge.
    let mut count: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    for t in &ix.tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = count.entry(if a < b { (a, b) } else { (b, a) }).or_insert((0, 0));
            e.0 += 1;
            if a < b {
                e.1 += 1;
            }
        }
    }
    // The hole walks each boundary half-edge backwards: next[b] = a.
    let mut next: HashMap<u32, u32> = HashMap::new();
    let mut entries: Vec<EdgeUse> = count.into_iter().map(|((lo, hi), (t, f))| EdgeUse { lo, hi, total: t, forward: f }).collect();
    entries.sort_unstable_by_key(|e| (e.lo, e.hi));
    for EdgeUse { lo, hi, total, forward } in entries {
        let (pa, pb) = (ix.pts[lo as usize], ix.pts[hi as usize]);
        match total {
            1 => {
                let (a, b) = if forward == 1 { (lo, hi) } else { (hi, lo) };
                if next.insert(b, a).is_some() {
                    return Err(GeoError::Invalid(format!(
                        "non-manifold vertex at {}: two holes meet there",
                        at(ix.pts[b as usize])
                    )));
                }
            }
            2 if forward != 1 => {
                return Err(GeoError::Invalid(format!(
                    "inconsistently wound edge {}-{}: its two facets face opposite ways",
                    at(pa),
                    at(pb)
                )));
            }
            2 => {}
            n => {
                return Err(GeoError::Invalid(format!(
                    "non-manifold edge {}-{} shared by {n} facets",
                    at(pa),
                    at(pb)
                )));
            }
        }
    }
    let mut starts: Vec<u32> = next.keys().copied().collect();
    starts.sort_unstable();
    let mut done: HashSet<u32> = HashSet::with_capacity(next.len());
    let mut loops = Vec::new();
    for start in starts {
        if done.contains(&start) {
            continue;
        }
        let mut ring = vec![start];
        done.insert(start);
        let mut cur = start;
        loop {
            let Some(&n) = next.get(&cur) else {
                return Err(GeoError::Invalid(format!(
                    "boundary chain does not close at {}",
                    at(ix.pts[cur as usize])
                )));
            };
            if n == start {
                break;
            }
            if !done.insert(n) {
                return Err(GeoError::Invalid(format!(
                    "boundary is not a simple loop: pinch vertex at {}",
                    at(ix.pts[n as usize])
                )));
            }
            ring.push(n);
            cur = n;
        }
        if ring.len() < 3 {
            return Err(GeoError::Invalid(format!(
                "degenerate boundary loop of {} edge(s) at {}",
                ring.len(),
                at(ix.pts[start as usize])
            )));
        }
        loops.push(ring);
    }
    Ok(loops)
}

fn unit(p: Point3) -> Option<Point3> {
    let l2 = p.dot(p);
    if l2.is_finite() && l2 > 0.0 {
        let inv = 1.0 / l2.sqrt();
        Some(Point3::new(p.x * inv, p.y * inv, p.z * inv))
    } else {
        None
    }
}

/// Best-fit plane of a ring by Newell's method: the unit normal (facing
/// the side from which the ring runs counter-clockwise), the centroid
/// and the bounding-box diagonal.
fn fit_plane(ring: &[Point3]) -> Option<(Point3, Point3, f64)> {
    let n = ring.len() as f64;
    let mut nrm = Point3::ZERO;
    let mut c = Point3::ZERO;
    let first = *ring.first()?;
    let (mut lo, mut hi) = (first, first);
    for (i, &p) in ring.iter().enumerate() {
        let q = ring[(i + 1) % ring.len()];
        nrm.x += (p.y - q.y) * (p.z + q.z);
        nrm.y += (p.z - q.z) * (p.x + q.x);
        nrm.z += (p.x - q.x) * (p.y + q.y);
        c = Point3::new(c.x + p.x / n, c.y + p.y / n, c.z + p.z / n);
        lo = Point3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Point3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let diag = hi - lo;
    Some((unit(nrm)?, c, diag.dot(diag).sqrt()))
}

/// Signed doubled area of `a b c` in the plane (positive = counter-clockwise).
fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn on_segment(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    p[0] >= a[0].min(b[0]) && p[0] <= a[0].max(b[0]) && p[1] >= a[1].min(b[1]) && p[1] <= a[1].max(b[1])
}

/// Do segments `ab` and `cd` meet (crossing, touching or overlapping)?
fn segments_meet(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return true; // a proper crossing
    }
    (o1 == 0.0 && on_segment(a, b, c))
        || (o2 == 0.0 && on_segment(a, b, d))
        || (o3 == 0.0 && on_segment(c, d, a))
        || (o4 == 0.0 && on_segment(c, d, b))
}

/// Is `p` inside or on triangle `a b c` (counter-clockwise)?
fn in_triangle(a: [f64; 2], b: [f64; 2], c: [f64; 2], p: [f64; 2]) -> bool {
    orient(a, b, p) >= 0.0 && orient(b, c, p) >= 0.0 && orient(c, a, p) >= 0.0
}

/// Close one ring: project onto its plane, prove it simple, clip ears.
fn triangulate_loop(pts: &[Point3], lp: &[u32], out: &mut Vec<Triangle>) -> Result<(), GeoError> {
    let ring: Vec<Point3> = lp.iter().map(|&i| pts[i as usize]).collect();
    let p0 = ring.first().copied().unwrap_or(Point3::ZERO);
    let Some((n, c, diag)) = fit_plane(&ring) else {
        return Err(GeoError::Invalid(format!(
            "degenerate hole boundary of {} edges at {} (collinear, or lobes that cancel to no plane)",
            ring.len(),
            at(p0)
        )));
    };
    let planar_tol = (PLANAR_FRACTION * diag).max(1e-9);
    let sag = ring.iter().map(|&p| (p - c).dot(n).abs()).fold(0.0, f64::max);
    if sag > planar_tol {
        return Err(GeoError::Invalid(format!(
            "non-planar hole of {} edges at {}: {sag:.4} off its plane, {planar_tol:.4} allowed",
            ring.len(),
            at(p0)
        )));
    }
    let axis = if n.x.abs() <= n.y.abs() && n.x.abs() <= n.z.abs() {
        Point3::new(1.0, 0.0, 0.0)
    } else if n.y.abs() <= n.z.abs() {
        Point3::new(0.0, 1.0, 0.0)
    } else {
        Point3::new(0.0, 0.0, 1.0)
    };
    let Some(u) = unit(axis.cross(n)) else {
        return Err(GeoError::Invalid("hole plane has no usable basis".into()));
    };
    let v = n.cross(u);
    let mut q: Vec<[f64; 2]> = ring.iter().map(|&p| [(p - c).dot(u), (p - c).dot(v)]).collect();
    let area2: f64 = (0..q.len()).map(|i| {
        let j = (i + 1) % q.len();
        q[i][0] * q[j][1] - q[j][0] * q[i][1]
    }).sum();
    if area2 < 0.0 {
        for p in &mut q {
            p[1] = -p[1];
        }
    }
    let m = q.len();
    for i in 0..m {
        for j in i + 2..m {
            if i == 0 && j == m - 1 {
                continue; // the closing edge is adjacent to the first
            }
            if segments_meet(q[i], q[(i + 1) % m], q[j], q[(j + 1) % m]) {
                return Err(GeoError::Invalid(format!(
                    "self-intersecting hole boundary of {} edges at {}",
                    m,
                    at(ring[i])
                )));
            }
        }
    }
    clip_ears(&ring, &q, diag, out)
}

/// Ear clipping on a simple counter-clockwise polygon `q` (positions
/// `ring` in the same order). Collinear corners drop without a facet.
fn clip_ears(ring: &[Point3], q: &[[f64; 2]], diag: f64, out: &mut Vec<Triangle>) -> Result<(), GeoError> {
    let eps = 1e-12 * diag * diag;
    let mut idx: Vec<usize> = (0..q.len()).collect();
    let mut from = 0usize;
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = None;
        for k in 0..m {
            let i = (from + k) % m;
            let (p, cur, nx) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let turn = orient(q[p], q[cur], q[nx]);
            if turn < -eps {
                continue; // reflex corner
            }
            if turn <= eps {
                clipped = Some((i, false)); // straight corner: nothing to add
                break;
            }
            let blocked = idx.iter().any(|&o| {
                o != p && o != cur && o != nx && in_triangle(q[p], q[cur], q[nx], q[o])
            });
            if !blocked {
                clipped = Some((i, true));
                break;
            }
        }
        let Some((i, emit)) = clipped else {
            return Err(GeoError::Invalid(format!(
                "cannot triangulate hole boundary at {}: no ear found",
                at(ring[idx[0]])
            )));
        };
        if emit {
            let (p, cur, nx) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            out.push(Triangle::new(ring[p], ring[cur], ring[nx]));
        }
        idx.remove(i);
        from = i % idx.len().max(1);
    }
    if let [p, cur, nx] = idx[..]
        && orient(q[p], q[cur], q[nx]) > eps
    {
        out.push(Triangle::new(ring[p], ring[cur], ring[nx]));
    }
    Ok(())
}
