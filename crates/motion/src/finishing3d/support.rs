//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::geometry::{Point3, Triangle};

use super::params::{SPLIT_MZ, TOUCH_EPS};
/// A facet prepared for the raster's support scan. `m` is the unit
/// normal pointed into the sky (the ball rests on the plane from
/// above whichever way the winding went); `shallow` marks the facets
/// the raster may touch; the padded box rejects far facets per
/// sample without touching their vertices.
pub(super) struct Sup {
    pub(super) tri: [Point3; 3],
    pub(super) m: Point3,
    pub(super) shallow: bool,
    pub(super) xmin: f64,
    pub(super) ymin: f64,
    pub(super) xmax: f64,
    pub(super) ymax: f64,
}

pub(super) fn build_sups(facets: &[Triangle], r: f64) -> Vec<Sup> {
    facets
        .iter()
        .map(|f| {
            let mut m = f.normal();
            if m.z < 0.0 {
                m = Point3::new(-m.x, -m.y, -m.z);
            }
            let tri = [f.a, f.b, f.c];
            Sup {
                tri,
                shallow: m.is_finite() && m.z >= SPLIT_MZ,
                xmin: tri[0].x.min(tri[1].x.min(tri[2].x)) - r,
                xmax: tri[0].x.max(tri[1].x.max(tri[2].x)) + r,
                ymin: tri[0].y.min(tri[1].y.min(tri[2].y)) - r,
                ymax: tri[0].y.max(tri[1].y.max(tri[2].y)) + r,
                m,
            }
        })
        .collect()
}

/// Point-to-triangle squared distance by the classic Voronoi-region
/// decomposition (published closed form - dot products and one
/// barycentric solve, no iteration).
pub(super) fn dist2_pt_tri(p: Point3, t: &[Point3; 3]) -> f64 {
    let q = closest_pt_tri(p, t);
    let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
    (dx * dx + dy * dy + dz * dz).max(0.0)
}

/// The closest point of the triangle to `p` - the contact point of a
/// ball centered at `p` the instant it touches. The Voronoi-region
/// decomposition is published closed form (dot products and one
/// barycentric solve, no iteration); `dist2_pt_tri` above IS this
/// function's distance, so the two can never disagree. Made
/// crate-public for M5.1 stage C: the 5-axis stamp asks "what facet
/// point does this ball-center rest on" - the same question the
/// raster's clearance law asks, from the other side.
pub(crate) fn closest_pt_tri(p: Point3, t: &[Point3; 3]) -> Point3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (abx, aby, abz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (acx, acy, acz) = (c.x - a.x, c.y - a.y, c.z - a.z);
    let (apx, apy, apz) = (p.x - a.x, p.y - a.y, p.z - a.z);
    let d1 = abx * apx + aby * apy + abz * apz;
    let d2 = acx * apx + acy * apy + acz * apz;
    let (bpx, bpy, bpz) = (p.x - b.x, p.y - b.y, p.z - b.z);
    let d3 = abx * bpx + aby * bpy + abz * bpz;
    let d4 = acx * bpx + acy * bpy + acz * bpz;
    let (cpx, cpy, cpz) = (p.x - c.x, p.y - c.y, p.z - c.z);
    let d5 = abx * cpx + aby * cpy + abz * cpz;
    let d6 = acx * cpx + acy * cpy + acz * cpz;
    let vc = d1 * d4 - d3 * d2;
    let vb = d5 * d2 - d1 * d6;
    let va = d3 * d6 - d5 * d4;
    // One region per arm; each computes the closest point, and the
    // arms are exhaustive by the published case proof.
    if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else if d3 >= 0.0 && d4 <= d3 {
        b
    } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3).max(1e-300);
        Point3::new(a.x + abx * v, a.y + aby * v, a.z + abz * v)
    } else if d6 >= 0.0 && d5 <= d6 {
        c
    } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6).max(1e-300);
        Point3::new(a.x + acx * w, a.y + acy * w, a.z + acz * w)
    } else if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6)).max(1e-300);
        Point3::new(b.x + (c.x - b.x) * w, b.y + (c.y - b.y) * w, b.z + (c.z - b.z) * w)
    } else {
        let denom = (va + vb + vc).max(1e-300);
        let (v, w) = (vb / denom, vc / denom);
        Point3::new(
            a.x + abx * v + acx * w,
            a.y + aby * v + acy * w,
            a.z + abz * v + acz * w,
        )
    }
}

/// The center is clear when every facet keeps at least `r` of ball
/// around it - the same no-gouge law the ring enforces through the
/// forbidden region, asked of one point at a time.
pub(super) fn ball_clear(c: Point3, sups: &[Sup], r: f64) -> bool {
    for s in sups {
        if c.x < s.xmin || c.x > s.xmax || c.y < s.ymin || c.y > s.ymax {
            continue; // outside the padded footprint: the ball cannot reach it
        }
        if dist2_pt_tri(c, &s.tri) < (r - TOUCH_EPS) * (r - TOUCH_EPS) {
            return false;
        }
    }
    true
}

