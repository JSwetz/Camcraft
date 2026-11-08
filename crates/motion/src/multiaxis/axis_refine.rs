//! The direction refinement of the axis fit (issue #62 review). The
//! second moment of the normals gives the axis direction exactly only
//! when the tessellation is even: a cone whose facets are cut at
//! uneven bearings weights its tilted normals unevenly, and the
//! extremal eigenvector leans by tens of microradians - over a 60 mm
//! shaft, a micron of drift, which the lathe spindle law reads (and
//! the turning frame then cuts) as a real lean.
//!
//! The refinement asks the concurrency law itself: every facet's
//! normal line (anchored at its circumcenter, see `axis.rs`) meets
//! the axis. In Plucker coordinates a normal line is `(n, q x n)` and
//! the axis `(u, m)`; two lines meet iff `n . m + u . (q x n) = 0`.
//! Moving the estimate in its own normal plane - `u = u0 + a e1 + b e2`
//! and the axis point by `s e1 + t e2`, so `m = t e1 - s e2` to first
//! order - makes every row linear in `(a, b, s, t)`: one 4x4
//! area-weighted Gauss-Newton step, re-centred and repeated twice to
//! settle the second-order terms. The caller keeps the refined
//! direction only when it moves past the noise floor and does not
//! raise the fit's own residual, so an even revolve keeps its eigen
//! direction bit for bit (the goldens' identity frame) - and it does
//! not refine an eigen line that already meets every normal line to
//! rounding ([`worth_refining`], #228).
//!
//! Provenance: line-complex fitting of revolved surfaces from normal
//! lines in Plucker coordinates (Pottmann & Wallner, Computational
//! Line Geometry; Pottmann & Randrup, rotational and helical surface
//! approximation, 1998); Gaussian elimination is textbook. Written
//! from those sources and our own design notes.

use crate::geometry::Point3;

use super::axis::{Axis, Datum as Line};
use super::linalg::unit;

/// Below this step (radians) a refinement is rounding, not a fact
/// about the part, and the loop stops.
const SETTLED: f64 = 1e-13;
/// The refinement's unknowns: two tilts, two shifts.
const N: usize = 4;
/// A fit residual this small, per mm of the part's mean radius, is
/// rounding: the line already meets every normal line (#228).
const CONCURRENT: f64 = 1e-9;

/// Is the eigen line `ax`, whose fit residual is `r`, worth refining
/// (#228)? Not when it already meets every normal line to rounding: a
/// refinement can then only trade it for a tilt the rows cannot see.
/// A bar tessellated as two rings anchors every side facet's line at
/// the bar's mid-station, so no row sees a tilt about it; rounding
/// picked one of 26 microradians, accepted on a residual "lower" by
/// noise, and the rotary wrap's seam law read the leaning columns as a
/// feature on the seam.
pub(super) fn worth_refining(lines: &[Line], ax: Axis, r: f64, a_total: f64) -> bool {
    super::axis::line(lines, ax.dir, a_total).is_some_and(|(_, rho)| r > CONCURRENT * rho)
}

/// Refine the direction `u0` of an axis through `o0` against the
/// normal lines; `None` when the system is singular or not finite (the
/// caller keeps the eigen direction). The result is a unit vector on
/// `u0`'s side.
pub(super) fn refine_dir(lines: &[Line], u0: Point3, o0: Point3) -> Option<Point3> {
    let mut u = unit(u0)?;
    let mut o = o0;
    for _ in 0..3 {
        let any = if u.z.abs() < 0.9 { Point3::new(0.0, 0.0, 1.0) } else { Point3::new(1.0, 0.0, 0.0) };
        let e1 = unit(u.cross(any))?;
        let e2 = u.cross(e1);
        let mut m = [[0.0f64; N]; N];
        let mut b = [0.0f64; N];
        for l in lines {
            let k = l.n.cross(u);
            if k.dot(k) <= 1e-8 {
                continue; // a pole facet's normal runs along the axis
            }
            // Moments about the current axis point, so the rows' scale
            // is the part's, not its distance from the world origin.
            let w = (l.p - o).cross(l.n);
            let row = [w.dot(e1), w.dot(e2), -l.n.dot(e2), l.n.dot(e1)];
            let rhs = -w.dot(u);
            for i in 0..N {
                for j in 0..N {
                    m[i][j] += l.a * row[i] * row[j];
                }
                b[i] += l.a * row[i] * rhs;
            }
        }
        let x = solve(m, b)?;
        u = unit(Point3::new(
            u.x + x[0] * e1.x + x[1] * e2.x,
            u.y + x[0] * e1.y + x[1] * e2.y,
            u.z + x[0] * e1.z + x[1] * e2.z,
        ))?;
        o = Point3::new(
            o.x + x[2] * e1.x + x[3] * e2.x,
            o.y + x[2] * e1.y + x[3] * e2.y,
            o.z + x[2] * e1.z + x[3] * e2.z,
        );
        let step = (x[0] * x[0] + x[1] * x[1]).sqrt();
        if !step.is_finite() {
            return None;
        }
        if step <= SETTLED {
            break;
        }
    }
    u.is_finite().then_some(u)
}

/// Gaussian elimination with partial pivoting; `None` when singular.
fn solve(mut m: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    let scale = m.iter().flatten().fold(0.0f64, |s, v| s.max(v.abs()));
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    for c in 0..N {
        let piv = (c..N).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[piv][c].abs() <= 1e-14 * scale {
            return None;
        }
        m.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..N {
            let f = m[r][c] / m[c][c];
            let pivot = m[c];
            for (v, p) in m[r].iter_mut().zip(pivot).skip(c) {
                *v -= f * p;
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0f64; N];
    for c in (0..N).rev() {
        let mut s = b[c];
        for k in c + 1..N {
            s -= m[c][k] * x[k];
        }
        x[c] = s / m[c][c];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_singular_system_is_refused_not_divided() {
        assert!(solve([[0.0; N]; N], [1.0; N]).is_none());
    }

    #[test]
    fn a_diagonal_system_solves() {
        let mut m = [[0.0; N]; N];
        for (i, row) in m.iter_mut().enumerate() {
            row[i] = (i + 1) as f64;
        }
        let x = solve(m, [1.0, 2.0, 3.0, 4.0]).expect("regular");
        assert!(x.iter().all(|v| (v - 1.0).abs() < 1e-12), "{x:?}");
    }
}
