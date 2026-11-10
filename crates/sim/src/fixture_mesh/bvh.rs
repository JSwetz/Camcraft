//! A bounding-volume hierarchy over a fixture's triangles (#339): the
//! broad phase that keeps the mesh law near O(log n) per tool band on
//! 100k-triangle vises.
//!
//! Provenance: the axis-aligned BVH with object-median splits along the
//! widest centroid axis is textbook (Ericson, "Real-Time Collision
//! Detection", 2005, ch. 6; Pharr, Jakob, Humphreys, "Physically Based
//! Rendering", ch. 4.3); the ray-box slab test is Kay and Kajiya,
//! SIGGRAPH 1986. Our own code.
//!
//! Conservative by construction: a node's box is the exact union of its
//! triangles' corner boxes, so pruning a node whose box a query provably
//! misses can never drop a triangle the query meets.

use super::gjk::V;

/// Triangles per leaf.
const LEAF: usize = 4;

#[derive(Clone, Copy, Debug)]
pub(super) struct Node {
    pub lo: V,
    pub hi: V,
    /// Leaf: first index into `order`; inner: the left child (the right
    /// child is `first + 1`).
    pub first: u32,
    /// Leaf: triangle count (> 0); inner: 0.
    pub count: u32,
}

/// The hierarchy: nodes (root first) and the triangle order the leaves
/// index into.
#[derive(Clone, Debug, Default)]
pub(super) struct Bvh {
    pub nodes: Vec<Node>,
    pub order: Vec<u32>,
}

pub(super) fn tri_box(t: &[V; 3]) -> (V, V) {
    let mut lo = t[0];
    let mut hi = t[0];
    for p in &t[1..] {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

impl Bvh {
    /// Build over `tris` (finite coordinates - the caller's check). The
    /// build is iterative: no recursion depth to blow, whatever the
    /// count.
    pub(super) fn build(tris: &[[V; 3]]) -> Self {
        let boxes: Vec<(V, V)> = tris.iter().map(tri_box).collect();
        let cent: Vec<V> = boxes.iter().map(|(l, h)| [0, 1, 2].map(|k| 0.5 * (l[k] + h[k]))).collect();
        let mut order: Vec<u32> = (0..tris.len()).filter_map(|i| u32::try_from(i).ok()).collect();
        let mut nodes: Vec<Node> = Vec::with_capacity(2 * tris.len() / LEAF + 1);
        if order.is_empty() {
            return Self { nodes, order };
        }
        nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: 0 });
        // (node index, range start, range end) still to split.
        let mut work: Vec<(usize, usize, usize)> = vec![(0, 0, order.len())];
        while let Some((ni, s, e)) = work.pop() {
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            let (mut clo, mut chi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for &t in &order[s..e] {
                let (l, h) = boxes[t as usize];
                let c = cent[t as usize];
                for k in 0..3 {
                    lo[k] = lo[k].min(l[k]);
                    hi[k] = hi[k].max(h[k]);
                    clo[k] = clo[k].min(c[k]);
                    chi[k] = chi[k].max(c[k]);
                }
            }
            let axis = (0..3)
                .max_by(|&a, &b| (chi[a] - clo[a]).total_cmp(&(chi[b] - clo[b])))
                .unwrap_or(0);
            let leaf = e - s <= LEAF || chi[axis] - clo[axis] <= 0.0;
            let left = nodes.len();
            let Some(node) = nodes.get_mut(ni) else { continue };
            node.lo = lo;
            node.hi = hi;
            if leaf {
                node.first = s as u32;
                node.count = (e - s) as u32;
                continue;
            }
            node.first = left as u32;
            node.count = 0;
            let mid = s + (e - s) / 2;
            order[s..e].select_nth_unstable_by(mid - s, |&a, &b| {
                cent[a as usize][axis].total_cmp(&cent[b as usize][axis])
            });
            let blank = Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: 0 };
            nodes.push(blank);
            nodes.push(blank);
            work.push((left, s, mid));
            work.push((left + 1, mid, e));
        }
        Self { nodes, order }
    }

    /// Visit every triangle in a leaf whose box `keep` admits, depth
    /// first; `visit` returns true to stop the walk. Returns whether
    /// the walk was stopped.
    pub(super) fn walk(&self, keep: impl Fn(&Node) -> bool, mut visit: impl FnMut(usize) -> bool) -> bool {
        let mut stack: Vec<usize> = Vec::with_capacity(64);
        if !self.nodes.is_empty() {
            stack.push(0);
        }
        while let Some(ni) = stack.pop() {
            let Some(n) = self.nodes.get(ni) else { continue };
            if !keep(n) {
                continue;
            }
            if n.count > 0 {
                let (s, e) = (n.first as usize, n.first as usize + n.count as usize);
                for &t in self.order.get(s..e).unwrap_or(&[]) {
                    if visit(t as usize) {
                        return true;
                    }
                }
            } else {
                stack.push(n.first as usize + 1);
                stack.push(n.first as usize);
            }
        }
        false
    }
}

/// Do two closed boxes overlap, each grown by `pad`?
pub(super) fn boxes_meet(alo: V, ahi: V, blo: V, bhi: V, pad: f64) -> bool {
    (0..3).all(|k| alo[k] - pad <= bhi[k] && blo[k] - pad <= ahi[k])
}

/// Does the ray `o + t d` (t >= 0) meet the box grown by `pad`? The
/// slab test; a zero direction component reads as "inside the slab or
/// not", and anything non-finite reads as a meet (the caller then
/// tests the triangles, which decide).
pub(super) fn ray_meets(o: V, d: V, lo: V, hi: V, pad: f64) -> bool {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for k in 0..3 {
        let (l, h) = (lo[k] - pad, hi[k] + pad);
        if d[k] == 0.0 {
            if o[k] < l || o[k] > h {
                return false;
            }
            continue;
        }
        let (mut a, mut b) = ((l - o[k]) / d[k], (h - o[k]) / d[k]);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if a.is_nan() || b.is_nan() {
            return true;
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return false;
        }
    }
    true
}
