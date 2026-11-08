//! Where a cross hole's drill first meets the raw stock (issue #85):
//! along the hole's own line, against the stock mesh, rather than at
//! the stock's circumscribed radius - a box counts at its face, not its
//! corner. The body is read as the square tube of half-width `half`
//! about the line (across it in XY, and in Z): a superset of the round
//! drill, so the outermost stock point inside it is no nearer the axis
//! than the first point the real body touches - the entry stays
//! conservative.
//!
//! A tessellated round's facets are chords lying INSIDE the true
//! surface, so each facet is read as the slab its true surface can
//! occupy: thickened by its sagitta allowance either side (from the
//! dihedral angle to its edge neighbours and its width across that
//! edge, `sag_of`). A facet meeting its neighbours only at sharp edges
//! (a box face, a hex flat) or flat is read exactly.
//!
//! Issue #119: the same reading, over a rectangular tube, is the cross
//! drill's holder footprint on one station (`tube_reach`).
//!
//! Provenance: Sutherland-Hodgman polygon clipping against slab
//! half-spaces, the first-contact point of a prism swept along its
//! axis, and the chord sagitta s = (w/2) tan(theta/4) of an inscribed
//! polygon are textbook computational geometry. Our own design notes;
//! no vendor material consulted.

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

/// A dihedral angle above this between edge neighbours is a real edge
/// (a box's 90, a hex bar's 60 degrees), not a tessellated round's
/// crease; at or below it the facet carries a sagitta allowance (an
/// octagon reads as a coarse round: air, never metal).
const SHARP: f64 = std::f64::consts::FRAC_PI_4;

/// Stock facets in the spindle frame, each with the sagitta allowance
/// its true surface may stand beyond the facet plane (0: read exactly).
#[derive(Default)]
pub(super) struct StockMesh {
    tris: Vec<Triangle>,
    sag: Vec<f64>,
}

impl StockMesh {
    pub fn new(tris: Vec<Triangle>) -> Self {
        let sag = sag_of(&tris);
        StockMesh { tris, sag }
    }

    /// Add another solid's facets (their allowances read on their own:
    /// two solids' coincident edges are no crease of either).
    pub fn extend(&mut self, other: StockMesh) {
        self.tris.extend(other.tris);
        self.sag.extend(other.sag);
    }
}

fn unit_normal(t: &Triangle) -> Option<Point3> {
    let n = (t.b - t.a).cross(t.c - t.a);
    let l = n.dot(n).sqrt();
    (l.is_finite() && l > 1e-12).then(|| Point3::new(n.x / l, n.y / l, n.z / l))
}

/// Distance of `p` from the line through `a` and `b`.
fn off_line(p: Point3, a: Point3, b: Point3) -> f64 {
    let e = b - a;
    let l = e.dot(e).sqrt();
    if !(l.is_finite() && l > 1e-12) {
        return 0.0;
    }
    let c = (p - a).cross(e);
    c.dot(c).sqrt() / l
}

/// An edge by its two quantised (1e-6 mm) endpoints, smaller first.
type EdgeKey = ([i64; 3], [i64; 3]);

/// Each facet's sagitta allowance. Across an edge shared with a facet
/// whose normal turns by `theta` (0 < theta <= SHARP; |cos| read, so a
/// flipped winding never hides a crease), the true round through the
/// shared vertices bulges at most `(h/2) tan(theta/2)` past the facet,
/// `h` the facet's width across that edge - the inscribed polygon's
/// `(h/2) tan(theta/4)` doubled, which stays an upper bound however
/// narrow the neighbour is. A facet's allowance is its largest.
fn sag_of(tris: &[Triangle]) -> Vec<f64> {
    let q = |p: Point3| [p.x, p.y, p.z].map(|c| (c * 1e6).round() as i64);
    let normals: Vec<Option<Point3>> = tris.iter().map(unit_normal).collect();
    let mut edges: HashMap<EdgeKey, Vec<(usize, usize)>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        if normals[i].is_none() || !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
            continue;
        }
        let v = [t.a, t.b, t.c];
        for k in 0..3 {
            let (a, b) = (q(v[k]), q(v[(k + 1) % 3]));
            edges.entry(if a <= b { (a, b) } else { (b, a) }).or_default().push((i, k));
        }
    }
    let mut sag = vec![0.0_f64; tris.len()];
    let width = |i: usize, k: usize| {
        let v = [tris[i].a, tris[i].b, tris[i].c];
        off_line(v[(k + 2) % 3], v[k], v[(k + 1) % 3])
    };
    for side in edges.values() {
        for (x, &(i, ki)) in side.iter().enumerate() {
            for &(j, kj) in &side[x + 1..] {
                let (Some(ni), Some(nj)) = (normals[i], normals[j]) else { continue };
                let theta = ni.dot(nj).abs().min(1.0).acos();
                if !(theta > 1e-9 && theta <= SHARP) {
                    continue;
                }
                let tn = (0.5 * theta).tan();
                sag[i] = sag[i].max(0.5 * width(i, ki) * tn);
                sag[j] = sag[j].max(0.5 * width(j, kj) * tn);
            }
        }
    }
    sag
}

