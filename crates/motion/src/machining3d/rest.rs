//! The 3-D ladder's islands law: the part material standing above a
//! level plane (split from `machining3d.rs` by M5.0.1; the hanging
//! shadow added by the blind-floor fix). Provenance: plane sectioning
//! plus the XY shadow of a closed solid's down-facing boundary - the
//! standard projection identity of computational geometry (a column's
//! solid above a plane is either cut by the plane or starts at a
//! downward-facing face above it); no vendor material consulted.

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
/// Half-open slices sit exactly on vertices for tessellated steps; this
/// is the same lattice the weld uses, so a grazing plane nudges rather
/// than lies.
pub(super) const EPS: f64 = 1e-6;

/// A hanging shadow that overlaps a level's target by less than this
/// (mm²) changes nothing a cutter can reach: the target is kept as it
/// was, byte for byte (the region ops would only re-normalize it).
const AREA_EPS: f64 = 1e-6;

pub(super) fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The part material standing above `z`, projected to XY: the model's
/// cross-section strictly above the plane (nudge-retried when a plane
/// grazes a tessellated shelf - reading an errored slice as no-island
/// would be a silent gouge, the retry lands above the shelf instead,
/// failing safe), united with the shadow of every down-facing face
/// above it ([`hanging_above`]). The cross-section alone is only the
/// whole story for a part solid all the way down to the plane: a plane
/// under the part's underside - a blind floor's level run to the stock
/// bottom - or under an overhang sections nothing there, yet the
/// material above it shadows every column it covers. Public because it
/// is the islands law itself: the generator reads it to shape each
/// level, and the tests pin the monotonicity the ladder assumes (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    let cut = slice::material_above(facets, z).map_err(from_slice)?;
    let hang = hanging_above(facets, z)?;
    if hang.is_empty() {
        return Ok(cut);
    }
    Ok(cut.union(&hang)?)
}

/// A level's target: the footprint minus the material standing above
/// `z` ([`material_above`]'s law). Read in two steps so a part solid
/// down to the plane - every level a ladder cut before the blind-floor
/// fix - keeps its target exactly: the hanging shadow is subtracted
/// only when it overlaps what the cross-section left.
pub(super) fn level_target(facets: &[Triangle], footprint: &Region, z: f64) -> Result<Region, GenError> {
    let islands = slice::material_above(facets, z).map_err(from_slice)?;
    let target = footprint.difference(&islands)?;
    if target.is_empty() {
        return Ok(target);
    }
    let hang = hanging_above(facets, z)?;
    if hang.is_empty() || target.intersect(&hang)?.area() <= AREA_EPS {
        return Ok(target);
    }
    Ok(target.difference(&hang)?)
}

/// The XY shadow of the model's down-facing faces strictly above `z`
/// (each clipped to the half-space over the plane). Together with the
/// cross-section it is the projection of all material above the plane:
/// a column the plane finds in air but material crosses higher up meets
/// that material first at a face whose outward normal points down. Facet
/// winding is the outward right-hand rule every importer and primitive
/// here keeps (`Triangle::normal`).
pub(super) fn hanging_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    if !z.is_finite() {
        return Err(GenError::Invalid("non-finite level plane".into()));
    }
    let c = z + EPS;
    let mut loops = Vec::new();
    for t in facets {
        if t.a.z.max(t.b.z).max(t.c.z) <= c {
            continue; // wholly at or under the plane
        }
        let n = (t.b - t.a).cross(t.c - t.a);
        if n.z.is_nan() || n.z >= 0.0 {
            continue; // up-facing, vertical, degenerate or non-finite
        }
        let poly = clip_above([t.a, t.b, t.c], c);
        // Down-facing projects clockwise; the union wants material CCW.
        let pts: Vec<Point2> = poly.iter().rev().map(|p| Point2::new(p.x, p.y)).collect();
        if pts.len() < 3 {
            continue;
        }
        if let Ok(l) = Loop::new(pts)
            && l.area() > 0.0
        {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Ok(Region::empty());
    }
    Ok(Region::union_all(loops)?)
}

/// The part of a triangle at or above the plane `c` (Sutherland-Hodgman
/// against one half-space): 0, 3 or 4 vertices, in the input's order.
fn clip_above(v: [Point3; 3], c: f64) -> Vec<Point3> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (p, q) = (v[i], v[(i + 1) % 3]);
        let (pin, qin) = (p.z >= c, q.z >= c);
        if pin {
            out.push(p);
        }
        if pin != qin {
            let t = (c - p.z) / (q.z - p.z);
            out.push(Point3::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t, c));
        }
    }
    out
}
