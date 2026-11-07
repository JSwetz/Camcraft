//! Issue #103: the turned family's axial rapids judged in the meridian (turning) plane against the revolved stock - the shop rule that a rapid never enters uncut material (machine-tool handbooks) read about the spindle as a body of revolution; our own design, no vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::turning::profile::{silhouette, Frame};
use crate::verify::Warning;

use super::drill_prior::{PriorCell, PriorRead};
use super::kinds::Section;
use super::OpParams;

/// Chord tolerance of the revolved outlines (mm). The outline is read
/// `OUTLINE_TOL` wider than the silhouette says, so a chord below an
/// arc never reads air where steel stands.
const OUTLINE_TOL: f64 = 0.01;
/// Most stations one rapid is sampled at: a longer rapid samples more
/// sparsely, never unboundedly.
const MAX_SAMPLES: f64 = 20_000.0;

/// A revolved outline: the material radius per station.
enum Outline {
    /// The silhouette's ascending (station, radius) polyline.
    Poly(Vec<(f64, f64)>),
    /// No silhouette could be read: the vertices' station span at
    /// their largest radius - exact for a box or bar, never less.
    Span { s0: f64, s1: f64, r: f64 },
}

impl Outline {
    fn of(facets: &[Triangle], f: &Frame) -> Option<Outline> {
        if let Ok(p) = silhouette(facets, f, OUTLINE_TOL)
            && p.len() >= 2
        {
            return Some(Outline::Poly(p));
        }
        let (mut s0, mut s1, mut r) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
        for q in facets.iter().flat_map(|t| [t.a, t.b, t.c]) {
            let (s, rho) = f.sr(q);
            if !(s.is_finite() && rho.is_finite()) {
                return None;
            }
            (s0, s1, r) = (s0.min(s), s1.max(s), r.max(rho));
        }
        (s0 <= s1).then_some(Outline::Span { s0, s1, r })
    }

    /// The material radius at station `s` (0 off the body).
    fn radius(&self, s: f64) -> f64 {
        match self {
            Outline::Span { s0, s1, r } => if (*s0..=*s1).contains(&s) { *r + OUTLINE_TOL } else { 0.0 },
            Outline::Poly(p) => {
                let mut best = 0.0f64;
                for w in p.windows(2) {
                    let ((sa, ra), (sb, rb)) = (w[0], w[1]);
                    if s < sa.min(sb) || s > sa.max(sb) {
                        continue;
                    }
                    let d = sb - sa;
                    let r = if d.abs() <= 1e-12 { ra.max(rb) } else { ra + (rb - ra) * (s - sa) / d };
                    best = best.max(r);
                }
                if best > 0.0 { best + OUTLINE_TOL } else { 0.0 }
            }
        }
    }
}

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
    let part = Outline::of(cx.part, &frame);
    let stock = cx.stock.and_then(|s| Outline::of(s, &frame));
    let Some(part) = part else { return };
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
    part: &Outline,
    stock: Option<&Outline>,
    cap: Option<f64>,
    cx: &Meridian,
) -> bool {
    let rn = if cx.nose.is_finite() && cx.nose > 0.0 { cx.nose } else { 0.0 };
    let rd = (rn - cx.margin).max(0.0);
    let (lo, hi) = (a.0.min(b.0) - rd, a.0.max(b.0) + rd);
    // This path's own earlier feeds that can reach the rapid's band.
    let feeds: Vec<((f64, f64), (f64, f64))> = path.moves[..i.min(path.moves.len())]
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| (sm.sr(m.from), sm.sr(m.to)))
        .filter(|(p, q)| p.0.max(q.0) + rn >= lo && p.0.min(q.0) - rn <= hi)
        .collect();
    let solid = |s: f64, rho: f64| {
        let r = rho.abs();
        let in_part = r < part.radius(s) - cx.margin;
        let in_stock = stock.is_some_and(|o| r < o.radius(s) - cx.margin && cap.is_none_or(|c| s < c - cx.margin));
        if !(in_part || in_stock) {
            return false;
        }
        let reach = rn + cx.margin;
        !feeds.iter().any(|&(p, q)| seg_dist((s, rho), p, q) <= reach || seg_dist((s, -rho), p, q) <= reach)
    };
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let step = (rn * 0.5).max(0.05).max(len / MAX_SAMPLES);
    let n = (len / step).ceil().max(1.0) as usize;
    let ring: Vec<(f64, f64)> = (0..8)
        .map(|k| f64::from(k) * std::f64::consts::FRAC_PI_4)
        .flat_map(|t| [(rd * t.cos(), rd * t.sin()), (0.5 * rd * t.cos(), 0.5 * rd * t.sin())])
        .chain(std::iter::once((0.0, 0.0)))
        .collect();
    (0..=n).any(|k| {
        let t = k as f64 / n as f64;
        let (s, rho) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        ring.iter().any(|&(ds, dr)| solid(s + ds, rho + dr))
    })
}

