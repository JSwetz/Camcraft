//! The little numeric kernels the multiaxis fits are written from
//! (M5.1): a Jacobi eigen-solve and a pivot-guarded 3x3 solve, the
//! textbook pair behind every least-squares line fit. Kept out of
//! the law files so those read as geometry, not bookkeeping.
//!
//! Provenance: standard numerical recipes (Jacobi rotations, Gaussian
//! elimination with partial pivoting); written from those published
//! descriptions, nothing of any vendor's consulted.

use crate::geometry::Point3;

/// Unit vector along `v`; `None` for a zero or non-finite one.
pub(super) fn unit(v: Point3) -> Option<Point3> {
    let l = v.dot(v).sqrt();
    if !l.is_finite() || l <= 0.0 {
        return None;
    }
    Some(Point3::new(v.x / l, v.y / l, v.z / l))
}

/// Jacobi eigen-solve of a symmetric 3x3: rotations to diagonal,
/// eigenvalues ascending with the eigenvectors as the COLUMNS of
/// `vecs`. A few sweeps of the classic closed-form angle; exact
/// enough at doubles for any tolerance a machining law states.
pub(super) fn sym_eigen(a0: [[f64; 3]; 3]) -> Option<([f64; 3], [[f64; 3]; 3])> {
    let mut a = a0;
    let mut v = [[0.0f64; 3]; 3];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..12 {
        let off = (a[0][1] * a[0][1] + a[0][2] * a[0][2] + a[1][2] * a[1][2]).sqrt();
        if off <= 1e-15 * (a[0][0].abs() + a[1][1].abs() + a[2][2].abs()) {
            break;
        }
        for (p, q) in [(0usize, 1usize), (0, 2), (1, 2)] {
            let apq = a[p][q];
            if apq == 0.0 {
                continue;
            }
            let app = a[p][p];
            let aqq = a[q][q];
            // sin(2t)(app - aqq)/2... the rotation that takes this
            // pair's off-diagonal to zero in one stroke:
            let theta = (2.0 * apq).atan2(aqq - app) / 2.0;
            let (c, s) = (theta.cos(), theta.sin());
            for row in a.iter_mut() {
                let (akp, akq) = (row[p], row[q]);
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            let (ap, aq) = (a[p], a[q]);
            a[p] = std::array::from_fn(|k| c * ap[k] - s * aq[k]);
            a[q] = std::array::from_fn(|k| s * ap[k] + c * aq[k]);
            for col in v.iter_mut() {
                let (vkp, vkq) = (col[p], col[q]);
                col[p] = c * vkp - s * vkq;
                col[q] = s * vkp + c * vkq;
            }
        }
    }
    let mut vals = [a[0][0], a[1][1], a[2][2]];
    if !vals.iter().all(|x| x.is_finite()) {
        return None;
    }
    // Sort ascending, permuting the eigenvector columns alongside.
    let mut order = [0usize, 1, 2];
    for i in 0..3 {
        for j in i + 1..3 {
            if vals[j] < vals[i] {
                vals.swap(i, j);
                order.swap(i, j);
            }
        }
    }
    let mut vs = [[0.0f64; 3]; 3];
    for (j, &o) in order.iter().enumerate() {
        for i in 0..3 {
            vs[i][j] = v[i][o];
        }
    }
    Some((vals, vs))
}

/// Symmetric-positive 3x3 solve by Gaussian elimination with partial
/// pivoting; `None` on a vanished pivot (a direction nothing in the
/// data constrains).
pub(super) fn solve3(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<Point3> {
    let mut a = m;
    let mut r = b;
    for col in 0..3 {
        let (mut piv, mut pr) = (col, a[col][col].abs());
        for (row, ar) in a.iter().enumerate().skip(col + 1) {
            if ar[col].abs() > pr {
                (piv, pr) = (row, ar[col].abs());
            }
        }
        let scale = a
            .iter()
            .map(|row| row.iter().fold(0.0f64, |x, y| x.max(y.abs())))
            .fold(0.0, f64::max);
        if !pr.is_finite() || pr <= 1e-12 * scale.max(1e-30) {
            return None;
        }
        if piv != col {
            a.swap(piv, col);
            r.swap(piv, col);
        }
        for row in col + 1..3 {
            let f = a[row][col] / a[col][col];
            if f != 0.0 {
                let (arow, acol) = (a[row], a[col]);
                a[row] = std::array::from_fn(|j| arow[j] - f * acol[j]);
                let t = r[col];
                r[row] -= f * t;
            }
        }
    }
    let mut x = [0.0f64; 3];
    for row in (0..3).rev() {
        let mut s = r[row];
        for j in row + 1..3 {
            s -= a[row][j] * x[j];
        }
        if a[row][row] == 0.0 {
            return None;
        }
        x[row] = s / a[row][row];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(Point3::new(x[0], x[1], x[2]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eigen_solver_diagonalizes_a_known_matrix() {
        // diag(1, 5, 2) arrives rotated 30 degrees about y: the
        // eigenvalues must come back sorted and the largest one's
        // vector must be the rotated z.
        let c = 30f64.to_radians().cos();
        let s = 30f64.to_radians().sin();
        let m = [
            [c * c + 5.0 * s * s, 0.0, (1.0 - 5.0) * s * c],
            [0.0, 2.0, 0.0],
            [s * c * (1.0 - 5.0), 0.0, s * s + 5.0 * c * c],
        ];
        let (v, vecs) = sym_eigen(m).unwrap();
        assert!((v[0] - 1.0).abs() < 1e-9 && (v[1] - 2.0).abs() < 1e-9 && (v[2] - 5.0).abs() < 1e-9, "{v:?}");
        let e = Point3::new(vecs[0][2], vecs[1][2], vecs[2][2]);
        let want = Point3::new(-s, 0.0, c);
        let dot = e.x * want.x + e.y * want.y + e.z * want.z;
        assert!((dot.abs() - 1.0).abs() < 1e-9, "{e:?}");
    }

    #[test]
    fn garbage_in_refuses() {
        let m = [[f64::NAN, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(sym_eigen(m).is_none());
        let s = solve3([[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]], [1.0, 0.0, 0.0]);
        assert!(s.is_none(), "an unconstrained direction must not solve");
    }
}
