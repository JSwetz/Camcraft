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
//! Face clustering (M1.5 step 2) joins facets across shared smooth seams
//! into planar faces with boundary loops - the entity the user selects to
//! define a machining boundary. The plane test is exact: facet normals are
//! cross products of lattice-integer edges, reduced to a primitive integer
//! direction plus an integer offset, so two coplanar facets agree
//! bit-for-bit regardless of tessellation. Face ids hash that plane
//! signature together with the boundary's welded corner positions and the
//! quantized area - never facet indices - so ids survive STEP's
//! nondeterministic shell order and model moves (they resolve in the
//! entity's local frame).
//!
//! Robustness bar (AGENTS.md Prime Directive): vertices that refuse to
//! weld (non-finite) never alias real lattice points; their edges are
//! emitted as [`EdgeKind::Degenerate`] - a wrong-but-visible line beats a
//! silently missing one. Facets whose plane cannot be represented exactly
//! (integer overflow on absurd input) become [`FaceKind::Degenerate`]
//! faces, never wrong planes. No panics on any input.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::geometry::Triangle;

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
/// Values beyond 9e18 steps (9 Tm) refuse to weld rather than saturate
/// onto the range clamp - two absurd coordinates landing on `i64::MAX`
/// would alias each other silently, the exact failure we exist to avoid.
fn weld_pos(v: &[f64; 3]) -> Option<Pos> {
    let s = |x: f64| {
        let q = (x * S).round();
        (q.is_finite() && (-9.0e18..=9.0e18).contains(&q)).then_some(q as i64)
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

// ===================== face clustering =====================

/// Stable identity of a face: a hash over its exact plane signature, the
/// welded lattice positions of its boundary corners, and its quantized
/// area. Facet order (STEP) and facet count (re-tessellation of the same
/// region) never change it - interior weld points are not corners.
pub type FaceId = u64;

/// Role of one of a face's boundary rings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopKind {
    /// The ring enclosing the face; the largest closed ring.
    Outer,
    /// A closed ring inside [`LoopKind::Outer`] - a hole through the face.
    Hole,
    /// The boundary refused a clean ring walk (rings touching at a vertex,
    /// non-manifold seams, failed nesting validation): real edges, but
    /// region-building must treat the face as un-closable, never guess.
    Open,
}

/// Whether a face carries a usable plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceKind {
    /// All member facets share one exactly-representable plane.
    Planar,
    /// Corners or plane refused to compute; visible in the wire, inert
    /// for selection.
    Degenerate,
}

/// One boundary ring, as vertex ids into [`Features::verts`]. Closed rings
/// are oriented so the lattice shoelace agrees with the face normal;
/// [`LoopKind::Open`] trails run start to end as walked.
#[derive(Clone, Debug)]
pub struct Loop {
    pub kind: LoopKind,
    pub verts: Vec<u32>,
}

/// A planar face: its facets, boundary rings, and exact plane.
#[derive(Clone, Debug)]
pub struct Face {
    pub id: FaceId,
    pub kind: FaceKind,
    /// Unit plane normal (winding-independent); `[0.0; 3]` when
    /// [`FaceKind::Degenerate`].
    pub normal: [f64; 3],
    /// Signed plane offset in mm along [`Face::normal`] from the origin.
    pub offset: f64,
    /// Summed facet area, mm².
    pub area: f64,
    pub loops: Vec<Loop>,
    /// Facet indices into the caller's array.
    pub facets: Vec<u32>,
    /// Exact primitive plane (lattice units); private - geometry consumers
    /// use normal/offset, the id builder uses this.
    plane: Option<Plane>,
}

/// A welded topological feature edge for chains (step 3) and picking.
/// Only the first `n_facets.min(3)` entries of `facets` are meaningful.
#[derive(Clone, Copy, Debug)]
pub struct TopEdge {
    pub a: u32,
    pub b: u32,
    pub kind: EdgeKind,
    pub facets: [u32; 3],
    pub n_facets: u32,
}

