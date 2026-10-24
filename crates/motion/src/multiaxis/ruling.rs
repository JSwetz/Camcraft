//! The RULING finder of the swarf kinds (M5.1 stage D). A ruled
//! surface - the bladed family this pattern exists for - is swept
//! along straight lines: at every point one principal curvature
//! vanishes and its direction is the ruling. From a triangle soup
//! with no face structure, that direction field is recovered the
//! published way: the shape operator S = -dn/dp fitted per facet by
//! least squares over the normal differences of its neighbourhood
//! (differential geometry of meshes, the standard multiscale
//! curvature-estimation literature), restricted to the facet plane
//! and eigen-decomposed, and the region's single straight-line
//! direction found by scanning angles: the ruling angle is the one
//! along which the normals move the least. A model whose curvature
//! is the same in every direction (a dome) has no ruling to find and
//! is refused BY NAME, pointing at the kind that speaks for it; a
//! model ruled along Z (a vertical blade or cylinder wall) is
//! refused too, because the raster this direction drives is
//! XY-following - its flanks belong to the waterline rings, i.e. to
//! 5-Axis Contour.
//!
//! Provenance: ruled-surface side machining (swarf) and the
//! principal-direction characterization of developability (one
//! principal curvature zero) are textbook differential geometry;
//! the least-squares shape-operator fit over mesh neighbourhoods is
//! published multiscale curvature-estimation practice. Written from
//! those sources and our own design notes.

use std::collections::HashMap;

use crate::cl::lattice;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// The verdict of the field analysis: one global pass direction, and
/// which facets actually hold a ruling (the stamp's law choice).
#[derive(Clone, Debug)]
pub struct Field {
    /// The region's dominant ruling direction, unit, in XY - the
    /// raster's pass direction.
    pub ux: f64,
    pub uy: f64,
    /// Per input facet: it carries a straight direction whose
    /// curvature is negligible next to its cross-curvature.
    pub ruled: Vec<bool>,
}

/// A facet is "ruled" when its least-curving direction bends with
/// under a tenth of the curvature of its most-curving direction (a
/// cylinder: exactly 0; a sphere: ratio 1).
const RATIO_RULED: f64 = 0.1;
/// A facet whose biggest curvature is under this (1/mm - a radius of
/// curvature over ten metres) is FLAT for the law's purposes: no
/// direction claim either way, and it votes nothing.
const K_FLAT: f64 = 1e-4;
/// Share of area that must hold a ruling before the θ scan means
/// anything, and the share of answered-flat area that lets a
/// near-planar model through without one.
const RULED_SHARE: f64 = 0.05;
const FLAT_SHARE: f64 = 0.5;
/// A direction earns its name when it moves the normals under this
/// fraction of what the average direction moves them (the scan's
/// coherence law), and a vertical score under this fraction of the
/// best horizontal one says the ruling runs along Z.
const COHERENCE: f64 = 0.5;
/// Angles of the scan (half turn; a ruling is a line, not a vector).
const N_THETA: usize = 48;
/// Neighbours a shape-operator fit asks of: fewer and the 6x6
/// normal equations cannot be answered; more than the cap adds
/// nothing the R-radius didn't already promise.
const N_MIN: usize = 6;
const N_CAP: usize = 64;
/// How many mean-edge spacings out the neighbourhood reaches.
const R_EDGES: f64 = 4.0;

/// The symmetric 3x3 shape operator of one facet, stored by its six
/// independent entries, with the principal curvatures squared of
/// its tangent plane (kmax2 >= kmin2 >= 0).
#[derive(Clone, Copy)]
struct Op {
    /// S = [[a,d,e],[d,b,f],[e,f,c]]
    s: [f64; 6],
    /// The facet's own normal, unit - what the scan projects into.
    n: Point3,
    kmax2: f64,
    kmin2: f64,
}

/// `S · p` for the stored operator, spelled once.
fn s_mul(o: &[f64; 6], p: Point3) -> Point3 {
    Point3::new(
        o[0] * p.x + o[3] * p.y + o[4] * p.z,
        o[3] * p.x + o[1] * p.y + o[5] * p.z,
        o[4] * p.x + o[5] * p.y + o[2] * p.z,
    )
}

