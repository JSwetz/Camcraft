//! Issue #103: a body read as a solid of revolution per station - its material an annulus [r_in, r_out] about the spindle, the outer radius from the turned silhouette and the inner one by ray parity (Moller-Trumbore ray/triangle intersection, 1997, and the even-odd point-in-solid rule); our own design, no vendor material consulted.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};
use crate::turning::profile::{silhouette, Frame};

/// Chord tolerance of the revolved outlines (mm). The outline is read
/// `OUTLINE_TOL` wider than the silhouette says, so a chord below an
/// arc never reads air where steel stands.
const OUTLINE_TOL: f64 = 0.01;
/// The inner reading is taken this far either side of the station and
/// the more solid side kept: a ray cast exactly in an end face's plane
/// grazes its triangles edge-on and counts nothing honestly.
const NUDGE: f64 = 1e-6;
/// Ray directions about the axis, off every rational slice angle a
/// tessellated revolve puts its seams on (radians).
const ANGLES: [f64; 4] = [
    0.37,
    0.37 + std::f64::consts::FRAC_PI_2,
    0.37 + std::f64::consts::PI,
    0.37 + 3.0 * std::f64::consts::FRAC_PI_2,
];

/// The outer radius per station.
enum Outer {
    /// The silhouette's ascending (station, radius) polyline.
    Poly(Vec<(f64, f64)>),
    /// No silhouette could be read: the vertices' station span at
    /// their largest radius - exact for a box or bar, never less.
    Span { s0: f64, s1: f64, r: f64 },
}

