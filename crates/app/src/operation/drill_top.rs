//! The material top over a drill's body disk, and the R-plane law it
//! feeds (issue #91): a cycle rapids straight down onto its R plane, so
//! nothing of the part or the stock may stand above R there. Each facet
//! is read over its part inside the disk (a drafted top counts at the
//! hole, not at its far vertex), never under-estimated.
//!
//! Provenance: polygon clipping (Sutherland-Hodgman) and a plane's
//! maximum over a convex region at a corner are textbook computational
//! geometry; "never rapid into uncut stock" is the machine-tool
//! handbooks' shop rule. Our own design notes; no vendor material
//! consulted.

use super::{TOL, seg_dist};
use crate::cl::Cycle;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// The highest material over the XY disk (`c`, `r`): the part's facets
/// and the stock's, as they stand (issue #91: the stock above the R
/// plane is no longer clipped away - [`r_plane_clear`] refuses it
/// first, so the rapid laws are never asked to vouch for it). `prior`
/// is the surface the setup's earlier operations left over the same
/// disk (issue #89, read conservatively by the caller): the stock
/// stands no higher than it. `None` is the raw stock.
pub(super) fn top_over(
    part: &[Triangle],
    stock: &[Triangle],
    cx: f64,
    cy: f64,
    r: f64,
    prior: Option<f64>,
) -> Option<f64> {
    let p = highest(part, cx, cy, r);
    let s = cut_by(highest(stock, cx, cy, r), prior);
    match (p, s) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// A raw height `z` under the prior surface (issue #89): the lower of
/// the two when the prior is a number, the raw height otherwise - an
/// unknown prior is the raw stock, never air.
pub(super) fn cut_by(z: Option<f64>, prior: Option<f64>) -> Option<f64> {
    match (z, prior.filter(|p| !p.is_nan())) {
        // `NaN.min(p)` is `p`: a non-finite raw height stays NaN so the
        // laws refuse it (#89 review).
        (Some(z), _) if z.is_nan() => Some(z),
        (Some(z), Some(p)) => Some(z.min(p)),
        (z, _) => z,
    }
}

/// The highest point of the facets whose projection meets the disk,
/// each read over its part inside the disk ([`facet_top`]) - on a
/// drafted top the far vertex does not count (#91 review).
fn highest(facets: &[Triangle], cx: f64, cy: f64, r: f64) -> Option<f64> {
    facets.iter().filter(|t| meets_disk(t, cx, cy, r)).map(|t| facet_top(t, cx, cy, r)).reduce(f64::max)
}

/// The facet's highest Z over the disk: the triangle clipped to the
/// octagon circumscribing the disk (Sutherland-Hodgman, the cut points
/// interpolated along the 3D edges, so on the facet's plane), read at
/// the clipped polygon's corners - a plane's maximum over a convex
/// region stands at one of its corners, and the octagon holds the disk,
/// so this never under-estimates. A non-finite height reads +inf (the
/// laws then refuse it); a clip lost to rounding reads the facet's
/// highest vertex.
pub(super) fn facet_top(t: &Triangle, cx: f64, cy: f64, r: f64) -> f64 {
    let v = [t.a, t.b, t.c];
    if v.iter().any(|p| !p.z.is_finite()) {
        return f64::INFINITY;
    }
    let mut poly: Vec<Point3> = v.to_vec();
    let d = r + TOL;
    for k in 0..8 {
        let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
        let (nx, ny) = (a.cos(), a.sin());
        let side = |p: &Point3| (p.x - cx) * nx + (p.y - cy) * ny - d;
        let mut out = Vec::with_capacity(poly.len() + 1);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (sp, sq) = (side(&p), side(&q));
            if sp <= 0.0 {
                out.push(p);
            }
            if (sp < 0.0 && sq > 0.0) || (sp > 0.0 && sq < 0.0) {
                let s = sp / (sp - sq);
                out.push(Point3::new(p.x + s * (q.x - p.x), p.y + s * (q.y - p.y), p.z + s * (q.z - p.z)));
            }
        }
        poly = out;
        if poly.is_empty() {
            break;
        }
    }
    let vmax = t.a.z.max(t.b.z).max(t.c.z);
    poly.iter().map(|p| p.z).reduce(f64::max).filter(|z| z.is_finite()).unwrap_or(vmax)
}

