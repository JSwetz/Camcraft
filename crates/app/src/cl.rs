//! Cutter-location (CL) data: a linked sequence of machine moves.
//!
//! Provenance: our own minimal CL model. Move kinds map to the G0/G1
//! vocabulary of ISO 6983 / RS-274 (public standards); the rapid/feed/plunge
//! split is general machining practice. Arc moves carry the exact circular
//! segment (center + direction); the arc is geometry, the kind stays the
//! motion class. Chord flattening - for verification, display and the
//! golden dump - is [`Move::chords`], the one sagitta law for the whole
//! codebase. Helical (in-Z) arcs are M2.1: an arc move is planar, and
//! [`PathBuilder::push_arc`] says so by falling back to a straight move.
//!
//! Invariant: every move's `from` equals the previous move's `to` (the path
//! is a chain, no gaps); generators build through [`PathBuilder`], which
//! maintains it by construction.

use std::f64::consts::PI;

use crate::geo2d::Point2;
use crate::geometry::Point3;

/// G0 rapid, G1 feed, G1 with a plunge (reduced) feed - the three verbs a
/// 2.5-D path needs. The distinction drives display color now and post
/// vocabulary later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKind {
    Rapid,
    Feed,
    Plunge,
}

/// The circle an arc move traverses: its center in XY and the direction
/// of travel. The endpoints are the move's own `from`/`to`; together
/// they pin the sweep exactly (`from == to` never reaches a move - the
/// builder drops zero-sweep arcs, so fitted full circles are split by
/// `machining2d::fit_arcs` before they get here).
///
/// Centers stand on the 1 um lattice (see [`lattice`]): a fitted
/// circle's center is an ill-conditioned float, and the shortest
/// decimal of an off-lattice float need not parse back to the exact
/// bits it was written from - an arc cache could not then round-trip
/// byte-identically. Endpoints stay exact, so chaining and the sagitta
/// law are measured against the plan, never against the snap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcMove {
    pub center: Point2,
    pub ccw: bool,
}