/// A hole line in the spindle frame: `a` the XY axis it plunges on
/// (0 X, 1 Y), `out` the outward sign along it (+1: outward is +a), and
/// the line's transverse XY coordinate `t` and its Z.
#[derive(Clone, Copy)]
pub(super) struct Line {
    pub a: usize,
    pub out: f64,
    pub t: f64,
    pub z: f64,
}

impl Line {
    /// The outward coordinate of a point along this line.
    pub fn u(&self, p: Point3) -> f64 {
        self.out * if self.a == 0 { p.x } else { p.y }
    }
    fn w(&self, p: Point3) -> f64 {
        if self.a == 0 { p.y } else { p.x }
    }
}

/// A rectangular tube along a hole line, unbounded along it: `hw`
/// either side of the line's transverse coordinate `line.t` across it
/// in XY, `hz` either side of `line.z` in Z. A body's square tube is
/// `hw == hz`, its half-width; a holder sliding across its line or in
/// Z on one station (issue #119) widens the side it sweeps.
#[derive(Clone, Copy)]
pub(super) struct Tube {
    pub line: Line,
    pub hw: f64,
    pub hz: f64,
}

/// The outermost outward coordinate of the planar polygon `pts` inside
/// `tube`.
fn poly_max(pts: &[Point3], tube: Tube) -> Option<f64> {
    let line = tube.line;
    let mut poly: Vec<(f64, f64, f64)> =
        pts.iter().filter(|p| p.is_finite()).map(|&p| (line.u(p), line.w(p) - line.t, p.z - line.z)).collect();
    if poly.len() < 3 {
        return None;
    }
    // Four slabs: |w| <= hw and |z| <= hz, each as two halves
    // (`k` picks w or z, `sg` the side).
    for (k, sg) in [(1, 1.0), (1, -1.0), (2, 1.0), (2, -1.0)] {
        poly = clip(&poly, |p| if k == 1 { tube.hw - sg * p.1 } else { tube.hz - sg * p.2 });
        if poly.is_empty() {
            return None;
        }
    }
    poly.iter().map(|p| p.0).reduce(f64::max)
}

/// The outermost outward coordinate of any stock facet - a curved one
/// as its sagitta slab - inside `tube`, or `None` when the tube meets
/// no facet. A slab's
/// outermost point in the tube lies on one of its five faces (the tube
/// is unbounded along the line), so clipping them suffices.
fn tube_max(mesh: &StockMesh, tube: Tube) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut take = |m: Option<f64>| {
        if let Some(m) = m {
            best = Some(best.map_or(m, |b: f64| b.max(m)));
        }
    };
    for (tri, &s) in mesh.tris.iter().zip(&mesh.sag) {
        let v = [tri.a, tri.b, tri.c];
        let n = if s > 0.0 && s.is_finite() { unit_normal(tri) } else { None };
        let Some(n) = n else {
            take(poly_max(&v, tube));
            continue;
        };
        let off = |p: Point3, k: f64| Point3::new(p.x + k * n.x, p.y + k * n.y, p.z + k * n.z);
        let (lo, hi) = (v.map(|p| off(p, -s)), v.map(|p| off(p, s)));
        take(poly_max(&lo, tube));
        take(poly_max(&hi, tube));
        for k in 0..3 {
            let j = (k + 1) % 3;
            take(poly_max(&[lo[k], lo[j], hi[j], hi[k]], tube));
        }
    }
    best
}

