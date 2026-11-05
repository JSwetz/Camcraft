//! The field's surface as triangles when a consumer needs them (M7.2
//! D): iso-surface extraction at d = 0 over the field's own lattice.
//! Provenance: marching cubes (Lorensen & Cline, SIGGRAPH 1987) in its
//! tetrahedral form - each cube split into six tetrahedra around one
//! main diagonal (marching tetrahedra, as in Bloomenthal's "An Implicit
//! Surface Polygonizer", Graphics Gems IV, 1994), which needs no case
//! table and has no ambiguous faces. Our own Rust; nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **A cache of the field, never a second truth.** The mesh is
//!   sampled from `SdfField::distance` - the same query the eye marches
//!   and the verdict reads - and nothing reads it back as material.
//! - **Watertight by construction.** Every cube splits along the same
//!   diagonal direction, so two cubes sharing a face split it the same
//!   way and their triangles meet edge to edge. The sample grid is
//!   padded past the solid by a cell, so the surface closes.
//! - **Outward, always.** Each triangle is turned so its normal points
//!   from the inside corners toward the outside ones - STL's rule, and
//!   the one the viewport's back-face cull needs.
//! - **Refuse, never guess.** A grid past the sample budget, a NaN
//!   sample (an absent body has no surface to extract), or a degenerate
//!   cell is an `Err` by name - never a mesh with holes in it.

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

use super::field::SdfField;
use super::kernel::StockError;

/// Samples at most: the dense pack's own budget (`pack::MAX_TEXELS`).
pub const MAX_SAMPLES: usize = 1 << 23;

/// Cube corners as (dx, dy, dz) offsets, bit 0 = x, bit 1 = y, bit 2 = z.
const CORNER: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

/// The six tetrahedra around the 0-7 diagonal.
const TETS: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 3, 2, 7],
    [0, 2, 6, 7],
    [0, 6, 4, 7],
    [0, 4, 5, 7],
    [0, 5, 1, 7],
];

/// Extract the field's d = 0 surface at the field's own cell.
pub fn extract(f: &SdfField) -> Result<Vec<Triangle>, StockError> {
    extract_at(f, f.cell())
}

/// Extract at a sample spacing `step`, the grid anchored on the field's
/// cell centres (`step` = the cell samples each centre exactly once;
/// coarser skips cells for a lighter mesh).
pub fn extract_at(f: &SdfField, step: f64) -> Result<Vec<Triangle>, StockError> {
    let refuse = |s: String| Err(StockError::Invalid(format!("iso-surface: {s}")));
    if !(step.is_finite() && step > 0.0) {
        return refuse(format!("the sample step must be finite and positive, not {step}"));
    }
    let b = f.bounds();
    // The field's values are exact at its cell CENTRES (`distance` reads
    // the cell a point falls in), so the sample grid stands on them: a
    // grid on the cell faces would read each face sample from whichever
    // neighbour the rounding picked and bias the surface by half a cell.
    let cell = f.cell();
    let centre = |v: f64| ((v / cell).floor() - 1.0 + 0.5) * cell;
    let lo = Point3::new(centre(b.min.x), centre(b.min.y), centre(b.min.z));
    let span = [b.max.x - lo.x, b.max.y - lo.y, b.max.z - lo.z];
    if !(lo.is_finite() && span.iter().all(|s| s.is_finite() && *s >= 0.0)) {
        return refuse("the field's bounds are not a finite box".into());
    }
    // Cells per axis, padded a cell past the solid on both sides.
    let mut n = [0usize; 3];
    for (k, s) in span.iter().enumerate() {
        let c = (s / step).ceil() + 1.0;
        if !(1.0..1e7).contains(&c) {
            return refuse(format!("axis {k} gives no sane cell count ({c})"));
        }
        n[k] = c as usize;
    }
    let samples = (n[0] + 1)
        .checked_mul(n[1] + 1)
        .and_then(|v| v.checked_mul(n[2] + 1))
        .filter(|&v| v <= MAX_SAMPLES);
    let Some(samples) = samples else {
        return refuse(format!("a {}x{}x{} grid is past the {MAX_SAMPLES}-sample budget", n[0], n[1], n[2]));
    };
    let at = |i: usize, j: usize, k: usize| {
        Point3::new(lo.x + i as f64 * step, lo.y + j as f64 * step, lo.z + k as f64 * step)
    };
    let idx = |i: usize, j: usize, k: usize| (k * (n[1] + 1) + j) * (n[0] + 1) + i;
    let mut d = vec![0.0f64; samples];
    for k in 0..=n[2] {
        for j in 0..=n[1] {
            for i in 0..=n[0] {
                let v = f.distance(at(i, j, k));
                if v.is_nan() {
                    return refuse(format!("the field answered NaN at {:?} - no surface to extract", at(i, j, k)));
                }
                d[idx(i, j, k)] = v;
            }
        }
    }
    let mut out = Vec::new();
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                let mut p = [Point3::ZERO; 8];
                let mut v = [0.0f64; 8];
                for (c, o) in CORNER.iter().enumerate() {
                    let (a, bb, cc) = (i + o[0], j + o[1], k + o[2]);
                    p[c] = at(a, bb, cc);
                    v[c] = d[idx(a, bb, cc)];
                }
                if v.iter().all(|x| *x < 0.0) || v.iter().all(|x| *x >= 0.0) {
                    continue;
                }
                for t in TETS {
                    tet(&mut out, t.map(|c| p[c]), t.map(|c| v[c]));
                }
            }
        }
    }
    Ok(out)
}

