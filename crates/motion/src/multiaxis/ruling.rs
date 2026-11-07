//! The RULING finder of the swarf kinds (M5.1 stage D). A ruled
//! surface - the bladed family this pattern exists for - is swept
//! along straight lines: at every point one principal curvature
//! vanishes and its direction is the ruling. From a triangle soup
//! with no face structure, that direction field is recovered the
//! published way: the shape operator S = -dn/dp fitted per facet by
//! least squares over the normal differences of its neighbourhood
//! (differential geometry of meshes, the standard multiscale
//! curvature-estimation literature), restricted to the facet plane
//! and eigen-decomposed, and the region's single straight-line
//! direction found by scanning angles: the ruling angle is the one
//! along which the normals move the least. A model whose curvature
//! is the same in every direction (a dome) has no ruling to find and
//! is refused BY NAME, pointing at the kind that speaks for it; a
//! model ruled along Z (a vertical blade or cylinder wall) is
//! refused too, because the raster this direction drives is
//! XY-following - its flanks belong to the waterline rings, i.e. to
//! 5-Axis Contour.
//!
//! Provenance: ruled-surface side machining (swarf) and the
//! principal-direction characterization of developability (one
//! principal curvature zero) are textbook differential geometry;
//! the least-squares shape-operator fit over mesh neighbourhoods is
//! published multiscale curvature-estimation practice. Written from
//! those sources and our own design notes. The fit itself lives in
//! [`super::curv`], shared with the flowline field so the two kinds
//! can never see different curvatures.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::curv::{self, Op, s_mul};

/// The verdict of the field analysis: one global pass direction, and
/// which facets actually hold a ruling (the stamp's law choice).
#[derive(Clone, Debug)]
pub struct Field {
    /// The region's dominant ruling direction, unit, in XY - the
    /// raster's pass direction.
    pub ux: f64,
    pub uy: f64,
    /// Per input facet: it carries a straight direction whose
    /// curvature is negligible next to its cross-curvature.
    pub ruled: Vec<bool>,
}

/// A facet is "ruled" when its least-curving direction bends with
/// under a tenth of the curvature of its most-curving direction (a
/// cylinder: exactly 0; a sphere: ratio 1).
const RATIO_RULED: f64 = 0.1;
/// A facet whose biggest curvature is under this (1/mm - a radius of
/// curvature over ten metres) is FLAT for the law's purposes: no
/// direction claim either way, and it votes nothing.
const K_FLAT: f64 = 1e-4;
/// Share of area that must hold a ruling before the θ scan means
/// anything, and the share of answered-flat area that lets a
/// near-planar model through without one.
const RULED_SHARE: f64 = 0.05;
const FLAT_SHARE: f64 = 0.5;
/// A direction earns its name when it moves the normals under this
/// fraction of what the average direction moves them (the scan's
/// coherence law), and a vertical score under this fraction of the
/// best horizontal one says the ruling runs along Z.
const COHERENCE: f64 = 0.5;
/// Angles of the scan (half turn; a ruling is a line, not a vector).
const N_THETA: usize = 48;

