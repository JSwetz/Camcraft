//! Split from `machining2d.rs` by M5.0.1; structure only.

use std::f64::consts::PI;
use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::contour::{Lead, unit};
use super::ring::dist;
/// One piece of tool motion: a straight feed walk through the listed
/// points (exactly how generators have always walked polylines), or one
/// arc move ending at `to` about `center`. A [`Seg`] list describes a
/// walk starting where the builder's cursor already is, so endpoints
/// chain exactly.
#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Line(Vec<Point2>),
    Arc { center: Point2, ccw: bool, to: Point2 },
}

/// Chord tolerance for arc fitting where no parameter carries one
/// (waterline rings, and the depth kinds without a `tolerance` field).
/// An arc may only replace a polyline if both sit within this of each
/// other - the same budget a chord's gouge allowance already spends.
pub const ARC_FIT_TOL: f64 = 0.01;

/// Longest vertex run a single candidate arc may span: bounds the
/// greedy walk on pathological input (no honest tessellation needs it).
pub(super) const ARC_FIT_MAX_RUN: usize = 4096;

/// How many still-too-straight windows a run may look ahead for its
/// first claim of curvature. A finely tessellated circle only proves
/// "this is a curve" once the window spans enough of it - two vertices
/// into a 1000-gon look flat - and a greedy that gave up at the first
/// flat window would never fit such a rim at all. Straight runs pay
/// this cost once per start vertex and then walk on as lines; no
/// honest tessellation that a `tol`-budgeted arc could replace needs
/// more than a handful of these (sagitta grows quadratically in the
/// window), so the cap only ever bounds pathological flat input.
pub(super) const ARC_FIT_STRAIGHT_LOOKAHEAD: usize = 64;

/// Walk a polyline's pieces: lines feed through their points, arcs get
/// one [`PathBuilder::push_arc`] each. At cut level `z` with `feed`.
pub(super) fn push_segs(b: &mut PathBuilder, segs: &[Seg], z: f64, feed: f64) {
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
                }
            }
            Seg::Arc { center, ccw, to } => {
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed), *center, *ccw);
            }
        }
    }
}

/// Travel direction of a fitted walk at its front vertex (`front`) or
/// its last one: the true circle tangent where that end rides an arc,
/// the adjacent-vertex chord otherwise (a straight's chord IS its
/// tangent, and the polyline fallback covers degenerate walks). A lead
/// must leave or join the wall on this vector for the join to be smooth.
pub(super) fn walk_tangent(segs: &[Seg], poly: &[Point2], front: bool) -> Point2 {
    let n = poly.len();
    if n < 2 {
        return Point2::new(1.0, 0.0);
    }
    let chord = if front {
        unit(poly[1].x - poly[0].x, poly[1].y - poly[0].y)
    } else {
        unit(poly[n - 1].x - poly[n - 2].x, poly[n - 1].y - poly[n - 2].y)
    };
    let arc = match (front, segs.first(), segs.last()) {
        (true, Some(Seg::Arc { center, ccw, .. }), _) => Some((*center, *ccw, poly[0])),
        (false, _, Some(Seg::Arc { center, ccw, to })) => Some((*center, *ccw, *to)),
        _ => None,
    };
    match arc {
        Some((c, ccw, p)) => {
            let (dx, dy) = (p.x - c.x, p.y - c.y);
            let m = (dx * dx + dy * dy).sqrt();
            if m == 0.0 || !m.is_finite() {
                chord
            } else if ccw {
                Point2::new(-dy / m, dx / m)
            } else {
                Point2::new(dy / m, -dx / m)
            }
        }
        None => chord,
    }
}

