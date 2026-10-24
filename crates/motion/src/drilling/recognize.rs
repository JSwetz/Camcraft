//! Split from `drilling.rs` by M5.0.1; structure only.

use super::hole::Hole;

use std::collections::HashMap;
use crate::cl::lattice;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
/// Facets with a vertical-normal component at or above this are caps,
/// floors, or too steep to be a drilled wall. A 45° chamfer sits at
/// 0.707 - outside by design; a shallower chamfer ring is caught and
/// merged below.
pub(super) const WALL_NZ: f64 = 0.7;

/// Farthest a fitted vertex may sit from the shared circle before the
/// component is "not a cylinder". 20 um: over the chord noise of our
/// own tessellations, under any real taper.
pub(super) const FIT_TOL: f64 = 0.02;

/// A hole is a hole, not a pinprick or a silo.
pub(super) const R_MIN: f64 = 0.05;

pub(super) const R_MAX: f64 = 500.0;

/// Two wall components this close in XY are one hole stepped in Z
/// (counterbore, shallow chamfer): fold their z-spans, keep the narrow
/// radius - the actual bore.
pub(super) const MERGE_DXY: f64 = 0.05;

/// A closed rim spans this much of the compass; a flat wall's vertices
/// straddle two opposite rays (span ~180), a slot's arc end 180. Both
/// fail; a full circle passes.
pub(super) const SPAN_MIN: f64 = std::f64::consts::TAU * 0.75;

/// Coincident-position union-find keys on exact bits: `world_facets`
/// transforms shared vertices identically, so seams weld exactly.
pub(super) fn vkey(p: &Point3) -> (u64, u64, u64) {
    (p.x.to_bits(), p.y.to_bits(), p.z.to_bits())
}

pub(super) fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