/// Read the field: the pass direction the whole region shares, or
/// the named refusal that says which kind speaks for this model.
pub fn analyze(facets: &[Triangle]) -> Result<Field, GenError> {
    let ops = curv::shape_operators(facets);
    let (mut a_all, mut a_ruled, mut a_flat, mut a_ok) = (0.0, 0.0, 0.0, 0.0);
    let mut facet_area = Vec::with_capacity(facets.len());
    let mut ruled = Vec::with_capacity(facets.len());
    for (i, op) in ops.iter().enumerate() {
        // Area of a facet, recomputed cheaply from the same law the
        // fit used (None ops still count toward the total).
        let f = &facets[i];
        let (u, v) = (f.b - f.a, f.c - f.a);
        let cr = u.cross(v);
        let area = 0.5 * cr.dot(cr).sqrt();
        facet_area.push(if area.is_finite() && area > 0.0 { area } else { 0.0 });
        a_all += facet_area[i];
        match op {
            // No claim, but still a slot: `ruled` is indexed by facet
            // (the stamp's `ruled[fi]`, the scan's `ruled[i]`), and a
            // skipped push shifted every later facet's verdict - and
            // ran the scan off the end (#17).
            None => ruled.push(false),
            Some(o) => {
                a_ok += facet_area[i];
                if o.kmax2 <= K_FLAT * K_FLAT {
                    a_flat += facet_area[i];
                    ruled.push(false);
                } else if o.kmin2 < (RATIO_RULED * RATIO_RULED) * o.kmax2 {
                    a_ruled += facet_area[i];
                    ruled.push(true);
                } else {
                    ruled.push(false);
                }
            }
        }
    }
    if !a_all.is_finite() || a_all <= 0.0 {
        return Err(GenError::Invalid("swarf has no surface to read a ruling from".into()));
    }
    if a_ok < 0.25 * a_all {
        return Err(GenError::Invalid(
            "the mesh is too coarse for the ruling to be seen anywhere: refine it or finish with 5-Axis Parallel".into(),
        ));
    }
    if a_ruled < RULED_SHARE * a_all {
        if a_flat >= FLAT_SHARE * a_all {
            // Near-planar ground: there is no direction to honour and
            // nothing to refuse - every pass direction cuts the same
            // path, and the stamp poses by the lean law alone.
            return Ok(Field { ux: 1.0, uy: 0.0, ruled });
        }
        return Err(GenError::Invalid(
            "swarf found no ruling to follow - this surface curves about the same in every direction; doubly-curved ground is 5-Axis Parallel or Scallop work".into(),
        ));
    }
    // The scan, over ruled facets only: score(theta) is the area-
    // weighted squared normal-change along the direction theta -
    // the ruling is the angle that moves the surface least.
    let (mut best, mut best_k, mut sum) = (f64::INFINITY, 0usize, 0.0);
    for k in 0..N_THETA {
        let th = std::f64::consts::PI * (k as f64) / (N_THETA as f64);
        let s = score_dir(&ops, &facet_area, &ruled, Point3::new(th.cos(), th.sin(), 0.0));
        sum += s;
        if s < best {
            (best, best_k) = (s, k);
        }
    }
    let mean = sum / (N_THETA as f64);
    let sv = score_dir(&ops, &facet_area, &ruled, Point3::new(0.0, 0.0, 1.0));
    if sv <= COHERENCE * best {
        return Err(GenError::Invalid(
            "this model's ruling runs along Z (vertical walls): its flanks belong to the waterline rings, which is 5-Axis Contour work".into(),
        ));
    }
    if best > COHERENCE * mean {
        return Err(GenError::Invalid(
            "the ruling turns across the region - no single straight pass direction follows it; split the ruled faces or finish with 5-Axis Parallel".into(),
        ));
    }
    let th = std::f64::consts::PI * (best_k as f64) / (N_THETA as f64);
    Ok(Field { ux: th.cos(), uy: th.sin(), ruled })
}

/// The area-weighted squared normal-change along a direction: each
/// ruled facet contributes `area * |S * d3|^2`, where `d3` is the
/// direction projected into the facet's own plane (a facet seen
/// edge-on to the pass moves its normals nowhere along it, and one
/// whose plane contains the direction says `d3 = d` exactly).
fn score_dir(ops: &[Option<Op>], area: &[f64], ruled: &[bool], d: Point3) -> f64 {
    let mut s = 0.0;
    for (i, op) in ops.iter().enumerate() {
        if !(ruled[i]) {
            continue;
        }
        let Some(o) = op else { continue };
        let n = o.n;
        let dn = n.dot(d);
        let d3 = Point3::new(d.x - n.x * dn, d.y - n.y * dn, d.z - n.z * dn);
        let sd = s_mul(&o.s, d3);
        s += area[i] * (sd.x * sd.x + sd.y * sd.y + sd.z * sd.z);
    }
    s
}

