//! The quadratic error function a dual-contouring cell minimizes (M13.1,
//! #23): the point closest, in the least-squares sense, to every
//! tangent plane of the cell's Hermite data.
//! Provenance: Ju, Losasso, Schaefer & Warren, "Dual Contouring of
//! Hermite Data" (SIGGRAPH 2002) - the QEF E(x) = sum (n_i . (x - p_i))^2,
//! solved by a pseudo-inverse whose small singular values are truncated
//! so an under-constrained direction keeps the mass point's coordinate;
//! the symmetric eigensolver is the cyclic Jacobi method (Golub & Van
//! Loan, "Matrix Computations", sec. 8.5). Our own Rust; nothing from
//! vendor binaries.
//!
//! ## The laws here
//!
//! - **Ill-conditioned is the mass point, never a guess.** A direction
//!   whose singular value is under `TRUNC` of the largest is dropped from
//!   the solve: a flat face's vertex rides its mass point in-plane, a
//!   cell with no usable normal is its mass point outright.
//! - **The vertex stays in its cell.** The answer is clamped to the
//!   cell's box, so no quad can reach across the lattice; a non-finite
//!   answer is the mass point, clamped the same way.
//! - **Bounded work.** The Jacobi sweep count is capped: no input hangs.

type V3 = [f64; 3];

/// Singular values below this fraction of the largest are truncated
/// (Ju et al. 2002 truncate at 0.1 on unit normals).
const TRUNC: f64 = 0.1;

/// Jacobi sweeps at most; a 3x3 converges in a handful.
const MAX_SWEEPS: usize = 32;

/// The cell vertex: minimize the QEF of `planes` (point, unit normal),
/// solved about `mass`, clamped into `[lo, hi]`.
pub(crate) fn solve(planes: &[(V3, V3)], mass: V3, lo: V3, hi: V3) -> V3 {
    let clamp = |x: V3| -> V3 {
        let mut o = [0.0; 3];
        for k in 0..3 {
            let (a, b) = (lo[k].min(hi[k]), lo[k].max(hi[k]));
            o[k] = if x[k].is_finite() { x[k].clamp(a, b) } else { 0.5 * (a + b) };
        }
        o
    };
    let mut a = [[0.0f64; 3]; 3];
    let mut rhs = [0.0f64; 3];
    for (p, n) in planes {
        let r = (0..3).map(|k| n[k] * (p[k] - mass[k])).sum::<f64>();
        if !r.is_finite() || n.iter().any(|c| !c.is_finite()) {
            continue;
        }
        for i in 0..3 {
            rhs[i] += n[i] * r;
            for j in 0..3 {
                a[i][j] += n[i] * n[j];
            }
        }
    }
    let (lam, v) = eigen_sym(a);
    let top = lam.iter().fold(0.0f64, |m, l| m.max(*l));
    let mut x = mass;
    if top > f64::MIN_POSITIVE {
        for k in 0..3 {
            // Singular values are the square roots of A's eigenvalues.
            if lam[k] > 0.0 && lam[k] >= TRUNC * TRUNC * top {
                let vk = [v[0][k], v[1][k], v[2][k]];
                let c = (0..3).map(|i| vk[i] * rhs[i]).sum::<f64>() / lam[k];
                for i in 0..3 {
                    x[i] += c * vk[i];
                }
            }
        }
    }
    if x.iter().any(|c| !c.is_finite()) {
        x = mass;
    }
    clamp(x)
}

