//! The render lattice's eikonal band re-normalization (M7.1's owed
//! "re-normalize the band with one eikonal sweep before the render
//! pass", M13.1, issue #22): one bounded fast-sweeping pass over the
//! packed lattice's narrow band, so the shading gradient of the
//! sphere-traced view reads |grad phi| ~ 1 where the BFS magnitudes and
//! the CSG `max` of the cut layer leave it bent.
//!
//! Provenance: the fast sweeping method - Gauss-Seidel over the 2^d
//! alternating sweep orders with the first-order Godunov upwind update
//! of |grad u| = 1 (H. Zhao, "A fast sweeping method for Eikonal
//! equations", Math. Comp. 74, 2005) - and the frozen-interface
//! re-initialization of a level-set function near its zero set
//! (Sussman, Smereka & Osher, J. Comput. Phys. 114, 1994; the
//! interface-preserving variant of Russo & Smereka, JCP 163, 2000).
//! Our own Rust over this codebase's own lattice; nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **The surface never moves.** Every texel with a 26-neighbour of
//!   the other sign (or a zero, or a non-finite neighbour) is FROZEN:
//!   its composed value is kept bit for bit, so every lattice edge,
//!   face diagonal and cube diagonal that crosses the zero set keeps
//!   both endpoint values, and the crossing a trilinear or a
//!   marching-tetrahedra read takes from them is unchanged. Every
//!   other band texel keeps its SIGN: the solved magnitude is at
//!   least a frozen value plus `cell / sqrt(3)`, never zero.
//! - **Band only, bounded.** A texel whose composed magnitude is at
//!   or past `cap` is out of band and untouched (a no-op, by
//!   construction - it is not even read as anything but `cap`); a
//!   solved magnitude is clamped at `cap`, the field's own far-field
//!   law. One cycle of the eight sweep orders, no iteration to
//!   convergence: the cost is `8 x texels` cheap updates, stated up
//!   front and deterministic (fixed orders, no hashing, no threads).
//! - **The cut layer stays the cut layer.** The re-normalized composed
//!   value `max(mesh, cut)` is written into the MESH layer and the cut
//!   layer is left as packed: at every texel `max(new_mesh, cut)` has
//!   the old composed sign (a negative composed value bounds `cut`
//!   below zero too), and playback's later cut layers only lift
//!   cells, never un-remove one - so a base re-normalized once stays
//!   sign-exact under every frame that cuts more.
//! - **Display only.** This is the render's lattice. The verdict paths
//!   (`march`, `deviate_field`) read the sparse field, whose
//!   under-reporting magnitude is the blessed pessimism; nothing here
//!   ever reaches a verdict.

use crate::stock_sim::kernel::StockError;

use super::pack::FieldLattice;

/// Sweep orders: every sign combination of the three axes once
/// (Zhao's 2^d Gauss-Seidel orderings, d = 3).
const ORDERS: [[bool; 3]; 8] = [
    [false, false, false],
    [true, false, false],
    [false, true, false],
    [true, true, false],
    [false, false, true],
    [true, false, true],
    [false, true, true],
    [true, true, true],
];

/// What one re-normalization did - the cost and the coverage, stated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Renormalized {
    /// Texels kept bit for bit at the zero set.
    pub frozen: usize,
    /// Band texels whose magnitude was re-solved.
    pub solved: usize,
    /// Texel updates spent (`sweeps x solved`): the bounded cost.
    pub updates: usize,
    /// Sweep orders run.
    pub sweeps: u32,
}

/// Per-texel role in the sweep.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Out of band (or non-finite): never written, read as `cap`.
    Far,
    /// At the zero set: kept, read as its own magnitude.
    Frozen,
    /// In band: re-solved.
    Band,
}

impl FieldLattice {
    /// Re-normalize the lattice's narrow band in place with one cycle
    /// of fast sweeping (the module laws). Refuses, unchanged, a
    /// lattice whose layers do not match its own dimensions.
    pub fn renormalize(&mut self) -> Result<Renormalized, StockError> {
        let [nx, ny, nz] = self.meta.dims.map(|d| d as usize);
        let n = nx.checked_mul(ny).and_then(|v| v.checked_mul(nz));
        if n != Some(self.mesh.len()) || self.cut.len() != self.mesh.len() {
            return Err(StockError::Invalid(format!(
                "eikonal: a {nx}x{ny}x{nz} lattice cannot hold {} mesh and {} cut texels",
                self.mesh.len(),
                self.cut.len()
            )));
        }
        let (cell, cap) = (self.meta.cell, self.meta.cap);
        if !(cell.is_finite() && cell > 0.0 && cap.is_finite() && cap > 0.0) {
            return Err(StockError::Invalid(format!("eikonal: cell {cell} and cap {cap} must be finite and positive")));
        }
        let phi: Vec<f64> = self.mesh.iter().zip(&self.cut).map(|(m, c)| f64::from(m.max(*c))).collect();
        let (u, role, stats) = solve(&phi, [nx, ny, nz], cell, cap);
        for i in 0..phi.len() {
            if role[i] == Role::Band {
                let mag = u[i].min(cap);
                self.mesh[i] = (if phi[i] < 0.0 { -mag } else { mag }) as f32;
            } else if role[i] == Role::Frozen {
                // Keep the composed value: max(new, cut) == old composed.
                self.mesh[i] = phi[i] as f32;
            }
        }
        Ok(stats)
    }
}

