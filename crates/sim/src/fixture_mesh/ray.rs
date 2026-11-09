//! Point-in-solid for a closed fixture mesh (#339): the one question the
//! surface-distance test cannot answer - a tool band standing wholly
//! INSIDE steel is far from every triangle, yet a crash.
//!
//! Provenance: ray-crossing inclusion with signed crossings (the winding
//! number of a closed oriented surface; Haines, "Point in Polygon
//! Strategies", Graphics Gems IV, 1994, and its 3-D form in O'Rourke,
//! "Computational Geometry in C", 2nd ed., ch. 7); the ray-triangle test
//! is Moller and Trumbore, "Fast, Minimum Storage Ray-Triangle
//! Intersection", J. Graphics Tools 2(1), 1997. Our own code.
//!
//! ## Why the answer is conservative
//!
//! A ray that crosses only triangle interiors, transversally, counts
//! the winding number of a closed oriented mesh exactly. Any crossing
//! near an edge, a vertex, or a plane the ray runs along is AMBIGUOUS:
//! that ray is discarded and the next fixed direction tried. "Outside"
//! is answered only from an unambiguous ray that saw winding zero AND
//! even parity; when every direction is ambiguous the point reads as
//! inside - a hit, never a silent pass.

use super::bvh::ray_meets;
use super::gjk::{cross, dot, sub, V};
use super::FixtureMesh;

/// Fixed, deliberately non-axis-aligned directions (unit-ish - only the
/// direction matters): axis-aligned vises then never present a face
/// edge-on or an edge exactly in line.
const DIRS: [V; 6] = [
    [0.573_576_436, 0.611_483_773, 0.545_517_416],
    [-0.433_012_702, 0.250_000_000, 0.866_025_404],
    [0.316_227_766, -0.709_403_271, 0.629_861_003],
    [-0.618_033_989, -0.381_966_011, -0.687_157_220],
    [0.801_783_726, 0.267_261_242, -0.534_522_484],
    [-0.267_261_242, 0.801_783_726, -0.534_522_484],
];

/// Barycentric margin under which a crossing is "at an edge".
const EDGE_EPS: f64 = 1e-7;
/// Sine of the angle under which the ray "runs along" a facet's plane.
const GRAZE_SIN: f64 = 1e-6;

/// One ray's reading: Some((winding, crossings)) or None (ambiguous).
fn cast(mesh: &FixtureMesh, o: V, d: V) -> Option<(i64, u64)> {
    let mut wind = 0i64;
    let mut count = 0u64;
    let mut ambiguous = false;
    mesh.bvh.walk(
        |n| ray_meets(o, d, n.lo, n.hi, mesh.pad),
        |t| {
            let Some(tri) = mesh.tris.get(t) else { return false };
            if mesh.flat.get(t).copied().unwrap_or(false) {
                return false; // no interior to cross; its neighbours' edges decide
            }
            let [a, b, c] = *tri;
            let (e1, e2) = (sub(b, a), sub(c, a));
            let n = cross(e1, e2);
            let nn = dot(n, n).sqrt();
            let dn = dot(d, n);
            let p = cross(d, e2);
            let det = dot(e1, p);
            if !det.is_finite() {
                ambiguous = true;
                return true;
            }
            if dn.abs() <= GRAZE_SIN * dot(d, d).sqrt() * nn {
                // The ray runs (nearly) along this facet's plane: if it
                // can reach the facet's box at all, the count is not
                // trusted.
                let (lo, hi) = super::bvh::tri_box(tri);
                if ray_meets(o, d, lo, hi, mesh.pad) {
                    ambiguous = true;
                    return true;
                }
                return false;
            }
            let inv = 1.0 / det;
            let s = sub(o, a);
            let u = dot(s, p) * inv;
            let q = cross(s, e1);
            let v = dot(d, q) * inv;
            let tt = dot(e2, q) * inv;
            let w = 1.0 - u - v;
            if u < -EDGE_EPS || v < -EDGE_EPS || w < -EDGE_EPS {
                return false; // misses the facet clear of its edges
            }
            if u <= EDGE_EPS || v <= EDGE_EPS || w <= EDGE_EPS {
                ambiguous = true; // through an edge or a vertex
                return true;
            }
            let reach = mesh.pad / dot(d, d).sqrt().max(f64::MIN_POSITIVE);
            if tt.abs() <= reach {
                ambiguous = true; // the origin is on the surface
                return true;
            }
            if tt > 0.0 {
                count += 1;
                wind += if dn > 0.0 { 1 } else { -1 };
            }
            false
        },
    );
    (!ambiguous).then_some((wind, count))
}

/// Is `q` inside the closed mesh's solid? Inside on any nonzero winding
/// or odd crossing count (overlapping shells, an inside-out shell);
/// inside when no direction gives an unambiguous count.
pub(super) fn inside(mesh: &FixtureMesh, q: V) -> bool {
    if !q.iter().all(|c| c.is_finite()) {
        return true;
    }
    for d in DIRS {
        if let Some((wind, count)) = cast(mesh, q, d) {
            return wind != 0 || count % 2 == 1;
        }
    }
    true
}
