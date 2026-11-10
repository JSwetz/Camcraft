//! Building the brick SDF from the mesh (M7.1 A): exact seeds, a
//! multi-source BFS over the six-neighbor lattice, truncation at the
//! band. Split from `field.rs` by the 400-line law; the storage, the
//! query, and the input gate live there.
//!
//! Provenance: the multi-source BFS distance transform (Barilla et
//! al. 2024 - seeds at boundary cells, unit-hop relaxation, linear
//! time) and the narrow-band truncation of signed distance fields (a
//! level-set staple). The seed kernel is `gouge`'s exact
//! point-triangle distance, already the codebase's own. Nothing from
//! vendor binaries.
//!
//! ## The construction laws
//!
//! - **Seeds are exact.** Every cell center within SEED_R cells of a
//!   facet gets the true point-triangle distance, so the shell always
//!   has honest material to grow from and the band's near-surface
//!   values are seeded, not approximated.
//! - **Every stored magnitude is a graph distance**: a seed value
//!   plus hops of `cell / sqrt(3)`. An L1 staircase costs L1/sqrt(3)
//!   <= its own straight length, so a straight path's graph cost is
//!   at most the true distance plus the seed's half-diagonal - near
//!   the surface the value sits within about a cell of the truth,
//!   and everywhere it UNDER-reports at worst and over-reports by no
//!   more than that bound: clearance rounds toward the surface, the
//!   direction the field module's header names and the Prime
//!   Directive blesses.
//! - **The band truncates on the graph value and clamps at cap.** A
//!   cell stores min(v, cap) for BFS values v up to cap + 2 cells;
//!   everything else answers cap, stored or not. Two inequalities
//!   carry the law. OVER: the staircase path to a cell at true
//!   distance t costs seed + hops <= t + 1.73*cell of value, so every
//!   cell at truth up to `cap` lies inside the horizon and gets
//!   stored - the cap's over-report of clearance never exceeds two
//!   cells. UNDER: a hop steps one whole cell for `cell / sqrt(3)`
//!   of value, so a stored v stands at least sqrt(3) below its own
//!   geometric reach: magnitudes under-report by that factor at the
//!   worst (along lattice diagonals), never collapse. Nothing is
//!   stored far from the surface: the footprint is an L1 ball of
//!   radius sqrt(3)*(band+2) plus the seed shell around the shell -
//!   narrow in the graph metric the BFS lives in. The Euclidean-thin
//!   band and the everywhere-exact magnitude are stage C's eikonal
//!   and clipmap passes.
//! - **One facet, one scan bound.** A facet whose inflated AABB
//!   would touch more than MAX_CELLS_PER_FACET cells refuses with its
//!   index and the count named (the dexel grid's stadium-at-1um law
//!   in the field's clothing): the mesh needs tessellating, and a
//!   hang is not an answer.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::geometry::{Point3, Triangle};

use super::field::{
    center_of, cell_of, in_brick, validate_shell, SdfField, HOP, SEED_R,
};
use super::gouge::dist_pt_tri;
use super::kernel::StockError;

/// Refusal ceiling for one facet's seed scan (the same
/// input-nonsense law as `kernel::MAX_CELLS_PER_AXIS`, sized for a
/// scan instead of a grid).
const MAX_CELLS_PER_FACET: u64 = 1 << 20;

/// The six-neighbor offsets, in cells.
const NEIGHBORS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