/// Topology of one mesh, in the entity's local frame. Deterministic for a
/// facet order; [`Face`] identities are order-free by construction.
#[derive(Clone, Debug, Default)]
pub struct Features {
    /// Unique welded vertex positions, mm.
    pub verts: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
    /// facet index -> face index, covering every input facet.
    pub facet_face: Vec<u32>,
    /// Feature edges only (no interior seams), ordered by (a, b).
    pub edges: Vec<TopEdge>,
}

/// Exact integer plane in lattice units: sign-canonical primitive normal
/// plus offset. Coplanar facets agree bit-for-bit, whatever their
/// tessellation, because cross products of lattice edges are exact.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Plane([i128; 3], i128);

/// Winding-preserving primitive normal (for the dihedral test), the
/// sign-canonical plane (for identity), and area in mm².
#[derive(Clone, Copy)]
struct FacetGeom {
    plane: Option<Plane>,
    raw: Option<[i128; 3]>,
    area: f64,
}

impl FacetGeom {
    const BAD: FacetGeom = FacetGeom {
        plane: None,
        raw: None,
        area: 0.0,
    };
}

/// Incident facets of one welded edge.
struct EdgeEntry {
    facets: [u32; 3],
    count: u32,
}

/// Cluster a triangle soup into planar faces with boundary loops.
///
/// Total on any input, like [`feature_edges`]: facets that refuse to weld
/// or define an exact plane become [`FaceKind::Degenerate`] faces; rings
/// that refuse to close come back [`LoopKind::Open`].
#[must_use]
pub fn features_of_facets(facets: &[Triangle]) -> Features {
    if facets.is_empty() {
        return Features::default();
    }

    // Welded vertex table; a facet with an unweldable corner is invalid.
    let mut vmap: HashMap<Pos, u32> = HashMap::new();
    let mut lattice: Vec<Pos> = Vec::new();
    let mut fv: Vec<Option<[u32; 3]>> = Vec::with_capacity(facets.len());
    for tri in facets {
        let pts = [
            [tri.a.x, tri.a.y, tri.a.z],
            [tri.b.x, tri.b.y, tri.b.z],
            [tri.c.x, tri.c.y, tri.c.z],
        ];
        let mut row = [0u32; 3];
        let mut ok = true;
        for (k, p) in pts.iter().enumerate() {
            match weld_pos(p) {
                Some(pos) => {
                    let id = *vmap.entry(pos).or_insert_with(|| {
                        let id = lattice.len() as u32;
                        lattice.push(pos);
                        id
                    });
                    row[k] = id;
                }
                None => ok = false,
            }
        }
        fv.push(ok.then_some(row));
    }
    let verts: Vec<[f64; 3]> = lattice
        .iter()
        .map(|p| [p.0 as f64 / S, p.1 as f64 / S, p.2 as f64 / S])
        .collect();

    // Exact geometry per facet, and the welded-edge table.
    let geoms: Vec<FacetGeom> = fv
        .iter()
        .map(|row| match row {
            Some([a, b, c]) => {
                facet_geom(lattice[*a as usize], lattice[*b as usize], lattice[*c as usize])
            }
            None => FacetGeom::BAD,
        })
        .collect();
    let mut edge_map: HashMap<(u32, u32), EdgeEntry> = HashMap::new();
    for (t, row) in fv.iter().enumerate() {
        let Some([a, b, c]) = *row else { continue };
        for (x, y) in [(a, b), (b, c), (c, a)] {
            let key = if x <= y { (x, y) } else { (y, x) };
            let e = edge_map
                .entry(key)
                .or_insert(EdgeEntry { facets: [u32::MAX; 3], count: 0 });
            if (e.count as usize) < e.facets.len() {
                e.facets[e.count as usize] = t as u32;
            }
            e.count += 1;
        }
    }

    // Join facets across shared *smooth* seams (winding-aware dihedral,
    // the same 40-degree rule the wire uses).
    let mut parent: Vec<u32> = (0..facets.len() as u32).collect();
    for e in edge_map.values() {
        if e.count == 2
            && smooth_join(&geoms[e.facets[0] as usize], &geoms[e.facets[1] as usize])
        {
            union(&mut parent, e.facets[0], e.facets[1]);
        }
    }

    // Split each smooth patch by exact plane signature: a subdivided
    // planar top stays ONE face; a faceted dome yields one strip per
    // segment (documented coarse-mesh limit). Invalid facets stand alone.
    let mut groups: HashMap<(u32, Option<Plane>), Vec<u32>> = HashMap::new();
    for t in 0..facets.len() as u32 {
        let cell = match fv[t as usize] {
            Some(_) => (find(&mut parent, t), geoms[t as usize].plane),
            None => (t, None),
        };
        groups.entry(cell).or_default().push(t);
    }
    let mut groups: Vec<Vec<u32>> = groups.into_values().collect();
    groups.sort_by_key(|g| g[0]); // deterministic face order

    let mut faces: Vec<Face> = Vec::with_capacity(groups.len());
    let mut facet_face = vec![0u32; facets.len()];
    for g in groups {
        for &t in &g {
            facet_face[t as usize] = faces.len() as u32;
        }
        let plane = geoms[g[0] as usize].plane;
        let (normal, offset, kind) = match plane {
            Some(Plane(n, d)) => {
                let m = int_norm(n);
                let off = if m > 0.0 && m.is_finite() {
                    d as f64 / m / S
                } else {
                    0.0
                };
                (unit_int(n), off, FaceKind::Planar)
            }
            None => ([0.0; 3], 0.0, FaceKind::Degenerate),
        };
        let mut areas: Vec<f64> = g.iter().map(|&t| geoms[t as usize].area).collect();
        areas.sort_by(|x, y| x.total_cmp(y)); // order-free summation
        faces.push(Face {
            id: 0, // filled below, once the loops are known
            kind,
            normal,
            offset,
            area: areas.iter().sum(),
            loops: Vec::new(),
            facets: g,
            plane,
        });
    }

    // Classify edges (the wire's kinds, over face structure): an edge is a
    // feature edge of exactly those faces it separates; edges inside one
    // face are seams and stay hidden.
    let mut boundary: Vec<Vec<(u32, u32)>> = vec![Vec::new(); faces.len()];
    let mut edges: Vec<TopEdge> = Vec::new();
    let mut ordered: Vec<((u32, u32), EdgeEntry)> = edge_map.into_iter().collect();
    ordered.sort_by_key(|(k, _)| *k);
    for ((a, b), e) in ordered {
        let mut face_ids: Vec<u32> = Vec::new();
        let mut all_planar = true;
        for slot in 0..e.count.min(3) as usize {
            let t = e.facets[slot] as usize;
            let fi = facet_face[t];
            if !face_ids.contains(&fi) {
                face_ids.push(fi);
            }
            if geoms[t].plane.is_none() {
                all_planar = false;
            }
        }
        if face_ids.len() == 1 && e.count >= 2 {
            // All its facets are one face: an interior seam, invisible -
            // unless the fold is non-manifold, which is a real line even
            // though it bounds nothing.
            if e.count >= 3 {
                edges.push(TopEdge {
                    a,
                    b,
                    kind: EdgeKind::NonManifold,
                    facets: e.facets,
                    n_facets: e.count,
                });
            }
            continue;
        }
        let kind = if e.count == 1 {
            EdgeKind::Boundary
        } else if e.count >= 3 {
            EdgeKind::NonManifold
        } else if !all_planar {
            EdgeKind::Degenerate
        } else {
            EdgeKind::Crease // includes tangent seams between distinct faces
        };
        for &fi in &face_ids {
            boundary[fi as usize].push((a, b));
        }
        edges.push(TopEdge {
            a,
            b,
            kind,
            facets: e.facets,
            n_facets: e.count,
        });
    }

    // Ring-walk every face boundary, then id every face.
    for fi in 0..faces.len() {
        let bis = std::mem::take(&mut boundary[fi]);
        let plane = faces[fi].plane;
        faces[fi].loops = build_loops(&bis, &lattice, plane);
        faces[fi].id = face_id(&faces[fi], &lattice);
    }

    Features {
        verts,
        faces,
        facet_face,
        edges,
    }
}

