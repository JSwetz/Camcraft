//! Split from `stock_sim.rs` by M5.0.1; the facet gouge law is M5.2 E's.

use super::kernel::StockError;

use super::scan_arc::ScanArc;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
/// The facet gouge law: how deep the model's own surface stands
/// INSIDE the swept envelope of the balls. A gouge, physically, is a
/// point of the finished part that some instant's ball swallowed - so
/// the measurement is exactly that: between each model triangle and
/// each ball-center segment, the distance of two convex sets, and `r`
/// minus it where positive.
///
/// This is the same distance truth the ridge law already owes its
/// rest number to (`dist_pt_seg3`), paid against whole triangles
/// instead of column points. On a horizontal floor it reads what the
/// Dexel floor read (a surface point, a vertical drop to the ball's
/// bottom); it also reads, honestly and for the first time, tilted
/// flanks (where the vertical reading is inflated by 1/cos), vertical
/// walls (whose columns carry NO height for a floor to have fallen
/// below), and a pass legitimately running UNDER an overhang - the
/// M5.2 E phantom, where a tangent chamfer ride stood the bar's whole
/// length under its own top cap and the floor law convicted it. The
/// triangles enter as the exact surfaces they are; the discretization
/// left is the machine's own (the emitted chords, at most `tol/4` of
/// arc sagitta, inside this scan's gate) - the lattice no longer
/// speaks at all.
///
/// Pair discovery walks each segment's swept band over the same
/// lattice the scan builds and visits the facet bins it crosses. For
/// slice `k` the walk takes the segment points whose along-axis `u`
/// lies in the column window grown by `r` on both sides, and marks
/// their cross-axis `v` extent grown by `r`. Any facet point `p`
/// within `r` of a segment point `q` then lands in a visited cell:
/// `q.u` is within `r` of `p.u`, hence in slice `k = p`'s column's
/// grown window; `q.v` is in that slice's marked band; and `p.v` is
/// within `r` of `q.v`, hence in a marked cell of the column. Proved,
/// never tuned. A model/path pair over the pop budget is a stated
/// refusal (the caller turns refusals into a proves-nothing warning,
/// never silence).
#[allow(clippy::too_many_arguments)] // the scan's whole geometry frame, threaded once from envelope_scan
pub(super) fn facet_gouge(
    facets: &[Triangle],
    segs: &[(Point3, Point3)],
    arcs: &[Option<ScanArc>],
    r: f64,
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
) -> Result<(f64, Point2), StockError> {
    const POP_BUDGET: usize = 60_000_000; // exact distances this scan will pay
    if segs.is_empty() || facets.is_empty() {
        return Ok((0.0, Point2::new(0.0, 0.0)));
    }
    let fx = |v: f64, o: f64, n: usize| {
        ((v - o) / cell).floor().clamp(0.0, n as f64 - 1.0) as usize
    };
    // Facet bins: every triangle in every cell its XY bounding box
    // touches - the `part_surface` rasterization shape, now answering
    // "which surfaces live near this stretch of path".
    let mut bins: Vec<Vec<u32>> = vec![Vec::new(); nx * ny];
    for (fi, t) in facets.iter().enumerate() {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue; // the scan validates the model before arriving
        }
        let (i0, i1) = (
            fx(a.x.min(b.x).min(c.x), x0, nx),
            fx(a.x.max(b.x).max(c.x), x0, nx),
        );
        let (j0, j1) = (
            fx(a.y.min(b.y).min(c.y), y0, ny),
            fx(a.y.max(b.y).max(c.y), y0, ny),
        );
        for iy in j0..=j1 {
            for ix in i0..=i1 {
                bins[ix + iy * nx].push(fi as u32);
            }
        }
    }
    let mut seen = vec![u32::MAX; facets.len()];
    let mut best = 0.0f64;
    let mut at = Point2::new(0.0, 0.0);
    let mut pops = 0usize;
    for (si, (sa, sb)) in segs.iter().enumerate() {
        let epoch = si as u32;
        // An XY arc piece (#413, `scan_arc`) reaches past its chord by
        // its bulge: the walk grows by it, the distance is the arc's.
        let arc = arcs.get(si).copied().flatten();
        let bulge = arc.map_or(0.0, |a| a.bulge());
        let r_walk = r + bulge;
        // (u, v): the band walk's axes, u the one the segment runs
        // mostly along - so slices are few and per-slice bands thin
        // (slope at most 1, so a slice's band costs O(r/cell)).
        let xmajor = (sb.x - sa.x).abs() >= (sb.y - sa.y).abs();
        let (ua, va, ub, vb) = if xmajor {
            (sa.x, sa.y, sb.x, sb.y)
        } else {
            (sa.y, sa.x, sb.y, sb.x)
        };
        let (u0, o_u, v0, o_v) = if xmajor { (x0, nx, y0, ny) } else { (y0, ny, x0, nx) };
        let (du, dv) = (ub - ua, vb - va);
        let (umin, umax) = (ua.min(ub), ua.max(ub));
        // Columns whose grown window can reach the segment at all:
        // left of these, every facet point sits farther than r from
        // every segment point and can only contribute dev <= 0.
        let (k0, k1) = (fx(umin - r_walk, u0, o_u), fx(umax + r_walk, u0, o_u));
        for k in k0..=k1 {
            let (w0, w1) = (
                (u0 + k as f64 * cell - r_walk).max(umin),
                (u0 + (k + 1) as f64 * cell + r_walk).min(umax),
            );
            if w0 > w1 {
                continue; // outside the path's reach in u
            }
            let (ta, tb) = if du.abs() > f64::EPSILON {
                (
                    ((w0 - ua) / du).clamp(0.0, 1.0),
                    ((w1 - ua) / du).clamp(0.0, 1.0),
                )
            } else {
                (0.0, 1.0) // a plunge: the whole segment is one u
            };
            let (vy0, vy1) = (va + dv * ta, va + dv * tb);
            let (l0, l1) = (fx(vy0.min(vy1) - r_walk, v0, o_v), fx(vy0.max(vy1) + r_walk, v0, o_v));
            for l in l0..=l1 {
                let (ix, iy) = if xmajor { (k, l) } else { (l, k) };
                for fi in bins[ix + iy * nx].iter().copied() {
                    if seen[fi as usize] == epoch {
                        continue; // this facet already answered for this segment
                    }
                    seen[fi as usize] = epoch;
                    pops += 1;
                    if pops > POP_BUDGET {
                        return Err(StockError::Invalid(format!(
                            "envelope gouge scan needs more than {POP_BUDGET} facet/path distance evaluations; the model is too large for a verdict on this lattice"
                        )));
                    }
                    let t = &facets[fi as usize];
                    let dc = dist_seg_tri(*sa, *sb, t);
                    let d = match arc {
                        // A chord a bulge past the ball: the arc is too.
                        Some(a) if dc - bulge < r => a.dist_tri(t),
                        _ => dc,
                    };
                    let dev = r - d;
                    if dev > best {
                        best = dev;
                        at = Point2::new(
                            (t.a.x + t.b.x + t.c.x) / 3.0,
                            (t.a.y + t.b.y + t.c.y) / 3.0,
                        );
                    }
                }
            }
        }
    }
    Ok((best.max(0.0), at))
}

