//! Each stock facet's chord-sagitta allowance for the cross-hole entry
//! (issues #85, #121): a tessellated round's facets are chords lying
//! inside the true surface, so a facet creased against its edge
//! neighbours by a round's shallow angle is read as the slab its true
//! surface can occupy. Neighbours are found by edge proximity, not by
//! bit-equal vertices: the mesh's vertices are first welded within a
//! tolerance (1e-6 of the model's size) on a spatial hash, so a
//! non-conforming export - each facet carrying its own, slightly
//! different copies of its corners - gets the same allowance as a
//! welded one. A genuinely open edge (no facet edge within tolerance)
//! still gets none.
//!
//! Provenance: the chord sagitta s = (w/2) tan(theta/4) of an inscribed
//! polygon and tolerance vertex welding on a uniform grid hash are
//! textbook computational geometry. Our own design notes; no vendor
//! material consulted.

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

/// A dihedral angle above this between edge neighbours is a real edge
/// (a box's 90, a hex bar's 60 degrees), not a tessellated round's
/// crease; at or below it the facet carries a sagitta allowance (an
/// octagon reads as a coarse round: air, never metal).
const SHARP: f64 = std::f64::consts::FRAC_PI_4;

/// The weld tolerance as a fraction of the model's bounding diagonal.
const WELD_REL: f64 = 1e-6;
/// The weld tolerance's floor (mm): a degenerate or tiny model still
/// welds bit-near copies.
const WELD_MIN: f64 = 1e-9;

pub(super) fn unit_normal(t: &Triangle) -> Option<Point3> {
    let n = (t.b - t.a).cross(t.c - t.a);
    let l = n.dot(n).sqrt();
    (l.is_finite() && l > 1e-12).then(|| Point3::new(n.x / l, n.y / l, n.z / l))
}

/// Distance of `p` from the line through `a` and `b`.
fn off_line(p: Point3, a: Point3, b: Point3) -> f64 {
    let e = b - a;
    let l = e.dot(e).sqrt();
    if !(l.is_finite() && l > 1e-12) {
        return 0.0;
    }
    let c = (p - a).cross(e);
    c.dot(c).sqrt() / l
}

/// Welds points within `tol` of an earlier representative onto it, on a
/// grid hash of cell `tol`: each lookup scans the 27 cells about the
/// point, and representatives are more than `tol` apart, so a cell
/// holds a bounded few - O(n) overall, deterministic in input order.
struct Welder {
    tol: f64,
    reps: Vec<Point3>,
    grid: HashMap<[i64; 3], Vec<usize>>,
}

impl Welder {
    fn new(tol: f64) -> Self {
        Welder { tol, reps: Vec::new(), grid: HashMap::new() }
    }

    fn cell(&self, p: Point3) -> [i64; 3] {
        // `as` saturates: a far-off coordinate lands in an edge cell, never panics.
        [p.x, p.y, p.z].map(|c| (c / self.tol).floor() as i64)
    }

    fn id(&mut self, p: Point3) -> usize {
        let c = self.cell(p);
        let mut best: Option<usize> = None;
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let k = [c[0].saturating_add(dx), c[1].saturating_add(dy), c[2].saturating_add(dz)];
                    for &r in self.grid.get(&k).into_iter().flatten() {
                        let d = p - self.reps[r];
                        if d.dot(d) <= self.tol * self.tol && best.is_none_or(|b| r < b) {
                            best = Some(r);
                        }
                    }
                }
            }
        }
        best.unwrap_or_else(|| {
            let r = self.reps.len();
            self.reps.push(p);
            self.grid.entry(c).or_default().push(r);
            r
        })
    }
}

/// The weld tolerance for `tris`: `WELD_REL` of the finite vertices'
/// bounding diagonal, never under `WELD_MIN`.
fn weld_tol(tris: &[Triangle]) -> f64 {
    let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]).filter(|p| p.is_finite()) {
        lo = Point3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Point3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let d = hi - lo;
    let diag = d.dot(d).sqrt();
    if diag.is_finite() { (WELD_REL * diag).max(WELD_MIN) } else { WELD_MIN }
}

