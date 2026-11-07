//! The tangent biarc refit for the HSM plan's arc output (issue #11):
//! where the single-arc fit over a run of the plan's chords is refused
//! by the kink law (`arc_proof`) - the morph spiral's curvature changes
//! continuously and one constant-radius arc cannot meet both neighbours
//! within the plan's own turn - the run is fitted again as BIARCS: two
//! tangent-continuous arcs between the run's ends, each end tangent the
//! plan's own (the centred difference of the unit chords there, the
//! bisector of the plan's turn), so every junction turns at most half
//! what the plan turned. A run that does not hug its chords within the
//! budget is halved at its middle vertex (the halves share that vertex's
//! tangent, so they meet tangentially too) down to a few chords; every
//! arc is held to the same proofs as the single fit - both directions of
//! the tolerance, the area bracket, the kink law measured on the arcs'
//! true tangents - and to the curvature reading (its heading stays in
//! the cone of the plan's chords at every vertex and chord middle), or
//! its run stays the plan's chords. Its two radii stay within
//! [`RATIO`] of each other (round 2: a pair past it lags the plan's
//! tangent at the joint and overloads the cutter). The crossing law
//! (`arc_cross`) then judges each biarc as one unit.
//!
//! Provenance: the biarc of K. M. Bolton, "Biarc curves" (Computer-Aided
//! Design 7(2), 1975) with equal tangent-polygon legs, and the
//! tangent-interpolating arc spline of D. S. Meek and D. J. Walton,
//! "Approximating smooth planar curves by arc splines" (J. Comput. Appl.
//! Math. 59, 1995); plane geometry otherwise. No vendor material
//! consulted.

use std::f64::consts::{PI, TAU};

use crate::geo2d::{Point2, Region};

use super::arc_proof::{kink_free, proved, push_line};
use super::arcs::{Seg, chord_dev};

/// The fewest chords a biarc may replace (two moves for fewer would buy
/// nothing).
const MIN_CHORDS: usize = 3;

/// Halvings at most before a run gives up and stays chords.
const MAX_DEPTH: usize = 12;

/// The largest radius an arc of a biarc may carry: past it the piece is
/// a straight the chords already say exactly.
const R_MAX: f64 = 1.0e4;

/// The largest radius ratio between the two arcs of a biarc (round 2).
/// The plan's curvature changes continuously; a pair past it bunches the
/// run's turn into one tight arc beside a near-straight one, and the
/// near-straight arc's heading at the joint lags the plan's own tangent
/// toward the stock. On the L-pocket's tight spiral start an R 45 arc met
/// an R 0.43 one at the load peak: its true tangent sat 0.85 deg behind
/// the plan's centred tangent there and the reading rose 15.4 -> 16.7 % D
/// (past the 14.6 % + 10 % law); at 4 the peak reads 15.4 % again and the
/// moves stay a third down.
const RATIO: f64 = 4.0;

/// The turn the two arcs may differ by at their joint (radians): zero by
/// construction, this is the floating-point allowance only.
const JOINT_SLACK: f64 = 1e-6;

/// The in-cut break law's turn (5 deg): a plan vertex turning more is a
/// corner, which a biarc may not round.
const CORNER: f64 = 5.0 * PI / 180.0;

/// One arc of a biarc.
#[derive(Clone, Copy, Debug)]
struct Arc {
    from: Point2,
    center: Point2,
    r: f64,
    ccw: bool,
    to: Point2,
    sweep: f64,
}

fn sub(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x - b.x, a.y - b.y)
}