/// Content digest for the `model::features_of` cache: sorted per-facet
/// corner keys (order-free), so a STEP re-import of the same geometry
/// keeps its cache entry and its face ids.
#[must_use]
pub fn digest_of(facets: &[Triangle]) -> u64 {
    let mut digs: Vec<u64> = Vec::with_capacity(facets.len());
    for tri in facets {
        let mut corners: Vec<Option<Pos>> = vec![
            weld_pos(&[tri.a.x, tri.a.y, tri.a.z]),
            weld_pos(&[tri.b.x, tri.b.y, tri.b.z]),
            weld_pos(&[tri.c.x, tri.c.y, tri.c.z]),
        ];
        corners.sort_by_key(|c| *c); // None first, then lattice order
        let mut h = DefaultHasher::new();
        corners.hash(&mut h);
        digs.push(h.finish());
    }
    digs.sort_unstable();
    let mut h = DefaultHasher::new();
    facets.len().hash(&mut h);
    digs.hash(&mut h);
    h.finish()
}

// ---------------- clustering internals ----------------

/// Exact plane + winding primitive + area of one welded facet; None-parts
/// for zero-area or integer-overflow (absurd-input) facets.
fn facet_geom(a: Pos, b: Pos, c: Pos) -> FacetGeom {
    let inner = || -> Option<(Plane, [i128; 3], f64)> {
        let e = |p: Pos, q: Pos| {
            [
                (q.0 - p.0) as i128,
                (q.1 - p.1) as i128,
                (q.2 - p.2) as i128,
            ]
        };
        let (e1, e2) = (e(a, b), e(a, c));
        let m = |x: i128, y: i128| x.checked_mul(y);
        let cr = [
            m(e1[1], e2[2])?.checked_sub(m(e1[2], e2[1])?)?,
            m(e1[2], e2[0])?.checked_sub(m(e1[0], e2[2])?)?,
            m(e1[0], e2[1])?.checked_sub(m(e1[1], e2[0])?)?,
        ];
        let mag2 = cr
            .iter()
            .fold(0.0f64, |acc, x| acc + (*x as f64) * (*x as f64));
        let area = if mag2.is_finite() {
            0.5 * mag2.sqrt() / (S * S)
        } else {
            f64::INFINITY
        };
        let g = gcd_u128(
            gcd_u128(cr[0].unsigned_abs(), cr[1].unsigned_abs()),
            cr[2].unsigned_abs(),
        );
        if g == 0 {
            return None; // zero-area facet: no plane to trust
        }
        let gi = g as i128;
        let raw = [cr[0] / gi, cr[1] / gi, cr[2] / gi];
        let mut n = raw;
        let mut neg = false;
        for x in n.iter() {
            if *x != 0 {
                neg = *x < 0;
                break;
            }
        }
        if neg {
            n = [-n[0], -n[1], -n[2]];
        }
        let d = m(n[0], a.0 as i128)?
            .checked_add(m(n[1], a.1 as i128)?)?
            .checked_add(m(n[2], a.2 as i128)?)?;
        Some((Plane(n, d), raw, area))
    };
    match inner() {
        Some((plane, raw, area)) => FacetGeom {
            plane: Some(plane),
            raw: Some(raw),
            area,
        },
        None => FacetGeom::BAD,
    }
}

