//! The pick feature graph (#692): one mesh's welded faces, feature edges,
//! feature points and round-edge centres - the only things a click may
//! land on, for every picker in the app.
//!
//! Provenance: our own code over `geometry::feature`'s weld (this repo's
//! M1.5 dihedral-threshold clustering); edge-adjacency walks and turn-angle
//! corner detection from general mesh-processing practice; no vendor
//! material consulted.
//!
//! The weld is `geometry::feature`'s, not a second one: a welded face is
//! one `facet_patch` root (facets joined across seams under its 40-degree
//! dihedral rule), so the faces `list_features` (#180) reports and the
//! faces a click picks are the same faces. What this module adds is the
//! filter that topology does not: `Features::edges` also carries the
//! "tangent seams between distinct faces" its exact-plane split makes -
//! every facet edge of a tessellated cylinder - and those must never be
//! pickable. Here an edge is a feature edge only when it separates two
//! welded faces (or bends past the crease angle inside one), or bounds an
//! open sheet, or is a non-manifold fold.

use std::collections::HashMap;
use std::sync::Arc;

use crate::geometry::Triangle;
use crate::geometry::feature::{self, EdgeKind, Features, VertexId};
use crate::model;
use crate::operation::FeatureSel;

use super::feature_arc::{ArcCentre, fit_arc};

/// Dihedral angle past which an edge inside one welded face still counts
/// as a crease (degrees). Matches `geometry::feature`'s weld rule, so the
/// two can only disagree on an edge whose patch closed around it.
pub const CREASE_DEG: f64 = 40.0;

/// A degree-2 feature vertex is a feature point when its two feature
/// edges turn by more than this (degrees). Equals the tangent rule of
/// `feature::chains` (`CHAIN_TANGENT_DEG`): what splits a chain is a corner.
pub const TURN_DEG: f64 = 30.0;

/// Why a vertex is a feature point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    /// Three or more feature edges meet (a box corner).
    Corner,
    /// Two feature edges meet at a turn sharper than [`TURN_DEG`].
    Turn,
    /// The end of an open feature-edge chain (one edge).
    End,
}

/// One feature point, in the mesh's local frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeaturePoint {
    /// Index into `Features::verts`.
    pub vert: u32,
    /// Stable id - the `FeatureSel::Vertex` key.
    pub id: VertexId,
    pub kind: PointKind,
    pub at: [f64; 3],
}

/// One welded face: the facets a click anywhere on it selects.
#[derive(Clone, Debug)]
pub struct WeldedFace {
    /// Facet indices into the mesh's facet array.
    pub facets: Vec<u32>,
    /// The selection the rest of the app keys this face by: a `Patch` for
    /// a multi-strip smooth wall, else the planar `Face`. `None` only for
    /// a face the topology could not name (a degenerate facet).
    pub sel: Option<FeatureSel>,
}

/// The pickable structure of one mesh, local frame.
#[derive(Clone, Debug, Default)]
pub struct FeatureGraph {
    /// The shared topology this graph filters.
    pub features: Arc<Features>,
    /// facet index -> index into [`FeatureGraph::faces`].
    pub face_of_facet: Vec<u32>,
    pub faces: Vec<WeldedFace>,
    /// Indices into `features.edges` that are feature edges, ascending.
    pub edges: Vec<u32>,
    pub points: Vec<FeaturePoint>,
    /// Centres of round feature-edge runs (circles and arcs).
    pub arcs: Vec<ArcCentre>,
}

impl FeatureGraph {
    /// The welded face a facet belongs to.
    #[must_use]
    pub fn face_of(&self, facet: usize) -> Option<&WeldedFace> {
        self.faces.get(*self.face_of_facet.get(facet)? as usize)
    }

    /// The local endpoints of feature edge `edge` (an index into
    /// `features.edges`).
    #[must_use]
    pub fn edge_ends(&self, edge: u32) -> Option<([f64; 3], [f64; 3])> {
        let e = self.features.edges.get(edge as usize)?;
        Some((*self.features.verts.get(e.a as usize)?, *self.features.verts.get(e.b as usize)?))
    }
}

