//! 5-axis flowline finishing (M5.1 tail): the finishing pass that
//! CURVES with the part. [`crate::finishing3d::flow`] is the
//! geometry - seeds, the exclusion-spaced line family, the march;
//! what lives here is the FIELD it runs on. Each facet asks the
//! shared curvature fit ([`super::curv`], the same numbers
//! [`super::ruling`] scans for swarf) for its least-curving
//! direction, and the pass direction at any column is that claim
//! read off the nearest facet centre. A facet that makes no claim
//! (flat ground, isotropic domes, a fit too starved to answer)
//! takes the region's dominant fallback: the double-angle area sum
//! of every claim, or plain +x when nothing claims at all. The
//! fallback matters: flat interiors must never be left without a
//! pass (the waterline rings do not machine them), so an
//! unclaimed column still cuts, straight - flow on plain ground
//! degrades to the parallel raster and says so in its path, never
//! in a warning.
//!
//! Provenance: flowline (principal-direction-following) finishing
//! is published multi-axis practice; reading the directions from a
//! least-squares shape operator is the multiscale curvature
//! literature [`super::curv`] cites. Written from those sources and
//! our own design notes. No vendor material consulted.

use std::collections::HashMap;

use crate::cl::Toolpath;
use crate::finishing3d::{self, Finish3DParams};
use crate::geo2d::Point2;
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::avoid::Shank;
use super::curv;
use super::stamp::stamp_axes;
use super::tilt::AxisControl;

/// The claim of one facet centre: its XY direction, or `None` for
/// a facet that makes no directional claim (and a query landing
/// nearest to it walks the fallback).
type Claim = Option<(f64, f64)>;

/// A grid bucket: facet centres (position, claim) keyed by cell.
type Grid = HashMap<(i64, i64), Vec<(f64, f64, Claim)>>;

/// The direction field, on a grid one cell wide per two mean facet
/// edges: a 3x3 query sees the centre whose Voronoi cell the column
/// stands in whenever the mesh is anywhere near uniform.
pub(super) struct Field {
    cell: f64,
    grid: Grid,
    fb: (f64, f64),
}

/// The field over a mesh: per-facet claims from the shared shape-
/// operator fit, the dominant fallback direction, and the lookup
/// grid. Never fails - a mesh too starved for curvature sees no
/// claims and the fallback's straight lines, which is the parallel
/// raster's answer spoken honestly.
fn build_field(facets: &[Triangle]) -> Field {
    let ops = curv::shape_operators(facets);
    let (mut vx, mut vy) = (0.0f64, 0.0f64);
    let (mut area_sum, mut n_live) = (0.0f64, 0usize);
    let mut grid: Grid = HashMap::new();
    let mut cent: Vec<(f64, f64, Claim)> = Vec::with_capacity(facets.len());
    for (i, f) in facets.iter().enumerate() {
        let (u, v) = (f.b - f.a, f.c - f.a);
        let cr = u.cross(v);
        let a2 = cr.dot(cr);
        let area = if a2.is_finite() && a2 > 0.0 { 0.5 * a2.sqrt() } else { 0.0 };
        area_sum += area;
        if area > 0.0 {
            n_live += 1;
        }
        let claim = ops
            .get(i)
            .and_then(|o| o.as_ref())
            .and_then(curv::min_dir_xy)
            .map(|d| (d[0], d[1]));
        if let Some((dx, dy)) = claim {
            // The double-angle vote: a ruling is a line, not a
            // vector, and theta and theta+PI must agree.
            let th = dy.atan2(dx);
            vx += area * (2.0 * th).cos();
            vy += area * (2.0 * th).sin();
        }
        let c = (
            (f.a.x + f.b.x + f.c.x) / 3.0,
            (f.a.y + f.b.y + f.c.y) / 3.0,
        );
        cent.push((c.0, c.1, claim));
    }
    let fb = if vx.is_finite() && vy.is_finite() && (vx * vx + vy * vy).sqrt() > 1e-12 {
        let th = 0.5 * vy.atan2(vx);
        (th.cos(), th.sin())
    } else {
        (1.0, 0.0)
    };
    let mean_edge = if n_live > 0 { (area_sum / n_live as f64).sqrt().max(1e-9) } else { 1.0 };
    let cell = (2.0 * mean_edge).max(1e-9);
    for (cx, cy, claim) in cent {
        if cx.is_finite() && cy.is_finite() {
            grid.entry(((cx / cell).floor() as i64, (cy / cell).floor() as i64))
                .or_default()
                .push((cx, cy, claim));
        }
    }
    Field { cell, grid, fb }
}

