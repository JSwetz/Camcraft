//! Feature edges of a triangle soup: the wireframe a CAM user expects to
//! see - boundary rims, creases, and non-manifold seams, but never the
//! coplanar facet stitches in between (shading carries those surfaces).
//!
//! Provenance: our own algorithm, specced from public CAD-viewer practice
//! (dihedral-threshold crease extraction); lifted verbatim in intent from
//! `crates/ui/src/viewport.rs` into the app layer for M1.5, so face and
//! chain topology build on one shared weld.
//!
//! Welding is by quantized position on the project's canonical lattice
//! (1 µm, the same factor `slice::WELD` and `geo2d::SCALE` use), so STL's
//! duplicated corners count as shared. Welding is per call - never across
//! parts: a stock face coplanar with a model face must not erase either
//! solid's outline.
//!
//! Robustness bar (AGENTS.md Prime Directive): vertices that refuse to
//! weld (non-finite) never alias real lattice points; their edges are
//! emitted as [`EdgeKind::Degenerate`] - a wrong-but-visible line beats a
//! silently missing one. No panics on any input.

use std::collections::HashMap;

/// Weld lattice: 1e6 steps per mm = 1 µm. Coarser than the old viewport
/// lattice (1/4096 mm), so clean meshes weld at least as aggressively.
const S: f64 = 1e6;

/// Dihedral cutoff: an edge between two facets whose normals sit closer
/// than this angle apart is a smooth seam and stays hidden. Named here so
/// face clustering (M1.5) and this wire agree on one tuning point.
const CREASE_DEG: f64 = 40.0;

/// The three corner pairs of a triangle, as slot offsets.
const CORNERS: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];

type Pos = (i64, i64, i64);
type EdgeKey = (Pos, Pos);

/// Why an edge made it into the wireframe. All kinds are drawn; the
/// distinction is for topology (faces and chains) and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// Exactly one facet owns the seam - the rim of the surface or a hole.
    Boundary,
    /// Two facets bending past 40 degrees.
    Crease,
    /// Three or more facets share the seam - suspect, drawn anyway.
    NonManifold,
    /// An endpoint would not weld (NaN/inf) or a normal could not be
    /// computed - drawn so the flaw is visible, never dropped.
    Degenerate,
}

/// One feature edge, as slot indices into the caller's flat array:
/// triangle `t` owns corner `k` at slot `3 * t + k`, endpoints
/// `pts[i]` and `pts[j]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureEdge {
    pub i: u32,
    pub j: u32,
    pub kind: EdgeKind,
}

/// Feature edges of a flat triangle soup (3 positions per facet, f64).
///
/// Total on any input: empty in, empty out; NaN vertices come back tagged
/// [`EdgeKind::Degenerate`] instead of poisoning the weld map.
#[must_use]
pub fn feature_edges(pts: &[[f64; 3]]) -> Vec<FeatureEdge> {
    let tri_count = pts.len() / 3;
    if tri_count == 0 {
        return Vec::new();
    }
    let keys: Vec<Option<Pos>> = pts.iter().map(weld_pos).collect();

    // Pass 1: count how many facets own each weldable edge. `first`
    // remembers the owner that emits it; `other` the last co-owner seen,
    // used for the dihedral test.
    #[derive(Clone, Copy)]
    struct Side {
        first: usize,
        other: usize,
        count: u32,
    }
    let mut shared: HashMap<EdgeKey, Side> = HashMap::new();
    for t in 0..tri_count {
        for (ci, cj) in CORNERS {
            let (i, j) = (3 * t + ci, 3 * t + cj);
            let Some(key) = edge_key(&keys, i, j) else {
                continue; // unweldable edges are never shared
            };
            shared
                .entry(key)
                .and_modify(|s| s.other = t)
                .or_insert(Side {
                    first: t,
                    other: t,
                    count: 0,
                })
                .count += 1;
        }
    }

    // Pass 2: emit in triangle order. Each welded edge is emitted once by
    // its owner; coplanar smooth seams are skipped entirely.
    let cos_crease = CREASE_DEG.to_radians().cos();
    let mut out: Vec<FeatureEdge> = Vec::new();
    for t in 0..tri_count {
        for (ci, cj) in CORNERS {
            let (i, j) = (3 * t + ci, 3 * t + cj);
            let kind = match edge_key(&keys, i, j) {
                None => EdgeKind::Degenerate,
                Some(key) => {
                    let Some(side) = shared.get(&key) else {
                        continue; // unreachable: pass 1 registered it
                    };
                    if side.first != t {
                        continue; // only the owning triangle emits
                    }
                    match side.count {
                        1 => EdgeKind::Boundary,
                        c if c >= 3 => EdgeKind::NonManifold,
                        _ => match (unit_normal(&keys, pts, t), unit_normal(&keys, pts, side.other)) {
                            (Some(x), Some(y)) => {
                                if x[0] * y[0] + x[1] * y[1] + x[2] * y[2] < cos_crease {
                                    EdgeKind::Crease
                                } else {
                                    continue; // smooth seam
                                }
                            }
                            _ => EdgeKind::Degenerate,
                        },
                    }
                }
            };
            out.push(FeatureEdge {
                i: i as u32,
                j: j as u32,
                kind,
            });
        }
    }
    out
}

/// Lattice key of a vertex, or None if it must never alias another point.
fn weld_pos(v: &[f64; 3]) -> Option<Pos> {
    let s = |x: f64| {
        let q = (x * S).round();
        q.is_finite().then_some(q as i64)
    };
    Some((s(v[0])?, s(v[1])?, s(v[2])?))
}

/// Canonical edge key (low endpoint first), None if either end won't weld.
fn edge_key(keys: &[Option<Pos>], i: usize, j: usize) -> Option<EdgeKey> {
    let (ki, kj) = (keys[i]?, keys[j]?);
    Some(if ki <= kj { (ki, kj) } else { (kj, ki) })
}

/// Unit facet normal from the welded lattice positions (bit-equal corners
/// cannot fake a normal), None for degenerate or non-finite facets.
fn unit_normal(keys: &[Option<Pos>], pts: &[[f64; 3]], t: usize) -> Option<[f64; 3]> {
    let corner = |k: usize| -> Option<[f64; 3]> {
        let slot = 3 * t + k;
        keys[slot]?; // reject non-finite endpoints
        Some(pts[slot])
    };
    let (a, b, c) = (corner(0)?, corner(1)?, corner(2)?);
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
    if len2.is_finite() && len2 > 1e-24 {
        let inv = 1.0 / len2.sqrt();
        Some([n[0] * inv, n[1] * inv, n[2] * inv])
    } else {
        None
    }
}