impl Outer {
    fn radius(&self, s: f64) -> f64 {
        match self {
            Outer::Span { s0, s1, r } => if (*s0..=*s1).contains(&s) { *r + OUTLINE_TOL } else { 0.0 },
            Outer::Poly(p) => {
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

/// One body - the part or the stock - read about the spindle.
pub(super) struct Revolved<'a> {
    outer: Outer,
    facets: &'a [Triangle],
    /// Each facet's station span, so a ray reads only the facets its
    /// station crosses.
    spans: Vec<(f64, f64)>,
    origin: Point3,
    dir: Point3,
    /// Two unit radials spanning the plane normal to the axis.
    e1: Point3,
    e2: Point3,
    inner: RefCell<HashMap<u64, f64>>,
}

impl<'a> Revolved<'a> {
    /// `None` when the body has a non-finite vertex (unknown: the
    /// caller keeps the column's convictions).
    pub(super) fn of(facets: &'a [Triangle], f: &Frame) -> Option<Self> {
        let origin = f.point(0.0, 0.0);
        let dir = f.point(0.0, 1.0) - origin;
        let e1 = f.point(1.0, 0.0) - origin;
        let e2 = dir.cross(e1);
        let mut spans = Vec::with_capacity(facets.len());
        let (mut s0, mut s1, mut r) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
        for t in facets {
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for q in [t.a, t.b, t.c] {
                let (s, rho) = f.sr(q);
                if !(s.is_finite() && rho.is_finite()) {
                    return None;
                }
                (lo, hi) = (lo.min(s), hi.max(s));
                (s0, s1, r) = (s0.min(s), s1.max(s), r.max(rho));
            }
            spans.push((lo, hi));
        }
        if s0 > s1 {
            return None;
        }
        let outer = match silhouette(facets, f, OUTLINE_TOL) {
            Ok(p) if p.len() >= 2 => Outer::Poly(p),
            _ => Outer::Span { s0, s1, r },
        };
        Some(Self { outer, facets, spans, origin, dir, e1, e2, inner: RefCell::new(HashMap::new()) })
    }

    /// Is there material at station `s`, radius `r` (>= 0), by more
    /// than `margin`? The annulus between the hole and the skin.
    pub(super) fn solid(&self, s: f64, r: f64, margin: f64) -> bool {
        r < self.outer.radius(s) - margin && {
            let inner = self.inner(s);
            inner <= 0.0 || r > inner + margin // a solid core reaches the axis
        }
    }

    /// The stations the outer skin bends at (#369): between two of
    /// them it runs straight, so a reading at both ends reads its
    /// extremes.
    pub(super) fn knots(&self) -> Vec<f64> {
        match &self.outer {
            Outer::Poly(p) => p.iter().map(|q| q.0).collect(),
            Outer::Span { s0, s1, .. } => vec![*s0, *s1],
        }
    }

    /// The material annulus at station `s`, unshrunk (#369): the hole's
    /// radius ([`Self::inner`]: 0 for a solid core, `INFINITY` with no
    /// wall) and the skin's (0 where the body has no material).
    pub(super) fn annulus(&self, s: f64) -> (f64, f64) {
        (self.inner(s), self.outer.radius(s))
    }

    /// The hole's radius at station `s`: 0 when the axis is inside the
    /// body (an odd ray parity on either side of the station, in any
    /// direction), else the nearest wall any ray meets (`INFINITY`
    /// when none meets one - no material on that station).
    fn inner(&self, s: f64) -> f64 {
        if let Some(r) = self.inner.borrow().get(&s.to_bits()) {
            return *r;
        }
        let mut best = f64::INFINITY;
        'sides: for s in [s - NUDGE, s + NUDGE] {
            let o = Point3::new(self.origin.x + self.dir.x * s, self.origin.y + self.dir.y * s, self.origin.z + self.dir.z * s);
            for a in ANGLES {
                let (c, n) = (a.cos(), a.sin());
                let d = Point3::new(self.e1.x * c + self.e2.x * n, self.e1.y * c + self.e2.y * n, self.e1.z * c + self.e2.z * n);
                let (mut n, mut near) = (0usize, f64::INFINITY);
                for (t, &(lo, hi)) in self.facets.iter().zip(&self.spans) {
                    if s < lo || s > hi {
                        continue;
                    }
                    if let Some(h) = hit(o, d, t) {
                        n += 1;
                        near = near.min(h);
                    }
                }
                if n % 2 == 1 {
                    best = 0.0;
                    break 'sides;
                }
                best = best.min(near);
            }
        }
        self.inner.borrow_mut().insert(s.to_bits(), best);
        best
    }
}

/// The distance along unit `d` from `o` to triangle `t`, if the ray
/// meets it (Moller-Trumbore).
fn hit(o: Point3, d: Point3, t: &Triangle) -> Option<f64> {
    let (e1, e2) = (t.b - t.a, t.c - t.a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let tv = o - t.a;
    let u = tv.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = tv.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let h = e2.dot(q) * inv;
    (h > 1e-12).then_some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed revolve of the meridian polygon `m`, 16 ways.
    fn revolve(m: &[(f64, f64)]) -> Vec<Triangle> {
        let v = |j: usize, r: f64, z: f64| {
            let th = std::f64::consts::TAU * j as f64 / 16.0;
            Point3::new(r * th.cos(), r * th.sin(), z)
        };
        let mut f = Vec::new();
        for w in m.windows(2) {
            let ((ra, za), (rb, zb)) = (w[0], w[1]);
            for j in 0..16 {
                let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
                if ra > 1e-12 {
                    f.push(Triangle::new(a1, a2, b2));
                }
                if rb > 1e-12 {
                    f.push(Triangle::new(a1, b2, b1));
                }
            }
        }
        f
    }

    #[test]
    fn a_bored_bar_reads_its_hole_empty_and_its_floor_and_wall_solid() {
        // r 10 bar from -50 to 50, a r 6 hole from the top down to -20.
        let bar = revolve(&[(0.0, -50.0), (10.0, -50.0), (10.0, 50.0), (6.0, 50.0), (6.0, -20.0), (0.0, -20.0)]);
        let solid = revolve(&[(0.0, -50.0), (10.0, -50.0), (10.0, 50.0), (0.0, 50.0)]);
        let f = Frame::about(&solid).expect("a revolve");
        let b = Revolved::of(&bar, &f).expect("readable");
        assert!(!b.solid(f.station_of(0.0), 0.0, 0.01), "the hole is air");
        assert!(!b.solid(f.station_of(50.0), 3.0, 0.01), "the hole's mouth is air");
        assert!(b.solid(f.station_of(0.0), 8.0, 0.01), "the wall is steel");
        assert!(b.solid(f.station_of(-30.0), 0.0, 0.01), "under the floor is steel");
        let s = Revolved::of(&solid, &f).expect("readable");
        assert!(s.solid(f.station_of(0.0), 0.0, 0.01), "a bar with no bore is solid to the axis");
        assert!(!s.solid(f.station_of(60.0), 0.0, 0.01), "past its end is air");
    }
}