/// Snap to the repo's fixed-point grid (microns): the same lattice the
/// golden writer, the geo2d bridge and the stamp digests quantize to.
/// Non-finite values pass through untouched - refusing is the callers'
/// job, and the chord flattening already falls back around them.
pub(crate) fn lattice(v: f64) -> f64 {
    if v.is_finite() {
        (v * 1e6).round() / 1e6
    } else {
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub kind: MoveKind,
    pub from: Point3,
    pub to: Point3,
    /// mm/min while in contact with material; None for rapids.
    pub feed: Option<f64>,
    /// None: a straight segment. Some: the circular arc from `from` to
    /// `to` about this center - planar (the endpoints share Z;
    /// `push_arc` keeps that by construction).
    pub arc: Option<ArcMove>,
}

impl Move {
    pub fn new(kind: MoveKind, from: Point3, to: Point3, feed: Option<f64>) -> Self {
        Move {
            kind,
            from,
            to,
            feed,
            arc: None,
        }
    }

    /// This move, traversing the circular segment about `center`
    /// (snapped to the arc center lattice - see [`ArcMove`]).
    pub fn with_arc(mut self, center: Point2, ccw: bool) -> Self {
        self.arc = Some(ArcMove {
            center: Point2::new(lattice(center.x), lattice(center.y)),
            ccw,
        });
        self
    }

    pub fn is_finite(self) -> bool {
        self.from.is_finite()
            && self.to.is_finite()
            && self.feed.is_none_or(|f| f.is_finite() && f > 0.0)
            && self
                .arc
                .is_none_or(|a| a.center.x.is_finite() && a.center.y.is_finite())
    }

    /// The XY segments this move traverses: itself for a straight move,
    /// chord samples about `tol` for an arc (endpoints exact). Anything
    /// that measures along a path - the verifier's laws, the overlay,
    /// the golden dump - goes through here so all three agree on what
    /// an arc means geometrically.
    pub fn chords(&self, tol: f64) -> Vec<(Point2, Point2)> {
        let a = Point2::new(self.from.x, self.from.y);
        let b = Point2::new(self.to.x, self.to.y);
        match self.arc {
            None => vec![(a, b)],
            Some(arc) => arc_chords(a, b, arc, tol),
        }
    }
}

/// Hard cap on chord pieces per arc: a tolerance small enough to ask
/// for more gets the cap (coarser chords) rather than a hang. With
/// endpoints always kept, the chord set still proves nothing about
/// finer features than the cap resolves - honest by construction, and
/// generation never approaches it (1024 chords is ~2.5 degrees each).
pub const MAX_ARC_CHORDS: usize = 1024;

/// Tessellate the arc from `a` to `b` about `arc.center`, travelling in
/// `arc.ccw`'s direction (sweep in (0, 2pi]), into chords whose sagitta
/// stays under `tol`. Endpoints are exact; the last chord ends at `b`.
/// Degenerate input (a zero radius, a non-finite center, zero sweep)
/// falls back to the single straight chord - never an empty result,
/// because empty would silently erase motion.
pub fn arc_chords(a: Point2, b: Point2, arc: ArcMove, tol: f64) -> Vec<(Point2, Point2)> {
    let straight = vec![(a, b)];
    let o = arc.center;
    if !o.x.is_finite() || !o.y.is_finite() {
        return straight;
    }
    let ra = ((a.x - o.x).powi(2) + (a.y - o.y).powi(2)).sqrt();
    let rb = ((b.x - o.x).powi(2) + (b.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    if r <= f64::EPSILON {
        return straight;
    }
    let a0 = (a.y - o.y).atan2(a.x - o.x);
    let a1 = (b.y - o.y).atan2(b.x - o.x);
    let mut d = if arc.ccw { a1 - a0 } else { a0 - a1 };
    if !d.is_finite() {
        return straight;
    }
    d = d.rem_euclid(2.0 * PI);
    if d <= 0.0 {
        // From and to share a ray: the zero-sweep "arc" the builder
        // refuses. Hand back the chord (zero motion) and let the
        // caller's own chaining decide; nothing sane produces this.
        return straight;
    }
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { 0.01 };
    // Sagitta r(1 - cos(step/2)) <= tol bounds the sweep per chord.
    let max_step = 2.0 * (1.0 - (tol / r).clamp(-1.0, 1.0)).clamp(-1.0, 1.0).acos();
    let n = if max_step > 0.0 {
        (d / max_step).ceil() as usize
    } else {
        usize::MAX
    }
    .clamp(1, MAX_ARC_CHORDS);
    let mut pts = Vec::with_capacity(n + 1);
    pts.push(a);
    for j in 1..n {
        // Measured off `a0` in the travel direction, so rounding cannot
        // accumulate along the run.
        let th = a0 + if arc.ccw { d * j as f64 / n as f64 } else { -d * j as f64 / n as f64 };
        pts.push(Point2::new(o.x + r * th.cos(), o.y + r * th.sin()));
    }
    pts.push(b);
    pts.windows(2).map(|w| (w[0], w[1])).collect()
}

/// A generated path: linked moves, nothing else. Caching, staleness and
/// serialization wrap this at the schema/project layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub moves: Vec<Move>,
}

impl Toolpath {
    pub fn new(moves: Vec<Move>) -> Self {
        Toolpath { moves }
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Where the machine is when the path starts (first move's `from`).
    pub fn start(&self) -> Option<Point3> {
        self.moves.first().map(|m| m.from)
    }

    pub fn end(&self) -> Option<Point3> {
        self.moves.last().map(|m| m.to)
    }

    pub fn all_finite(&self) -> bool {
        self.moves.iter().all(|m| m.is_finite())
    }

    /// True when every move starts where its predecessor ended.
    pub fn is_chained(&self) -> bool {
        self.moves.windows(2).all(|w| w[0].to == w[1].from)
    }

    /// Does any move traverse an arc? Decides the golden format
    /// version, among other things.
    pub fn has_arcs(&self) -> bool {
        self.moves.iter().any(|m| m.arc.is_some())
    }

    /// How many moves are arcs (the status line's honest count).
    pub fn arc_count(&self) -> usize {
        self.moves.iter().filter(|m| m.arc.is_some()).count()
    }

    /// The status line after a successful generate, counted from the
    /// moves themselves: plunge count with the plunge feed the path
    /// actually carries, the number of distinct cut levels (feeds at
    /// that height, whatever lifted the tool there), arc moves, and the
    /// verify warnings. Nothing a generator merely intended - only what
    /// the path shows.
    #[must_use]
    pub fn status_summary(&self, name: &str, warnings: usize) -> String {
        let n = self.moves.len();
        let p = self.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
        let a = self.arc_count();
        let mut levels: Vec<f64> = Vec::new();
        for m in &self.moves {
            if m.kind == MoveKind::Feed && !levels.contains(&m.to.z) {
                levels.push(m.to.z);
            }
        }
        let feed = self
            .moves
            .iter()
            .find(|m| m.kind == MoveKind::Plunge)
            .and_then(|m| m.feed)
            .map_or(String::new(), |f| format!(" @ {f}"));
        format!(
            "generated {name}: {n} moves ({p} plunge(s){feed}, {} level(s), {a} arc(s)), {warnings} warning(s)",
            levels.len()
        )
    }
}

/// Chains moves onto a Toolpath, maintaining the link invariant by
/// construction: a move's `from` is always the cursor.
pub struct PathBuilder {
    at: Point3,
    moves: Vec<Move>,
}

impl PathBuilder {
    pub fn new(at: Point3) -> Self {
        PathBuilder { at, moves: Vec::new() }
    }

    pub fn at(&self) -> Point3 {
        self.at
    }

    /// Append a move to `to`; degenerate zero-length moves are dropped (a
    /// "move" to where you already are posts nothing real and only pollutes
    /// golden comparisons).
    pub fn push(&mut self, kind: MoveKind, to: Point3, feed: Option<f64>) {
        if to.x == self.at.x && to.y == self.at.y && to.z == self.at.z {
            return;
        }
        self.moves.push(Move::new(kind, self.at, to, feed));
        self.at = to;
    }

    /// Append a move to `to` traversing the circular arc about `center`,
    /// travelling `ccw`. Endpoints stay the cursor and `to` exactly, so
    /// the chaining invariant holds by construction the same way
    /// [`push`](Self::push) holds it.
    ///
    /// Refusals, never normalizations: a zero-sweep arc (no XY motion)
    /// is dropped like any no-motion move - which is why `fit_arcs`
    /// splits full circles before they reach this call, so a ring is
    /// never silently erased. And anything an XY arc cannot describe -
    /// a Z change (helical is M2.1), a non-finite center - falls back
    /// to the straight move: the motion always posts, only its exact
    /// shape gets the honest simpler answer.
    pub fn push_arc(
        &mut self,
        kind: MoveKind,
        to: Point3,
        feed: Option<f64>,
        center: Point2,
        ccw: bool,
    ) {
        let planar = to.z == self.at.z
            && (to.x != self.at.x || to.y != self.at.y)
            && center.x.is_finite()
            && center.y.is_finite();
        if !planar {
            self.push(kind, to, feed);
            return;
        }
        self.moves
            .push(Move::new(kind, self.at, to, feed).with_arc(center, ccw));
        self.at = to;
    }

    pub fn finish(self) -> Toolpath {
        Toolpath { moves: self.moves }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn builder_chains_and_drops_no_motion() {
        let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
        b.push(MoveKind::Plunge, p(0.0, 0.0, 1.0), Some(100.0));
        b.push(MoveKind::Feed, p(0.0, 0.0, 1.0), Some(500.0)); // no motion
        b.push(MoveKind::Feed, p(10.0, 0.0, 1.0), Some(500.0));
        let t = b.finish();
        assert_eq!(t.len(), 2);
        assert!(t.is_chained());
        assert_eq!(t.start(), Some(p(0.0, 0.0, 5.0)));
        assert_eq!(t.end(), Some(p(10.0, 0.0, 1.0)));
        assert!(t.all_finite());
    }

    #[test]
    fn builder_drops_zero_sweep_arcs() {
        // An arc back to where the tool already stands is no motion:
        // dropped, like every other zero-length move. This is exactly
        // why fit_arcs splits full circles - the ring must not reach
        // here as one zero-sweep arc and evaporate.
        let mut b = PathBuilder::new(p(1.0, 0.0, 0.0));
        b.push_arc(MoveKind::Feed, p(1.0, 0.0, 0.0), Some(500.0), pt(0.0, 0.0), true);
        assert!(b.finish().is_empty());
        // Same XY, different Z: that is a vertical move, and the arc
        // (helical - not a thing until M2.1) falls back to the honest
        // straight plunge. The motion survives.
        let mut b = PathBuilder::new(p(1.0, 0.0, 0.0));
        b.push_arc(MoveKind::Plunge, p(1.0, 0.0, -2.0), Some(100.0), pt(0.0, 0.0), true);
        let t = b.finish();
        assert_eq!(t.len(), 1);
        assert_eq!(t.moves[0].kind, MoveKind::Plunge);
        assert!(t.moves[0].arc.is_none());
        assert!(t.is_chained());
    }

    #[test]
    fn status_summary_counts_levels_and_arcs() {
        // Two cut levels, a lead-in arc on the first, two warnings: the
        // line reports exactly what the path shows.
        let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
        b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
        b.push(MoveKind::Plunge, p(10.0, 0.0, 0.0), Some(100.0));
        b.push_arc(MoveKind::Feed, p(0.0, 10.0, 0.0), Some(500.0), pt(0.0, 0.0), true);
        b.push(MoveKind::Feed, p(-10.0, 0.0, 0.0), Some(500.0));
        b.push(MoveKind::Rapid, p(-10.0, 0.0, 3.0), None);
        b.push(MoveKind::Rapid, p(-10.0, 0.0, 5.0), None);
        b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
        b.push(MoveKind::Plunge, p(10.0, 0.0, -2.0), Some(100.0));
        b.push(MoveKind::Feed, p(0.0, 0.0, -2.0), Some(500.0));
        let t = b.finish();
        assert_eq!(t.len(), 9, "every move above is real motion");
        assert_eq!(
            t.status_summary("P1", 2),
            "generated P1: 9 moves (2 plunge(s) @ 100, 2 level(s), 1 arc(s)), 2 warning(s)"
        );
    }

    #[test]
    fn arc_moves_are_planar_and_chain() {
        let mut b = PathBuilder::new(p(10.0, 0.0, -2.0));
        b.push_arc(MoveKind::Feed, p(0.0, 10.0, -2.0), Some(500.0), pt(0.0, 0.0), true);
        let t = b.finish();
        assert_eq!(t.len(), 1);
        assert_eq!(t.arc_count(), 1);
        assert!(t.has_arcs());
        assert!(t.is_chained());
        assert!(t.all_finite());
        // Chords: quarter circle ccw from (10,0) to (0,10), sagitta
        // bound respected, endpoints exact.
        let cs = t.moves[0].chords(0.01);
        assert_eq!(cs.first().unwrap().0, pt(10.0, 0.0));
        assert_eq!(cs.last().unwrap().1, pt(0.0, 10.0));
        for (a, _) in &cs {
            let r = ((a.x * a.x + a.y * a.y).sqrt() - 10.0).abs();
            assert!(r <= 1e-9, "chord endpoint off the circle by {r}");
        }
        assert!(cs.windows(2).all(|w| w[0].1 == w[1].0), "chords must chain");
    }

    fn pt(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }
}