/// Each facet's sagitta allowance. Across an edge shared (within the
/// weld tolerance) with a facet whose normal turns by `theta`
/// (0 < theta <= SHARP; |cos| read, so a flipped winding never hides a
/// crease), the true round through the shared vertices bulges at most
/// `(h/2) tan(theta/2)` past the facet, `h` the facet's width across
/// that edge - the inscribed polygon's `(h/2) tan(theta/4)` doubled,
/// which stays an upper bound however narrow the neighbour is. A
/// facet's allowance is its largest.
pub(super) fn sag_of(tris: &[Triangle]) -> Vec<f64> {
    let normals: Vec<Option<Point3>> = tris.iter().map(unit_normal).collect();
    let mut weld = Welder::new(weld_tol(tris));
    let mut edges: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        if normals[i].is_none() || !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
            continue;
        }
        let v = [t.a, t.b, t.c].map(|p| weld.id(p));
        for k in 0..3 {
            let (a, b) = (v[k], v[(k + 1) % 3]);
            if a != b {
                edges.entry((a.min(b), a.max(b))).or_default().push((i, k));
            }
        }
    }
    let mut sag = vec![0.0_f64; tris.len()];
    let width = |i: usize, k: usize| {
        let v = [tris[i].a, tris[i].b, tris[i].c];
        off_line(v[(k + 2) % 3], v[k], v[(k + 1) % 3])
    };
    for side in edges.values() {
        for (x, &(i, ki)) in side.iter().enumerate() {
            for &(j, kj) in &side[x + 1..] {
                let (Some(ni), Some(nj)) = (normals[i], normals[j]) else { continue };
                let theta = ni.dot(nj).abs().min(1.0).acos();
                if !(theta > 1e-9 && theta <= SHARP) {
                    continue;
                }
                let tn = (0.5 * theta).tan();
                sag[i] = sag[i].max(0.5 * width(i, ki) * tn);
                sag[j] = sag[j].max(0.5 * width(j, kj) * tn);
            }
        }
    }
    sag
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Side quad `k` of a 16-gon of r20, z in [-10, 10], as two facets,
    /// moved rigidly by `(dt, dz)`: `dt` along the rim tangent at its
    /// first vertex, `dz` along Z.
    fn quad(k: usize, dt: f64, dz: f64) -> [Triangle; 2] {
        let th = |j: usize| std::f64::consts::TAU * j as f64 / 16.0;
        let (s0, c0) = th(k).sin_cos();
        let v = |j: usize, z: f64| Point3::new(20.0 * th(j).cos() - dt * s0, 20.0 * th(j).sin() + dt * c0, z + dz);
        let (a1, a2, b1, b2) = (v(k, -10.0), v(k + 1, -10.0), v(k, 10.0), v(k + 1, 10.0));
        [Triangle::new(a1, a2, b2), Triangle::new(a1, b2, b1)]
    }

    /// Two neighbouring facets of a round whose shared edge's copies
    /// differ within the weld tolerance carry the welded allowance; a
    /// gap past it (a genuinely open edge) carries none.
    #[test]
    fn an_unwelded_round_edge_carries_the_allowance_and_an_open_edge_none() {
        let pair = |dt: f64, dz: f64| {
            let mut t = quad(0, 0.0, 0.0).to_vec();
            t.extend(quad(1, dt, dz));
            sag_of(&t)
        };
        let welded = pair(0.0, 0.0);
        // The facets either side of the shared edge (quad 0's first, quad
        // 1's second) carry its crease; the others meet only open edges.
        assert!(welded[0] > 0.1 && welded[3] > 0.1, "a welded crease: {welded:?}");
        for (dt, dz) in [(0.0, 5e-6), (5e-6, 0.0), (-3e-6, 3e-6)] {
            let got = pair(dt, dz);
            for (g, w) in got.iter().zip(&welded) {
                assert!((g - w).abs() < 1e-4, "copies {dt},{dz} apart: {got:?} vs welded {welded:?}");
            }
        }
        for (dt, dz) in [(0.0, 0.5), (1e-3, 0.0)] {
            let got = pair(dt, dz);
            assert!(got.iter().all(|&s| s == 0.0), "an open edge {dt},{dz} apart: {got:?}");
        }
        // A lone strip: no neighbour at all.
        assert!(sag_of(&quad(0, 0.0, 0.0)).iter().all(|&s| s == 0.0));
    }
}
