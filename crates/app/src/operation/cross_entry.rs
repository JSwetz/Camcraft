//! Where a cross hole's drill first meets the raw stock (issue #85):
//! along the hole's own line, against the stock mesh, rather than at
//! the stock's circumscribed radius - a box counts at its face, not its
//! corner. The body is read as the square tube of half-width `half`
//! about the line (across it in XY, and in Z): a superset of the round
//! drill, so the outermost stock point inside it is no nearer the axis
//! than the first point the real body touches - the entry stays
//! conservative.
//!
//! Provenance: Sutherland-Hodgman polygon clipping against slab
//! half-spaces, and the first-contact point of a prism swept along its
//! axis, are textbook computational geometry. Our own design notes; no
//! vendor material consulted.

use crate::geometry::{Point3, Triangle};

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

/// The outermost outward coordinate of any stock facet inside the
/// square tube of half-width `half` about `line`, or `None` when the
/// tube meets no facet.
fn tube_max(mesh: &[Triangle], line: Line, half: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for tri in mesh {
        let mut poly: Vec<(f64, f64, f64)> = [tri.a, tri.b, tri.c]
            .iter()
            .filter(|p| p.is_finite())
            .map(|&p| (line.u(p), line.w(p) - line.t, p.z - line.z))
            .collect();
        if poly.len() < 3 {
            continue;
        }
        // Four slabs: |w| <= half and |z| <= half, each as two halves
        // (`k` picks w or z, `sg` the side).
        for (k, sg) in [(1, 1.0), (1, -1.0), (2, 1.0), (2, -1.0)] {
            poly = clip(&poly, |p| half - sg * if k == 1 { p.1 } else { p.2 });
            if poly.is_empty() {
                break;
            }
        }
        if let Some(m) = poly.iter().map(|p| p.0).reduce(f64::max) {
            best = Some(best.map_or(m, |b| b.max(m)));
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
pub(super) fn entry(mesh: &[Triangle], raw: f64, line: Line, half: f64) -> Option<f64> {
    let edge = (line.t.abs() - half).max(0.0);
    let circ = (edge < raw).then(|| (raw * raw - edge * edge).max(0.0).sqrt())?;
    let Some(m) = tube_max(mesh, line, half) else { return Some(circ) };
    let band = raw * (1.0 - (std::f64::consts::PI / 16.0).cos());
    let flat = m.is_finite() && m < circ && raw - m.hypot(edge) > band;
    Some(if flat { m } else { circ })
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
        let b = cube(Point3::new(-30.0, -30.0, -20.0), Point3::new(30.0, 30.0, 20.0));
        let raw = 1800.0_f64.sqrt();
        assert_eq!(entry(&b.facets, raw, line(0.0), 2.0), Some(30.0));
        assert_eq!(entry(&b.facets, raw, line(7.0), 3.0), Some(30.0));
        // Above the box: no crossing, the circumscribed reading stands.
        let above = Line { z: 50.0, ..line(0.0) };
        assert_eq!(entry(&b.facets, raw, above, 2.0), Some(raw));
        assert_eq!(entry(&b.facets, raw, line(raw + 3.0), 2.0), None);
    }
}
