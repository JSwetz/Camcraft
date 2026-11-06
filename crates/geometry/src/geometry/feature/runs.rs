//! Edge runs (M11.5 boundary UX): the unit a boundary pick takes when the
//! user wants ONE edge and not the whole tangent loop. A tangent chain
//! ([`chains`]) of a filleted rim is one closed ring - every line and arc
//! of the rim at once - so a plain Edge pick could never stay open. A run
//! is one geometric edge of a chain: its straight stretches (maximal
//! collinear walks) grouped while neighbouring stretches stay within
//! [`RUN_LENGTH_RATIO`] of each other's length - so a long line and the
//! short chords of the fillet beside it part, while the equal chords of
//! one arc (or a whole tessellated circle) stay together. A chain that
//! never splits IS its run, with the chain's own id, so every pick made
//! before runs existed resolves exactly as it did.
//!
//! Provenance: our own design notes; "an edge is a maximal run of one
//! curve" is the B-rep textbook definition, approximated on a mesh by the
//! chord-length test above. No vendor material consulted.

use super::*;

/// Neighbouring straight stretches whose lengths differ by more than this
/// factor belong to different edges (a line beside a fillet's chords).
pub const RUN_LENGTH_RATIO: f64 = 2.0;

const SENTINEL: Pos = (i64::MIN, i64::MIN, i64::MIN);

fn lattice_of(features: &Features) -> Vec<Pos> {
    features.verts.iter().map(|p| weld_pos(p).unwrap_or(SENTINEL)).collect()
}

/// Walk-ordered vertices of `edges` (one more than edges; a closed walk
/// repeats its start), or `None` when an index dangles or two consecutive
/// edges do not share a vertex.
fn walk(features: &Features, edges: &[u32]) -> Option<Vec<u32>> {
    let first = *edges.first()?;
    let e0 = features.edges.get(first as usize)?;
    // Start at the end of the first edge the second does not touch; a
    // lone edge starts at `b`, the Euler start `chains` gives it.
    let v0 = match edges.get(1) {
        Some(&i) => {
            let e1 = features.edges.get(i as usize)?;
            if e0.a == e1.a || e0.a == e1.b { e0.b } else { e0.a }
        }
        None => e0.b,
    };
    let mut vs = Vec::with_capacity(edges.len() + 1);
    vs.push(v0);
    let mut cur = v0;
    for &ei in edges {
        let e = features.edges.get(ei as usize)?;
        cur = if e.a == cur {
            e.b
        } else if e.b == cur {
            e.a
        } else {
            return None;
        };
        vs.push(cur);
    }
    let n = features.verts.len();
    vs.iter().all(|&v| (v as usize) < n).then_some(vs)
}

fn run_with(features: &Features, lattice: &[Pos], edges: &[u32]) -> Option<Chain> {
    let vs = walk(features, edges)?;
    let closed = vs.first() == vs.last() && edges.len() >= 3;
    let ring = if closed { &vs[..vs.len() - 1] } else { &vs[..] };
    let mut segs = Vec::new();
    ring_segments(ring, lattice, closed, &mut segs);
    Some(Chain { edges: edges.to_vec(), closed, id: chain_id(segs, closed) })
}

/// The chain the given feature edges spell, walked in the order given:
/// closed flag and id exactly as [`chains`] computes them for the same
/// edge set (the id is direction- and rotation-free). `None` when the
/// list is empty, an index dangles, or the edges do not connect end to
/// end in that order.
#[must_use]
pub fn run_of_edges(features: &Features, edges: &[u32]) -> Option<Chain> {
    run_with(features, &lattice_of(features), edges)
}

fn dist(features: &Features, a: u32, b: u32) -> f64 {
    match (features.verts.get(a as usize), features.verts.get(b as usize)) {
        (Some(p), Some(q)) => ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt(),
        _ => 0.0,
    }
}

fn parts(a: f64, b: f64) -> bool {
    let (lo, hi) = (a.min(b), a.max(b));
    !(lo > 0.0 && hi.is_finite()) || hi > lo * RUN_LENGTH_RATIO
}