/// Issue #91: the cycle rapids straight down onto its R plane, so no
/// material may stand above R over the tool's body disk at the hole -
/// the rapid would cross it (a collision no feed law sees). Material is
/// the part (its facets and the hole's entry surface) and the stock
/// together - the part alone when there is no stock, or the stock is
/// the op's own source; the message names whichever stands higher.
/// Removal by the setup's earlier operations (`prior`, issue #89: the
/// surface they left over the disk, `None` when unknown) lowers the
/// stock and the hole's own mouth - a faced or pocketed top is air; an
/// unknown prior leaves both at their raw heights. A non-finite height
/// is refused, never waved through.
pub(super) fn r_plane_clear(
    part: &[Triangle],
    stock: &[Triangle],
    c: &Cycle,
    body: f64,
    prior: Option<f64>,
) -> Result<(), GenError> {
    // The part as it stands before this cycle: its facets over the disk,
    // and the hole's own mouth - the modelled hole is what the cycle
    // cuts, so its bore is not yet air (unless an earlier op cut it).
    let entry = cut_by(Some(c.entry_z), prior).unwrap_or(c.entry_z);
    let p = highest(part, c.xy.x, c.xy.y, body).map_or(entry, |p| p.max(entry));
    let (top, what) = match cut_by(highest(stock, c.xy.x, c.xy.y, body), prior) {
        Some(s) if s >= p || p.is_nan() => (s, "stock"),
        _ => (p, "part"),
    };
    if top.is_finite() && c.entry_z.is_finite() && c.r_z.is_finite() && c.r_z >= top - TOL {
        return Ok(());
    }
    // #677: the spot is the material top over the hole (the entry when
    // the top is unknown); a non-finite one is dropped by `invalid_at`.
    let z = if top.is_finite() { top } else { c.entry_z };
    Err(GenError::invalid_at(
        format!(
            "R plane Z {:.3} is below the {what} top Z {top:.3} over the hole at ({:.3}, {:.3}) - the rapid down to R \
             would cross uncut stock (a collision); raise Top of Part Z (the R plane) to the {what} top or above \
             (an earlier operation's removal counts only once that operation is generated, upright, in this setup)",
            c.r_z, c.xy.x, c.xy.y
        ),
        Point3::new(c.xy.x, c.xy.y, z),
    ))
}

/// Does the facet's XY projection meet the disk? A facet with no area
/// (collapsed to a point or a line - a bad facet) is skipped, so one
/// sliver cannot set a false top. A wall (area in 3D, none in XY) is
/// read by its edges alone: its projection is a segment, and the
/// all-same-sign "inside" test would pass every disk on that
/// segment's line.
pub(super) fn meets_disk(t: &Triangle, cx: f64, cy: f64, r: f64) -> bool {
    let (u, w) = (t.b - t.a, t.c - t.a);
    let n = [u.y * w.z - u.z * w.y, u.z * w.x - u.x * w.z, u.x * w.y - u.y * w.x];
    let area = n[0].hypot(n[1]).hypot(n[2]);
    if area.is_nan() || area <= 1e-12 {
        return false;
    }
    let v = [(t.a.x - cx, t.a.y - cy), (t.b.x - cx, t.b.y - cy), (t.c.x - cx, t.c.y - cy)];
    if v.iter().any(|(x, y)| !(x.is_finite() && y.is_finite())) {
        return false;
    }
    let cross = |i: usize| {
        let (p, q) = (v[i], v[(i + 1) % 3]);
        p.0 * q.1 - p.1 * q.0
    };
    let s = [cross(0), cross(1), cross(2)];
    let flat = n[2].abs() > 1e-12;
    let inside = flat && (s.iter().all(|&c| c >= 0.0) || s.iter().all(|&c| c <= 0.0));
    inside || (0..3).any(|i| seg_dist(v[i], v[(i + 1) % 3]) <= r)
}