/// Replace maximal arc-fittable runs of a polyline with arc moves,
/// keeping everything else point-for-point.
///
/// A run earns arcs only if both directions agree within `tol`: every
/// vertex lies within `tol` of one circle (the arc is no lie about the
/// tessellation), and every chord lies within `tol` of that circle's
/// arc (the arc is no surprise against the verified polygon) - so
/// trading chords for the arc moves the tool nowhere the plan didn't
/// already tolerate. Near-straight runs (all deviations within `tol` of
/// the end chord) stay lines on principle: a huge-radius "arc" along a
/// straight would re-describe motion the golden already spells point by
/// point - but a window that is merely too short to have proven its
/// curvature yet extends the run instead of ending it (bounded by
/// [`ARC_FIT_STRAIGHT_LOOKAHEAD`]), because a fine tessellation of a
/// real circle looks flat until the chord spans enough of it. Runs that
/// genuinely fail - corners, coarse tessellations, backtracks - end the
/// run, and a run that claimed nothing advances one vertex at a time,
/// which is exactly the old walk, so polygonal paths are byte-identical
/// through this function.
///
/// Sweeps over half a turn split at a vertex into <= pi pieces (the
/// builder refuses zero-sweep arcs, so an unsplit full circle would
/// erase its ring here); endpoints are always polyline points, so
/// chaining is exact by construction. Consumers fit at use; nothing
/// about a [`crate::geometry::Features`] digest ever depends on this.
pub fn fit_arcs(pts: &[Point2], tol: f64) -> Vec<Seg> {
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { ARC_FIT_TOL };
    let n = pts.len();
    let mut segs = Vec::new();
    let mut s = 0usize;
    while s + 1 < n {
        let cap = (s + ARC_FIT_MAX_RUN).min(n - 1);
        let mut best: Option<(usize, Point2, bool, f64)> = None;
        let mut e = s + 2;
        let mut flat = 0;
        while e <= cap {
            match arc_over(&pts[s..=e], tol) {
                Over::Fit(center, ccw, total) => {
                    best = Some((e, center, ccw, total));
                    e += 1;
                }
                // Too straight to have said anything yet: extend, up
                // to the lookahead's word.
                Over::Straight if flat < ARC_FIT_STRAIGHT_LOOKAHEAD => {
                    flat += 1;
                    e += 1;
                }
                Over::Straight | Over::Fail => break,
            }
        }
        match best {
            Some((e, center, ccw, total)) => {
                // Split at vertices into sub-sweeps of at most half a
                // turn; a closing full circle becomes two arcs meeting at
                // the far vertex, so the builder never sees the
                // zero-sweep single arc that would erase the ring.
                // M4.2b: a closed ring's `total` is 2*pi to the last
                // accumulated atan2 bit - glibc and the Windows CRT
                // disagree there, and a raw ceil(2.0 ± ULP) would let
                // the platform pick 2 or 3 splits for every ring in
                // the shop. The lattice swallows that noise (it is
                // 1e-13 here, a thousand times under the quantum)
                // and still honours a genuine fraction: a ratio that
                // really sits > 5e-7 above an integer wanted another
                // piece and keeps it.
                let k = (lattice(total / PI) + 1e-12).ceil().max(1.0) as usize;
                for m in 1..=k {
                    let idx = if m == k { e } else { s + (e - s) * m / k };
                    segs.push(Seg::Arc {
                        center,
                        ccw,
                        to: pts[idx],
                    });
                }
                s = e;
            }
            None => {
                segs.push(Seg::Line(vec![pts[s + 1]]));
                s += 1;
            }
        }
    }
    segs
}

/// What a window of at least 3 points says about sitting on one circle
/// (see [`fit_arcs`]).
pub(super) enum Over {
    /// A circle within `tol`, both directions: (center, ccw, total
    /// sweep).
    Fit(Point2, bool, f64),
    /// Flatter than `tol` end to end - the window cannot tell arc from
    /// line yet, and the run's next, longer window might.
    Straight,
    /// Not one circle within `tol`.
    Fail,
}

