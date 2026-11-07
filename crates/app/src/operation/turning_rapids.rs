//! Issue #103: the turned family's axial rapids judged in the meridian (turning) plane against the revolved stock - the shop rule that a rapid never enters uncut material (machine-tool handbooks) read about the spindle as a body of revolution; our own design, no vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::rest::DrillAhead;
use crate::turning::profile::Frame;
use crate::verify::Warning;

use super::drill_prior::{PriorCell, PriorRead};
use super::kinds::Section;
use super::turning_outline::Revolved;
use super::OpParams;

/// Most stations one rapid is sampled at: a longer rapid samples more
/// sparsely, never unboundedly.
const MAX_SAMPLES: f64 = 20_000.0;

/// What a turned path's axial rapids are judged against.
pub(super) struct Meridian<'a> {
    pub part: &'a [Triangle],
    /// The setup's stock; `None` reads the part as the stock.
    pub stock: Option<&'a [Triangle]>,
    /// The insert's nose radius: the moves carry its centre.
    pub nose: f64,
    /// The margin a reading must clear by (the gouge tolerance).
    pub margin: f64,
    pub z_tol: f64,
    /// The Drill ops ahead: an axial pilot is a removed cylinder with
    /// its point's cone, read on a lathe too.
    pub drills: &'a [DrillAhead],
    /// Half the blade's band along the axis (#103 review B2): a groove
    /// or parting blade's move carries the band's CENTRE, a thread's
    /// its crest - the edge spans this far either side. 0 for a point.
    pub band: f64,
    /// A boring bar's body (#103 review B1, B3): its DIAMETER and its
    /// reach up the axis from the nose (flute and shank). The insert
    /// sits at the body's edge and the body lies toward the axis from
    /// it - signed radius [rho - d, rho] for a nose at rho > 0 - so a bar
    /// with its nose on the axis reaches a whole diameter off it. The
    /// nose disc stays the cutting tip's; the body must fit the hole too.
    pub body: Option<(f64, f64)>,
}

/// Law (d) for the turned family: the column law's word on a turned
/// path's AXIAL rapids (no XY travel) is dropped - the spindle stands
/// on world Z, the column cannot see the meridian (#96) - and each such
/// rapid is judged in the meridian plane instead. At its radius from the
/// spindle the rapid's nose disc must not meet material between its two
/// stations: the stock and the part revolved about the axis (the spindle
/// carries every angle past the insert), less what the program ahead
/// faced off (the prior grid read revolved, asked only when needed) and
/// less the nose sweep of this path's own earlier feeds - a retract
/// along a turned diameter or an approach down a bored hole stands in
/// air this op made. A body the axis fit refuses keeps the column's
/// convictions (unknown is stock).
pub(super) fn judge_axial_rapids(
    params: &OpParams,
    path: &Toolpath,
    cx: &Meridian,
    prior: &mut PriorCell,
    warns: &mut Vec<Warning>,
) {
    if params.kind().info().section != Section::Turning {
        return;
    }
    let axial = |at: usize| {
        path.moves.get(at).is_some_and(|m| {
            let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
            m.kind == MoveKind::Rapid && dx * dx + dy * dy <= cx.z_tol * cx.z_tol
        })
    };
    let Ok(frame) = Frame::about(cx.part) else { return };
    let Some(part) = Revolved::of(cx.part, &frame) else { return };
    let stock = match cx.stock {
        Some(s) => match Revolved::of(s, &frame) {
            Some(r) => Some(r),
            None => return, // an unreadable stock: the column's word stands
        },
        None => None,
    };
    warns.retain(|w| !matches!(*w, Warning::RapidInStock { at } if axial(at)));
    let sm = SignedMeridian::of(&frame, path);
    let mut cap: Option<Option<f64>> = None; // the faced end, read once
    for (i, m) in path.moves.iter().enumerate() {
        if !axial(i) {
            continue;
        }
        let (a, b) = (sm.sr(m.from), sm.sr(m.to));
        let hit = |cap: Option<f64>| {
            enters(a, b, i, path, &sm, &part, stock.as_ref(), cap, cx)
        };
        if !hit(None) {
            continue;
        }
        // Only a conviction pays for the program ahead's simulation.
        let cap = *cap.get_or_insert_with(|| faced_end(prior, &frame, &sm, cx));
        if cap.is_some() && !hit(cap) {
            continue;
        }
        warns.push(Warning::RapidInStock { at: i });
    }
}