/// The sweep proper over a dense composed field `phi` (x fastest):
/// magnitudes per texel, roles, and the cost spent.
fn solve(phi: &[f64], dims: [usize; 3], cell: f64, cap: f64) -> (Vec<f64>, Vec<Role>, Renormalized) {
    let [nx, ny, nz] = dims;
    let idx = |x: usize, y: usize, z: usize| (z * ny + y) * nx + x;
    let class = |v: f64| if v < 0.0 { 1u8 } else { 0u8 };
    let mut role = vec![Role::Far; phi.len()];
    let mut u = vec![cap; phi.len()];
    let mut stats = Renormalized { sweeps: ORDERS.len() as u32, ..Renormalized::default() };
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let i = idx(x, y, z);
                let v = phi[i];
                if !v.is_finite() {
                    continue;
                }
                let mut frozen = v == 0.0;
                'n: for dz in -1i64..=1 {
                    for dy in -1i64..=1 {
                        for dx in -1i64..=1 {
                            let (a, b, c) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
                            if frozen {
                                break 'n;
                            }
                            if a < 0 || b < 0 || c < 0 || a >= nx as i64 || b >= ny as i64 || c >= nz as i64 {
                                continue;
                            }
                            let w = phi[idx(a as usize, b as usize, c as usize)];
                            frozen = !w.is_finite() || class(w) != class(v) || w == 0.0;
                        }
                    }
                }
                if frozen {
                    role[i] = Role::Frozen;
                    u[i] = v.abs();
                    stats.frozen += 1;
                } else if v.abs() < cap {
                    role[i] = Role::Band;
                    stats.solved += 1;
                }
            }
        }
    }
    // Neighbour magnitude along one axis: off-lattice reads `cap`.
    let h = cell;
    for order in ORDERS {
        let walk = |n: usize, rev: bool| -> Box<dyn Iterator<Item = usize>> {
            if rev {
                Box::new((0..n).rev())
            } else {
                Box::new(0..n)
            }
        };
        for z in walk(nz, order[2]) {
            for y in walk(ny, order[1]) {
                for x in walk(nx, order[0]) {
                    let i = idx(x, y, z);
                    if role[i] != Role::Band {
                        continue;
                    }
                    let pick = |lo: Option<usize>, hi: Option<usize>| {
                        let r = |j: Option<usize>| j.map_or(cap, |j| u[j]);
                        r(lo).min(r(hi))
                    };
                    let a = pick(x.checked_sub(1).map(|x| idx(x, y, z)), (x + 1 < nx).then(|| idx(x + 1, y, z)));
                    let b = pick(y.checked_sub(1).map(|y| idx(x, y, z)), (y + 1 < ny).then(|| idx(x, y + 1, z)));
                    let c = pick(z.checked_sub(1).map(|z| idx(x, y, z)), (z + 1 < nz).then(|| idx(x, y, z + 1)));
                    let next = godunov(a, b, c, h);
                    if next < u[i] {
                        u[i] = next;
                    }
                    stats.updates += 1;
                }
            }
        }
    }
    (u, role, stats)
}

/// Zhao's first-order Godunov update of |grad u| = 1 on a uniform
/// grid of spacing `h`, from the smaller neighbour per axis: the
/// largest root that stays above every neighbour it uses.
fn godunov(a: f64, b: f64, c: f64, h: f64) -> f64 {
    let mut s = [a, b, c];
    s.sort_by(f64::total_cmp);
    let [a1, a2, a3] = s;
    let one = a1 + h;
    if one <= a2 {
        return one;
    }
    let two = 0.5 * (a1 + a2 + (2.0 * h * h - (a1 - a2) * (a1 - a2)).max(0.0).sqrt());
    if two <= a3 {
        return two;
    }
    let sum = a1 + a2 + a3;
    let disc = sum * sum - 3.0 * (a1 * a1 + a2 * a2 + a3 * a3 - h * h);
    (sum + disc.max(0.0).sqrt()) / 3.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_godunov_update_is_exact_on_planes_and_diagonals() {
        let inf = f64::INFINITY;
        assert!((godunov(2.0, inf, inf, 1.0) - 3.0).abs() < 1e-12, "1-D: one step on");
        // A 45-degree plane: both neighbours at d, the point at d + h/sqrt2.
        let two = godunov(1.0, 1.0, inf, 1.0);
        assert!((two - (1.0 + std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-12, "{two}");
        let three = godunov(1.0, 1.0, 1.0, 1.0);
        assert!((three - (1.0 + 1.0 / 3f64.sqrt())).abs() < 1e-12, "{three}");
    }
}
