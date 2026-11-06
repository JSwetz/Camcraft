//! Feature vertices (M13.4): the corners where feature edges meet, keyed
//! by their welded lattice position so a pick survives facet reorder and
//! re-import the same way face and chain ids do.
//!
//! Provenance: our own code - position-hash identity from the stable-id
//! design of this module; no vendor material consulted.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{Features, weld_pos};

/// Stable identity of a feature vertex: a hash over its welded lattice
/// position. Transform-free (the features live in the entity's local
/// frame), order-free, and blind to how many facets meet there.
pub type VertexId = u64;

/// The vertex id of `features.verts[vi]`; None when the index is out of
/// range or the position refuses to weld (non-finite or absurd).
#[must_use]
pub fn vertex_id(features: &Features, vi: u32) -> Option<VertexId> {
    let pos = weld_pos(features.verts.get(vi as usize)?)?;
    let mut h = DefaultHasher::new();
    b'V'.hash(&mut h); // domain byte: never collide with a face/chain hash
    pos.hash(&mut h);
    Some(h.finish())
}

/// The pickable vertices: every endpoint of a feature edge, ascending and
/// unique. Interior tessellation points of a smooth wall touch no feature
/// edge and so are not corners - a pick lands on what the wire shows.
#[must_use]
pub fn corner_verts(features: &Features) -> Vec<u32> {
    let mut out: Vec<u32> = features.edges.iter().flat_map(|e| [e.a, e.b]).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The corner index whose id is `id`, if the vertex is still in the model.
#[must_use]
pub fn resolve_vertex(features: &Features, id: VertexId) -> Option<u32> {
    corner_verts(features).into_iter().find(|&vi| vertex_id(features, vi) == Some(id))
}

/// Every corner's id, ascending - the revision check's binary-search set.
#[must_use]
pub fn vertex_ids(features: &Features) -> Vec<VertexId> {
    let mut ids: Vec<VertexId> =
        corner_verts(features).into_iter().filter_map(|vi| vertex_id(features, vi)).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}