/// Keep the part of polygon `poly` where `inside(p) >= 0`.
fn clip(poly: &[(f64, f64, f64)], inside: impl Fn(&(f64, f64, f64)) -> f64) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (dp, dq) = (inside(&p), inside(&q));
        if dp >= 0.0 {
            out.push(p);
        }
        if (dp >= 0.0) != (dq >= 0.0) {
            let s = dp / (dp - dq);
            if s.is_finite() {
                out.push((p.0 + s * (q.0 - p.0), p.1 + s * (q.1 - p.1), p.2 + s * (q.2 - p.2)));
            }
        }
    }
    out
}

/// The outward coordinate where a body of half-width `half` on `line`
/// first meets the raw stock: the stock mesh's own crossing, or - when
/// the mesh stands no further inside the stock's circumscribed radius
/// `raw` than a round bar's tessellation does - the circumscribed
/// reading (#77's, kept bit for bit for a round bar). The mesh is read
/// when the edge point nearest the axis at its crossing stands more
/// than `raw (1 - cos(pi/16))` inside `raw`: a flat face, not the
/// chords of a turned bar (whose facets lie inside the true round, so
/// trusting them there would put the mouth inside the real bar). A
/// tube that meets no facet falls back to the circumscribed reading, as
/// before #85. `None`: the body misses the stock's radius on this line.
/// A round well inside `raw` (a stepped bar's smaller section, an
/// off-axis cylinder, a boss) is read at its sagitta slabs, so the
/// entry is never nearer the axis than its true curved surface.
pub(super) fn entry(mesh: &StockMesh, raw: f64, line: Line, half: f64) -> Option<f64> {
    let edge = (line.t.abs() - half).max(0.0);
    let circ = (edge < raw).then(|| (raw * raw - edge * edge).max(0.0).sqrt())?;
    let Some(m) = tube_max(mesh, Tube { line, hw: half, hz: half }) else { return Some(circ) };
    Some(face_or_circ(m, circ, raw, edge))
}

/// The mesh's crossing `m` when it is a flat (more than a turned bar's
/// tessellation inside `raw` at the edge offset `edge`), else the
/// circumscribed reading `circ` - `entry`'s choice.
fn face_or_circ(m: f64, circ: f64, raw: f64, edge: f64) -> f64 {
    let band = raw * (1.0 - (std::f64::consts::PI / 16.0).cos());
    let flat = m.is_finite() && m < circ && raw - m.hypot(edge) > band;
    if flat { m } else { circ }
}

