//! The rest-material model the verifier's laws share - per-plane island sections and column surface heights; our own conservative approximation, stated in `verify`'s module law (split from `engine.rs` by M13.7; structure only).

use super::cut_index::CutIndex;
use super::laws::dist2_seg;
use super::types::VerifyContext;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;

/// A cutting chord: its XY ends, the plane it cuts at, its move index.
pub(super) type Cut = (Point2, Point2, f64, usize);

/// A column's cut-order staircase (M13.7): `(move index, height)` steps,
/// index ascending and height strictly descending - the column's surface
/// once every chord of a move index below `upto` has cut is the last
/// step whose index is below `upto`, capped at the stock top.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Steps {
    steps: Vec<(usize, f64)>,
    top: f64,
}

impl Steps {
    /// The column surface before move `upto` - exactly what
    /// [`Rest::surface_before`] answers for the same column.
    pub(super) fn before(&self, upto: usize) -> f64 {
        let k = self.steps.partition_point(|s| s.0 < upto);
        let z = match k.checked_sub(1).and_then(|j| self.steps.get(j)) {
            Some(s) => s.1,
            None => f64::INFINITY,
        };
        z.min(self.top)
    }
}

/// One path's rest-material view: the plane-section cache and the
/// cutting chords the column surface is read from.
pub(super) struct Rest<'c, 'a> {
    cx: &'c VerifyContext<'a>,
    /// A model-driven path's islands per plane - the generator's own
    /// `material_above`, so one law has two readers (M2.1a). Cached by
    /// the plane's bits: the level ladder repeats its planes across all
    /// three readers. `None` means the slice failed; every use of a
    /// failed slice fails closed (no proof, no pass).
    island_cache: RefCell<HashMap<u64, Option<Region>>>,
    /// Cutting (non-rapid) XY segments with the level they cut at: a
    /// segment's whole disc sweep clears its column down to `to.z`
    /// (plunges are degenerate segments at their XY, arcs contribute
    /// their chords - all within chord_tol of the true arc).
    /// Deepest plane first, so `surface`'s first reach is its deepest
    /// reach - the min the law always asked for, found without a scan.
    /// Each chord carries its move's index (M13.7 cut order): a move is
    /// judged only against the chords cut BEFORE it, never against
    /// stock a later move removes.
    cut: Vec<Cut>,
    /// The chords' grid broad phase (M13.7 cut order): with order a
    /// query can no longer stop at the first in-reach chord, so it walks
    /// only the chords that can reach its column.
    index: CutIndex,
    /// The stock hidden under overhangs per plane (M13.7 B) - read by
    /// [`super::hidden`]; `None` is a failed slice, failing closed.
    pub(super) hidden_cache: RefCell<HashMap<u64, Option<Rc<super::hidden::Hidden>>>>,
    /// ALL part material at or above a plane, projected (the
    /// generators' islands law, `slice::material_overhead`), per
    /// plane, read by the posed rapid reading ([`super::rapids_posed`]);
    /// `None` is a failed slice, failing closed.
    overhead_cache: RefCell<HashMap<u64, Option<Rc<Region>>>>,
}

impl<'c, 'a> Rest<'c, 'a> {
    pub(super) fn new(path: &Toolpath, cx: &'c VerifyContext<'a>, chord_tol: f64) -> Self {
        let mut cut: Vec<Cut> = path
            .moves
            .iter()
            .enumerate()
            .filter(|(_, m)| m.kind != MoveKind::Rapid)
            .flat_map(|(i, m)| {
                m.chords(chord_tol)
                    .into_iter()
                    .map(move |(a, b)| (a, b, m.to.z, i))
            })
            .collect();
        cut.sort_by(|a, b| a.2.total_cmp(&b.2));
        let index = CutIndex::new(&cut, cx.tool.radius + 1e-9);
        Self {
            cx,
            island_cache: RefCell::new(HashMap::new()),
            cut,
            index,
            hidden_cache: RefCell::new(HashMap::new()),
            overhead_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Whether part material stands at or above (q, `h`): the section
    /// at `h` united with the shadow of what hangs over it. `None` is
    /// no model or a failed slice - the caller fails closed.
    pub(super) fn part_over(&self, q: Point2, h: f64) -> Option<bool> {
        let f = self.cx.model?;
        if !h.is_finite() {
            return None;
        }
        let key = h.to_bits();
        let hit = self.overhead_cache.borrow().get(&key).cloned();
        let region = match hit {
            Some(r) => r,
            None => {
                let fresh = crate::geometry::slice::material_overhead(f, h).ok().map(Rc::new);
                self.overhead_cache.borrow_mut().entry(key).or_insert_with(|| fresh.clone());
                fresh
            }
        };
        region.map(|r| r.contains(q))
    }

    /// The context the laws are judged in.
    pub(super) fn cx(&self) -> &VerifyContext<'a> {
        self.cx
    }

    /// The cutting chords, deepest plane first.
    pub(super) fn cut(&self) -> &[Cut] {
        &self.cut
    }

    /// The model's section strictly above plane `z` (cached).
    pub(super) fn islands_at(&self, facets: &[Triangle], z: f64) -> Option<Region> {
        let key = z.to_bits();
        if let Some(hit) = self.island_cache.borrow().get(&key) {
            return hit.clone();
        }
        let fresh = crate::geometry::slice::material_above(facets, z).ok();
        self.island_cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| fresh.clone());
        fresh
    }