/// Split one tangent chain into its runs (see the module law).
fn split_into(features: &Features, lattice: &[Pos], c: &Chain, out: &mut Vec<Chain>) {
    let n = c.edges.len();
    let whole = |out: &mut Vec<Chain>| out.push(c.clone());
    let Some(vs) = walk(features, &c.edges) else {
        return whole(out);
    };
    if n < 2 {
        return whole(out);
    }
    let pos = |k: usize| lattice[vs[k] as usize];
    // Corners: vertex k where the walk changes direction.
    let corner = |k: usize| -> bool {
        let (prev, next) = if c.closed {
            ((k + n - 1) % n, (k + 1) % n)
        } else {
            (k - 1, k + 1)
        };
        turn_nonzero(pos(prev), pos(k), pos(next))
    };
    let mut ks: Vec<usize> = if c.closed {
        (0..n).filter(|&k| corner(k)).collect()
    } else {
        let mut v = vec![0];
        v.extend((1..n).filter(|&k| corner(k)));
        v.push(n);
        v
    };
    ks.dedup();
    // Stretch j runs from corner ks[j] to the next; its length is the
    // chord between them.
    let m = ks.len();
    let stretches = if c.closed { m } else { m.saturating_sub(1) };
    if stretches < 2 {
        return whole(out);
    }
    let len: Vec<f64> = (0..stretches)
        .map(|j| dist(features, vs[ks[j]], vs[ks[(j + 1) % m]]))
        .collect();
    // Split vertices: a corner whose two stretches part by length.
    let splits: Vec<usize> = if c.closed {
        (0..m).filter(|&j| parts(len[(j + m - 1) % m], len[j])).map(|j| ks[j]).collect()
    } else {
        (1..m - 1).filter(|&j| parts(len[j - 1], len[j])).map(|j| ks[j]).collect()
    };
    if splits.is_empty() {
        return whole(out);
    }
    // Edge k runs from vertex k to k+1; cut before every split vertex,
    // starting a closed ring at its first split.
    let start = if c.closed { splits[0] } else { 0 };
    let mut run: Vec<u32> = Vec::new();
    for step in 0..n {
        let k = (start + step) % n;
        if step > 0 && splits.contains(&k) && !run.is_empty() {
            if let Some(r) = run_with(features, lattice, &run) {
                out.push(r);
            }
            run.clear();
        }
        run.push(c.edges[k]);
    }
    if let Some(r) = run_with(features, lattice, &run) {
        out.push(r);
    }
}

/// Every run of the model: each tangent chain split at its geometric
/// edge ends. Total on any input, like [`chains`].
#[must_use]
pub fn edge_runs(features: &Features) -> Vec<Chain> {
    let lattice = lattice_of(features);
    let mut out = Vec::new();
    for c in chains(features) {
        split_into(features, &lattice, &c, &mut out);
    }
    out
}

/// The run holding feature edge `edge` - what a plain boundary Edge pick
/// takes.
#[must_use]
pub fn run_containing(features: &Features, edge: u32) -> Option<Chain> {
    edge_runs(features).into_iter().find(|r| r.edges.contains(&edge))
}

/// Resolve a picked chain id: a tangent chain first (every pick made
/// before runs existed, byte for byte), then a run, then one feature
/// edge on its own (the Alt pick). `None` for an id the model no longer
/// holds.
#[must_use]
pub fn resolve_chain(features: &Features, id: ChainId) -> Option<Chain> {
    let cs = chains(features);
    if let Some(c) = cs.iter().find(|c| c.id == id) {
        return Some(c.clone());
    }
    let lattice = lattice_of(features);
    let mut runs = Vec::new();
    for c in &cs {
        runs.clear();
        split_into(features, &lattice, c, &mut runs);
        if let Some(r) = runs.iter().find(|r| r.id == id) {
            return Some(r.clone());
        }
    }
    (0..features.edges.len() as u32)
        .filter_map(|i| run_with(features, &lattice, &[i]))
        .find(|r| r.id == id)
}

#[cfg(test)]
mod law {
    use super::*;

    /// A planar polyline as features: every edge bounds face 0 (so
    /// `chains` may join them), one facet each.
    fn polyline(pts: &[[f64; 3]], closed: bool) -> Features {
        let n = pts.len() as u32;
        let count = if closed { n } else { n - 1 };
        let edges = (0..count)
            .map(|i| {
                let (a, b) = (i, (i + 1) % n);
                TopEdge { a: a.min(b), b: a.max(b), kind: EdgeKind::Boundary, facets: [0, 0, 0], n_facets: 1 }
            })
            .collect();
        Features {
            verts: pts.to_vec(),
            faces: Vec::new(),
            facet_face: vec![0],
            facet_patch: vec![0],
            edges,
        }
    }