/// The frame read with a SIGNED radius: the facing passes cross the
/// axis, and their sweep reaches the other side of it.
struct SignedMeridian {
    origin: Point3,
    dir: Point3,
    radial: Point3,
}

impl SignedMeridian {
    /// The sign's half plane is the path's own: the first point off
    /// the axis (the kernels walk one plane through it).
    fn of(f: &Frame, path: &Toolpath) -> Self {
        let origin = f.point(0.0, 0.0);
        let dir = f.point(0.0, 1.0) - origin;
        let off = |q: Point3| {
            let d = q - origin;
            let s = d.dot(dir);
            Point3::new(d.x - dir.x * s, d.y - dir.y * s, d.z - dir.z * s)
        };
        let radial = path
            .moves
            .iter()
            .map(|m| off(m.from))
            .find(|w| w.dot(*w) > 1e-12)
            .unwrap_or_else(|| f.point(1.0, 0.0) - origin);
        Self { origin, dir, radial }
    }

    fn sr(&self, q: Point3) -> (f64, f64) {
        let d = q - self.origin;
        let s = d.dot(self.dir);
        let w = Point3::new(d.x - self.dir.x * s, d.y - self.dir.y * s, d.z - self.dir.z * s);
        let r = w.dot(w).sqrt();
        (s, if w.dot(self.radial) < 0.0 { -r } else { r })
    }
}

/// The station the program ahead faced the stock down to, or `None`
/// when nothing ahead cut or it cannot be read (the raw stock stands).
fn faced_end(prior: &mut PriorCell, f: &Frame, sm: &SignedMeridian, cx: &Meridian) -> Option<f64> {
    let dex = prior.get()?;
    let c = sm.origin;
    // The whole face out to the stock's reach: the highest ring stands.
    let reach = cx.stock.unwrap_or(cx.part).iter().flat_map(|t| [t.a, t.b, t.c]).map(|q| f.sr(q).1).fold(0.0f64, f64::max);
    let top = PriorRead { dex, revolved: true }.surface(crate::geo2d::Point2::new(c.x, c.y), reach + cx.nose)?;
    top.is_finite().then(|| f.station_of(top))
}