    /// The same membership asked as a question about one point, without
    /// cloning the plane's region to ask it (M2.5 E): `surface`'s binary
    /// search and `part_at` probe thousands of cached slices per program,
    /// and a Region clone per probe turned a continuous-Z path (a flow
    /// line rides a new plane with every chord) into a minutes-long walk.
    pub(super) fn island_holds(&self, facets: &[Triangle], z: f64, q: Point2) -> bool {
        let key = z.to_bits();
        if let Some(hit) = self.island_cache.borrow().get(&key) {
            return hit.as_ref().is_some_and(|s| s.contains(q));
        }
        let fresh = crate::geometry::slice::material_above(facets, z).ok();
        let holds = fresh.as_ref().is_some_and(|s| s.contains(q));
        self.island_cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| fresh.clone());
        holds
    }

    /// Material footprint for the reach laws: the stock prism when given,
    /// the region standing in for it otherwise.
    pub(super) fn in_stock(&self, q: Point2) -> bool {
        let cx = self.cx;
        cx.stock.map_or_else(|| cx.region.contains(q), |s| s.contains(q))
    }

    /// m(q) before move `upto`: the surface height of remaining material
    /// in q's column once every chord of a move index below `upto` has
    /// cut (M13.7 cut order) - a move is judged against the cuts before
    /// it, never against stock a later move removes. The index filter
    /// only shrinks the clearing set, so the height only rises: a
    /// verdict can gain a conviction from it, never lose one.
    /// A model-driven path adds one clause (M2.1a): a chord at plane z
    /// cleared q's column only if the section at z does not hold q -
    /// cutting around an island removes nothing inside it, and the
    /// staircase's top is the shallower plane that did reach. Sections
    /// are monotone in z, so the island test is a point-in-region on a
    /// cached slice, run only for chords that are already within reach.
    pub(super) fn surface_before(&self, q: Point2, upto: usize) -> f64 {
        let lo = self.island_threshold(q);
        // The upward test keeps the island clause: above the threshold
        // it never fires (that is the threshold's meaning), so this is
        // the old predicate restated, not an approximation of it. The
        // walk is z-ascending, so the first hit is the deepest reach.
        let cleared = self
            .near(q, lo)
            .find(|c| c.3 < upto && self.clears(c, q))
            .map_or(f64::INFINITY, |c| c.2);
        cleared.min(self.cx.stock_top_z)
    }

    /// q's whole cut-order staircase (see [`Steps`]), for a reader that
    /// memoizes one column across many moves (law (c3)'s path-wide
    /// memo). One upward walk: a clearing chord becomes a step when its
    /// move comes before every step found so far - deeper chords were
    /// seen first - so the heights descend as the indices ascend, and
    /// [`Steps::before`] reads exactly what [`Self::surface_before`]
    /// would answer for every `upto`.
    pub(super) fn surface_steps(&self, q: Point2) -> Steps {
        let lo = self.island_threshold(q);
        let mut steps: Vec<(usize, f64)> = Vec::new();
        let mut best = usize::MAX;
        for c in self.near(q, lo) {
            if c.3 < best && self.clears(c, q) {
                steps.push((c.3, c.2));
                best = c.3;
                if best == 0 {
                    break; // no move comes before the first
                }
            }
        }
        steps.reverse();
        Steps { steps, top: self.cx.stock_top_z }
    }

    /// The chords from z-sorted position `from` up that can reach q's
    /// column ([`CutIndex::near`]), deepest plane first.
    pub(super) fn near(&self, q: Point2, from: usize) -> impl Iterator<Item = &Cut> + '_ {
        self.index.near(q, from).filter_map(|k| self.cut.get(k))
    }

    /// Whether chord `c` cleared q's column: within the tool radius, and
    /// (model-driven) on a plane whose section leaves q open.
    pub(super) fn clears(&self, c: &Cut, q: Point2) -> bool {
        let cx = self.cx;
        let r = cx.tool.radius;
        let r2 = (r + 1e-9) * (r + 1e-9);
        dist2_seg(q, c.0, c.1) <= r2 && cx.model.is_none_or(|f| !self.island_holds(f, c.2, q))
    }

    /// The first chord at or above q's island threshold. The island
    /// clause is a monotone threshold in z, not a scan (M2.5 E): `q`
    /// sits inside the plane-z island exactly while z < h(q) - the
    /// slice IS the part above z - so below the threshold every chord
    /// fails the island test whatever its reach or move index, and above
    /// it every chord clears iff it reaches. Binary-searched over log
    /// probed planes, no clones (`island_holds`).
    fn island_threshold(&self, q: Point2) -> usize {
        let cut = &self.cut;
        let mut lo = 0usize;
        let mut hi = cut.len();
        if let Some(f) = self.cx.model {
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if cut.get(mid).is_some_and(|c| self.island_holds(f, c.2, q)) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
        }
        lo
    }
}

