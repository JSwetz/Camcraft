//! The microlift return (M11.4 C round 4): the HSM plan's link between
//! two biting pieces of its spiral (`adaptive::lift` proved it: inside
//! the center area, its swept disc over cleared stock only, crossing no
//! other return and no entry orbit). Posted as feeds: a climb to its
//! lift over the floor (`adaptive::MICROLIFT`, or a multiple of it where
//! the returns below would cross), the lifted traverse
//! (straight, or routed through its via points), and the ramp down
//! along the next piece's own air lead (the spiral's path there, so the
//! descent reads only the air the plan read) - the tool never drags its
//! end face over the floor it just cut and never rides the walls to get
//! there: the lifted, non-cutting return of trochoidal and adaptive
//! clearing (Otkur & Lazoglu 2007, Int. J. Mach. Tools Manuf. 47). The
//! ramp's run is ours.

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::adaptive::Return;
use super::levels::Motion;

/// A ramp runs this many times its rise (about 18 degrees).
pub(super) const RUN_PER_RISE: f64 = 3.0;

/// The return's vertices from `a` (on the plane): each XY point with its
/// height over the plane, lattice-snapped. Up along `ret.up` (straight up
/// where it is empty), across at its lift through `ret.via` to the
/// ramp's top, down `ret.down` by its own length (ending on the next
/// pass's start) - a one-point `down` descends along the traverse's own
/// last run.
pub(super) fn profile(a: Point2, ret: &Return) -> Vec<(Point2, f64)> {
    let mut out = vec![(a, 0.0)];
    let Some(&e) = ret.down.last() else { return out };
    let snap = |p: Point2| Point2::new(lattice(p.x), lattice(p.y));
    let seg = |w: &[Point2]| (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
    let hz = lattice(ret.lift);
    if ret.high {
        // Over the retract plane: the feeds start at the ramp's top.
        let mut out = vec![(snap(ret.down[0]), hz)];
        let total: f64 = ret.down.windows(2).map(seg).sum();
        let mut acc = 0.0;
        for w in ret.down.windows(2) {
            acc += seg(w);
            let f = if total > 1e-12 { (acc / total).min(1.0) } else { 1.0 };
            out.push((snap(w[1]), lattice(hz * (1.0 - f))));
        }
        return out;
    }
    // A polyline from `from` walked by its own length, the height running
    // from `h0` to `h1`.
    let ramp = |out: &mut Vec<(Point2, f64)>, from: Point2, pts: &[Point2], (h0, h1): (f64, f64)| {
        let mut poly = vec![from];
        poly.extend_from_slice(pts);
        let total: f64 = poly.windows(2).map(seg).sum();
        let mut acc = 0.0;
        for w in poly.windows(2) {
            acc += seg(w);
            let f = if total > 1e-12 && total.is_finite() { (acc / total).min(1.0) } else { 1.0 };
            out.push((snap(w[1]), lattice(h0 + (h1 - h0) * f)));
        }
    };
    // The climb.
    let top = ret.up.last().copied().unwrap_or(a);
    if ret.up.is_empty() {
        out.push((snap(a), hz));
    } else {
        ramp(&mut out, a, &ret.up, (0.0, hz));
    }
    // The traverse, all at the lift but a one-point ramp's last run.
    let mut trav: Vec<Point2> = ret.via.clone();
    trav.push(ret.down[0]);
    if ret.down.len() >= 2 {
        for &q in &trav {
            out.push((snap(q), hz));
        }
        ramp(&mut out, ret.down[0], &ret.down[1..], (hz, 0.0));
        return out;
    }
    let mut poly = vec![top];
    poly.extend_from_slice(&trav);
    let total: f64 = poly.windows(2).map(seg).sum();
    let fall = (ret.lift * RUN_PER_RISE).min(total);
    let mut acc = 0.0;
    for w in poly.windows(2) {
        let l = seg(w);
        // The ramp's top, where it falls inside this leg.
        let c = total - fall;
        if c > acc + 1e-9 && c < acc + l - 1e-9 && l.is_finite() {
            let t = (c - acc) / l;
            out.push((snap(Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t)), hz));
        }
        acc += l;
        let h = if fall > 1e-12 { hz * ((total - acc) / fall).clamp(0.0, 1.0) } else { 0.0 };
        out.push((snap(w[1]), lattice(h)));
    }
    if let Some(last) = out.last_mut() {
        *last = (snap(e), 0.0);
    }
    out
}

/// Feed from where the tool stands on plane `z` along [`profile`] onto
/// the next pass's start on plane `z`.
pub(super) fn link(b: &mut PathBuilder, ret: &Return, z: f64, m: &Motion) {
    link_at(b, ret, z, m.hop_z, m.feed);
}

/// [`link`] on bare numbers: the hop plane a `high` return rides and
/// the cutting feed (the 3-D ladder's per-level port, `machining3d::hsm`).
pub(crate) fn link_at(b: &mut PathBuilder, ret: &Return, z: f64, hop_z: f64, feed: f64) {
    let at = b.at();
    let prof = profile(Point2::new(at.x, at.y), ret);
    if ret.high {
        // Up, across and down to the lift in rapid (vertical rapids in
        // the cut's own kerf and column; the lateral one over the
        // retract plane), then down the lead at feed.
        let Some(&(top, hz)) = prof.first() else { return };
        let hop = hop_z.max(z + hz);
        b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
        b.push(MoveKind::Rapid, Point3::new(top.x, top.y, hop), None);
        b.push(MoveKind::Rapid, Point3::new(top.x, top.y, lattice(z + hz)), None);
        for (p, dz) in prof.into_iter().skip(1) {
            let zz = if dz > 0.0 { lattice(z + dz) } else { z };
            b.push(MoveKind::Feed, Point3::new(p.x, p.y, zz), Some(feed));
        }
        return;
    }
    for (p, dz) in prof.into_iter().skip(1) {
        let zz = if dz > 0.0 { lattice(z + dz) } else { z };
        b.push(MoveKind::Feed, Point3::new(p.x, p.y, zz), Some(feed));
    }
}