/// Winding-aware dihedral test on the raw primitive normals: smooth iff
/// the normals sit closer than the shared `CREASE_DEG` threshold.
fn smooth_join(g1: &FacetGeom, g2: &FacetGeom) -> bool {
    let (Some(n1), Some(n2)) = (g1.raw, g2.raw) else {
        return false;
    };
    let dot = n1[0] as f64 * n2[0] as f64
        + n1[1] as f64 * n2[1] as f64
        + n1[2] as f64 * n2[2] as f64;
    let (m1, m2) = (int_norm_sq(n1), int_norm_sq(n2));
    m1 > 0.0 && m2 > 0.0 && dot >= (m1 * m2).sqrt() * CREASE_DEG.to_radians().cos()
}

fn int_norm_sq(n: [i128; 3]) -> f64 {
    n.iter().fold(0.0f64, |acc, x| acc + (*x as f64) * (*x as f64))
}

fn int_norm(n: [i128; 3]) -> f64 {
    int_norm_sq(n).sqrt()
}

fn unit_int(n: [i128; 3]) -> [f64; 3] {
    let m = int_norm(n);
    if m > 0.0 && m.is_finite() {
        [n[0] as f64 / m, n[1] as f64 / m, n[2] as f64 / m]
    } else {
        [0.0; 3]
    }
}

