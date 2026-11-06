//! The rest-material model the verifier's laws share - per-plane island sections and column surface heights; our own conservative approximation, stated in `verify`'s module law (split from `engine.rs` by M13.7; structure only).

use super::laws::dist2_seg;
use super::types::VerifyContext;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;

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
    cut: Vec<(Point2, Point2, f64)>,
    /// The stock hidden under overhangs per plane (M13.7 B) - read by
    /// [`super::hidden`]; `None` is a failed slice, failing closed.
    pub(super) hidden_cache: RefCell<HashMap<u64, Option<Rc<super::hidden::Hidden>>>>,
}

impl<'c, 'a> Rest<'c, 'a> {
    pub(super) fn new(path: &Toolpath, cx: &'c VerifyContext<'a>, chord_tol: f64) -> Self {
        let mut cut: Vec<(Point2, Point2, f64)> = path
            .moves
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid)
            .flat_map(|m| {
                m.chords(chord_tol)
                    .into_iter()
                    .map(move |(a, b)| (a, b, m.to.z))
            })
            .collect();
        cut.sort_by(|a, b| a.2.total_cmp(&b.2));
        Self {
            cx,
            island_cache: RefCell::new(HashMap::new()),
            cut,
            hidden_cache: RefCell::new(HashMap::new()),
        }
    }

    /// The context the laws are judged in.
    pub(super) fn cx(&self) -> &VerifyContext<'a> {
        self.cx
    }

    /// The cutting chords, deepest plane first.
    pub(super) fn cut(&self) -> &[(Point2, Point2, f64)] {
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

    /// m(q): the surface height of remaining material in q's column.
    /// A model-driven path adds one clause (M2.1a): a chord at plane z
    /// cleared q's column only if the section at z does not hold q -
    /// cutting around an island removes nothing inside it, and the
    /// staircase's top is the shallower plane that did reach. Sections
    /// are monotone in z, so the island test is a point-in-region on a
    /// cached slice, run only for chords that are already within reach.
    pub(super) fn surface(&self, q: Point2) -> f64 {
        let cx = self.cx;
        let cut = &self.cut;
        let r = cx.tool.radius;
        let r2 = (r + 1e-9) * (r + 1e-9);
        // The island clause is a monotone threshold in z, not a scan
        // (M2.5 E): `q` sits inside the plane-z island exactly while
        // z < h(q) - the slice IS the part above z - so below the
        // threshold every chord fails the island test whatever its
        // reach, and above it every chord clears iff it reaches. The
        // first success is therefore the first in-reach chord at or
        // above the threshold: binary-search the threshold (log probed
        // planes, no clones - `island_holds`), then walk upward. The
        // old deepest-first `find` returns the same chord, move for
        // move; it just walked every deep chord's island to learn it.
        let mut lo = 0usize;
        let mut hi = cut.len();
        if let Some(f) = cx.model {
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if self.island_holds(f, cut[mid].2, q) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
        }
        // The upward test keeps the island clause: above the threshold
        // it never fires (that is the threshold's meaning), so this is
        // the old predicate restated, not an approximation of it.
        let cleared = cut[lo..]
            .iter()
            .find(|(a, b, z)| {
                dist2_seg(q, *a, *b) <= r2
                    && cx.model
                        .is_none_or(|f| !self.island_holds(f, *z, q))
            })
            .map_or(f64::INFINITY, |(_, _, z)| *z);
        cleared.min(cx.stock_top_z)
    }
}
