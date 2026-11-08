//! Dual contouring of the stock field (M13.1, #23): the extracted
//! machined-stock mesh keeps its sharp interior corners - a pocket's
//! floor/wall edge stays an edge, where marching tetrahedra (`iso`)
//! chamfers it across a cell.
//! Provenance: Ju, Losasso, Schaefer & Warren, "Dual Contouring of
//! Hermite Data", SIGGRAPH 2002 - one vertex per sign-changing cell at
//! the minimizer of its QEF (`qef`), one quad per sign-changing lattice
//! edge joining the four cells around it. The Hermite data (`hermite`)
//! are the field's own: crossings on the sampled lattice, normals read
//! from the crossing sheet. Our own Rust; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Same samples, same sign rule.** The lattice is `iso::sample`'s;
//!   inside is d < 0 exactly as the marching extractor reads it, so the
//!   two meshes bound the same set of cells.
//! - **A corner is read from trusted planes.** A cell whose own crossing
//!   sits beside a ridge (the cell AT a corner, `Hermite::tight` false)
//!   borrows the trusted, same-facing planes of the 3x3x3 cells around
//!   it, so the face it misread still pins the vertex; a smooth cell
//!   reads only its own, as in the paper. The vertex stays clamped into
//!   its own cell (`qef`).
//! - **Closed or not at all.** Every sign-changing edge inside the
//!   lattice gets its quad, wound so the normal points from material to
//!   air. A sign change on the lattice's pad (or any cell left without a
//!   vertex) leaves an open edge; `iso::extract_with` then refuses the
//!   dual mesh and serves the marching one, never a mesh with a hole.
//! - **Total input.** A NaN sample reads as air (the sampler already
//!   refuses NaN by name); a crossing with no normal gives no plane, and
//!   a cell with none is its mass point (`qef`).

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

use super::hermite::{self, inside, EdgeKey, Hermite};
use super::iso::{Grid, CORNER};
use super::qef;

type V3 = [f64; 3];

/// The twelve cube edges, as pairs of `CORNER` indices.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

fn v3(p: Point3) -> V3 {
    [p.x, p.y, p.z]
}

fn offset(s: [usize; 3], o: [usize; 3]) -> [usize; 3] {
    [s[0] + o[0], s[1] + o[1], s[2] + o[2]]
}

/// The trusted planes of the 3x3x3 cells around `c` facing along
/// `facing`: every tight lattice edge of the 4x4x4-sample block.
fn block_planes(data: &HashMap<EdgeKey, Hermite>, c: [usize; 3], facing: V3) -> Vec<(V3, V3)> {
    let mut planes = Vec::new();
    for dk in 0..4usize {
        for dj in 0..4usize {
            for di in 0..4usize {
                let d = [di, dj, dk];
                let (Some(x), Some(y), Some(z)) =
                    ((c[0] + di).checked_sub(1), (c[1] + dj).checked_sub(1), (c[2] + dk).checked_sub(1))
                else {
                    continue;
                };
                for k in (0..3).filter(|&k| d[k] < 3) {
                    let Some(h) = data.get(&([x, y, z], k)).filter(|h| h.tight) else { continue };
                    let Some(n) = h.n else { continue };
                    if n.iter().zip(facing).map(|(a, b)| a * b).sum::<f64>() > 0.0 {
                        planes.push((h.p, n));
                    }
                }
            }
        }
    }
    planes
}

/// The QEF vertex of cell `c`, or `None` when the cell has no crossing.
fn cell_vertex(g: &Grid, data: &HashMap<EdgeKey, Hermite>, c: [usize; 3]) -> Option<Point3> {
    let own: Vec<&Hermite> = EDGES
        .iter()
        .filter_map(|(ea, eb)| {
            let (sa, sb) = (offset(c, CORNER[*ea]), offset(c, CORNER[*eb]));
            let k = (0..3).find(|&k| sb[k] != sa[k])?;
            data.get(&(sa, k))
        })
        .collect();
    if own.is_empty() {
        return None;
    }
    let m = own.len() as f64;
    let mass = own.iter().fold([0.0; 3], |s, h| [s[0] + h.p[0] / m, s[1] + h.p[1] / m, s[2] + h.p[2] / m]);
    let all: Vec<(V3, V3)> = own.iter().filter_map(|h| h.n.map(|n| (h.p, n))).collect();
    let planes = if own.iter().all(|h| h.tight) {
        all
    } else {
        let facing = all.iter().fold([0.0; 3], |s, (_, n)| [s[0] + n[0], s[1] + n[1], s[2] + n[2]]);
        let borrowed = block_planes(data, c, facing);
        if borrowed.is_empty() {
            all
        } else {
            borrowed
        }
    };
    let (lo, hi) = (v3(g.at(c)), v3(g.at(offset(c, [1, 1, 1]))));
    let x = qef::solve(&planes, mass, lo, hi);
    Some(Point3::new(x[0], x[1], x[2]))
}