/// Component of a lattice position by axis index (Pos is a tuple).
fn pos_get(p: Pos, k: usize) -> i64 {
    match k {
        0 => p.0,
        1 => p.1,
        _ => p.2,
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        let g = parent[parent[x as usize] as usize];
        parent[x as usize] = g;
        x = g;
    }
    x
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra as usize] = rb;
    }
}

/// Walk boundary edges into rings. Degrees are computed on the face's own
/// boundary set; a closed ring that passes through a vertex of degree != 2
/// (rings touching, a figure-eight) is reported [`LoopKind::Open`], never
/// silently split wrong.
fn build_loops(bis: &[(u32, u32)], lattice: &[Pos], plane: Option<Plane>) -> Vec<Loop> {
    if bis.is_empty() {
        return Vec::new();
    }
    let mut adj: HashMap<u32, Vec<(u32, usize)>> = HashMap::new();
    for (k, &(a, b)) in bis.iter().enumerate() {
        adj.entry(a).or_default().push((b, k));
        adj.entry(b).or_default().push((a, k));
    }
    let degree = |v: u32| adj.get(&v).map_or(0, Vec::len);
    let mut used = vec![false; bis.len()];
    let mut trails: Vec<(Vec<u32>, bool)> = Vec::new();
    while let Some(start_e) = used.iter().position(|u| !*u) {
        let (a, b) = bis[start_e];
        let mut cur = if degree(a) != 2 { a } else { b };
        let start = cur;
        let mut path = vec![cur];
        let mut closed = false;
        loop {
            let nxt = match adj.get(&cur) {
                Some(list) => list.iter().find(|(_, e)| !used[*e]).copied(),
                None => None,
            };
            let Some((w, e)) = nxt else { break };
            used[e] = true;
            path.push(w);
            cur = w;
            if cur == start && path.len() > 2 {
                closed = true;
                break;
            }
        }
        if closed {
            path.pop();
            if path.iter().any(|&v| degree(v) != 2) {
                closed = false; // shared a knot vertex - refuse to guess
            }
        }
        trails.push((path, closed));
    }

    let Some(Plane(n, _)) = plane else {
        // No trusted plane: every ring stays Open - the honest floor.
        return trails
            .into_iter()
            .map(|(p, _)| Loop {
                kind: LoopKind::Open,
                verts: p,
            })
            .collect();
    };

    // Project on the axis pair perpendicular to the dominant normal axis.
    let (ku, kv, handed) = project_basis(n);
    let mut loops: Vec<Loop> = Vec::new();
    let mut rings: Vec<(Vec<u32>, u128)> = Vec::new(); // (oriented path, |2*area|)
    for (mut path, closed) in trails {
        if !closed {
            loops.push(Loop {
                kind: LoopKind::Open,
                verts: path,
            });
            continue;
        }
        let Some(a2) = ring_area2(&path, lattice, ku, kv) else {
            loops.push(Loop {
                kind: LoopKind::Open,
                verts: path,
            });
            continue; // absurd magnitudes: not a trustworthy ring
        };
        if (a2 > 0) != (handed > 0.0) {
            path.reverse();
        }
        if a2 == 0 {
            loops.push(Loop {
                kind: LoopKind::Open,
                verts: path,
            });
            continue; // degenerate ring: cannot rank outer vs hole
        }
        rings.push((path, a2.unsigned_abs()));
    }
    if rings.is_empty() {
        return loops;
    }
    // Largest ring is the outer boundary; every other closed ring must
    // nest inside it or it is mislabeled (Open), never a silent wrong Hole.
    rings.sort_by_key(|r| std::cmp::Reverse(r.1));
    let (outer, _) = rings.swap_remove(0);
    let poly: Vec<(f64, f64)> = outer
        .iter()
        .map(|&v| {
            let p = lattice[v as usize];
            (pos_get(p, ku) as f64 / S, pos_get(p, kv) as f64 / S)
        })
        .collect();
    loops.push(Loop {
        kind: LoopKind::Outer,
        verts: outer,
    });
    for (path, _) in rings {
        let misplaced = path
            .first()
            .is_none_or(|&v| {
                let p = lattice[v as usize];
                !point_in_poly(
                    pos_get(p, ku) as f64 / S,
                    pos_get(p, kv) as f64 / S,
                    &poly,
                )
            });
        let kind = if misplaced {
            LoopKind::Open
        } else {
            LoopKind::Hole
        };
        loops.push(Loop { kind, verts: path });
    }
    loops
}