/// The verdict on whether a run of >= 3 points sits on one circle
/// within `tol` in both directions, and if so which circle (see
/// [`fit_arcs`]). `Straight` is the honest answer when no vertex leaves
/// the end chord by more than `tol`: there is nothing there an arc
/// could describe better than the chords already do.
pub(super) fn arc_over(run: &[Point2], tol: f64) -> Over {
    let (a, b) = (run[0], run[run.len() - 1]);
    let dev = |p: Point2| chord_dev(p, a, b);
    // Farthest interior vertex from the end chord anchors the circle.
    let mut pivot = None;
    let mut pd = 0.0;
    for p in &run[1..run.len() - 1] {
        let d = dev(*p);
        if !d.is_finite() {
            return Over::Fail;
        }
        if d > pd {
            pd = d;
            pivot = Some(*p);
        }
    }
    // Curvature must exceed the tolerance to justify an arc at all -
    // but a short window on a finely tessellated curve is no refusal,
    // just not yet evidence.
    let Some(pivot) = pivot.filter(|_| pd > tol) else {
        return Over::Straight;
    };
    // A run closing a full circle has an end chord of zero length - no
    // usable anchor at `b`. Second anchor: the vertex farthest from the
    // a-pivot line, which for an honest tessellation is a quarter turn
    // out. Either anchor pair defines the circle - and the turning
    // direction, from the same triple it was fitted on; the validation
    // loops are the actual judges.
    let (c, r, ccw) = match circle3(a, pivot, b) {
        Some((c, r)) => (
            c,
            r,
            ((pivot.x - a.x) * (b.y - pivot.y) - (pivot.y - a.y) * (b.x - pivot.x)) > 0.0,
        ),
        None if dist(a, b) <= tol => {
            let mut second = None;
            let mut sd = 0.0;
            for p in &run[1..] {
                let d = chord_dev(*p, a, pivot);
                if d > sd && d.is_finite() {
                    sd = d;
                    second = Some(*p);
                }
            }
            let Some(anchor) = second.filter(|_| sd > tol) else {
                return Over::Straight;
            };
            let Some((c, r)) = circle3(a, pivot, anchor) else {
                return Over::Fail;
            };
            (
                c,
                r,
                ((pivot.x - a.x) * (anchor.y - pivot.y) - (pivot.y - a.y) * (anchor.x - pivot.x))
                    > 0.0,
            )
        }
        None => return Over::Fail,
    };
    if !r.is_finite() || r <= tol {
        return Over::Fail;
    }
    // Direction one: vertices on the circle.
    for p in run {
        let d = (((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt() - r).abs();
        if d > tol {
            return Over::Fail;
        }
    }
    // Direction two: chords hug the arc; a step against the claimed
    // direction lands near a full turn here and fails the same test.
    let sag_bound = 1.0 - tol / r;
    let mut total = 0.0;
    for w in run.windows(2) {
        let t0 = (w[0].y - c.y).atan2(w[0].x - c.x);
        let t1 = (w[1].y - c.y).atan2(w[1].x - c.x);
        let step = if ccw { t1 - t0 } else { t0 - t1 };
        if !step.is_finite() {
            return Over::Fail;
        }
        let step = step.rem_euclid(2.0 * PI);
        if (step * 0.5).cos() < sag_bound {
            return Over::Fail;
        }
        total += step;
    }
    if total <= 0.0 || total > 2.0 * PI + tol / r {
        return Over::Fail;
    }
    Over::Fit(c, ccw, total)
}

/// Distance from `p` to segment `a`-`b`.
pub(super) fn chord_dev(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    if dd <= f64::EPSILON {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    ((p.x - (a.x + t * dx)).powi(2) + (p.y - (a.y + t * dy)).powi(2)).sqrt()
}

/// Circle through three points: center and radius, or None when they
/// are (near-)collinear or land outside the finite horizon.
pub(super) fn circle3(a: Point2, b: Point2, c: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if !d.is_finite() || d.abs() <= f64::EPSILON {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    if !ux.is_finite() || !uy.is_finite() {
        return None;
    }
    let o = Point2::new(ux, uy);
    let r = dist(a, o);
    if !r.is_finite() {
        return None;
    }
    Some((o, r))
}

/// (plunge point, segments from it onto the path, ending exactly at
/// `s`).
pub(super) fn lead_in(s: Point2, t: Point2, air: Point2, lead: Lead) -> (Point2, Vec<Seg>) {
    match lead {
        Lead::None => (s, Vec::new()),
        // M11.3 C: straight in along the air normal, square to the wall.
        Lead::Perpendicular { length } => (
            Point2::new(s.x + air.x * length, s.y + air.y * length),
            vec![Seg::Line(vec![s])],
        ),
        Lead::Linear { length } => (
            Point2::new(s.x - t.x * length, s.y - t.y * length),
            vec![Seg::Line(vec![s])],
        ),
        Lead::Arc { radius } => {
            let o = Point2::new(s.x + air.x * radius, s.y + air.y * radius);
            let e = Point2::new(
                s.x + (air.x - t.x) * radius,
                s.y + (air.y - t.y) * radius,
            );
            // Quarter arc e -> s about o: ccw iff s bears left of the
            // start radius. The chord error at p.tolerance only appears
            // when something downstream flattens the move.
            let ccw = ((e.x - o.x) * (s.y - o.y) - (e.y - o.y) * (s.x - o.x)) > 0.0;
            (e, vec![Seg::Arc { center: o, ccw, to: s }])
        }
        Lead::Tangential { radius } => {
            // Semicircle e -> s about o = s + air*radius, with the plunge
            // point e = s + 2*radius*air: e - o and s - o are antiparallel,
            // so the quarter arm's radius-vector cross would vanish. The
            // surviving law is its limit: arrive at s moving along +t means
            // turning ccw iff air bears left of t - cross(air, t) < 0.
            // air_side puts climb's air right of t: cw; conventional ccw
            // (arcs.rs and machining2d.rs derive this from the law, never
            // hardcode it).
            let o = Point2::new(s.x + air.x * radius, s.y + air.y * radius);
            let e = Point2::new(s.x + air.x * 2.0 * radius, s.y + air.y * 2.0 * radius);
            let ccw = (air.x * t.y - air.y * t.x) < 0.0;
            (e, vec![Seg::Arc { center: o, ccw, to: s }])
        }
    }
}

/// Segments carrying the tool off the path end (not including it).
pub(super) fn lead_out(e: Point2, t: Point2, air: Point2, lead: Lead) -> Vec<Seg> {
    match lead {
        Lead::None => Vec::new(),
        Lead::Perpendicular { length } => {
            vec![Seg::Line(vec![Point2::new(e.x + air.x * length, e.y + air.y * length)])]
        }
        Lead::Linear { length } => vec![Seg::Line(vec![Point2::new(
            e.x + t.x * length,
            e.y + t.y * length,
        )])],
        Lead::Arc { radius } => {
            let o = Point2::new(e.x + air.x * radius, e.y + air.y * radius);
            let q = Point2::new(
                e.x + (air.x + t.x) * radius,
                e.y + (air.y + t.y) * radius,
            );
            let ccw = ((e.x - o.x) * (q.y - o.y) - (e.y - o.y) * (q.x - o.x)) > 0.0;
            vec![Seg::Arc { center: o, ccw, to: q }]
        }
        Lead::Tangential { radius } => {
            // The mirror of the lead-in half arc: leave the path end
            // overruning tangentially, sweep the semicircle out through
            // air, and stand off the wall 2*radius on the normal, moving
            // parallel to where the cut was going. Same sign law at the
            // tangent point as the lead-in: climb cw, conventional ccw.
            let o = Point2::new(e.x + air.x * radius, e.y + air.y * radius);
            let q = Point2::new(e.x + air.x * 2.0 * radius, e.y + air.y * 2.0 * radius);
            let ccw = (air.x * t.y - air.y * t.x) < 0.0;
            vec![Seg::Arc { center: o, ccw, to: q }]
        }
    }
}