/// Dual-contour the sampled lattice: one QEF vertex per sign-changing
/// cell, one quad (two triangles) per sign-changing lattice edge.
pub(crate) fn contour(g: &Grid) -> Vec<Triangle> {
    let data = hermite::edges(g);
    let mut verts: HashMap<[usize; 3], Point3> = HashMap::new();
    for k in 0..g.n[2] {
        for j in 0..g.n[1] {
            for i in 0..g.n[0] {
                let c = [i, j, k];
                let ins = CORNER.iter().filter(|o| inside(g.val(offset(c, **o)))).count();
                if ins == 0 || ins == 8 {
                    continue;
                }
                if let Some(v) = cell_vertex(g, &data, c) {
                    verts.insert(c, v);
                }
            }
        }
    }
    let mut out = Vec::new();
    for k in 0..=g.n[2] {
        for j in 0..=g.n[1] {
            for i in 0..=g.n[0] {
                for a in 0..3 {
                    quad(g, &verts, [i, j, k], a, &mut out);
                }
            }
        }
    }
    out
}

/// The quad of the lattice edge from sample `s` along axis `a`, if it
/// changes sign and all four cells around it carry a vertex.
fn quad(g: &Grid, verts: &HashMap<[usize; 3], Point3>, s: [usize; 3], a: usize, out: &mut Vec<Triangle>) {
    if s[a] >= g.n[a] {
        return;
    }
    let mut e = s;
    e[a] += 1;
    let lower_in = inside(g.val(s));
    if lower_in == inside(g.val(e)) {
        return;
    }
    let (b, c) = ((a + 1) % 3, (a + 2) % 3);
    if s[b] == 0 || s[c] == 0 || s[b] >= g.n[b] || s[c] >= g.n[c] {
        return; // on the pad: the open edge hands the mesh to marching
    }
    // Counter-clockwise about +a in the (b, c) plane (b x c = a).
    let mut q = [Point3::ZERO; 4];
    for (slot, (db, dc)) in q.iter_mut().zip([(1, 1), (0, 1), (0, 0), (1, 0)]) {
        let mut cell = s;
        cell[b] -= db;
        cell[c] -= dc;
        let Some(v) = verts.get(&cell) else { return };
        *slot = *v;
    }
    if !lower_in {
        q.swap(1, 3); // material above: the outward normal is -a
    }
    let len2 = |u: Point3, v: Point3| (u - v).dot(u - v);
    let tris = if len2(q[0], q[2]) <= len2(q[1], q[3]) {
        [(q[0], q[1], q[2]), (q[0], q[2], q[3])]
    } else {
        [(q[0], q[1], q[3]), (q[1], q[2], q[3])]
    };
    for (x, y, z) in tris {
        if x == y || y == z || z == x {
            continue; // two cells clamped to one point: no surface here
        }
        out.push(Triangle::new(x, y, z));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, f: impl Fn(Point3) -> f64) -> Grid {
        let mut g = Grid { lo: Point3::ZERO, step: 1.0, n: [n; 3], d: Vec::new() };
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let p = g.at([i, j, k]);
                    g.d.push(f(p));
                }
            }
        }
        g
    }

    #[test]
    fn an_all_air_or_all_material_lattice_is_empty() {
        assert!(contour(&grid(3, |_| 1.0)).is_empty());
        assert!(contour(&grid(3, |_| -1.0)).is_empty());
        assert!(contour(&Grid { lo: Point3::ZERO, step: 1.0, n: [0; 3], d: vec![-1.0] }).is_empty());
    }

    #[test]
    fn a_single_material_sample_closes_around_itself() {
        // One inside sample in a 2-cell lattice: its six lattice edges
        // give six quads around it - a closed shell, wound outward.
        let g = grid(2, |p| if p == Point3::new(1.0, 1.0, 1.0) { -0.5 } else { 0.5 });
        let tris = contour(&g);
        assert!(!tris.is_empty());
        assert_eq!(super::super::iso::open_edges(&tris), 0);
        let vol: f64 = tris.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum();
        assert!(vol > 0.0, "outward: {vol}");
    }

    #[test]
    fn nan_samples_never_panic_and_stay_in_their_cells() {
        let g = grid(4, |p| if p.x == 2.0 { f64::NAN } else { p.z - 1.7 });
        let tris = contour(&g);
        for t in &tris {
            for p in [t.a, t.b, t.c] {
                assert!(p.is_finite() && (0.0..=4.0).contains(&p.z), "{p:?}");
            }
        }
        // A truncated lattice (fewer values than samples) reads NaN too.
        let mut short = grid(3, |p| p.z - 1.5);
        short.d.truncate(10);
        let _ = contour(&short);
    }
}