/// Right-handed (u, v, sign) axis choice: drop the dominant normal axis k
/// and take the other two cyclically; handedness is +n_k's sign.
fn project_basis(n: [i128; 3]) -> (usize, usize, f64) {
    let (ku, kv) = if n[0].unsigned_abs() >= n[1].unsigned_abs()
        && n[0].unsigned_abs() >= n[2].unsigned_abs()
    {
        (1, 2)
    } else if n[1].unsigned_abs() >= n[2].unsigned_abs() {
        (2, 0)
    } else {
        (0, 1)
    };
    let k = 3 - ku - kv; // the dropped axis
    (ku, kv, n[k] as f64)
}

/// Twice the signed lattice shoelace of a closed ring in (ku, kv).
fn ring_area2(path: &[u32], lattice: &[Pos], ku: usize, kv: usize) -> Option<i128> {
    let mut acc: i128 = 0;
    for i in 0..path.len() {
        let p = lattice[path[i] as usize];
        let q = lattice[path[(i + 1) % path.len()] as usize];
        let (pu, pv, qu, qv) = (
            pos_get(p, ku) as i128,
            pos_get(p, kv) as i128,
            pos_get(q, ku) as i128,
            pos_get(q, kv) as i128,
        );
        acc = acc
            .checked_add(pu.checked_mul(qv)?.checked_sub(qu.checked_mul(pv)?)?)?;
    }
    Some(acc)
}

/// Even-odd point-in-polygon on the projection plane.
fn point_in_poly(x: f64, y: f64, poly: &[(f64, f64)]) -> bool {
    let mut inside = false;
    for i in 0..poly.len() {
        let (x1, y1) = poly[i];
        let (x2, y2) = poly[(i + 1) % poly.len()];
        if (y1 > y) != (y2 > y) {
            let t = (y - y1) * (x2 - x1) / (y2 - y1) + x1;
            if x < t {
                inside = !inside;
            }
        }
    }
    inside
}

