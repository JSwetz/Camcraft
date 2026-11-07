//! Drill reach (issue #84): a drill may go no deeper into material than
//! its usable flute, measured from the STOCK surface at the hole - past
//! the flutes the shank (no narrower than the drill on every catalog
//! drill) feeds into uncut stock, a collision - and its holder must
//! stay clear of the stock along every move. Both laws refuse by name,
//! before the path can reach the post.
//!
//! The tool stack read is the catalog's own: `flute_len` from the tip,
//! then `shank_len` of shank, then the holder (`holder_d` wide,
//! `holder_len` long) - so the holder's face stands `flute_len +
//! shank_len` behind the tip. A tool with no holder modelled
//! (`holder_d` or `holder_len` zero, the turret-held kinds) has nothing
//! for the holder law to sweep. The rule is the conservative one: a
//! necked tool (shank narrower than the drill) would let the shank in
//! further, but nothing here proves its wall clearance, so it is held
//! to its flute like the rest.
//!
//! Provenance: usable flute length versus hole depth, and holder
//! clearance over the work, are the machine-tool handbooks' drilling
//! practice; the swept-rectangle distance is textbook planar geometry
//! (convex hull, point-to-segment). Our own design notes; no vendor
//! material consulted.

use crate::cl::{Cycle, Toolpath};
use crate::drilling::{CrossHole, CycleChoice, SPOT_DEPTH, THROUGH_CLEAR};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;

use super::cross_stock::Standoff;

const TOL: f64 = 1e-6;

fn finite_or_zero(v: f64) -> f64 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// The tip-to-holder-face distance and the holder's radius, or `None`
/// when no holder is modelled.
fn holder(tool: &Tool) -> Option<(f64, f64)> {
    let (hd, hl) = (finite_or_zero(tool.holder_d), finite_or_zero(tool.holder_len));
    (hd > 0.0 && hl > 0.0).then(|| (finite_or_zero(tool.flute_len) + finite_or_zero(tool.shank_len), 0.5 * hd))
}

fn too_deep(what: &str, depth: f64, name: &str, flute: f64, fix: &str) -> GenError {
    GenError::Invalid(format!(
        "{what} is {depth:.3} mm deep from the stock surface; tool {name}'s flute reaches {flute:.3} mm - \
         past it the shank feeds into uncut stock (a collision); {fix}"
    ))
}

/// The cross holes' reach (spindle frame, mouths already on the raw
/// surface when the stock is wider than the part): the depth the kernel
/// will feed - the floor, plus the through-exit allowance, or the spot
/// dimple - against the flute. `world` names each hole as modelled.
pub(super) fn cross_reach(local: &[CrossHole], world: &[CrossHole], cycle: CycleChoice, tool: &Tool, name: &str) -> Result<(), GenError> {
    let flute = finite_or_zero(tool.flute_len);
    for (i, (h, w)) in local.iter().zip(world).enumerate() {
        let floor = h.floor_d - if h.through { THROUGH_CLEAR } else { 0.0 };
        let depth = if matches!(cycle, CycleChoice::Spot) { SPOT_DEPTH.min(-h.floor_d) } else { -floor };
        if depth.is_nan() || depth > flute + TOL {
            let o = w.origin;
            return Err(too_deep(
                &format!("cross hole {i} at ({:.3}, {:.3}, {:.3})", o.x, o.y, o.z),
                depth,
                name,
                flute,
                "drill it with a longer drill, or turn the bar to size first",
            ));
        }
    }
    Ok(())
}

