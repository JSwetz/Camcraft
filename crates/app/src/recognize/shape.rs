//! The shape read of a walled floor (#35): closed or open pocket, the
//! tightest inside corner, the open spans, and whether it is a slot - a
//! straight channel of constant width, fitted by the minimum-area
//! enclosing rectangle over the ring's edge directions (Freeman &
//! Shapira, "Determining the minimum-area encasing rectangle for an
//! arbitrary closed curve", CACM 18(7), 1975: an optimal rectangle has a
//! side collinear with an edge of the convex hull).
//!
//! Provenance: our own code from the cited literature and handbook slot /
//! pocket definitions; no vendor material consulted.

use super::walls::Floor;
use super::{Pocket, PocketKind, SlotEnd, SlotShape};
use crate::geo2d::Point2;
use crate::geometry::feature::FaceId;

/// Two wall directions this opposed (cosine) face each other across the
/// floor - the "walls on opposite sides" test of a pocket.
const OPPOSED: f64 = -0.9;
/// A turn below this is a collinear split, not a corner (radians, 1°).
const MIN_TURN: f64 = std::f64::consts::PI / 180.0;
/// A turn above this is a sharp corner, not an arc's tessellation (60°).
const SHARP_TURN: f64 = std::f64::consts::FRAC_PI_3;
/// A slot is at least this many widths long.
const SLOT_ASPECT: f64 = 2.0;
/// Relative slack between the floor's area and the slot shape's.
const SLOT_AREA_TOL: f64 = 0.03;

/// Shoelace area, counter-clockwise positive.
pub(super) fn signed_area(pts: &[Point2]) -> f64 {
    let n = pts.len();
    let mut s = 0.0;
    for i in 0..n {
        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
        s += a.x * b.y - b.x * a.y;
    }
    0.5 * s
}

fn sub(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x - b.x, a.y - b.y)
}