/// Polygonize one tetrahedron: inside is d < 0.
fn tet(out: &mut Vec<Triangle>, p: [Point3; 4], v: [f64; 4]) {
    let inside: Vec<usize> = (0..4).filter(|&c| v[c] < 0.0).collect();
    let outside: Vec<usize> = (0..4).filter(|&c| v[c] >= 0.0).collect();
    let cross = |a: usize, b: usize| {
        let (da, db) = (v[a], v[b]);
        let t = if (da - db).abs() > f64::EPSILON { (da / (da - db)).clamp(0.0, 1.0) } else { 0.5 };
        let (pa, pb) = (p[a], p[b]);
        Point3::new(pa.x + (pb.x - pa.x) * t, pa.y + (pb.y - pa.y) * t, pa.z + (pb.z - pa.z) * t)
    };
    let mean = |s: &[usize]| {
        let n = s.len().max(1) as f64;
        let (x, y, z) = s.iter().fold((0.0, 0.0, 0.0), |(x, y, z), &c| (x + p[c].x, y + p[c].y, z + p[c].z));
        Point3::new(x / n, y / n, z / n)
    };
    let outward = mean(&outside) - mean(&inside);
    let mut push = |a: Point3, b: Point3, c: Point3| {
        let t = Triangle::new(a, b, c);
        let nrm = (b - a).cross(c - a);
        if nrm.dot(nrm) <= f64::MIN_POSITIVE {
            return; // a zero-area sliver adds no surface
        }
        out.push(if nrm.dot(outward) >= 0.0 { t } else { Triangle::new(a, c, b) });
    };
    match (inside.as_slice(), outside.as_slice()) {
        ([a], [b, c, e]) | ([b, c, e], [a]) => push(cross(*a, *b), cross(*a, *c), cross(*a, *e)),
        ([a, b], [c, e]) => {
            let (q0, q1, q2, q3) = (cross(*a, *c), cross(*a, *e), cross(*b, *e), cross(*b, *c));
            push(q0, q1, q2);
            push(q0, q2, q3);
        }
        _ => {}
    }
}

/// Edges used by an odd number of triangles: zero on a closed surface.
/// Vertices are welded on the 1e-9 lattice (`extract` interpolates each
/// shared edge from the same two samples, so shared points are equal).
#[must_use]
pub fn open_edges(tris: &[Triangle]) -> usize {
    let key = |p: Point3| [(p.x * 1e9).round() as i64, (p.y * 1e9).round() as i64, (p.z * 1e9).round() as i64];
    let mut count: HashMap<([i64; 3], [i64; 3]), u32> = HashMap::new();
    for t in tris {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            let (ka, kb) = (key(a), key(b));
            let e = if ka <= kb { (ka, kb) } else { (kb, ka) };
            *count.entry(e).or_insert(0) += 1;
        }
    }
    count.values().filter(|&&c| c % 2 == 1).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cube;
    use crate::stock_sim::FIELD_BAND;

    fn volume(tris: &[Triangle]) -> f64 {
        tris.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum()
    }

    #[test]
    fn a_box_extracts_closed_outward_and_near_its_volume() {
        let m = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 8.0, 6.0));
        let f = SdfField::from_mesh(&m.facets, 1.0, FIELD_BAND).unwrap();
        let tris = extract(&f).unwrap();
        assert!(!tris.is_empty());
        assert_eq!(open_edges(&tris), 0, "the extracted shell is watertight");
        let v = volume(&tris);
        // Outward normals give a positive volume; faces land exactly on
        // the cell-centre crossing, edges and corners are chamfered by
        // the tetrahedral split, so within 5% of 480.
        assert!(v > 0.0, "outward-facing: {v}");
        assert!((v - 480.0).abs() < 0.05 * 480.0, "volume {v} vs 480");
    }

    #[test]
    fn nonsense_steps_and_huge_grids_refuse_by_name() {
        let m = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 2.0, 2.0));
        let f = SdfField::from_mesh(&m.facets, 0.5, FIELD_BAND).unwrap();
        for s in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(extract_at(&f, s).is_err(), "step {s} must refuse");
        }
        assert!(extract_at(&f, 1e-4).is_err(), "a 20000^3 grid is past the budget");
    }
}
