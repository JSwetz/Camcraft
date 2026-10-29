//! Model revisions: an order-free signature over a mesh's stable feature
//! ids, and the diff between two revisions of a model.
//!
//! Provenance: our own design. The mechanism is content addressing - a
//! standard distributed-systems technique - applied to the geometry-hash
//! face/patch/chain ids of [`crate::geometry::feature`] (our M1.5 design);
//! the user-visible behavior it serves (model update detection marking
//! dependent operations stale) is specced from published CAM documentation.
//! No HSMWorks internals involved.
//!
//! The law this module exists for: identity is the id, not a proximity
//! guess. A re-import that keeps the geometry keeps every id - facet
//! reorder, diagonal flips, even a different triangle soup for the same
//! faces - so a model can be updated a thousand times and operations on
//! unchanged features stay fresh. A real design change kills the ids it
//! touched; `changes` reports exactly which faces, welded faces and edge
//! chains were carried, added and missing. Successor pairing (a missing
//! face and an added face that are "the same face moved") is deliberately
//! NOT guessed here - re-picking is Stage C's decision, made visible to
//! the user, never assumed.

use std::hash::Hasher;

use super::feature::{self, Features, Patch, patches};

/// One stable-id domain's diff between two revisions. All three lists are
/// sorted and deduplicated; `carried` ids keep pointing at the same
/// geometry, so operations that reference them are unaffected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdDiff {
    pub carried: Vec<u64>,
    pub added: Vec<u64>,
    pub missing: Vec<u64>,
}

impl IdDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.missing.is_empty()
    }

    /// Diff two sorted, deduplicated id sets.
    #[must_use]
    pub fn between(old: &[u64], new: &[u64]) -> IdDiff {
        let mut d = IdDiff::default();
        let (mut i, mut j) = (0usize, 0usize);
        while i < old.len() && j < new.len() {
            match old[i].cmp(&new[j]) {
                std::cmp::Ordering::Less => {
                    d.missing.push(old[i]);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    d.added.push(new[j]);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    d.carried.push(old[i]);
                    i += 1;
                    j += 1;
                }
            }
        }
        d.missing.extend_from_slice(&old[i..]);
        d.added.extend_from_slice(&new[j..]);
        d
    }
}

/// Every feature-id change between two revisions of one model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureChanges {
    pub faces: IdDiff,
    pub patches: IdDiff,
    pub chains: IdDiff,
}

impl FeatureChanges {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty() && self.patches.is_empty() && self.chains.is_empty()
    }

    /// A short, honest phrase for the status bar and the tree: what moved
    /// between the two revisions, by count. Never a euphemism for "changed
    /// in a way we did not compute" - the counts come from the sets.
    #[must_use]
    pub fn describe(&self) -> String {
        if self.is_empty() {
            return "geometry unchanged".to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        let n = self.faces.added.len() + self.faces.missing.len();
        if n > 0 {
            parts.push(format!("{n} face{}", if n == 1 { "" } else { "s" }));
        }
        let n = self.patches.added.len() + self.patches.missing.len();
        if n > 0 {
            parts.push(format!("{n} welded face{}", if n == 1 { "" } else { "s" }));
        }
        let n = self.chains.added.len() + self.chains.missing.len();
        if n > 0 {
            parts.push(format!("{n} edge chain{}", if n == 1 { "" } else { "s" }));
        }
        format!("{} changed", parts.join(", "))
    }
}