/// Eigenvalues and eigenvectors (columns of the matrix) of a symmetric
/// 3x3, cyclic Jacobi.
fn eigen_sym(mut a: [[f64; 3]; 3]) -> (V3, [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..MAX_SWEEPS {
        let off = a[0][1] * a[0][1] + a[0][2] * a[0][2] + a[1][2] * a[1][2];
        let diag = a[0][0] * a[0][0] + a[1][1] * a[1][1] + a[2][2] * a[2][2];
        if off.is_nan() || off <= 1e-30 * diag.max(f64::MIN_POSITIVE) {
            break;
        }
        for (p, q) in [(0usize, 1usize), (0, 2), (1, 2)] {
            if a[p][q].abs() <= f64::MIN_POSITIVE {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            // signum(0) = 1: a zero theta rotates by 45 degrees.
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for row in a.iter_mut() {
                let (akp, akq) = (row[p], row[q]);
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
            }
            let (rp, rq) = (a[p], a[q]);
            a[p] = std::array::from_fn(|k| c * rp[k] - s * rq[k]);
            a[q] = std::array::from_fn(|k| s * rp[k] + c * rq[k]);
            for row in v.iter_mut() {
                let (vp, vq) = (row[p], row[q]);
                row[p] = c * vp - s * vq;
                row[q] = s * vp + c * vq;
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2]], v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LO: V3 = [0.0, 0.0, 0.0];
    const HI: V3 = [1.0, 1.0, 1.0];

    #[test]
    fn three_planes_meet_at_their_corner() {
        let planes = [
            ([0.3, 0.1, 0.9], [1.0, 0.0, 0.0]),
            ([0.8, 0.4, 0.2], [0.0, 1.0, 0.0]),
            ([0.1, 0.9, 0.7], [0.0, 0.0, 1.0]),
        ];
        let x = solve(&planes, [0.5, 0.5, 0.5], LO, HI);
        for (got, want) in x.iter().zip([0.3, 0.4, 0.7]) {
            assert!((got - want).abs() < 1e-9, "{x:?}");
        }
    }

    #[test]
    fn an_edge_keeps_the_mass_point_along_it() {
        // Two planes: x = 0.2, z = 0.6; the edge runs along y.
        let planes = [([0.2, 0.1, 0.0], [1.0, 0.0, 0.0]), ([0.9, 0.8, 0.6], [0.0, 0.0, 1.0])];
        let x = solve(&planes, [0.5, 0.45, 0.5], LO, HI);
        assert!((x[0] - 0.2).abs() < 1e-9 && (x[2] - 0.6).abs() < 1e-9, "{x:?}");
        assert!((x[1] - 0.45).abs() < 1e-9, "the free direction keeps the mass point: {x:?}");
    }

    #[test]
    fn ill_conditioned_and_empty_cells_are_their_clamped_mass_point() {
        assert_eq!(solve(&[], [0.25, 0.5, 0.75], LO, HI), [0.25, 0.5, 0.75]);
        let zero = [([0.5, 0.5, 0.5], [0.0, 0.0, 0.0])];
        assert_eq!(solve(&zero, [0.25, 0.5, 0.75], LO, HI), [0.25, 0.5, 0.75]);
        let nan = [([f64::NAN, 0.5, 0.5], [1.0, 0.0, 0.0]), ([0.5, 0.5, 0.5], [f64::NAN, 0.0, 1.0])];
        let x = solve(&nan, [0.25, 0.5, 0.75], LO, HI);
        assert!(x.iter().all(|c| c.is_finite()), "{x:?}");
        // Two nearly parallel planes would meet far outside: truncated.
        let s = 1e-3f64;
        let near = [([0.5, 0.5, 0.4], [0.0, 0.0, 1.0]), ([0.5, 0.5, 0.6], [s, 0.0, (1.0 - s * s).sqrt()])];
        let x = solve(&near, [0.5, 0.5, 0.5], LO, HI);
        assert!(x.iter().zip(HI).all(|(c, h)| (0.0..=h).contains(c)), "clamped in the cell: {x:?}");
        assert!((x[0] - 0.5).abs() < 1e-6, "the degenerate direction keeps the mass point: {x:?}");
    }

    #[test]
    fn a_far_intersection_is_clamped_into_the_cell() {
        // Planes x = 5 and y = -3 meet outside the unit cell.
        let planes = [([5.0, 0.5, 0.5], [1.0, 0.0, 0.0]), ([0.5, -3.0, 0.5], [0.0, 1.0, 0.0])];
        assert_eq!(solve(&planes, [0.5, 0.5, 0.5], LO, HI), [1.0, 0.0, 0.5]);
    }
}