/// Read the field: the pass direction the whole region shares, or
/// the named refusal that says which kind speaks for this model.
pub fn analyze(facets: &[Triangle]) -> Result<Field, GenError> {
    let ops = shape_operators(facets);
    let (mut a_all, mut a_ruled, mut a_flat, mut a_ok) = (0.0, 0.0, 0.0, 0.0);
    let mut facet_area = Vec::with_capacity(facets.len());
    let mut ruled = Vec::with_capacity(facets.len());
    for (i, op) in ops.iter().enumerate() {
        // Area of a facet, recomputed cheaply from the same law the
        // fit used (None ops still count toward the total).
        let f = &facets[i];
        let (u, v) = (f.b - f.a, f.c - f.a);
        let cr = u.cross(v);
        let area = 0.5 * cr.dot(cr).sqrt();
        facet_area.push(if area.is_finite() && area > 0.0 { area } else { 0.0 });
        a_all += facet_area[i];
        match op {
            None => {}
            Some(o) => {
                a_ok += facet_area[i];
                if o.kmax2 <= K_FLAT * K_FLAT {
                    a_flat += facet_area[i];
                    ruled.push(false);
                } else if o.kmin2 < (RATIO_RULED * RATIO_RULED) * o.kmax2 {
                    a_ruled += facet_area[i];
                    ruled.push(true);
                } else {
                    ruled.push(false);
                }
            }
        }
    }
    if !a_all.is_finite() || a_all <= 0.0 {
        return Err(GenError::Invalid("swarf has no surface to read a ruling from".into()));
    }
    if a_ok < 0.25 * a_all {
        return Err(GenError::Invalid(
            "the mesh is too coarse for the ruling to be seen anywhere: refine it or finish with 5-Axis Parallel".into(),
        ));
    }
    if a_ruled < RULED_SHARE * a_all {
        if a_flat >= FLAT_SHARE * a_all {
            // Near-planar ground: there is no direction to honour and
            // nothing to refuse - every pass direction cuts the same
            // path, and the stamp poses by the lean law alone.
            return Ok(Field { ux: 1.0, uy: 0.0, ruled });
        }
        return Err(GenError::Invalid(
            "swarf found no ruling to follow - this surface curves about the same in every direction; doubly-curved ground is 5-Axis Parallel or Scallop work".into(),
        ));
    }
    // The scan, over ruled facets only: score(theta) is the area-
    // weighted squared normal-change along the direction theta -
    // the ruling is the angle that moves the surface least.
    let (mut best, mut best_k, mut sum) = (f64::INFINITY, 0usize, 0.0);
    for k in 0..N_THETA {
        let th = std::f64::consts::PI * (k as f64) / (N_THETA as f64);
        let s = score_dir(&ops, &facet_area, &ruled, Point3::new(th.cos(), th.sin(), 0.0));
        sum += s;
        if s < best {
            (best, best_k) = (s, k);
        }
    }
    let mean = sum / (N_THETA as f64);
    let sv = score_dir(&ops, &facet_area, &ruled, Point3::new(0.0, 0.0, 1.0));
    if sv <= COHERENCE * best {
        return Err(GenError::Invalid(
            "this model's ruling runs along Z (vertical walls): its flanks belong to the waterline rings, which is 5-Axis Contour work".into(),
        ));
    }
    if best > COHERENCE * mean {
        return Err(GenError::Invalid(
            "the ruling turns across the region - no single straight pass direction follows it; split the ruled faces or finish with 5-Axis Parallel".into(),
        ));
    }
    let th = std::f64::consts::PI * (best_k as f64) / (N_THETA as f64);
    Ok(Field { ux: th.cos(), uy: th.sin(), ruled })
}