/// The exact minimum distance between a segment and a triangle. The
/// closest pair of two convex sets involves an endpoint of the segment
/// and a point of the triangle (the two point tests), or a pair of
/// edges (the three edge tests), or the segment pierces the face -
/// distance 0, the one configuration no endpoint or edge can witness
/// (Ericson, "Real-Time Collision Detection" 5.1: the reduction is
/// complete with all four enumerated - the face test is that fourth).
/// Made public for M2.5 A's verdict-lane fix: finishing3d's plunge
/// law asks the same sweep question the envelope scan asks - does the
/// ball, run along this segment, ever come nearer than r to a facet.
pub fn dist_seg_tri(a: Point3, b: Point3, t: &Triangle) -> f64 {
    let (v0, v1, v2) = (t.a, t.b, t.c);
    let (e0, e1) = (sub(v1, v0), sub(v2, v0));
    let n = cross(e0, e1);
    let nn = dot(n, n);
    if nn > 1e-24 {
        // Endpoints strictly on opposite sides of the face's plane:
        // the crossing point exists and is interior to the segment, so
        // only the in-face test remains. A coplanar segment that
        // crosses the triangle crosses an edge too - already covered.
        let (da, db) = (dot(n, sub(a, v0)), dot(n, sub(b, v0)));
        if da * db < 0.0 {
            let s = da / (da - db);
            let q = sub(a, v0);
            let r = sub(b, a);
            let w = (q.0 + r.0 * s, q.1 + r.1 * s, q.2 + r.2 * s);
            let (u, v) = (dot(cross(w, e1), n) / nn, dot(cross(e0, w), n) / nn);
            if u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9 {
                return 0.0;
            }
        }
    }
    let mut d = dist_pt_tri(a, t).min(dist_pt_tri(b, t));
    for (p0, p1) in [(v0, v1), (v1, v2), (v2, v0)] {
        d = d.min(dist_seg_seg(a, b, p0, p1));
    }
    d
}