/// The face ids of a mesh's features, sorted and deduplicated - the free
/// part of the signature: [`Features`] carries them already computed.
#[must_use]
pub fn face_ids(features: &Features) -> Vec<u64> {
    let mut ids: Vec<u64> = features.faces.iter().map(|f| f.id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The patch ids, sorted; one walk of the smooth bands.
#[must_use]
pub fn patch_ids(features: &Features) -> Vec<u64> {
    let mut ids: Vec<u64> = patches(features).iter().map(|p: &Patch| p.id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The chain ids, sorted; one walk of the feature edges.
#[must_use]
pub fn chain_ids(features: &Features) -> Vec<u64> {
    let mut ids: Vec<u64> = feature::chains(features).iter().map(|c| c.id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Every feature-id change between two revisions, in all three domains.
/// For the one-shot re-import summary, not the per-frame stamp path -
/// [`signature`] is the cheap order-free digest of the same information.
#[must_use]
pub fn changes(old: &Features, new: &Features) -> FeatureChanges {
    FeatureChanges {
        faces: IdDiff::between(&face_ids(old), &face_ids(new)),
        patches: IdDiff::between(&patch_ids(old), &patch_ids(new)),
        chains: IdDiff::between(&chain_ids(old), &chain_ids(new)),
    }
}

/// The order-free geometry signature used by the operation stamp's
/// whole-entity source arm: the multiset of face ids plus the facet count.
///
/// Face ids are content hashes in the LOCAL frame (welded corners, exact
/// plane, quantized area), so this is invariant under facet order, diagonal
/// flips and any re-tessellation that keeps the same faces - and changes
/// when the material moves. Weld and chain topology is not folded in:
/// changing a weld or a chain without changing a single face needs a
/// facet-count coincidence too, and the Selection arm digests the ids it
/// actually picked anyway (the law behind keeping this cheap: it runs per
/// frame in status checks - since M5.3 A the caller memoizes it per
/// entity against the geometry revision, so an unmoved mesh pays this
/// walk once, not once per operation per frame).
#[must_use]
pub fn signature(features: &Features) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    // v2: the M4.3 rename of the old brand's salt. Every stored digest
    // moves once - documents saved under the old salt read their
    // operations Stale exactly one time and settle for good (the
    // stamp.rs salt-rename precedent: working as designed).
    h.write(b"camcraft-signature-v2");
    h.write_usize(features.faces.len() + features.facet_face.len());
    for id in face_ids(features) {
        h.write_u64(id);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::feature::features_of_facets;
    use crate::geometry::{Mesh, Point3, Triangle, cube};

    fn tris(m: &Mesh) -> Vec<Triangle> {
        m.facets.clone()
    }

    fn p(q: [f64; 3]) -> Point3 {
        Point3::new(q[0], q[1], q[2])
    }

    /// The 0..10 cube's silent twin: twelve facets, the same AABB, the same
    /// height - a diamond-based prism. Every wall is a different plane.
    fn diamond_prism() -> Vec<Triangle> {
        let (a, b, c, d) = (p([5., 0., 0.]), p([10., 5., 0.]), p([5., 10., 0.]), p([0., 5., 0.]));
        let (a1, b1, c1, d1) = (p([5., 0., 10.]), p([10., 5., 10.]), p([5., 10., 10.]), p([0., 5., 10.]));
        vec![
            Triangle::new(a, b, c), Triangle::new(a, c, d), // bottom
            Triangle::new(a1, c1, b1), Triangle::new(a1, d1, c1), // top
            Triangle::new(a, b, b1), Triangle::new(a, b1, a1), // four walls
            Triangle::new(b, c, c1), Triangle::new(b, c1, b1),
            Triangle::new(c, d, d1), Triangle::new(c, d1, c1),
            Triangle::new(d, a, a1), Triangle::new(d, a1, d1),
        ]
    }

    fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3], flip: bool) -> Vec<Triangle> {
        let (a, b, c, d) = (p(a), p(b), p(c), p(d));
        if flip {
            vec![Triangle::new(b, c, d), Triangle::new(b, d, a)]
        } else {
            vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
        }
    }

    #[test]
    fn signature_ignores_facet_order_and_diagonals() {
        let flat_a = quad([0., 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.], false);
        let mut flat_b = quad([0., 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.], true);
        flat_b.reverse(); // facet reorder on top of the diagonal flip
        let fa = features_of_facets(&flat_a);
        let fb = features_of_facets(&flat_b);
        assert_eq!(signature(&fa), signature(&fb));
        assert!(changes(&fa, &fb).is_empty());
    }

    #[test]
    fn signature_moves_with_the_material() {
        // Cube vs the diamond prism: twelve facets, identical AABB,
        // different walls - the exact silent-gap case count+bounds missed.
        let c = tris(&cube(Point3::ZERO, Point3::new(10., 10., 10.)));
        let d = diamond_prism();
        assert_eq!(c.len(), d.len());
        let fc = features_of_facets(&c);
        let fd = features_of_facets(&d);
        assert_ne!(signature(&fc), signature(&fd));
        let ch = changes(&fc, &fd);
        assert!(!ch.faces.missing.is_empty() && !ch.faces.added.is_empty());
        assert!(!ch.chains.is_empty(), "the rim chains changed too");
    }

    #[test]
    fn total_on_garbage() {
        let g = [
            Triangle::new(
                Point3::new(f64::NAN, 0., 0.),
                Point3::new(1., 0., 0.),
                Point3::new(0., 1., 0.),
            ),
            Triangle::new(
                Point3::ZERO,
                Point3::new(1., 0., 0.),
                Point3::new(1., 0., 0.),
            ),
        ];
        let f = features_of_facets(&g);
        let _ = signature(&f);
        let _ = changes(&f, &f);
    }

    #[test]
    fn empty_mesh_has_a_signature() {
        let f = features_of_facets(&[]);
        let g = features_of_facets(&[]);
        assert_eq!(signature(&f), signature(&g));
        assert!(changes(&f, &g).is_empty());
    }
}
