//! The cylinder primitive behind hole recognition (M5.2 F2): the
//! three axis-aligned candidate axes, their transverse planes, the
//! plane-parameterized circle fit, and the edge-weld law that
//! connects wall facets of ONE surface without bridging surfaces
//! that meet at a dihedral angle.
//!
//! Provenance: the Kasa least-squares circle is public-domain
//! fitting lore (Kasa 1979, line/circle fit by algebraic distance);
//! the axis-candidate structure and the normal-angle weld are ours,
//! specced from the clean-room law that a drilled hole is a closed
//! round wall, wherever its axis points.

use crate::geometry::{Point3, Triangle};

/// Coincident-position union-find keys on exact bits: `world_facets`
/// transforms shared vertices identically, so seams weld exactly.
pub(super) fn vkey(p: &Point3) -> (u64, u64, u64) {
    (p.x.to_bits(), p.y.to_bits(), p.z.to_bits())
}

pub(super) fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

pub(super) fn cmp64(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// The three candidate axes, in report order: Z first, because it
/// is the axis the whole mill-side law was written on, then the two
/// radial axes a lathe's live tooling plunges along. Axis `a`
/// projects onto the transverse pair `TU[a]`, `TV[a]`.
pub(super) const AXES: [(usize, usize, usize); 3] =
    [(2, 0, 1), (0, 1, 2), (1, 0, 2)]; // (along, u, v): Z, X, Y

pub(super) fn coord(p: Point3, i: usize) -> f64 {
    match i {
        0 => p.x,
        1 => p.y,
        _ => p.z,
    }
}

/// The weld test between two facets sharing an edge: the cosine of
/// the angle between their normals must clear 0.7 (45 degrees).
/// Adjacent facets of one tessellated cylinder sit far inside it
/// (a 16-gon's are 22.5 degrees apart, an 8-gon's exactly 45 and
/// still pass at the epsilon); a hole wall meeting a cross hole,
/// a floor, or a rim meets at 78 degrees or more and never welds.
/// The test is SIGNED-positive: two facets facing opposite ways
/// (across an axis, far apart on any real surface) must not weld
/// just because their planes are parallel.
pub(super) fn same_surface(n1: Point3, n2: Point3) -> bool {
    n1.x * n2.x + n1.y * n2.y + n1.z * n2.z > 0.69
}

/// Möller-Trumbore: does the ray `o + t*u` (t > 1e-9) meet the
/// triangle's interior? The same parity-vote law the boring doors
/// stand on (Ericson, ch. 5, public-domain algorithm), kept local
/// so drilling never reaches across into turning's internals.
/// A parallel ray is a miss by convention.
pub(super) fn ray_hit(o: Point3, u: Point3, t: &Triangle) -> bool {
    let e1 = Point3::new(t.b.x - t.a.x, t.b.y - t.a.y, t.b.z - t.a.z);
    let e2 = Point3::new(t.c.x - t.a.x, t.c.y - t.a.y, t.c.z - t.a.z);
    let p = Point3::new(
        u.y * e2.z - u.z * e2.y,
        u.z * e2.x - u.x * e2.z,
        u.x * e2.y - u.y * e2.x,
    );
    let det = e1.x * p.x + e1.y * p.y + e1.z * p.z;
    if det.abs() < 1e-12 {
        return false;
    }
    let tv = Point3::new(o.x - t.a.x, o.y - t.a.y, o.z - t.a.z);
    let a1 = (tv.x * p.x + tv.y * p.y + tv.z * p.z) / det;
    if a1 < 0.0 || a1 > 1.0 {
        return false;
    }
    let q = Point3::new(
        tv.y * e1.z - tv.z * e1.y,
        tv.z * e1.x - tv.x * e1.z,
        tv.x * e1.y - tv.y * e1.x,
    );
    let a2 = (u.x * q.x + u.y * q.y + u.z * q.z) / det;
    if a2 < 0.0 || a1 + a2 > 1.0 {
        return false;
    }
    (e2.x * q.x + e2.y * q.y + e2.z * q.z) / det > 1e-9
}

/// Least-squares (Kasa) circle through the component's vertices
/// projected onto the (u, v) plane; `None` when the normal equations
/// are singular - which is exactly the flat-wall case, so the fit
/// itself does the first cut. 3x3 solve with partial pivoting, no
/// panics, no unwraps.
///
/// Conditioning law: Kasa's normal equations degrade quadratically in
/// the distance from the origin (an r = 2 circle at x = 32000 fit
/// *wrong-but-plausibly* before this re-centering). So everything is
/// accumulated against the point cloud's centroid and the center is
/// translated back at the end.
pub(super) fn fit_circle_uv(
    vids: &[usize],
    verts: &[Point3],
    u: usize,
    v: usize,
) -> Option<(f64, f64, f64)> {
    let n = vids.len() as f64;
    let (mut mu, mut mv) = (0.0, 0.0);
    for &vi in vids {
        mu += coord(verts[vi], u);
        mv += coord(verts[vi], v);
    }
    mu /= n;
    mv /= n;
    let (mut suu, mut suv, mut svv, mut su, mut sv) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut suuu, mut suvv, mut suuv, mut svvv) = (0.0, 0.0, 0.0, 0.0);
    for &vi in vids {
        let (x, y) = (coord(verts[vi], u) - mu, coord(verts[vi], v) - mv);
        let (x2, y2) = (x * x, y * y);
        suu += x2;
        suv += x * y;
        svv += y2;
        su += x;
        sv += y;
        suuu += x * x2;
        suvv += x * y2;
        suuv += x2 * y;
        svvv += y * y2;
    }
    // [suu suv su | -sum(u^3+uv^2)]
    // [suv svv sv | -sum(u^2v+v^3)]
    // [su  sv  n  | -sum(u^2+v^2) ]   ->  u^2+v^2 + Du + Ev + F = 0
    // (u, v local; su and sv are ~0 by construction of the centroid.)
    let mut a = [
        [suu, suv, su, -(suuu + suvv)],
        [suv, svv, sv, -(suuv + svvv)],
        [su, sv, n, -(suu + svv)],
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
        let scale = (suu + svv) / n.max(1.0);
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
    let (cu, cv) = (-d * 0.5 + mu, -e * 0.5 + mv);
    let r2 = (d * d + e * e) * 0.25 - f0;
    if !(r2 > 0.0 && r2.is_finite()) {
        return None;
    }
    Some((cu, cv, r2.sqrt()))
}

// The Z candidate's fit is `fit_circle_uv` with (u, v) = (0, 1) -
// the mill-side tests pinned the XY circle, and the candidate
// table now hands it the same coordinates through the same law.
