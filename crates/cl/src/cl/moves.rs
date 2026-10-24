//! The move vocabulary: kinds, the lattice, arcs and their chords.
//!
//! Provenance: Move kinds map to the G0/G1 vocabulary of ISO 6983 /
//! RS-274 (public standards); the rapid/feed/plunge split is general
//! machining practice. Arc moves carry the exact circular segment
//! (center + direction); the arc is geometry, the kind stays the
//! motion class. Chord flattening - for verification, display and the
//! golden dump - is [`Move::chords`], the one sagitta law for the
//! whole codebase. Arcs turn in XY; M2.1 (helical entries) widened
//! the law so an arc's endpoints may differ in Z - a Z change along
//! an XY circle is a helix, which the timeline (`sim::path_len`,
//! exact arc length over Z) and the post (`G2/G3` blocks carry the Z
//! word) already read. Endpoints whose XY coincides are never an arc:
//! that is a plunge, and straight it stays.

use std::f64::consts::PI;

use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::pose::normalize_axis;

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

/// Snap to the project's fixed-point grid (microns): the same lattice the
/// golden writer, the geo2d bridge and the stamp digests quantize to.
/// Public since M4.1 R2 - the generator crates quantize through this one
/// function, so the lattice is part of the CL contract, not a private
/// helper of the motion model.
/// Non-finite values pass through untouched - refusing is the callers'
/// job, and the chord flattening already falls back around them.
pub fn lattice(v: f64) -> f64 {
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
    /// `to` about this center - in XY; the endpoints' Zs may differ
    /// (a helix, M2.1) or agree (the planar case every 2.5-D path so far
    /// has had). `chords` speaks XY; the Z ride between the endpoints is
    /// the timeline's and the post's to read.
    pub arc: Option<ArcMove>,
    /// The tool's pose through this move (M5.1 stage A): a unit vector
    /// from tip toward shank, or None when the move states no angle -
    /// which is the ordinary, prismatic case. The pose is the whole
    /// tool's, not the path's: a move with a tilted axis travels its
    /// tip along `from..to` while the assembly leans along the axis's
    /// direction. Conversion to and from the RS-274 letter angles the
    /// stream commands is stated once, in [`super::pose`].
    pub axis: Option<[f64; 3]>,
}

impl Move {
    pub fn new(kind: MoveKind, from: Point3, to: Point3, feed: Option<f64>) -> Self {
        Move {
            kind,
            from,
            to,
            feed,
            arc: None,
            axis: None,
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

    /// This move holding the tool axis `axis` (unit-normalized here;
    /// the pose convention is [`super::pose`]'s). A vector that states
    /// no direction - non-finite, or shorter than any real direction -
    /// is refused LOUDLY and the move stays prismatic: silently keeping
    /// a garbage pose would be the wrong answer the Prime Directive
    /// forbids, and silently dropping one a generator asked for would
    /// be the same crime wearing a safe face.
    pub fn with_axis(mut self, axis: [f64; 3]) -> Self {
        match normalize_axis(axis) {
            Some(a) => self.axis = Some(a),
            None => {
                eprintln!("cl: refusing a move axis of {axis:?}: not a direction");
                self.axis = None;
            }
        }
        self
    }

    pub fn is_finite(self) -> bool {
        self.from.is_finite()
            && self.to.is_finite()
            && self.feed.is_none_or(|f| f.is_finite() && f > 0.0)
            && self
                .arc
                .is_none_or(|a| a.center.x.is_finite() && a.center.y.is_finite())
            && self.axis.is_none_or(|a| a.iter().all(|x| x.is_finite()))
    }

    /// Attach a pose VERBATIM - no normalization, no refusal. The
    /// door for callers that carry an already-stated value: the
    /// builder (whose `set_axis` normalized on the way in) and the
    /// project loader (whose stored components are the file's own
    /// lattice numbers, and a load that re-normalized them would
    /// break the save-load-save byte law). Anyone holding a raw
    /// vector uses [`Move::with_axis`], which normalizes and refuses
    /// loudly.
    #[must_use]
    pub fn attach_axis(mut self, axis: Option<[f64; 3]>) -> Self {
        self.axis = axis;
        self
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
        // M4.2b: `d` arrives from atan2 and `max_step` from acos, so a
        // sweep that is a clean multiple of the chord budget lands on
        // the ceil boundary ± a couple of last bits - and which side
        // of it the platform's libm leaves you is not a machining
        // fact. Quantize before the decision: one chord fewer only
        // when the budget is met to a relative 1e-6 (a sagitta short
        // of tol by 1e-11 mm), never a silent miss of a real one.
        (lattice(d / max_step) + 1e-12).ceil() as usize
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
