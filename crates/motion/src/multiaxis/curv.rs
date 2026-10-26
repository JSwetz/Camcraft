//! The per-facet curvature fit both direction readers share
//! (M5.1 stage D's machinery, split out for the flowline tail): the
//! shape operator S = -dn/dp fitted per facet by least squares over
//! the normal differences of its neighbourhood (differential geometry
//! of meshes, the standard multiscale curvature-estimation
//! literature), restricted to the facet plane. [`ruling`](super::ruling)
//! scans these operators for ONE straight direction the whole region
//! shares; [`flow`](super::flow) reads each operator's own least-
//! curving direction and lets the passes turn with it. Same fit, one
//! home - the field the two kinds see cannot drift apart.
//!
//! Provenance: the least-squares shape-operator fit over mesh
//! neighbourhoods is published multiscale curvature-estimation
//! practice; the eigen-decomposition is textbook linear algebra.
//! Written from those sources and our own design notes.

use std::collections::HashMap;

use crate::cl::lattice;
use crate::geometry::{Point3, Triangle};

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
pub(super) struct Op {
    /// S = [[a,d,e],[d,b,f],[e,f,c]]
    pub(super) s: [f64; 6],
    /// The facet's own normal, unit - what the scan projects into.
    pub(super) n: Point3,
    pub(super) kmax2: f64,
    pub(super) kmin2: f64,
}

/// `S · p` for the stored operator, spelled once.
pub(super) fn s_mul(o: &[f64; 6], p: Point3) -> Point3 {
    Point3::new(
        o[0] * p.x + o[3] * p.y + o[4] * p.z,
        o[3] * p.x + o[1] * p.y + o[5] * p.z,
        o[4] * p.x + o[5] * p.y + o[2] * p.z,
    )
}

/// The per-facet curvature estimate from the neighbourhood's normal
/// differences: rows `S * dp_i = dn_i` in the six symmetric
/// parameters, one 6x6 solve per facet, lattice-quantized inputs so
/// the answer speaks the same bits on every platform's libm
/// (M4.2b's law, bent to curvature).
pub(super) fn shape_operators(facets: &[Triangle]) -> Vec<Option<Op>> {
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

/// The direction the facet bends LEAST, projected to XY and unit -
/// the flowline's local pass direction. `None` when the facet makes
/// no directional claim: degenerate, unanswerable, isotropic (a
/// sphere's facet has no least-curving direction to prefer; the
/// eigenpair there is numerical noise, and noise must never steer a
/// spindle), or a claim that lies along Z (its XY projection is a
/// point - the flank belongs to the waterline rings, as ruling's
/// Z-wall refusal says for the straight case).
pub(super) fn min_dir_xy(o: &Op) -> Option<[f64; 2]> {
    if o.kmax2 <= 0.0 || !o.kmax2.is_finite() {
        return None;
    }
    // Isotropy: a facet whose least curvature stands within a
    // hundredth of its most curvature (a sphere: ratio 1, a cylinder
    // far under it) claims no direction - the eigenpair there is
    // numerical noise, and noise must never steer a spindle.
    if o.kmin2 >= 0.01 * o.kmax2 {
        return None;
    }
    let (e1, e2) = plane_basis(o.n);
    let s11 = s_mul(&o.s, e1).dot(e1);
    let s12 = s_mul(&o.s, e1).dot(e2);
    let s22 = s_mul(&o.s, e2).dot(e2);
    let (tr, det) = (s11 + s22, s11 * s22 - s12 * s12);
    let disc = ((tr * tr) / 4.0 - det).max(0.0).sqrt();
    let (k1, k2) = (tr / 2.0 + disc, tr / 2.0 - disc);
    let kmin = if k1.abs() <= k2.abs() { k1 } else { k2 };
    // The eigenpair of the 2x2 form: (s12, kmin - s11) solves
    // [(s11-k), s12; s12, (s22-k)] v = 0 whenever s12 moves; when it
    // does not, the basis vectors ARE the principal directions.
    let (u, v) = if s12 != 0.0 && s12.is_finite() {
        (s12, kmin - s11)
    } else if s11 <= s22 {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let w = Point3::new(
        e1.x * u + e2.x * v,
        e1.y * u + e2.y * v,
        e1.z * u + e2.z * v,
    );
    let l2 = (w.x * w.x + w.y * w.y).sqrt();
    let keep = (w.x * w.x + w.y * w.y + w.z * w.z).sqrt();
    if keep <= 1e-12 || !keep.is_finite() || l2 <= 1e-6 * keep {
        return None;
    }
    Some([w.x / l2, w.y / l2])
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