/// The area-weighted squared normal-change along a direction: each
/// ruled facet contributes `area * |S * d3|^2`, where `d3` is the
/// direction projected into the facet's own plane (a facet seen
/// edge-on to the pass moves its normals nowhere along it, and one
/// whose plane contains the direction says `d3 = d` exactly).
fn score_dir(ops: &[Option<Op>], area: &[f64], ruled: &[bool], d: Point3) -> f64 {
    let mut s = 0.0;
    for (i, op) in ops.iter().enumerate() {
        if !(ruled[i]) {
            continue;
        }
        let Some(o) = op else { continue };
        let n = o.n;
        let dn = n.dot(d);
        let d3 = Point3::new(d.x - n.x * dn, d.y - n.y * dn, d.z - n.z * dn);
        let sd = s_mul(&o.s, d3);
        s += area[i] * (sd.x * sd.x + sd.y * sd.y + sd.z * sd.z);
    }
    s
}

/// The per-facet curvature estimate from the neighbourhood's normal
/// differences: rows `S * dp_i = dn_i` in the six symmetric
/// parameters, one 6x6 solve per facet, lattice-quantized inputs so
/// the answer speaks the same bits on every platform's libm
/// (M4.2b's law, bent to curvature).
fn shape_operators(facets: &[Triangle]) -> Vec<Option<Op>> {
    let mut cent = Vec::with_capacity(facets.len());
    let mut nrm = Vec::with_capacity(facets.len());
    let mut areas = Vec::with_capacity(facets.len());
    let mut area_sum = 0.0;
    for f in facets {
        let (u, v) = (f.b - f.a, f.c - f.a);
        let cr = u.cross(v);
        let a2 = cr.dot(cr);
        // Degenerate facets keep a zero area and a zero normal; the
        // fit skips them and the ruled flag stays false - the Prime
        // Directive reads "no claim", never "a guessed claim".
        let n = if a2 > 0.0 && a2.is_finite() {
            Point3::new(cr.x / a2.sqrt(), cr.y / a2.sqrt(), cr.z / a2.sqrt())
        } else {
            Point3::ZERO
        };
        let c = Point3::new(
            (f.a.x + f.b.x + f.c.x) / 3.0,
            (f.a.y + f.b.y + f.c.y) / 3.0,
            (f.a.z + f.b.z + f.c.z) / 3.0,
        );
        let area = 0.5 * a2.sqrt();
        cent.push(c);
        nrm.push(n);
        areas.push(if area.is_finite() { area } else { 0.0 });
        area_sum += areas[areas.len() - 1].max(0.0);
    }
    let n = facets.len().max(1) as f64;
    let r = (R_EDGES * (area_sum / n).sqrt().max(1e-9)).max(1e-9);
    let cell = |p: Point3| -> (i64, i64, i64) {
        (
            (p.x / r).floor() as i64,
            (p.y / r).floor() as i64,
            (p.z / r).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, c) in cent.iter().enumerate() {
        if c.is_finite() {
            grid.entry(cell(*c)).or_default().push(i);
        }
    }
    let r2 = (1.5 * r) * (1.5 * r);
    let mut ops = Vec::with_capacity(facets.len());
    for i in 0..facets.len() {
        if areas[i] <= 0.0 || !cent[i].is_finite() {
            ops.push(None);
            continue;
        }
        let (gx, gy, gz) = cell(cent[i]);
        let mut m = [[0.0f64; 6]; 6];
        let mut rhs = [0.0f64; 6];
        let mut cnt = 0usize;
        'nbr: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(bucket) = grid.get(&(gx + dx, gy + dy, gz + dz)) else {
                        continue;
                    };
                    for &j in bucket {
                        if j == i || areas[j] <= 0.0 {
                            continue;
                        }
                        let (dp, dn) = (q3(cent[j] - cent[i]), q3(nrm[j] - nrm[i]));
                        if dp.dot(dp) > r2 {
                            continue;
                        }
                        // Three scalar equations per neighbour, the
                        // 6-parameter symmetric row of S * dp.
                        let rows = [
                            ([dp.x, 0.0, 0.0, dp.y, dp.z, 0.0], dn.x),
                            ([0.0, dp.y, 0.0, dp.x, 0.0, dp.z], dn.y),
                            ([0.0, 0.0, dp.z, 0.0, dp.x, dp.y], dn.z),
                        ];
                        for (cvec, obs) in rows {
                            for (p, &cv) in cvec.iter().enumerate() {
                                rhs[p] += cv * obs;
                                for (q, &cq) in cvec.iter().enumerate() {
                                    m[p][q] += cv * cq;
                                }
                            }
                        }
                        cnt += 1;
                        if cnt >= N_CAP {
                            break 'nbr;
                        }
                    }
                }
            }
        }
        if cnt < N_MIN {
            ops.push(None);
            continue;
        }
        let Some(sol) = solve6(&mut m, &mut rhs) else {
            ops.push(None);
            continue;
        };
        // Restrict S to the tangent plane of the facet: an
        // orthonormal (e1, e2) across the normal, the 2x2 symmetric
        // form, its closed-form eigenvalues.
        let n = nrm[i];
        let (e1, e2) = plane_basis(n);
        let s11 = s_mul(&sol, e1).dot(e1);
        let s12 = s_mul(&sol, e1).dot(e2);
        let s22 = s_mul(&sol, e2).dot(e2);
        let (tr, det) = (s11 + s22, s11 * s22 - s12 * s12);
        let disc = ((tr * tr) / 4.0 - det).max(0.0).sqrt();
        let (k1, k2) = (tr / 2.0 + disc, tr / 2.0 - disc);
        let (ka, kb) = (k1.abs(), k2.abs());
        if !(ka.is_finite() && kb.is_finite()) {
            ops.push(None);
            continue;
        }
        ops.push(Some(Op {
            s: sol,
            n,
            kmax2: ka.max(kb) * ka.max(kb),
            kmin2: ka.min(kb) * ka.min(kb),
        }));
    }
    ops
}