/// May the ball be lowered along the vertical column at XY `(x, y)`
/// from the center plane `top_cz` down to center height `cz` without
/// cutting on the way? `ball_clear` proves the STOP, not the DESCENT:
/// ground that hides beneath a canopy - the cone's base disc under
/// its own flank - keeps a tangent-clear center and is still
/// unreachable, because the ball crosses the canopy mid-column. The
/// exact certifier is the segment-triangle distance (the sweep of a
/// ball along a segment IS that segment inflated by `r` - the same
/// question the envelope scan asks, asked before posting rather than
/// after); the padded footprint filters facets exactly as
/// `ball_clear`'s does, since a column plus a radius only meets
/// triangles whose padded footprint it enters.
///
/// `limit` is the currency of the answer: an analytic tangent center
/// measures exactly `r` and belongs to `r - TOUCH_EPS`; a start point
/// read off a tessellated ring boundary sits a chord-sag inside the
/// true locus and belongs to `r - tol`, the same debt `leg_clears`
/// states.
pub(super) fn column_clear(x: f64, y: f64, cz: f64, top_cz: f64, limit: f64, sups: &[Sup]) -> bool {
    use crate::stock_sim::dist_seg_tri;
    let (hi, lo) = (top_cz.max(cz), top_cz.min(cz));
    let (a, b) = (Point3::new(x, y, hi), Point3::new(x, y, lo));
    for s in sups {
        if x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue; // outside the padded footprint: the swept ball cannot reach it
        }
        let t = Triangle::new(s.tri[0], s.tri[1], s.tri[2]);
        if dist_seg_tri(a, b, &t) < limit {
            return false;
        }
    }
    true
}

/// The march ground for families that enter by vertical plunge: the
/// support's answer, but only where the descent is possible. Returns
/// the center height and whether the column law TRIMMED an answer
/// `support_at` had given - a trimmed ground is loud (a pass note),
/// never a silently vanished floor; the rest scan answers for what
/// stays under the canopy.
pub(super) fn plunge_support(
    x: f64,
    y: f64,
    r: f64,
    floor_tip: f64,
    top_cz: f64,
    sups: &[Sup],
) -> (Option<f64>, bool) {
    match support_at(x, y, r, floor_tip, sups) {
        None => (None, false),
        Some(cz) => {
            if column_clear(x, y, cz, top_cz, r - TOUCH_EPS, sups) {
                (Some(cz), false)
            } else {
                (None, true)
            }
        }
    }
}

/// The tangent-center height of a ball resting on one shallow facet
/// above center XY `(x, y)`: the contact `q = (x, y) − r·m_xy` must
/// fall inside the facet's XY projection (a shallow facet's
/// projection is never degenerate), and the center sits at
/// `q_z + r·m_z`. None when this facet owns no tangent here.
pub(super) fn tangent_z(s: &Sup, x: f64, y: f64, r: f64) -> Option<f64> {
    if !s.shallow {
        return None;
    }
    let (a, b, c) = (s.tri[0], s.tri[1], s.tri[2]);
    let qx = x - r * s.m.x;
    let qy = y - r * s.m.y;
    // Same-sign barycentrics against the XY projection.
    let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let w0 = ((b.x - a.x) * (qy - a.y) - (b.y - a.y) * (qx - a.x)) / det;
    let w1 = ((c.x - a.x) * (a.y - qy) - (c.y - a.y) * (a.x - qx)) / det;
    let w2 = 1.0 - w0 - w1;
    let tol = 1e-9; // barycentric slack: contact may sit on an edge or a vertex
    if w0 < -tol || w1 < -tol || w2 < -tol {
        return None;
    }
    // The plane through the triangle, solved for z (m_z >= SPLIT_MZ).
    let qz = a.z - (s.m.x * (qx - a.x) + s.m.y * (qy - a.y)) / s.m.z;
    (qz.is_finite() && (qz + r * s.m.z).is_finite()).then_some(qz + r * s.m.z)
}

/// The raster's whole law in one question: where does the ball sit
/// above `(x, y)`? The HIGHEST tangent center that keeps the ball
/// clear of every facet - the topmost surface a descending tool
/// meets first - and `None` over air, over unreached overhangs, and
/// over anything whose tangent sits below the programmed floor: the
/// floor outranks the surface, nothing cuts below `bottom_z`.
pub(super) fn support_at(x: f64, y: f64, r: f64, floor_tip: f64, sups: &[Sup]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for s in sups {
        if !s.shallow || x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        let Some(cz) = tangent_z(s, x, y, r) else { continue };
        if cz - r < floor_tip - TOUCH_EPS {
            continue;
        }
        if best.is_some_and(|b| b >= cz) {
            continue; // below the current best: clearing it cannot raise the answer
        }
        if ball_clear(Point3::new(x, y, cz), sups, r) {
            best = Some(cz);
        }
    }
    best
}