fn dot(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn cross(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// The floor read as a pocket or slot; None when its walls do not stand
/// on opposite sides (a shoulder, a corner step, a plain top face).
pub(super) fn classify(model: &str, floor: FaceId, f: Floor, islands: usize) -> Option<Pocket> {
    let n = f.ring.len();
    let closed = f.wall.iter().all(|w| *w);
    if !closed && !opposed_walls(&f) {
        return None;
    }
    let slot = slot_of(&f, islands);
    let kind = match (slot.is_some(), closed) {
        (true, _) => PocketKind::Slot,
        (false, true) => PocketKind::Closed,
        (false, false) => PocketKind::Open,
    };
    let open_spans = open_spans(&f);
    let min_corner_radius = min_corner(&f);
    let depth = f.top - f.z;
    if n < 3 || !depth.is_finite() {
        return None;
    }
    Some(Pocket {
        model: model.to_owned(),
        floor,
        kind,
        boundary: f.ring,
        islands,
        floor_z: f.z,
        top_z: f.top,
        depth,
        area: f.area,
        min_corner_radius,
        open_spans,
        slot,
    })
}

/// The inward direction of span `i` (left of a counter-clockwise ring).
fn inward(f: &Floor, i: usize) -> Option<Point2> {
    let n = f.ring.len();
    let d = sub(*f.ring.get((i + 1) % n)?, *f.ring.get(i)?);
    let len = d.x.hypot(d.y);
    (len > 0.0 && len.is_finite()).then(|| Point2::new(-d.y / len, d.x / len))
}

fn opposed_walls(f: &Floor) -> bool {
    let dirs: Vec<Point2> = (0..f.ring.len()).filter(|&i| f.wall.get(i) == Some(&true)).filter_map(|i| inward(f, i)).collect();
    dirs.iter().any(|a| dirs.iter().any(|b| dot(*a, *b) < OPPOSED))
}

/// Each run of wall-less spans, as the segment from its first vertex to
/// its last.
fn open_spans(f: &Floor) -> Vec<[Point2; 2]> {
    let n = f.ring.len();
    let Some(start) = (0..n).find(|&i| f.wall.get(i) == Some(&true)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for k in 1..=n {
        let i = (start + k) % n;
        let open = f.wall.get(i) == Some(&false);
        match (open, run) {
            (true, None) => run = Some(i),
            (false, Some(s)) => {
                if let (Some(a), Some(b)) = (f.ring.get(s), f.ring.get(i)) {
                    out.push([*a, *b]);
                }
                run = None;
            }
            _ => {}
        }
    }
    out
}

/// The tightest corner where two walls meet: the circumradius of an
/// arc's tessellation, 0 for a sharp corner. Only left turns of the
/// counter-clockwise ring are inside corners a cutter must fit.
fn min_corner(f: &Floor) -> Option<f64> {
    let n = f.ring.len();
    let mut best: Option<f64> = None;
    for i in 0..n {
        let prev = (i + n - 1) % n;
        if f.wall.get(prev) != Some(&true) || f.wall.get(i) != Some(&true) {
            continue;
        }
        let (Some(&a), Some(&b), Some(&c)) = (f.ring.get(prev), f.ring.get(i), f.ring.get((i + 1) % n)) else {
            continue;
        };
        let (e1, e2) = (sub(b, a), sub(c, b));
        let turn = cross(e1, e2).atan2(dot(e1, e2));
        if !turn.is_finite() || turn < MIN_TURN {
            continue;
        }
        let r = if turn > SHARP_TURN {
            0.0
        } else {
            let chord = sub(c, a);
            e1.x.hypot(e1.y) * e2.x.hypot(e2.y) * chord.x.hypot(chord.y) / (2.0 * cross(e1, e2).abs())
        };
        if r.is_finite() {
            best = Some(best.map_or(r, |b| b.min(r)));
        }
    }
    best
}

/// The minimum-area enclosing rectangle over the ring's edge
/// directions: centre, long-axis unit direction, length, width.
fn min_rect(pts: &[Point2]) -> Option<(Point2, Point2, f64, f64)> {
    let n = pts.len();
    let mut best: Option<(f64, Point2, Point2, f64, f64)> = None;
    for i in 0..n {
        let (Some(&a), Some(&b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
        let d = sub(b, a);
        let len = d.x.hypot(d.y);
        if !(len > 1e-9 && len.is_finite()) {
            continue;
        }
        let u = Point2::new(d.x / len, d.y / len);
        let v = Point2::new(-u.y, u.x);
        let (mut u0, mut u1, mut v0, mut v1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for p in pts {
            let (pu, pv) = (dot(*p, u), dot(*p, v));
            u0 = u0.min(pu);
            u1 = u1.max(pu);
            v0 = v0.min(pv);
            v1 = v1.max(pv);
        }
        let area = (u1 - u0) * (v1 - v0);
        if !area.is_finite() || best.as_ref().is_some_and(|b| b.0 <= area) {
            continue;
        }
        let (cu, cv) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        let c = Point2::new(u.x * cu + v.x * cv, u.y * cu + v.y * cv);
        let (lu, lv) = (u1 - u0, v1 - v0);
        best = Some(if lu >= lv { (area, c, u, lu, lv) } else { (area, c, v, lv, lu) });
    }
    best.map(|(_, c, u, l, w)| (c, u, l, w))
}

/// The floor as a straight channel: both long sides walled along their
/// whole length, each end wholly walled (round or square) or wholly
/// open, no islands, and an area that matches the shape.
fn slot_of(f: &Floor, islands: usize) -> Option<SlotShape> {
    if islands > 0 {
        return None;
    }
    let (c, u, l, w) = min_rect(&f.ring)?;
    if !(w > 1e-3 && l >= SLOT_ASPECT * w) {
        return None;
    }
    let v = Point2::new(-u.y, u.x);
    let tol = (1e-3 * w).max(1e-3);
    let n = f.ring.len();
    let side = |p: Point2| {
        let pv = dot(sub(p, c), v);
        ((pv.abs() - 0.5 * w).abs() <= tol).then_some(pv.signum())
    };
    // Per end (0 = -u, 1 = +u): its span walls and its vertices.
    let mut walls: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
    let mut verts: [Vec<Point2>; 2] = [Vec::new(), Vec::new()];
    for i in 0..n {
        let (&p, &q, &is_wall) = (f.ring.get(i)?, f.ring.get((i + 1) % n)?, f.wall.get(i)?);
        match (side(p), side(q)) {
            (Some(sp), Some(sq)) if sp == sq => {
                if !is_wall {
                    return None;
                }
            }
            _ => {
                let mid = Point2::new(0.5 * (p.x + q.x), 0.5 * (p.y + q.y));
                let e = usize::from(dot(sub(mid, c), u) > 0.0);
                walls.get_mut(e)?.push(is_wall);
                verts.get_mut(e)?.extend([p, q]);
            }
        }
    }
    let mut ends = [SlotEnd::Open; 2];
    for e in 0..2 {
        let ws = walls.get(e)?;
        if ws.is_empty() {
            return None;
        }
        let sign = if e == 0 { -1.0 } else { 1.0 };
        let cap = Point2::new(c.x + u.x * sign * 0.5 * (l - w), c.y + u.y * sign * 0.5 * (l - w));
        let round_tol = (0.02 * w).max(0.01);
        let end = if ws.iter().all(|x| !*x) {
            SlotEnd::Open
        } else if ws.iter().all(|x| *x) {
            let round = verts.get(e)?.iter().all(|p| (sub(*p, cap).x.hypot(sub(*p, cap).y) - 0.5 * w).abs() <= round_tol);
            if round { SlotEnd::Round } else { SlotEnd::Square }
        } else {
            return None;
        };
        *ends.get_mut(e)? = end;
    }
    let rounds = ends.iter().filter(|e| **e == SlotEnd::Round).count() as f64;
    // A round end trades a w/2 x w corner block for a semicircle.
    let expected = l * w - rounds * w * w * (0.5 - std::f64::consts::FRAC_PI_8);
    let actual = signed_area(&f.ring).abs();
    if !(expected > 0.0 && (actual - expected).abs() <= SLOT_AREA_TOL * expected) {
        return None;
    }
    let inset = |e: SlotEnd| if e == SlotEnd::Round { 0.5 * w } else { 0.0 };
    let (ha, hb) = (0.5 * l - inset(ends[0]), 0.5 * l - inset(ends[1]));
    Some(SlotShape {
        a: Point2::new(c.x - u.x * ha, c.y - u.y * ha),
        b: Point2::new(c.x + u.x * hb, c.y + u.y * hb),
        width: w,
        length: l,
        ends,
    })
}