impl SdfField {
    /// The stock field of a closed mesh: cell edge `cell`, band
    /// `band` cells wide. The mesh must be watertight, finite, and
    /// non-degenerate; winding is irrelevant. The accuracy laws live
    /// in this module's header, the sign law in `field`'s.
    ///
    /// Provenance: our own design over the BFS-transform literature,
    /// per the Clean-Room Protocol.
    pub fn from_mesh(facets: &[Triangle], cell: f64, band: u32) -> Result<SdfField, StockError> {
        validate_shell(facets, cell, band)?;
        let cap = band as f64 * cell;
        let mut best: HashMap<[i32; 3], (f64, u32)> = HashMap::new();
        seed_exact(facets, cell, cap, &mut best)?;

        // Dial buckets on the graph metric: a relaxation raises the
        // value by exactly one hop cost, which lands the neighbor one
        // bucket up, so first valid pop = final value and the pass is
        // linear (Barilla's law, literal). Seeds occupy the low
        // buckets (their values are at most SEED_R cells of distance,
        // hence bucket <= ceil(SEED_R / HOP)). Truncation rides the
        // value, clamped at cap - the band law in this header.
        let inc = cell * HOP;
        // The band's value horizon: every cell whose BFS value lands
        // within two cells past `cap` is stored (its value, clamped);
        // space beyond answers `cap`. The +2 covers the graph bound
        // v <= t + 1.73*cells (this header's band law), so every cell
        // at truth up to `cap` is stored and `cap` over-reports by at
        // most two cells.
        let v_max = cap + 2.0 * cell;
        let nb = (v_max / inc).ceil() as usize + 2;
        let mut buckets: Vec<VecDeque<([i32; 3], f64, u32)>> =
            (0..nb).map(|_| VecDeque::new()).collect();
        for (&key, &(v, h)) in &best {
            buckets[bucket_of(v, inc, nb)].push_back((key, v, h));
        }
        let mut done: HashMap<[i32; 3], f64> = HashMap::with_capacity(best.len() * 2);
        for b in 0..nb {
            let mut q = std::mem::take(&mut buckets[b]);
            while let Some((key, v, h)) = q.pop_front() {
                if done.contains_key(&key) || best.get(&key) != Some(&(v, h)) {
                    continue; // finalized, or obsoleted by a better path
                }
                done.insert(key, v);
                for d in NEIGHBORS {
                    let [Some(x), Some(y), Some(z)] = [
                        key[0].checked_add(d[0]),
                        key[1].checked_add(d[1]),
                        key[2].checked_add(d[2]),
                    ] else {
                        continue; // off the lattice's integer span: no neighbor
                    };
                    let nk = [x, y, z];
                    let nv = v + inc;
                    if nv > v_max {
                        continue; // past the band's horizon
                    }
                    let better = match best.get(&nk) {
                        None => true,
                        Some(&(bv, bh)) => nv < bv || (nv == bv && h + 1 < bh),
                    };
                    if better {
                        best.insert(nk, (nv, h + 1));
                        buckets[bucket_of(nv, inc, nb)].push_back((nk, nv, h + 1));
                    }
                }
            }
        }

        let mut field = SdfField {
            cell,
            cap,
            mesh: Arc::new(facets.to_vec()),
            bricks: Arc::new(HashMap::new()),
            cuts: HashMap::new(),
            cut_ups: HashMap::new(),
            cost: Box::default(),
            analytic: None,
        };
        for (&key, &v) in &done {
            // Clamped: a stored value past the band is `cap`, exactly
            // like unstored space, so "beyond the band answers cap"
            // holds for the magnitude everywhere, not by luck.
            field.brick_mut(key)[in_brick(key)] = v.min(cap);
        }
        field.cost.region = super::field_batch::region_of(&field);
        Ok(field)
    }
}

/// Facet edge ceiling for `box_shell`, in cells: a 256-cell square's
/// inflated scan stays far under `MAX_CELLS_PER_FACET`.
const BOX_EDGE_CELLS: f64 = 256.0;

/// The closed box `[lo, hi]` as a watertight shell tessellated so no
/// facet's seed scan can refuse (M7.3: a deep block's side, as one
/// quad, spanned more candidate cells than the per-facet ceiling and
/// refused a stock the column builds). Every face is a grid on the
/// SAME per-axis coordinate rows, so shared edges meet at bit-equal
/// vertices and the edge gate sees every edge used exactly twice.
pub(crate) fn box_shell(lo: Point3, hi: Point3, cell: f64) -> Vec<Triangle> {
    let rows = |a: f64, b: f64| -> Vec<f64> {
        let n = ((b - a) / (BOX_EDGE_CELLS * cell)).ceil().clamp(1.0, 4096.0) as usize;
        (0..=n)
            .map(|k| if k == n { b } else { a + (b - a) * (k as f64 / n as f64) })
            .collect()
    };
    let (xs, ys, zs) = (rows(lo.x, hi.x), rows(lo.y, hi.y), rows(lo.z, hi.z));
    let mut out = Vec::new();
    // One face: the fixed axis at `w`, gridded over rows `us` x `vs`;
    // `pt` places (w, u, v) in the world.
    let mut face = |w: f64, us: &[f64], vs: &[f64], pt: &dyn Fn(f64, f64, f64) -> Point3| {
        for i in 0..us.len().saturating_sub(1) {
            for j in 0..vs.len().saturating_sub(1) {
                let (a, b) = (pt(w, us[i], vs[j]), pt(w, us[i + 1], vs[j]));
                let (c, d) = (pt(w, us[i + 1], vs[j + 1]), pt(w, us[i], vs[j + 1]));
                out.push(Triangle::new(a, b, c));
                out.push(Triangle::new(a, c, d));
            }
        }
    };
    let px = |w: f64, u: f64, v: f64| Point3::new(w, u, v);
    let py = |w: f64, u: f64, v: f64| Point3::new(u, w, v);
    let pz = |w: f64, u: f64, v: f64| Point3::new(u, v, w);
    for w in [lo.x, hi.x] {
        face(w, &ys, &zs, &px);
    }
    for w in [lo.y, hi.y] {
        face(w, &xs, &zs, &py);
    }
    for w in [lo.z, hi.z] {
        face(w, &xs, &ys, &pz);
    }
    out
}