fn dot(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn unit(v: Point2) -> Option<Point2> {
    let l = v.x.hypot(v.y);
    (l > 0.0 && l.is_finite()).then(|| Point2::new(v.x / l, v.y / l))
}

/// The plan's tangent at vertex `k`: the bisector of its two unit chords
/// (the centred difference), the one chord at a pass end.
fn tangent(pass: &[Point2], k: usize) -> Option<Point2> {
    let inc = k.checked_sub(1).and_then(|h| unit(sub(*pass.get(k)?, pass[h])));
    let out = pass.get(k + 1).and_then(|q| unit(sub(*q, pass[k])));
    match (inc, out) {
        (Some(a), Some(b)) => unit(Point2::new(a.x + b.x, a.y + b.y)),
        (a, b) => a.or(b),
    }
}

/// The travel direction on `arc`'s circle at `p`.
fn arc_tangent(center: Point2, ccw: bool, p: Point2) -> Point2 {
    let (dx, dy) = (p.x - center.x, p.y - center.y);
    if ccw { Point2::new(-dy, dx) } else { Point2::new(dy, -dx) }
}

/// The sweep from `from` to `to` about `center` in the arc's sense.
fn sweep(from: Point2, center: Point2, ccw: bool, to: Point2) -> f64 {
    let a0 = (from.y - center.y).atan2(from.x - center.x);
    let a1 = (to.y - center.y).atan2(to.x - center.x);
    (if ccw { a1 - a0 } else { a0 - a1 }).rem_euclid(TAU)
}

/// The circle tangent to `t` at `p` through `q`: its centre and sense,
/// the sense being the travel along `t` at `p` (the centre on its left:
/// ccw). `None` where it is a straight or past [`R_MAX`].
fn tangent_circle(p: Point2, t: Point2, q: Point2) -> Option<(Point2, f64, bool)> {
    let n = Point2::new(-t.y, t.x);
    let w = sub(q, p);
    let ww = dot(w, w);
    let den = 2.0 * dot(w, n);
    if !(ww > 0.0 && den.abs() > 1e-12 * ww) {
        return None;
    }
    let s = ww / den;
    (s.is_finite() && s.abs() <= R_MAX).then(|| (Point2::new(p.x + s * n.x, p.y + s * n.y), s.abs(), s > 0.0))
}

/// The equal-leg biarc from `p0` (tangent `t0`) to `p1` (tangent `t1`):
/// the legs `d` solve |p1 - p0 - d (t0 + t1)| = 2 d, the joint is the
/// middle of the inner leg (Bolton 1975). `None` where no proper pair
/// of arcs of at most half a turn each exists.
fn biarc(p0: Point2, t0: Point2, p1: Point2, t1: Point2) -> Option<[Arc; 2]> {
    let v = sub(p1, p0);
    let t = Point2::new(t0.x + t1.x, t0.y + t1.y);
    let (a, b, c) = (2.0 * (dot(t0, t1) - 1.0), -2.0 * dot(v, t), dot(v, v));
    // The positive root, in its cancellation-free form.
    let den = -b + (b * b - 4.0 * a * c).max(0.0).sqrt();
    let d = 2.0 * c / den;
    if !(den > 0.0 && d.is_finite() && d > 0.0) {
        return None;
    }
    let q0 = Point2::new(p0.x + d * t0.x, p0.y + d * t0.y);
    let q1 = Point2::new(p1.x - d * t1.x, p1.y - d * t1.y);
    let j = Point2::new((q0.x + q1.x) * 0.5, (q0.y + q1.y) * 0.5);
    let (c0, r0, ccw0) = tangent_circle(p0, t0, j)?;
    let (c1, r1, ccw1) = tangent_circle(p1, t1, j)?;
    let arcs = [
        Arc { from: p0, center: c0, r: r0, ccw: ccw0, to: j, sweep: sweep(p0, c0, ccw0, j) },
        Arc { from: j, center: c1, r: r1, ccw: ccw1, to: p1, sweep: sweep(j, c1, ccw1, p1) },
    ];
    let proper = arcs.iter().all(|a| a.sweep > 0.0 && a.sweep <= PI && a.r > 0.0) && r0.max(r1) <= RATIO * r0.min(r1);
    // The joint is tangent-continuous: verified, not assumed.
    let joint = turn(arc_tangent(c0, ccw0, j), arc_tangent(c1, ccw1, j)) <= JOINT_SLACK;
    (proper && joint).then_some(arcs)
}

/// The unsigned turn from `u` to `v` (radians; a full half turn where
/// either is no direction - never a pass).
fn turn(u: Point2, v: Point2) -> f64 {
    match (unit(u), unit(v)) {
        (Some(u), Some(v)) => dot(u, v).clamp(-1.0, 1.0).acos(),
        _ => PI,
    }
}

/// Distance from `p` to `arc`.
fn arc_dist(p: Point2, a: &Arc) -> f64 {
    if sweep(a.from, a.center, a.ccw, p) <= a.sweep {
        ((p.x - a.center.x).hypot(p.y - a.center.y) - a.r).abs()
    } else {
        (p.x - a.from.x).hypot(p.y - a.from.y).min((p.x - a.to.x).hypot(p.y - a.to.y))
    }
}

/// The point at fraction `f` along `arc`.
fn arc_at(a: &Arc, f: f64) -> Point2 {
    let t = if a.ccw { a.sweep * f } else { -a.sweep * f };
    let (s, c) = t.sin_cos();
    let (dx, dy) = (a.from.x - a.center.x, a.from.y - a.center.y);
    Point2::new(a.center.x + dx * c - dy * s, a.center.y + dx * s + dy * c)
}

/// Both directions of the tolerance (the single fit's law): every
/// vertex and chord middle of `run` within `tol` of the biarc, and the
/// biarc, sampled finer than the run's own chords, within `tol` of the
/// run's chords.
fn hugs(run: &[Point2], arcs: &[Arc; 2], tol: f64) -> bool {
    let near = |p: Point2| arcs.iter().map(|a| arc_dist(p, a)).fold(f64::INFINITY, f64::min) <= tol;
    let mids = run.windows(2).map(|w| Point2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5));
    if !(run.iter().all(|p| near(*p)) && mids.into_iter().all(near)) {
        return false;
    }
    let m = run.len().saturating_mul(2).saturating_add(8);
    arcs.iter().all(|a| {
        (0..=m).all(|k| {
            let q = arc_at(a, k as f64 / m as f64);
            run.windows(2).any(|w| chord_dev(q, w[0], w[1]) <= tol)
        })
    })
}

