//! Keeping the in-process stock polygon honest and small (M11.4 C):
//! every pass subtracts a swept disc, and the polygon algebra leaves
//! micron slivers and thousands of round-join vertices behind. The
//! stock is opened (slivers thinner than the planning grid dropped),
//! each loop simplified by Douglas-Peucker (Douglas & Peucker 1973,
//! "Algorithms for the reduction of the number of points required to
//! represent a digitized line", The Canadian Cartographer 10(2)) within
//! a fraction of the grid, and dust loops dropped. The tolerances are
//! the planner's resolution, below anything the measuring stock sees.

use crate::geo2d::{Loop, Point2, Region};

use crate::machining2d::GenError;

/// Perpendicular distance from `p` to the segment `a -> b`.
fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - dx * t).hypot(p.y - a.y - dy * t)
}

/// Douglas-Peucker on an open run (both ends kept), iterative.
fn dp(pts: &[Point2], tol: f64) -> Vec<Point2> {
    let n = pts.len();
    if n <= 2 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((i, j)) = stack.pop() {
        if j <= i + 1 {
            continue;
        }
        let (mut best, mut at) = (0.0f64, i);
        for k in i + 1..j {
            let d = seg_dist(pts[k], pts[i], pts[j]);
            if d > best {
                best = d;
                at = k;
            }
        }
        if best > tol {
            keep[at] = true;
            stack.push((i, at));
            stack.push((at, j));
        }
    }
    pts.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect()
}

/// Simplify one closed ring: split at its first vertex and the vertex
/// farthest from it, simplify both halves.
fn dp_closed(pts: &[Point2], tol: f64) -> Vec<Point2> {
    let n = pts.len();
    if n <= 4 {
        return pts.to_vec();
    }
    let far = (1..n)
        .max_by(|&a, &b| {
            let da = (pts[a].x - pts[0].x).hypot(pts[a].y - pts[0].y);
            let db = (pts[b].x - pts[0].x).hypot(pts[b].y - pts[0].y);
            da.total_cmp(&db)
        })
        .unwrap_or(n / 2);
    let mut a: Vec<Point2> = pts[..=far].to_vec();
    let mut b: Vec<Point2> = pts[far..].to_vec();
    b.push(pts[0]);
    a = dp(&a, tol);
    b = dp(&b, tol);
    a.pop();
    b.pop();
    a.extend(b);
    a
}

/// Spread every kink of a walk sharper than `max_deg` over its
/// neighbours (Laplacian smoothing - Taubin 1995's umbrella operator
/// on a polyline, half-weight, a few passes): a vertex moves toward its
/// neighbours' midpoint only where the move stays inside `center`, so
/// the smoothing can never carry a cutter center off the legal area.
/// The displacement is the kink's own sagitta - microns where the morph
/// or a blend seam left a few degrees too many.
pub(super) fn smooth_walk(pts: &mut [Point2], center: &Region, max_deg: f64, passes: usize) {
    let cos_max = max_deg.to_radians().cos();
    for _ in 0..passes {
        let mut moved = false;
        for i in 1..pts.len().saturating_sub(1) {
            let (p, v, q) = (pts[i - 1], pts[i], pts[i + 1]);
            let (ux, uy) = (v.x - p.x, v.y - p.y);
            let (wx, wy) = (q.x - v.x, q.y - v.y);
            let (lu, lw) = (ux.hypot(uy), wx.hypot(wy));
            if lu <= 1e-12 || lw <= 1e-12 {
                continue;
            }
            if (ux * wx + uy * wy) / (lu * lw) >= cos_max {
                continue;
            }
            let m = Point2::new(0.5 * v.x + 0.25 * (p.x + q.x), 0.5 * v.y + 0.25 * (p.y + q.y));
            // A kink turning toward the wall smooths onto the wall: the
            // midpoint is pulled back to the boundary, never past it.
            let m = if center.contains(m) { m } else { super::loops::nearest_on_boundary(center, m) };
            if (m.x - v.x).hypot(m.y - v.y) > 1e-9 {
                pts[i] = m;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

/// The stock region, opened by `open`, simplified within `tol`, loops
/// under `dust` mm^2 dropped.
pub(super) fn tidy(reg: &Region, open: f64, tol: f64, dust: f64) -> Result<Region, GenError> {
    let opened = reg.offset(-open)?.offset(open)?;
    let mut loops = Vec::with_capacity(opened.loops().len());
    for l in opened.loops() {
        if l.area().abs() < dust {
            continue;
        }
        let pts = dp_closed(l.points(), tol);
        if pts.len() >= 3
            && let Ok(nl) = Loop::new(pts)
        {
            loops.push(nl);
        }
    }
    // A simplification that made a loop cross itself is refused by the
    // region door; the opened, unsimplified stock is then the answer.
    Ok(Region::new(loops).unwrap_or(opened))
}