    /// A 20 mm line, then a 90 degree fillet of r = 5 in six 15 degree
    /// chords, then another 20 mm line: one tangent chain, three runs.
    fn line_arc_line() -> Vec<[f64; 3]> {
        let mut p = vec![[-20.0, 0.0, 0.0]];
        for k in 0..=6 {
            let a = (-90.0 + 15.0 * f64::from(k)).to_radians();
            p.push([5.0 * a.cos(), 5.0 + 5.0 * a.sin(), 0.0]);
        }
        p.push([5.0, 25.0, 0.0]);
        p
    }

    #[test]
    fn a_line_beside_a_fillet_is_its_own_run() {
        let f = polyline(&line_arc_line(), false);
        let cs = chains(&f);
        assert_eq!(cs.len(), 1, "one tangent chain");
        let runs = edge_runs(&f);
        assert_eq!(runs.iter().map(|r| r.edges.len()).collect::<Vec<_>>(), vec![1, 6, 1]);
        assert!(runs.iter().all(|r| !r.closed));
        for r in &runs {
            assert_eq!(resolve_chain(&f, r.id).map(|c| c.edges), Some(r.edges.clone()));
        }
        assert_eq!(run_containing(&f, 3).map(|r| r.edges.len()), Some(6));
        assert_eq!(resolve_chain(&f, cs[0].id).map(|c| c.edges.len()), Some(8), "the old pick still resolves");
    }

    #[test]
    fn a_circle_and_a_plain_edge_are_their_own_chain() {
        let circle: Vec<[f64; 3]> = (0..24)
            .map(|k| {
                let a = (15.0 * f64::from(k)).to_radians();
                [10.0 * a.cos(), 10.0 * a.sin(), 0.0]
            })
            .collect();
        let f = polyline(&circle, true);
        let cs = chains(&f);
        let runs = edge_runs(&f);
        assert_eq!(runs.len(), 1);
        assert_eq!((runs[0].id, runs[0].closed), (cs[0].id, true), "a circle is one closed edge");
        let f = polyline(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]], false);
        let cs = chains(&f);
        assert_eq!(edge_runs(&f).iter().map(|r| r.id).collect::<Vec<_>>(), cs.iter().map(|c| c.id).collect::<Vec<_>>());
    }

    #[test]
    fn a_rounded_ring_splits_into_open_runs_that_spell_it() {
        // A 40 x 40 rounded square, r = 5, six chords per corner.
        let mut p = Vec::new();
        for (cx, cy, a0) in [(15.0, -15.0, -90.0), (15.0, 15.0, 0.0), (-15.0, 15.0, 90.0), (-15.0, -15.0, 180.0)] {
            for k in 0..=6 {
                let a = (a0 + 15.0 * f64::from(k)).to_radians();
                p.push([cx + 5.0 * a.cos(), cy + 5.0 * a.sin(), 0.0]);
            }
        }
        let f = polyline(&p, true);
        let cs = chains(&f);
        assert_eq!(cs.len(), 1);
        assert!(cs[0].closed, "the tangent chain closes - the old auto-closed pick");
        let runs = edge_runs(&f);
        assert_eq!(runs.len(), 8, "four lines, four arcs");
        assert!(runs.iter().all(|r| !r.closed));
        let total: usize = runs.iter().map(|r| r.edges.len()).sum();
        assert_eq!(total, cs[0].edges.len());
    }

    #[test]
    fn a_dangling_or_broken_walk_is_none_never_a_panic() {
        let f = polyline(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [0.0, 10.0, 0.0]], false);
        assert!(run_of_edges(&f, &[]).is_none());
        assert!(run_of_edges(&f, &[99]).is_none());
        assert!(run_of_edges(&f, &[0, 2]).is_none(), "not end to end");
        assert!(resolve_chain(&f, 12345).is_none());
        assert!(run_of_edges(&f, &[0, 1, 2]).is_some_and(|c| !c.closed));
    }
}