/// Bucket of a value on the graph metric, clamped into range.
fn bucket_of(v: f64, inc: f64, nb: usize) -> usize {
    ((v / inc).ceil() as usize).min(nb - 1)
}

/// Phase one: every cell center within `r` of a facet gets its exact
/// distance, minned into `best`. The scan is the facet's inflated
/// AABB on the lattice - a rectangle is cheap, the point-triangle
/// kernel does the honest work.
fn seed_exact(
    facets: &[Triangle],
    cell: f64,
    cap: f64,
    best: &mut HashMap<[i32; 3], (f64, u32)>,
) -> Result<(), StockError> {
    let r = (SEED_R * cell).min(cap + cell);
    // World-unit span; cell_of below is the one division. (Dividing
    // here AND in cell_of put the scan at index/cell twice over -
    // invisible at cell 1.0, empty everywhere else.)
    let lo = |v: f64| v - r;
    let hi = |v: f64| v + r;
    for (n, t) in facets.iter().enumerate() {
        let (imin, imax) = (
            lo(t.a.x.min(t.b.x.min(t.c.x))),
            hi(t.a.x.max(t.b.x.max(t.c.x))),
        );
        let (jmin, jmax) = (
            lo(t.a.y.min(t.b.y.min(t.c.y))),
            hi(t.a.y.max(t.b.y.max(t.c.y))),
        );
        let (kmin, kmax) = (
            lo(t.a.z.min(t.b.z.min(t.c.z))),
            hi(t.a.z.max(t.b.z.max(t.c.z))),
        );
        let cnt = |a: f64, b: f64| ((b - a) / cell + 1.0).max(0.0) as u64;
        let total = cnt(imin, imax)
            .saturating_mul(cnt(jmin, jmax))
            .saturating_mul(cnt(kmin, kmax));
        if total > MAX_CELLS_PER_FACET {
            return Err(StockError::Invalid(format!(
                "sdf field: facet {n} spans {total} candidate cells at cell size {cell} - the mesh needs tessellating to the cell, and a hang is not an answer"
            )));
        }
        for i in cell_of(imin, cell)..=cell_of(imax, cell) {
            for j in cell_of(jmin, cell)..=cell_of(jmax, cell) {
                for k in cell_of(kmin, cell)..=cell_of(kmax, cell) {
                    let q = Point3::new(center_of(i, cell), center_of(j, cell), center_of(k, cell));
                    let d = dist_pt_tri(q, t);
                    if d <= r {
                        let key = [i, j, k];
                        if best.get(&key).is_none_or(|b| d < b.0) {
                            best.insert(key, (d, 0));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::field::tests::box_facets;
    use super::*;

    #[test]
    fn a_giant_facet_refuses_with_its_number_named() {
        // The 4mm box scaled to 1000mm at cell 0.001: each wall
        // spans 1e12 candidate cells. Refused, not attempted.
        let scale = |q: Point3| Point3::new(q.x * 250.0, q.y * 250.0, q.z * 250.0);
        let f: Vec<Triangle> = box_facets()
            .iter()
            .map(|t| Triangle::new(scale(t.a), scale(t.b), scale(t.c)))
            .collect();
        let Err(e) = SdfField::from_mesh(&f, 0.001, 4) else {
            panic!("a facet spanning 1e12 candidate cells must refuse, not hang");
        };
        let s = format!("{e}");
        assert!(s.contains("tessellating"), "{s}");
        assert!(s.contains("facet"), "{s}");
    }

    #[test]
    fn the_box_field_answers_its_own_interior() {
        // 4x4x4 box, cell 1, band 4: parity says where the material
        // is, and a point deep inside reads negative and no larger
        // than the cap.
        let f = box_facets();
        let field = SdfField::from_mesh(&f, 1.0, 4).unwrap();
        assert!(field.inside(Point3::new(2.0, 2.5, 2.5)));
        assert!(!field.inside(Point3::new(-0.5, 2.5, 2.5)));
        let d = field.distance(Point3::new(2.0, 2.0, 2.5));
        assert!(d < 0.0 && d.abs() <= field.cap(), "{d}");
    }
}