/// Does the nose disc riding `a` -> `b` (signed meridian points) meet
/// material the cuts before move `i` left standing?
#[allow(clippy::too_many_arguments)]
fn enters(
    a: (f64, f64),
    b: (f64, f64),
    i: usize,
    path: &Toolpath,
    sm: &SignedMeridian,
    part: &Revolved,
    stock: Option<&Revolved>,
    cap: Option<f64>,
    cx: &Meridian,
) -> bool {
    let rn = if cx.nose.is_finite() && cx.nose > 0.0 { cx.nose } else { 0.0 };
    let rd = (rn - cx.margin).max(0.0);
    let band = if cx.band.is_finite() { (cx.band - cx.margin).max(0.0) } else { 0.0 };
    let (rb, reach) = cx
        .body
        .filter(|(r, l)| r.is_finite() && l.is_finite())
        .map_or((0.0, 0.0), |(r, l)| ((r - cx.margin).max(0.0), l.max(0.0)));
    // The body stands up the axis from the nose: the rapid's upper end
    // carries it `reach` further (an axial rapid keeps its radius).
    let (a, b) = if a.0 >= b.0 { ((a.0 + reach, a.1), b) } else { (a, (b.0 + reach, b.1)) };
    let (lo, hi) = (a.0.min(b.0) - rd.max(band), a.0.max(b.0) + rd.max(band));
    // This path's own earlier feeds that can reach the rapid's band.
    let feeds: Vec<((f64, f64), (f64, f64))> = path.moves[..i.min(path.moves.len())]
        .iter()
        // An arc's chord is not its sweep (review S1): a threading
        // half-turn's chord crosses the axis. Arcs credit nothing.
        .filter(|m| m.kind != MoveKind::Rapid && m.arc.is_none())
        .map(|m| (sm.sr(m.from), sm.sr(m.to)))
        .filter(|(p, q)| p.0.max(q.0) + rn >= lo && p.0.min(q.0) - rn <= hi)
        .collect();
    let pilots = pilots(cx.drills, sm);
    let solid = |s: f64, rho: f64| {
        let r = rho.abs();
        let reach = rn + cx.margin;
        if feeds.iter().any(|&(p, q)| seg_dist((s, rho), p, q) <= reach || seg_dist((s, -rho), p, q) <= reach) {
            return false; // this path's own kerf
        }
        if pilots.iter().any(|p| p.removed(s, r)) {
            return false; // a drill ahead's hole
        }
        // The cheap outer reading first: the bodies' ray parity is
        // asked only inside their skins.
        part.solid(s, r, cx.margin)
            || (cap.is_none_or(|c| s < c - cx.margin) && stock.is_some_and(|o| o.solid(s, r, cx.margin)))
    };
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let step = (rn * 0.5).max(0.05).max(len / MAX_SAMPLES);
    let n = (len / step).ceil().max(1.0) as usize;
    let toward = if a.1 < 0.0 { -1.0 } else { 1.0 };
    let ring: Vec<(f64, f64)> = (0..8)
        .map(|k| f64::from(k) * std::f64::consts::FRAC_PI_4)
        .flat_map(|t| [(rd * t.cos(), rd * t.sin()), (0.5 * rd * t.cos(), 0.5 * rd * t.sin())])
        .chain(std::iter::once((0.0, 0.0)))
        .chain(spread(band).map(|ds| (ds, 0.0)))
        // The body toward the axis from the nose (B3): never centred
        // on it - the insert is at the bar's edge.
        .chain(spread(rb * 0.5).map(|k| (0.0, -toward * (k + rb * 0.5))))
        .collect();
    (0..=n).any(|k| {
        let t = k as f64 / n as f64;
        let (s, rho) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        ring.iter().any(|&(ds, dr)| solid(s + ds, rho + dr))
    })
}

/// Offsets across `[-h, h]`, at most 0.25 mm apart, ends included (none
/// for `h` = 0).
fn spread(h: f64) -> impl Iterator<Item = f64> {
    let n = if h > 0.0 { (2.0 * h / 0.25).ceil().min(4000.0) as usize } else { 0 };
    (0..=n).filter(move |_| n > 0).map(move |k| -h + 2.0 * h * k as f64 / n as f64)
}

/// A drill ahead's cut on the spindle axis, in the meridian plane:
/// stations `lo` (the point's tip) to `hi`, radius `r`, the point's
/// cone widening `1/cot` per unit above the tip (#89's drill atom).
struct Pilot {
    lo: f64,
    hi: f64,
    r: f64,
    cot: Option<f64>,
}

impl Pilot {
    fn removed(&self, s: f64, r: f64) -> bool {
        if s < self.lo || s > self.hi {
            return false;
        }
        let wide = match self.cot {
            Some(k) if k > 0.0 => ((s - self.lo) / k).min(self.r),
            _ => self.r,
        };
        r < wide
    }
}

/// The drills' cutting moves that run ON the spindle axis (an off-axis
/// hole on a lathe is not a cylinder about it, and credits nothing).
fn pilots(drills: &[DrillAhead], sm: &SignedMeridian) -> Vec<Pilot> {
    const ON_AXIS: f64 = 1e-3;
    drills
        .iter()
        .filter(|d| d.radius.is_finite() && d.radius > 0.0)
        .flat_map(|d| {
            d.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.arc.is_none()).filter_map(move |m| {
                let (a, b) = (sm.sr(m.from), sm.sr(m.to));
                (a.1.abs() <= ON_AXIS && b.1.abs() <= ON_AXIS)
                    .then(|| Pilot { lo: a.0.min(b.0), hi: a.0.max(b.0), r: d.radius, cot: d.cone })
            })
        })
        .collect()
}

/// Distance from `p` to the segment `a`-`b` in the meridian plane.
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - vx * t).hypot(p.1 - a.1 - vy * t)
}

#[cfg(test)]
#[path = "turning_rapids_tests.rs"]
mod tests;