/// Distance from a point to a triangle: the plane face where the
/// projection lands inside, the three edges everywhere else (a
/// degenerate triangle IS nothing but its edges - true geometry, not
/// a fallback).
pub(crate) fn dist_pt_tri(p: Point3, t: &Triangle) -> f64 {
    let (a, b, c) = (t.a, t.b, t.c);
    let (e0, e1) = (
        (b.x - a.x, b.y - a.y, b.z - a.z),
        (c.x - a.x, c.y - a.y, c.z - a.z),
    );
    let mut d = dist_pt_seg3p(p, a, b)
        .min(dist_pt_seg3p(p, b, c))
        .min(dist_pt_seg3p(p, c, a));
    let n = cross(e0, e1);
    let nn = dot(n, n);
    if nn <= 1e-24 {
        return d;
    }
    let w = (p.x - a.x, p.y - a.y, p.z - a.z);
    let u = dot(cross(w, e1), n) / nn;
    let v = dot(cross(e0, w), n) / nn;
    if u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9 {
        d = d.min(dot(w, n).abs() / nn.sqrt());
    }
    d
}

/// Distance between two segments: every endpoint-to-other-segment
/// distance plus the two-line solve with its clamped start - the
/// minimum of two segments is attained either interior to both (the
/// solve finds it) or at an endpoint (the four distances cover it),
/// so the candidate set is complete, not "usually" correct.
fn dist_seg_seg(p1: Point3, q1: Point3, p2: Point3, q2: Point3) -> f64 {
    let (d1, d2) = (sub(q1, p1), sub(q2, p2));
    let (a1, a2) = (dot(d1, d1), dot(d2, d2));
    let best = dist_pt_seg3p(p1, p2, q2)
        .min(dist_pt_seg3p(q1, p2, q2))
        .min(dist_pt_seg3p(p2, p1, q1))
        .min(dist_pt_seg3p(q2, p1, q1));
    if a1 <= 1e-24 || a2 <= 1e-24 {
        return best; // a point segment: the endpoint set is the answer
    }
    let (e, w) = (dot(d1, d2), sub(p1, p2));
    let (f, g) = (dot(d2, w), dot(d1, w));
    let det = a1 * a2 - e * e;
    let (s, t) = if det.abs() <= 1e-12 * a1 * a2 {
        (0.0, (f / a2).clamp(0.0, 1.0)) // parallel: separation constant along seg 1
    } else {
        let s0 = ((e * f - a2 * g) / det).clamp(0.0, 1.0);
        (s0, ((e * s0 + f) / a2).clamp(0.0, 1.0))
    };
    let c1 = Point3::new(p1.x + d1.0 * s, p1.y + d1.1 * s, p1.z + d1.2 * s);
    let c2 = Point3::new(p2.x + d2.0 * t, p2.y + d2.1 * t, p2.z + d2.2 * t);
    best.min(dist(c1, c2))
}

/// Point-to-segment distance - `scan::dist_pt_seg3` in
/// point-argument clothing (one law, two callers).
fn dist_pt_seg3p(p: Point3, a: Point3, b: Point3) -> f64 {
    let d = sub(b, a);
    let len2 = dot(d, d);
    let t = if len2 > 0.0 {
        (dot(sub(p, a), d) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, Point3::new(a.x + d.0 * t, a.y + d.1 * t, a.z + d.2 * t))
}

type V3 = (f64, f64, f64);

fn sub(a: Point3, b: Point3) -> V3 {
    (a.x - b.x, a.y - b.y, a.z - b.z)
}

fn dot(a: V3, b: V3) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn cross(a: V3, b: V3) -> V3 {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

fn dist(a: Point3, b: Point3) -> f64 {
    let d = sub(a, b);
    d.0.hypot(d.1).hypot(d.2)
}