/// The curvature reading: the biarc's heading never leaves the cone of
/// the plan's own chords - at each interior vertex it lies within the
/// turn the plan made there of both chords, at each chord's middle
/// within the larger of its ends' turns of the chord. A biarc may hug
/// the chords within the budget and still bunch the plan's curvature
/// into one tight arc beside a near-straight one; this refuses that.
fn follows(pass: &[Point2], (i, j): (usize, usize), arcs: &[Arc; 2]) -> bool {
    const SLACK: f64 = 1e-6;
    let heading = |p: Point2| {
        let a = if arc_dist(p, &arcs[0]) <= arc_dist(p, &arcs[1]) { &arcs[0] } else { &arcs[1] };
        arc_tangent(a.center, a.ccw, p)
    };
    let chord = |k: usize| sub(pass[k + 1], pass[k]);
    let bend = |k: usize| if k > 0 && k + 1 < pass.len() { turn(chord(k - 1), chord(k)) } else { 0.0 };
    // A vertex turning past the in-cut break law (5 deg) is a corner of
    // the plan, not a tessellated curve: no biarc smooths it away.
    if (i..=j).any(|k| bend(k) > CORNER) {
        return false;
    }
    let vertices = (i + 1..j).all(|k| {
        let t = heading(pass[k]);
        let lim = bend(k) + SLACK;
        turn(t, chord(k - 1)) <= lim && turn(t, chord(k)) <= lim
    });
    vertices
        && (i..j).all(|k| {
            let m = Point2::new((pass[k].x + pass[k + 1].x) * 0.5, (pass[k].y + pass[k + 1].y) * 0.5);
            turn(heading(m), chord(k)) <= bend(k).max(bend(k + 1)) + SLACK
        })
}

/// The biarc over `pass[i..=j]`, every proof held, or `None`.
fn proved_pair(pass: &[Point2], (i, j): (usize, usize), tol: f64, legal: &Region) -> Option<[Arc; 2]> {
    if j < i + MIN_CHORDS || j >= pass.len() {
        return None;
    }
    let arcs = biarc(pass[i], tangent(pass, i)?, pass[j], tangent(pass, j)?)?;
    let ok = hugs(&pass[i..=j], &arcs, tol)
        && follows(pass, (i, j), &arcs)
        && arcs.iter().all(|a| proved(a.from, a.center, a.ccw, a.to, legal))
        && kink_free(
            pass,
            (i, j),
            arc_tangent(arcs[0].center, arcs[0].ccw, pass[i]),
            arc_tangent(arcs[1].center, arcs[1].ccw, pass[j]),
        );
    ok.then_some(arcs)
}