/// The field's answer at a column: the nearest facet centre's
/// claim, its fallback when the nearest centre claims nothing or
/// the grid is silent this far from any facet.
fn dir_at(f: &Field, q: Point2) -> (f64, f64) {
    let (ci, cj) = ((q.x / f.cell).floor() as i64, (q.y / f.cell).floor() as i64);
    let mut best: Option<f64> = None;
    let mut d: Claim = None;
    for di in -1..=1 {
        for dj in -1..=1 {
            if let Some(bucket) = f.grid.get(&(ci + di, cj + dj)) {
                for (cx, cy, claim) in bucket {
                    let (ex, ey) = (q.x - cx, q.y - cy);
                    let d2 = ex * ex + ey * ey;
                    if best.is_none_or(|b| d2 < b) {
                        best = Some(d2);
                        d = *claim;
                    }
                }
            }
        }
    }
    // Within reach only: the 3x3 window can see a centre a long
    // way off across an empty field; farther than two cells and
    // the column is over ground no facet answers for - the
    // fallback's straight lines are the honest answer there.
    let reach = (2.0 * f.cell) * (2.0 * f.cell);
    match d {
        Some(v) if best.is_some_and(|b| b <= reach) => v,
        _ => f.fb,
    }
}

/// The flowline finish with the tool posed by stage C's lean law
/// (and, with a `shank`, stage F's dodge): the curved-line family
/// over the shallow terrain, the waterline rings on the flanks, and
/// an axis on every cut move.
pub fn flow5d(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
    shank: Option<&Shank>,
) -> Result<Toolpath, GenError> {
    let field = build_field(facets);
    let base = finishing3d::flow(facets, p, &|q| dir_at(&field, q))?;
    stamp_axes(&base, facets, p.tool_diameter * 0.5, c, shank)
}

/// The SAME field law cut by a 3-axis machine (M2.5 E): seeds,
/// traced lines, grounded march, rings on the flanks - everything
/// `flow5d` runs, minus the pose. No `stamp_axes`, no `AxisControl`,
/// no shank: every move carries the vertical axis as the XYZ
/// families always have. The field is not re-derived here - two
/// truths, one code: this is `flow5d`'s `build_field` and the
/// catalog's `FinishPattern::Flow` door.
pub fn flow3d(
    facets: &[Triangle],
    p: &Finish3DParams,
) -> Result<Toolpath, GenError> {
    let field = build_field(facets);
    finishing3d::flow(facets, p, &|q| dir_at(&field, q))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> Triangle {
        use crate::geometry::Point3;
        Triangle::new(
            Point3::new(x0, y0, 0.0),
            Point3::new(x1, y1, 0.0),
            Point3::new(x2, y2, 0.0),
        )
    }

    #[test]
    fn a_planar_mesh_speaks_the_fallback_straight() {
        // Flat ground makes no directional claim anywhere; the
        // field's answer at any column is then the +x fallback -
        // the parallel raster's direction, so a flow pass over
        // plain ground cuts and never stalls.
        let facets = vec![
            tri(0.0, 0.0, 10.0, 0.0, 10.0, 10.0),
            tri(0.0, 0.0, 10.0, 10.0, 0.0, 10.0),
        ];
        let f = build_field(&facets);
        let d = dir_at(&f, Point2::new(5.0, 5.0));
        assert!((d.0 - 1.0).abs() < 1e-12 && d.1.abs() < 1e-12, "got {d:?}");
    }
}