/// Position-keyed face identity: exact plane, the maximal straight
/// segments of every ring (collinear subdivision of the same outline, as
/// a re-import of the same NURBS face produces, cannot move them), the
/// loop structure, and the area bucketed to 64 nm² so float summation
/// noise cannot flicker the id either.
fn face_id(f: &Face, lattice: &[Pos]) -> FaceId {
    let mut h = DefaultHasher::new();
    (f.kind as u8).hash(&mut h);
    match f.plane {
        Some(Plane(n, d)) => {
            n.hash(&mut h);
            d.hash(&mut h);
        }
        None => 0i128.hash(&mut h),
    }
    let mut segs: Vec<(Pos, Pos)> = Vec::new();
    for l in &f.loops {
        let closed = matches!(l.kind, LoopKind::Outer | LoopKind::Hole);
        ring_segments(&l.verts, lattice, closed, &mut segs);
    }
    segs.sort_unstable();
    segs.dedup();
    segs.hash(&mut h);
    let mut ringkinds: Vec<u8> = f.loops.iter().map(|l| l.kind as u8).collect();
    ringkinds.sort_unstable();
    ringkinds.hash(&mut h);
    ((f.area * S * S) / 64.0).round().to_bits().hash(&mut h);
    h.finish()
}

/// Collapse a ring's collinear runs: push one segment per maximal straight
/// stretch, endpoints canonicalized (low position first).
fn ring_segments(verts: &[u32], lattice: &[Pos], closed: bool, out: &mut Vec<(Pos, Pos)>) {
    let n = verts.len();
    if n < 2 {
        return;
    }
    let pos = |i: usize| lattice[verts[if closed { i % n } else { i }] as usize];
    let mut keep: Vec<usize> = Vec::new();
    for i in 0..n {
        let corner = if !closed {
            i == 0 || i + 1 == n || turn_nonzero(pos(i - 1), pos(i), pos(i + 1))
        } else if i == 0 {
            // wrap-safe: prev is the last vertex
            turn_nonzero(pos(n - 1), pos(0), pos(1))
        } else {
            turn_nonzero(pos(i - 1), pos(i), pos(if i + 1 == n { 0 } else { i + 1 }))
        };
        if corner {
            keep.push(i);
        }
    }
    if keep.is_empty() {
        // An all-collinear ring (a two-vertex lens, or a broken walk):
        // its extent is the only honest segment - from the extreme corners.
        let mut all: Vec<Pos> = (0..n).map(pos).collect();
        all.sort_unstable();
        let (lo, hi) = (all[0], all[n - 1]); // n >= 2 holds here
        if lo != hi {
            out.push((lo, hi));
        }
        return;
    }
    let m = keep.len();
    for j in 0..m {
        let a = pos(keep[j]);
        let b = if closed {
            pos(keep[(j + 1) % m])
        } else if j + 1 < m {
            pos(keep[j + 1])
        } else {
            break;
        };
        out.push(if a <= b { (a, b) } else { (b, a) });
    }
}

/// Is `q` a direction change between `p -> q` and `q -> r`? Checked
/// integer cross product; absurd magnitudes count as a corner (visible
/// segments rather than silently merged ones).
fn turn_nonzero(p: Pos, q: Pos, r: Pos) -> bool {
    // i128 before subtracting: lattice coords reach 9e18, so an i64
    // difference of two of them can overflow.
    let d = |u: Pos, v: Pos| {
        [
            v.0 as i128 - u.0 as i128,
            v.1 as i128 - u.1 as i128,
            v.2 as i128 - u.2 as i128,
        ]
    };
    let (e1, e2) = (d(p, q), d(q, r));
    let term = |i: usize, j: usize| {
        match (e1[i].checked_mul(e2[j]), e1[j].checked_mul(e2[i])) {
            (Some(a), Some(b)) => a.checked_sub(b).map(|v| v != 0),
            _ => None,
        }
    };
    // Zero means collinear; an overflow we cannot prove away is a corner.
    [term(1, 2), term(2, 0), term(0, 1)]
        .into_iter()
        .any(|x| !matches!(x, Some(false)))
}