fn span(pass: &[Point2], (i, j): (usize, usize), tol: f64, legal: &Region, depth: usize, out: &mut Vec<Seg>, arcs: &mut usize) {
    if let Some(pair) = proved_pair(pass, (i, j), tol, legal) {
        out.extend(pair.iter().map(|a| Seg::Arc { center: a.center, ccw: a.ccw, to: a.to }));
        *arcs += 2;
    } else if j >= i + 2 * MIN_CHORDS && depth < MAX_DEPTH {
        let m = i + (j - i) / 2;
        span(pass, (i, m), tol, legal, depth + 1, out, arcs);
        span(pass, (m, j), tol, legal, depth + 1, out, arcs);
    } else if let Some(qs) = pass.get(i + 1..=j) {
        push_line(out, qs.to_vec());
    }
}

/// The run `pass[i..=j]` (a single arc the kink law refused) refitted as
/// proved biarcs and chords; with the arcs it holds. `None` where not
/// one arc could be proved (the run stays chords).
pub(super) fn refit(pass: &[Point2], (i, j): (usize, usize), tol: f64, legal: &Region) -> Option<(Vec<Seg>, usize)> {
    let mut out = Vec::new();
    let mut arcs = 0usize;
    span(pass, (i, j), tol, legal, 0, &mut out, &mut arcs);
    (arcs > 0).then_some((out, arcs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::arc_proof::tests::disc;

    /// An Archimedean spiral, finely tessellated: its curvature changes
    /// along every run.
    fn spiral(n: usize) -> Vec<Point2> {
        (0..=n)
            .map(|k| {
                let a = k as f64 * TAU / 200.0;
                let r = 2.0 + 0.8 * a / TAU;
                Point2::new(r * a.cos(), r * a.sin())
            })
            .collect()
    }

    /// The biarc meets its end tangents exactly and its joint smoothly,
    /// and the refit over a spiral run hugs the chords and adds no kink.
    #[test]
    fn a_spiral_run_refits_as_tangent_biarcs() {
        let (p0, t0, p1, t1) = (Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), Point2::new(2.0, 2.0), Point2::new(0.0, 1.0));
        let Some(arcs) = biarc(p0, t0, p1, t1) else { panic!("biarc") };
        assert!(turn(arc_tangent(arcs[0].center, arcs[0].ccw, p0), t0) < 1e-9);
        assert!(turn(arc_tangent(arcs[1].center, arcs[1].ccw, p1), t1) < 1e-9);
        let pass = spiral(120);
        let legal = disc(0.0, 0.0, 10.0, 720);
        let Some((segs, n)) = refit(&pass, (10, 60), 0.0025, &legal) else { panic!("refit") };
        assert!(n >= 2 && n % 2 == 0, "{segs:?}");
        let Some(Seg::Arc { to, .. }) = segs.last() else { panic!("ends on an arc") };
        assert_eq!(*to, pass[60]);
        // A degenerate run (all one point) is refused, never a panic.
        let flat = vec![Point2::new(1.0, 1.0); 8];
        assert!(refit(&flat, (0, 7), 0.0025, &legal).is_none());
    }

    /// Issue #11 round 2: the L-pocket's spiral start (D10, the plan's
    /// own vertices at the load peak), where the equal-leg biarc over the
    /// middle four chords pairs an R 45 arc with an R 0.43 one - past the
    /// radius ratio, so the run is not a biarc and stays the plan's.
    #[test]
    fn a_biarc_bunching_the_turn_into_one_arc_is_refused() {
        let pass: Vec<Point2> = [
            (20.7768, 27.1139),
            (20.7191, 27.1920),
            (20.6727, 27.2576),
            (20.6488, 27.2882),
            (20.6365, 27.3019),
            (20.6112, 27.3269),
            (20.5853, 27.3493),
        ]
        .iter()
        .map(|&(x, y)| Point2::new(x, y))
        .collect();
        let (Some(t0), Some(t1)) = (tangent(&pass, 1), tangent(&pass, 5)) else { panic!("tangents") };
        assert!(biarc(pass[1], t0, pass[5], t1).is_none());
        let legal = disc(20.6, 27.3, 5.0, 720);
        assert!(proved_pair(&pass, (1, 5), 0.0025, &legal).is_none());
    }
}