/// Micron lattice on a difference vector (M4.2b).
fn q3(p: Point3) -> Point3 {
    Point3::new(lattice(p.x), lattice(p.y), lattice(p.z))
}

/// An orthonormal pair spanning the plane through the origin
/// perpendicular to the unit `n`; the axis of smallest |component|
/// keeps the cross product well away from degenerate.
fn plane_basis(n: Point3) -> (Point3, Point3) {
    let ax = if n.x.abs() <= n.y.abs().min(n.z.abs()) {
        Point3::new(1.0, 0.0, 0.0)
    } else if n.y.abs() <= n.z.abs() {
        Point3::new(0.0, 1.0, 0.0)
    } else {
        Point3::new(0.0, 0.0, 1.0)
    };
    let e1 = n.cross(ax);
    let l = (e1.x * e1.x + e1.y * e1.y + e1.z * e1.z).sqrt();
    if !l.is_finite() || l <= 0.0 {
        return (Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0));
    }
    let e1 = Point3::new(e1.x / l, e1.y / l, e1.z / l);
    let e2 = n.cross(e1);
    (e1, e2)
}

/// Gaussian solve with partial pivoting on the 6x6 normal
/// equations; `None` when a pivot says the neighbourhood does not
/// span the operator's six unknowns (a line of facets, a flat
/// patch's rank-deficient dust).
fn solve6(m: &mut [[f64; 6]; 6], rhs: &mut [f64; 6]) -> Option<[f64; 6]> {
    let scale = m.iter().enumerate().map(|(i, r)| r[i].abs()).fold(0.0f64, f64::max);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    for col in 0..6 {
        let (mut piv, mut pr) = (col, m[col][col].abs());
        for (row, rrow) in m.iter().enumerate().skip(col + 1) {
            let v = rrow[col].abs();
            if v > pr {
                (piv, pr) = (row, v);
            }
        }
        if !pr.is_finite() || pr <= 1e-12 * scale {
            return None;
        }
        if piv != col {
            m.swap(piv, col);
            rhs.swap(piv, col);
        }
        let prow = m[col];
        for (row, rrow) in m.iter_mut().enumerate().skip(col + 1) {
            let f = rrow[col] / prow[col];
            if f != 0.0 {
                for (cv, &pv) in rrow.iter_mut().zip(prow.iter()) {
                    *cv -= f * pv;
                }
                let v = rhs[col];
                rhs[row] -= f * v;
            }
        }
    }
    let mut x = [0.0f64; 6];
    for (row, mrow) in m.iter().enumerate().rev() {
        let mut s = rhs[row];
        for (cv, &xv) in mrow.iter().skip(row + 1).zip(x.iter().skip(row + 1)) {
            s -= cv * xv;
        }
        x[row] = s / mrow[row];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}