/// The outermost outward coordinate of material inside `tube` (issue
/// #119: a holder's footprint on one station), read as `entry` reads a
/// body's: a flat at its face, a turned round at its circumscribed
/// reading at the tube's edge nearest the axis. `None` when the tube
/// meets no facet - the tube is unbounded along its line, so a solid
/// it entered would cross its wall: nothing is there. A non-finite
/// reading is `NaN`, which no holder clears.
pub(super) fn tube_reach(mesh: &StockMesh, raw: f64, tube: Tube) -> Option<f64> {
    let m = tube_max(mesh, tube)?;
    if !(m.is_finite() && raw.is_finite()) {
        return Some(f64::NAN);
    }
    let edge = (tube.line.t.abs() - tube.hw).max(0.0);
    // A crossing outside the circumscribed circle (only by rounding)
    // stands as read.
    if edge >= raw {
        return Some(m);
    }
    Some(face_or_circ(m, (raw * raw - edge * edge).max(0.0).sqrt(), raw, edge))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cube;

    fn line(t: f64) -> Line {
        Line { a: 0, out: 1.0, t, z: 0.0 }
    }

    #[test]
    fn a_box_face_is_the_entry_and_a_miss_falls_back_to_the_corner_circle() {
        let b = StockMesh::new(cube(Point3::new(-30.0, -30.0, -20.0), Point3::new(30.0, 30.0, 20.0)).facets.to_vec());
        let raw = 1800.0_f64.sqrt();
        assert_eq!(entry(&b, raw, line(0.0), 2.0), Some(30.0));
        assert_eq!(entry(&b, raw, line(7.0), 3.0), Some(30.0));
        // Above the box: no crossing, the circumscribed reading stands.
        let above = Line { z: 50.0, ..line(0.0) };
        assert_eq!(entry(&b, raw, above, 2.0), Some(raw));
        assert_eq!(entry(&b, raw, line(raw + 3.0), 2.0), None);
    }

    /// A closed `n`-gon prism of circumradius `r` about `(cx, cy)`, z in
    /// `[z0, z1]`, its vertices at `phase + k 2pi/n`.
    fn prism(n: usize, r: f64, (cx, cy): (f64, f64), (z0, z1): (f64, f64), phase: f64) -> Vec<Triangle> {
        let v = |k: usize, z: f64| {
            let th = phase + std::f64::consts::TAU * k as f64 / n as f64;
            Point3::new(cx + r * th.cos(), cy + r * th.sin(), z)
        };
        let mut f = Vec::new();
        for k in 0..n {
            let (a1, a2, b1, b2) = (v(k, z0), v(k + 1, z0), v(k, z1), v(k + 1, z1));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
            f.push(Triangle::new(Point3::new(cx, cy, z0), a2, a1));
            f.push(Triangle::new(Point3::new(cx, cy, z1), b1, b2));
        }
        f
    }

    /// A d40 section (z -20..0) stepping up to d60 (z 0..20): the
    /// global raw radius is the d60's r30, but a hole on the d40 section
    /// meets the d40's true round. Drill d4 with its edge at w 19.5: the
    /// true entry is sqrt(20^2 - 19.5^2) = 4.44 - the bare chords read
    /// ~3.99 (32 facets) and ~2.13 (16), the mouth inside metal.
    #[test]
    fn a_stepped_bars_small_section_is_entered_at_or_outside_its_true_round() {
        let truth = (400.0_f64 - 19.5 * 19.5).sqrt();
        for n in [32, 16] {
            for phase in [0.0, std::f64::consts::PI / n as f64] {
                let mut tris = prism(n, 20.0, (0.0, 0.0), (-20.0, 0.0), phase);
                tris.extend(prism(n, 30.0, (0.0, 0.0), (0.0, 20.0), phase));
                let st = StockMesh::new(tris);
                let raw = 30.0;
                let at = |t: f64| entry(&st, raw, Line { a: 0, out: 1.0, t, z: -10.0 }, 2.0);
                let got = at(21.5).unwrap_or(f64::NAN);
                assert!(got >= truth - 1e-9, "{n} facets, phase {phase}: entry {got:.3} inside the true d40 ({truth:.3})");
                assert!(got < 22.0, "{n} facets: the d40 section is read, not the d60 corner circle: {got:.3}");
                let mid = at(0.0).unwrap_or(f64::NAN);
                assert!((20.0 - 1e-9..21.0).contains(&mid), "{n} facets: on the centre line {mid:.3}");
            }
        }
    }

    /// A hex bar's flat is a flat: its edges are 60 degree corners, not a
    /// round's creases, so the entry is the flat exactly.
    #[test]
    fn a_hex_bar_flat_is_read_at_the_flat() {
        let st = StockMesh::new(prism(6, 25.0, (0.0, 0.0), (-20.0, 20.0), std::f64::consts::PI / 6.0));
        let flat = 25.0 * (std::f64::consts::PI / 6.0).cos();
        for t in [0.0, 5.0] {
            let got = entry(&st, 25.0, line(t), 2.0).unwrap_or(f64::NAN);
            assert!((got - flat).abs() < 1e-9, "t {t}: the hex flat r{flat:.3} read as {got:.3}");
        }
    }

    /// A d50 cylinder stock 5 mm off the spindle axis (raw r30): on the
    /// line whose drill edge is at w 20 the true round is at
    /// u = 5 + sqrt(25^2 - 20^2) = 20; the entry is no nearer the axis.
    #[test]
    fn an_off_axis_cylinder_stock_is_entered_at_or_outside_its_true_round() {
        for n in [32, 16] {
            let st = StockMesh::new(prism(n, 25.0, (5.0, 0.0), (-20.0, 20.0), 0.0));
            let raw = 30.0;
            let got = entry(&st, raw, line(22.0), 2.0).unwrap_or(f64::NAN);
            assert!(got >= 20.0 - 1e-9, "{n} facets: entry {got:.3} inside the true round (20)");
            let back = entry(&st, raw, Line { out: -1.0, ..line(22.0) }, 2.0).unwrap_or(f64::NAN);
            assert!(back >= 10.0 - 1e-9, "{n} facets, -X side: entry {back:.3} inside the true round (10)");
        }
    }
}