/// Find the drilled holes in a triangle mesh: closed, round, vertical
/// walls whose facets face their own axis (so raised bosses fall out),
/// merged across Z steps, snapped onto the lattice so re-detection of
/// an unmoved model is bit-stable. Degenerate or empty input yields an
/// empty list - never a panic, never a hang (Prime Directive).
///
/// Runs on raw facets, not the Feature layer: recognition must not
/// inherit (or drift from) feature grouping.
#[must_use]
pub fn recognize(tris: &[Triangle]) -> Vec<Hole> {
    // Global z floor: what makes a hole "through".
    let mut mesh_min_z = f64::INFINITY;
    for t in tris {
        if t.a.is_finite() && t.b.is_finite() && t.c.is_finite() {
            for v in [t.a, t.b, t.c] {
                if v.z < mesh_min_z {
                    mesh_min_z = v.z;
                }
            }
        }
    }
    if !mesh_min_z.is_finite() {
        return Vec::new();
    }

    // Wall facets -> components, welded on exact shared vertex bits.
    let mut ids: HashMap<(u64, u64, u64), usize> = HashMap::new();
    let mut parent: Vec<usize> = Vec::new();
    let mut verts: Vec<Point3> = Vec::new();
    // Per wall facet: its vertex ids, centroid, normal.
    let mut facets: Vec<([usize; 3], Point3, Point3)> = Vec::new();

    for t in tris {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let n = t.normal();
        if !n.is_finite() || n.z.abs() >= WALL_NZ {
            continue;
        }
        let mut v3 = [0usize; 3];
        for (slot, p) in v3.iter_mut().zip([a, b, c]) {
            *slot = match ids.get(&vkey(&p)) {
                Some(&i) => i,
                None => {
                    let i = verts.len();
                    ids.insert(vkey(&p), i);
                    verts.push(p);
                    parent.push(i);
                    i
                }
            };
        }
        let cen = Point3::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0, (a.z + b.z + c.z) / 3.0);
        facets.push(([v3[0], v3[1], v3[2]], cen, n));
    }

    // Path-compressed union over each facet's three vertices.
    pub(super) fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (v, _, _) in &facets {
        let (ra, rb) = (find(&mut parent, v[0]), find(&mut parent, v[1]));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
        let (ra, rc) = (find(&mut parent, v[0]), find(&mut parent, v[2]));
        if ra != rc {
            parent[ra.max(rc)] = ra.min(rc);
        }
    }
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for (fi, (v, _, _)) in facets.iter().enumerate() {
        let r = find(&mut parent, v[0]);
        by_root.entry(r).or_default().push(fi);
    }

    let mut holes: Vec<Hole> = Vec::new();
    for fis in by_root.values() {
        // The component's distinct vertices.
        let mut vids: Vec<usize> = Vec::new();
        for &fi in fis {
            for &v in &facets[fi].0 {
                if !vids.contains(&v) {
                    vids.push(v);
                }
            }
        }
        if vids.len() < 6 {
            continue; // a facet pair or a sliver is not a hole wall
        }
        let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in &vids {
            zmin = zmin.min(verts[v].z);
            zmax = zmax.max(verts[v].z);
        }
        if zmax - zmin <= 1e-6 {
            continue; // a ring of facets laid flat, not a wall
        }
        // Kasa circle fit on the XY projections; singular (a flat wall's
        // collinear vertices) drops the component here, for free.
        let Some((cx, cy, rad)) = fit_circle(&vids, &verts) else {
            continue;
        };
        if !(R_MIN < rad && rad < R_MAX) {
            continue;
        }
        // Fit quality: every vertex within FIT_TOL of the circle.
        let worst = vids
            .iter()
            .map(|&v| (hypot(verts[v].x - cx, verts[v].y - cy) - rad).abs())
            .fold(0.0f64, f64::max);
        if worst > FIT_TOL {
            continue; // a slot's racetrack fits no circle
        }
        // Closed-rim test: the vertices must wrap more than three
        // quarters of the compass.
        let mut angs: Vec<f64> = vids
            .iter()
            .map(|&v| (verts[v].y - cy).atan2(verts[v].x - cx))
            .collect();
        angs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut max_gap = angs[0] + std::f64::consts::TAU - angs[angs.len() - 1];
        for w in angs.windows(2) {
            max_gap = max_gap.max(w[1] - w[0]);
        }
        if std::f64::consts::TAU - max_gap <= SPAN_MIN {
            continue;
        }
        // Hole vs boss: on a hole's wall an outward-wound mesh's normals
        // point AT the axis. A raised cylinder's wall points away. Vote
        // by facet; a coin flip refuses.
        let (mut toward, mut away) = (0usize, 0usize);
        for &fi in fis {
            let (_, cen, n) = facets[fi];
            let d = n.x * (cx - cen.x) + n.y * (cy - cen.y);
            if d > 0.0 {
                toward += 1;
            } else if d < 0.0 {
                away += 1;
            }
        }
        if toward <= away {
            continue;
        }
        holes.push(Hole {
            xy: Point2::new(lattice(cx), lattice(cy)),
            radius: lattice(rad),
            entry_z: lattice(zmax),
            floor_z: lattice(zmin),
            through: (zmin - mesh_min_z).abs() <= 1e-6,
        });
    }

    // Merge Z-stepped same-axis components (counterbores, shallow
    // chamfers): one hole, narrowest radius, whole z span.
    let mut merged: Vec<Hole> = Vec::new();
    let mut used = vec![false; holes.len()];
    for i in 0..holes.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut h = holes[i];
        loop {
            let mut grew = false;
            for j in 0..holes.len() {
                if !used[j] && hypot(h.xy.x - holes[j].xy.x, h.xy.y - holes[j].xy.y) <= MERGE_DXY {
                    used[j] = true;
                    h = Hole {
                        xy: Point2::new(
                            lattice(h.xy.x.min(holes[j].xy.x)),
                            lattice(h.xy.y.min(holes[j].xy.y)),
                        ),
                        radius: h.radius.min(holes[j].radius),
                        entry_z: h.entry_z.max(holes[j].entry_z),
                        floor_z: h.floor_z.min(holes[j].floor_z),
                        through: false,
                    };
                    h.through = (h.floor_z - mesh_min_z).abs() <= 1e-6;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        merged.push(h);
    }
    merged.sort_by(|a, b| {
        cmp64(a.xy.y, b.xy.y)
            .then(cmp64(a.xy.x, b.xy.x))
            .then(cmp64(a.radius, b.radius))
    });
    merged
}

pub(super) fn cmp64(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// Least-squares (Kasa) circle through the projected vertices; `None`
/// when the normal equations are singular - which is exactly the
/// flat-wall case, so the fit itself does the first cut. 3x3 solve
/// with partial pivoting, no panics, no unwraps.
///
/// Conditioning law: Kasa's normal equations degrade quadratically in
/// the distance from the origin (an r = 2 circle at x = 32000 fit
/// *wrong-but-plausibly* before this re-centering). So everything is
/// accumulated against the point cloud's centroid and the center is
/// translated back at the end.
pub(super) fn fit_circle(vids: &[usize], verts: &[Point3]) -> Option<(f64, f64, f64)> {
    let n = vids.len() as f64;
    let (mut mx, mut my) = (0.0, 0.0);
    for &v in vids {
        mx += verts[v].x;
        my += verts[v].y;
    }
    mx /= n;
    my /= n;
    let (mut sxx, mut sxy, mut syy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sxxx, mut sxyy, mut sxxy, mut syyy) = (0.0, 0.0, 0.0, 0.0);
    for &v in vids {
        let (x, y) = (verts[v].x - mx, verts[v].y - my);
        let (x2, y2) = (x * x, y * y);
        sxx += x2;
        sxy += x * y;
        syy += y2;
        sx += x;
        sy += y;
        sxxx += x * x2;
        sxyy += x * y2;
        sxxy += x2 * y;
        syyy += y * y2;
    }
    // [sxx sxy sx | -sum(x^3+xy^2)]
    // [sxy syy sy | -sum(x^2y+y^3)]
    // [sx  sy  n  | -sum(x^2+y^2) ]   ->  x^2+y^2 + Dx + Ey + F = 0
    // (x, y local; sx and sy are ~0 by construction of the centroid.)
    let mut a = [
        [sxx, sxy, sx, -(sxxx + sxyy)],
        [sxy, syy, sy, -(sxxy + syyy)],
        [sx, sy, n, -(sxx + syy)],
    ];
    for col in 0..3 {
        let mut piv = col;
        for r in col + 1..3 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        // Relative to the cloud's own scale: a circle of radius rr has
        // diagonal sums ~ n*rr^2; anything under a nanometer-equivalent
        // of that is a flat (collinear) wall.
        let scale = (sxx + syy) / n.max(1.0);
        if a[piv][col].abs() <= 1e-12 * scale * n.max(1.0) + f64::EPSILON {
            return None;
        }
        a.swap(col, piv);
        for r in 0..3 {
            if r == col {
                continue;
            }
            let f = a[r][col] / a[col][col];
            let pivot_row = a[col]; // Copy; lets the elimination zip rows.
            for (cell, &pv) in a[r][col..4].iter_mut().zip(&pivot_row[col..4]) {
                *cell -= f * pv;
            }
        }
    }
    // Gauss-Jordan left a diagonal matrix: the solution sits in the
    // right-hand column divided by each row's own pivot.
    let sol = |i: usize| -> f64 {
        if a[i][i].abs() > f64::EPSILON {
            a[i][3] / a[i][i]
        } else {
            f64::NAN
        }
    };
    let (d, e, f0) = (sol(0), sol(1), sol(2));
    if !(d.is_finite() && e.is_finite() && f0.is_finite()) {
        return None;
    }
    let (cx, cy) = (-d * 0.5 + mx, -e * 0.5 + my);
    let r2 = (d * d + e * e) * 0.25 - f0;
    if !(r2 > 0.0 && r2.is_finite()) {
        return None;
    }
    Some((cx, cy, r2.sqrt()))
}

// ---------------------------------------------------------------- generator