/// Build the graph of a mesh from its facets (local frame). Total on any
/// input: empty or garbage facets give an empty or partial graph.
#[must_use]
pub fn feature_graph(tris: &[Triangle]) -> FeatureGraph {
    graph_of(model::features_of_local(tris), tris)
}

thread_local! {
    /// Cap-8 MRU of graphs keyed by the facets' content digest, the same
    /// key and policy as `model::features_of_local` (pure fuel: a miss
    /// only recomputes).
    static GRAPH_CACHE: std::cell::RefCell<Vec<(u64, Arc<FeatureGraph>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// [`feature_graph`], memoized by content - what a per-frame hover calls.
#[must_use]
pub fn feature_graph_cached(tris: &[Triangle]) -> Arc<FeatureGraph> {
    if tris.is_empty() {
        return Arc::new(FeatureGraph::default());
    }
    let key = feature::digest_of(tris);
    GRAPH_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(k, _)| *k == key) {
            let entry = cache.remove(i);
            cache.push(entry.clone());
            return entry.1;
        }
        let g = Arc::new(feature_graph(tris));
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((key, g.clone()));
        g
    })
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Cosine of the angle between two vectors; `None` when either is null
/// or the math is not finite.
fn cos_between(a: [f64; 3], b: [f64; 3]) -> Option<f64> {
    let m = (dot(a, a) * dot(b, b)).sqrt();
    let c = dot(a, b) / m;
    (m > 0.0 && c.is_finite()).then_some(c)
}

fn facet_normal(t: &Triangle) -> [f64; 3] {
    let (a, b, c) = ([t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]);
    cross(sub(b, a), sub(c, a))
}

fn welded_faces(f: &Features) -> (Vec<u32>, Vec<WeldedFace>) {
    // Face id -> patch id, for the selection key.
    let mut patch_of: HashMap<u64, u64> = HashMap::new();
    for p in feature::patches(f) {
        for fid in p.faces {
            patch_of.insert(fid, p.id);
        }
    }
    let mut slot: HashMap<u32, u32> = HashMap::new();
    let mut faces: Vec<WeldedFace> = Vec::new();
    let mut face_of_facet = Vec::with_capacity(f.facet_patch.len());
    for (t, &root) in f.facet_patch.iter().enumerate() {
        let k = *slot.entry(root).or_insert_with(|| {
            let sel = f
                .facet_face
                .get(t)
                .and_then(|&fi| f.faces.get(fi as usize))
                .map(|fa| patch_of.get(&fa.id).map_or(FeatureSel::Face(fa.id), |p| FeatureSel::Patch(*p)));
            faces.push(WeldedFace { facets: Vec::new(), sel });
            (faces.len() - 1) as u32
        });
        if let Some(face) = faces.get_mut(k as usize) {
            face.facets.push(t as u32);
        }
        face_of_facet.push(k);
    }
    (face_of_facet, faces)
}

/// A facet with real area: finite corners and a non-null normal. Slivers
/// (zero-area STL facets, T-junction fillers) bound nothing and so make
/// no feature edge of their own.
fn has_area(tris: &[Triangle], t: u32) -> bool {
    tris.get(t as usize).is_some_and(|t| {
        let n = facet_normal(t);
        let nn = dot(n, n);
        nn.is_finite() && nn > 1e-18
    })
}

/// Is `features.edges[i]` a real model edge (see the module doc)?
fn is_feature_edge(f: &Features, tris: &[Triangle], i: usize) -> bool {
    let Some(e) = f.edges.get(i) else { return false };
    let incident = &e.facets[..(e.n_facets.min(3) as usize)];
    let real = incident.iter().filter(|&&t| has_area(tris, t)).count();
    match e.kind {
        EdgeKind::Boundary => real == 1,
        EdgeKind::NonManifold => real >= 2,
        EdgeKind::Degenerate => false, // a flaw, drawn but never a pick
        EdgeKind::Crease => {
            let [t0, t1, _] = e.facets;
            if real < 2 {
                return false; // a sliver's seam
            }
            let root = |t: u32| f.facet_patch.get(t as usize).copied();
            if root(t0) != root(t1) {
                return true; // two welded faces meet here
            }
            // One welded face: a crease only if it really bends.
            let (Some(a), Some(b)) = (tris.get(t0 as usize), tris.get(t1 as usize)) else {
                return false;
            };
            cos_between(facet_normal(a), facet_normal(b))
                .is_some_and(|c| c < CREASE_DEG.to_radians().cos())
        }
    }
}