/// Distance from `p` to the segment `a`-`b` in the meridian plane.
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - vx * t).hypot(p.1 - a.1 - vy * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meridian_distance_is_to_the_segment_not_the_line() {
        assert!((seg_dist((0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)) - 1.0).abs() < 1e-12);
        assert!((seg_dist((3.0, 0.0), (-1.0, 0.0), (1.0, 0.0)) - 2.0).abs() < 1e-12);
        assert!((seg_dist((2.0, 2.0), (0.0, 0.0), (0.0, 0.0)) - 8f64.sqrt()).abs() < 1e-12);
    }

    fn cylinder(r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
        let v = |j: usize, r: f64, z: f64| {
            let th = std::f64::consts::TAU * j as f64 / 32.0;
            Point3::new(r * th.cos(), r * th.sin(), z)
        };
        let mut f = Vec::new();
        for j in 0..32 {
            let (a, b, c, d) = (v(j, r, z0), v(j + 1, r, z0), v(j, r, z1), v(j + 1, r, z1));
            f.push(Triangle::new(a, b, d));
            f.push(Triangle::new(a, d, c));
            f.push(Triangle::new(Point3::new(0.0, 0.0, z0), b, a));
            f.push(Triangle::new(Point3::new(0.0, 0.0, z1), c, d));
        }
        f
    }

    fn mv(kind: MoveKind, from: Point3, to: Point3) -> crate::cl::Move {
        crate::cl::Move { kind, from, to, feed: (kind != MoveKind::Rapid).then_some(100.0), arc: None, axis: None }
    }

    /// The axial rapids of `moves` the meridian law convicts, a 12 mm
    /// part in a 20 mm bar from Z -16 to 16, a 0.8 nose.
    fn convicted(moves: Vec<crate::cl::Move>) -> Vec<Warning> {
        let params = OpParams::ProfileRough {
            tolerance: 0.05,
            clearance_z: crate::operation::Height::fixed(25.0),
            top_z: crate::operation::Height::fixed(16.0),
            bottom_z: crate::operation::Height::fixed(-16.0),
            stepdown: 0.8,
            stock_to_leave: 0.25,
            feed: 150.0,
            plunge_feed: 80.0,
        };
        let path = Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new() };
        let (part, stock) = (cylinder(6.0, -16.0, 16.0), cylinder(10.0, -16.0, 16.0));
        let cx = Meridian { part: &part, stock: Some(&stock), nose: 0.8, margin: 0.01, z_tol: 1e-6 };
        let mut warns = Vec::new();
        judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
        warns
    }

    #[test]
    fn an_axial_rapid_is_judged_in_the_meridian_against_the_revolved_bar() {
        let p = Point3::new;
        // Into the bar at r 8, before any cut: convicted by index.
        assert_eq!(convicted(vec![mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, 5.0))]), vec![Warning::RapidInStock { at: 0 }]);
        // Beside the bar (r 10.9, the nose clear of r 10): clean.
        assert!(convicted(vec![mv(MoveKind::Rapid, p(10.9, 0.0, 25.0), p(10.9, 0.0, 5.0))]).is_empty());
        // A pass turned r 8 down to Z 0 from beside the bar's end; the
        // retract back up along that turned diameter is in its own air.
        let turned = vec![
            mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, 17.0)),
            mv(MoveKind::Feed, p(8.0, 0.0, 17.0), p(8.0, 0.0, 0.0)),
            mv(MoveKind::Rapid, p(8.0, 0.0, 0.0), p(8.0, 0.0, 25.0)),
            // ...but a rapid down past that pass's floor is not.
            mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, -4.0)),
        ];
        assert_eq!(convicted(turned), vec![Warning::RapidInStock { at: 3 }]);
    }

    #[test]
    fn a_span_outline_reads_the_stock_its_full_radius_inside_its_stations_and_air_off_them() {
        let o = Outline::Span { s0: 0.0, s1: 10.0, r: 20.0 };
        assert!(o.radius(5.0) >= 20.0);
        assert_eq!(o.radius(11.0), 0.0);
    }
}