/// The highest material over the XY disk (`c`, `r`): the part's facets
/// as they stand, the stock's clipped to the cycle's R plane (the
/// cycle rapids down to R, so stock above it is the rapid laws' word,
/// not stock this cycle feeds through). A facet counts at its highest
/// vertex when its projection meets the disk - an over-estimate on a
/// slope, never an under-estimate.
fn top_over(part: &[Triangle], stock: &[Triangle], cx: f64, cy: f64, r: f64, r_z: f64) -> Option<f64> {
    let over = |t: &Triangle| meets_disk(t, cx, cy, r).then(|| t.a.z.max(t.b.z).max(t.c.z));
    let p = part.iter().filter_map(over).reduce(f64::max);
    let s = stock.iter().filter_map(over).reduce(f64::max).map(|z| z.min(r_z));
    match (p, s) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

fn meets_disk(t: &Triangle, cx: f64, cy: f64, r: f64) -> bool {
    let v = [(t.a.x - cx, t.a.y - cy), (t.b.x - cx, t.b.y - cy), (t.c.x - cx, t.c.y - cy)];
    if v.iter().any(|(x, y)| !(x.is_finite() && y.is_finite())) {
        return false;
    }
    let cross = |i: usize| {
        let (p, q) = (v[i], v[(i + 1) % 3]);
        p.0 * q.1 - p.1 * q.0
    };
    let s = [cross(0), cross(1), cross(2)];
    let inside = s.iter().all(|&c| c >= 0.0) || s.iter().all(|&c| c <= 0.0);
    inside || (0..3).any(|i| seg_dist(v[i], v[(i + 1) % 3]) <= r)
}

/// Distance from the origin to the segment `p`-`q`.
fn seg_dist(p: (f64, f64), q: (f64, f64)) -> f64 {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let ll = dx * dx + dy * dy;
    let t = if ll > 0.0 { (-(p.0 * dx + p.1 * dy) / ll).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 + t * dx).hypot(p.1 + t * dy)
}

/// The axial (Z) holes' reach and holder, read off the cycle records the
/// kernel emitted: from the material top over the drill's (or a wider
/// shank's) disk down to the cycle's bottom against the flute, and the
/// holder's face - `flute_len + shank_len` above the bottom - against
/// the material top over the holder's disk.
pub(super) fn axial_reach(t: &Toolpath, part: &[Triangle], stock: Option<&[Triangle]>, tool: &Tool, name: &str) -> Result<(), GenError> {
    let stock = stock.unwrap_or(&[]);
    let flute = finite_or_zero(tool.flute_len);
    let body = 0.5 * finite_or_zero(tool.diameter).max(finite_or_zero(tool.shank_d));
    for c in &t.cycles {
        let Cycle { xy, r_z, bottom_z, .. } = *c;
        let top = top_over(part, stock, xy.x, xy.y, body, r_z).unwrap_or(c.entry_z).max(c.entry_z.min(r_z));
        let depth = top - bottom_z;
        if depth.is_nan() || depth > flute + TOL {
            return Err(too_deep(
                &format!("hole at ({:.3}, {:.3})", xy.x, xy.y),
                depth,
                name,
                flute,
                "drill it with a longer drill, or in stages from both sides",
            ));
        }
        if let Some((reach, hr)) = holder(tool) {
            let face = bottom_z + reach;
            if let Some(ht) = top_over(part, stock, xy.x, xy.y, hr, r_z).filter(|&z| face < z - TOL) {
                return Err(GenError::Invalid(format!(
                    "tool {name}'s holder (dia. {:.3}, {reach:.3} mm above the tip) sinks to Z {face:.3} at the \
                     hole at ({:.3}, {:.3}), below the material top {ht:.3} around it - the holder feeds into \
                     the stock (a collision); use a longer tool or a slimmer holder",
                    2.0 * hr, xy.x, xy.y
                )));
            }
        }
    }
    Ok(())
}

/// Every move of the cross-drill chain (spindle frame) proven to keep
/// the holder clear of the stock: the holder is a horizontal cylinder
/// of radius `hr` whose face stands `reach` behind the tip along the
/// move's tool axis. A move is clear when the holder's lowest edge
/// rides above the stock's Z top (the end-face swing), or when the XY
/// rectangle the holder's footprint sweeps never comes inside the
/// stock's radius `st.r`. Anything else is refused by name - the swing
/// is not raised to make room (a holder that wide wants a clearance
/// the user sets, not one the kernel invents).
pub(super) fn cross_holder(t: &Toolpath, st: &Standoff, tool: &Tool, name: &str) -> Result<(), GenError> {
    let Some((reach, hr)) = holder(tool) else { return Ok(()) };
    let hl = finite_or_zero(tool.holder_len);
    for (i, m) in t.moves.iter().enumerate() {
        if st.z_max.is_some_and(|z| m.from.z.min(m.to.z) - hr >= z - TOL) {
            continue;
        }
        let axis = m.axis.unwrap_or([0.0, 0.0, 1.0]);
        let got = holder_approach(m.from, m.to, axis, reach, hl, hr);
        if got.is_nan() || got < st.r - TOL {
            return Err(GenError::Invalid(format!(
                "cross-drill move {i}: tool {name}'s holder (dia. {:.3}, {reach:.3} mm behind the tip) passes \
                 {got:.3} mm from the spindle, inside the stock's radius {:.3} - the holder sweeps the stock \
                 (a collision); use a longer tool, raise the clearance above the end face, or turn the bar \
                 to size first",
                2.0 * hr,
                st.r
            )));
        }
    }
    Ok(())
}

/// Closest XY approach to the spindle of the holder's footprint (a
/// rectangle `hl` long, `2 hr` wide, starting `reach` behind the tip
/// along the axis's XY part) swept from `a` to `b`: the convex hull of
/// its eight corners. An axis with no XY part stands the holder upright
/// - a disk of radius `hr` about the tip's XY.
fn holder_approach(a: Point3, b: Point3, axis: [f64; 3], reach: f64, hl: f64, hr: f64) -> f64 {
    let l = axis[0].hypot(axis[1]);
    if l.is_nan() || l <= 1e-9 {
        return (seg_dist((a.x, a.y), (b.x, b.y)) - hr).max(0.0);
    }
    let (ux, uy) = (axis[0] / l, axis[1] / l);
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::with_capacity(8);
    for p in [a, b] {
        for s in [reach * l, (reach + hl) * l] {
            for k in [hr, -hr] {
                pts.push((p.x + s * ux + k * nx, p.y + s * uy + k * ny));
            }
        }
    }
    hull_dist(&mut pts)
}

/// Distance from the origin to the convex hull of `pts` (0 inside).
fn hull_dist(pts: &mut [(f64, f64)]) -> f64 {
    if pts.iter().any(|(x, y)| !(x.is_finite() && y.is_finite())) {
        return f64::NAN;
    }
    pts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(2 * pts.len());
    let rev: Vec<(f64, f64)> = pts.iter().rev().copied().collect();
    for order in [&*pts, &rev[..]] {
        let start = hull.len();
        for &p in order {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    let n = hull.len();
    if n == 0 {
        return f64::NAN;
    }
    if n >= 3 && (0..n).all(|i| cross(hull[i], hull[(i + 1) % n], (0.0, 0.0)) >= 0.0) {
        return 0.0;
    }
    (0..n).map(|i| seg_dist(hull[i], hull[(i + 1) % n])).fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_distance_is_zero_inside_and_the_edge_gap_outside() {
        let mut sq = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        assert_eq!(hull_dist(&mut sq), 0.0);
        let mut off = [(2.0, -1.0), (3.0, -1.0), (3.0, 1.0), (2.0, 1.0)];
        assert!((hull_dist(&mut off) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_holder_swept_across_the_spindle_reaches_it() {
        // Tip from x=-25 to x=25, axis +X, holder face 22 behind: it
        // starts at x=-3, over the spindle.
        let d = holder_approach(Point3::new(-25.0, 0.0, 0.0), Point3::new(25.0, 0.0, 0.0), [1.0, 0.0, 0.0], 22.0, 35.0, 22.0);
        assert_eq!(d, 0.0);
        // A tip parked at x=25 with the holder outward: face at x=47.
        let p = Point3::new(25.0, 0.0, 0.0);
        let d = holder_approach(p, p, [1.0, 0.0, 0.0], 22.0, 35.0, 22.0);
        assert!((d - 47.0).abs() < 1e-9, "{d}");
    }
}