fn graph_of(features: Arc<Features>, tris: &[Triangle]) -> FeatureGraph {
    let f = &*features;
    let (face_of_facet, faces) = welded_faces(f);
    let edges: Vec<u32> = (0..f.edges.len()).filter(|&i| is_feature_edge(f, tris, i)).map(|i| i as u32).collect();

    // Vertex -> incident feature edges.
    let mut incident: HashMap<u32, Vec<u32>> = HashMap::new();
    for &ei in &edges {
        if let Some(e) = f.edges.get(ei as usize) {
            incident.entry(e.a).or_default().push(ei);
            incident.entry(e.b).or_default().push(ei);
        }
    }
    let other_end = |ei: u32, v: u32| -> Option<u32> {
        let e = f.edges.get(ei as usize)?;
        Some(if e.a == v { e.b } else { e.a })
    };
    let vert = |v: u32| f.verts.get(v as usize).copied();

    let mut points: Vec<FeaturePoint> = Vec::new();
    let mut verts: Vec<&u32> = incident.keys().collect();
    verts.sort_unstable();
    for &v in verts {
        let Some(es) = incident.get(&v) else { continue };
        let kind = match es.len() {
            1 => Some(PointKind::End),
            2 => {
                let turn = (|| {
                    let (p, a, b) = (vert(v)?, vert(other_end(es[0], v)?)?, vert(other_end(es[1], v)?)?);
                    // Directions along the run: in from `a`, out to `b`.
                    cos_between(sub(p, a), sub(b, p))
                })();
                // A turn that refuses to measure is shown, not hidden.
                turn.is_none_or(|c| c < TURN_DEG.to_radians().cos()).then_some(PointKind::Turn)
            }
            _ => Some(PointKind::Corner),
        };
        if let (Some(kind), Some(at), Some(id)) = (kind, vert(v), feature::vertex_id(f, v)) {
            points.push(FeaturePoint { vert: v, id, kind, at });
        }
    }

    // Runs between feature points (or closed loops with none) -> arcs.
    let is_point = |v: u32| points.binary_search_by_key(&v, |p| p.vert).is_ok();
    let mut used = vec![false; f.edges.len()];
    let mut arcs = Vec::new();
    let walk = |start_v: u32, start_e: u32, used: &mut Vec<bool>| -> (Vec<u32>, Vec<[f64; 3]>, bool) {
        let mut run = Vec::new();
        let mut pts: Vec<[f64; 3]> = vert(start_v).into_iter().collect();
        let (mut v, mut e) = (start_v, start_e);
        while let Some(flag) = used.get_mut(e as usize) {
            if *flag {
                break;
            }
            *flag = true;
            run.push(e);
            let Some(n) = other_end(e, v) else { break };
            pts.extend(vert(n));
            if n == start_v {
                return (run, pts, true);
            }
            if is_point(n) {
                break;
            }
            let next = incident.get(&n).and_then(|es| es.iter().copied().find(|&x| x != e));
            match next {
                Some(x) => (v, e) = (n, x),
                None => break,
            }
        }
        (run, pts, false)
    };
    let mut starts: Vec<(u32, u32)> = Vec::new();
    for p in &points {
        for &ei in incident.get(&p.vert).map(Vec::as_slice).unwrap_or(&[]) {
            starts.push((p.vert, ei));
        }
    }
    for &ei in &edges {
        if let Some(e) = f.edges.get(ei as usize) {
            starts.push((e.a, ei)); // whatever is left is a closed loop
        }
    }
    for (v, ei) in starts {
        if used.get(ei as usize).copied().unwrap_or(true) {
            continue;
        }
        let (run, pts, closed) = walk(v, ei, &mut used);
        if let Some(arc) = fit_arc(&pts, closed, run) {
            arcs.push(arc);
        }
    }

    FeatureGraph { features, face_of_facet, faces, edges, points, arcs }
}
