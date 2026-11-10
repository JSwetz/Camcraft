//! Mate datums from picks (#561): a picked flat face read as a plane (a
//! point on it and its OUTWARD unit normal), or a picked vertex read as
//! a point, placed through a caller-given pose - the setup frame the
//! fixture locate solves in. The same face and vertex readers the 3-2-1
//! work-offset alignment (#187) and the lay-on-a-plane picks (#330)
//! use, so the setup and fixture tools share one picker.
//!
//! Provenance: plane and point datums of 3-2-1 fixturing as taught in
//! machine-tool and fixture-design handbooks; the area-weighted centroid
//! is standard computational geometry. Our own code, no vendor material.

use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{Point3, feature, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;

use super::align::{gone, outward_normal};
use super::face::facets_centroid;

/// A face with less area than this (mm^2) is degenerate.
const MIN_FACE_AREA_MM2: f64 = 1e-9;

/// One side of a mate, in the frame the caller placed it in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MateDatum {
    /// A flat face: a point on it (its area-weighted centroid) and its
    /// outward unit normal.
    Plane { at: [f64; 3], normal: [f64; 3] },
    /// A vertex.
    Point([f64; 3]),
}

/// The identity pose: the readers below answer in the entity's own frame
/// and [`mate_datum`] places the answer itself.
fn local(_: &World, _: &Entity) -> Option<Location> {
    None
}

/// Read a pick as a mate datum, placed by `pose`. Edges and curved faces
/// are refused by name (a mate is a face or a point), as are faces with
/// no area or no usable plane and picks no longer in the model.
pub fn mate_datum(world: &World, g: &GeoSel, pose: Location) -> Result<MateDatum, String> {
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let place = |p: [f64; 3]| {
        let w = transform_point(Point3::new(p[0], p[1], p[2]), pose.pos, pose.rot);
        [w.x, w.y, w.z]
    };
    match g.feature {
        FeatureSel::Vertex(id) => {
            let vi = feature::resolve_vertex(&features, id).ok_or_else(|| gone(g))?;
            let v = features.verts.get(vi as usize).ok_or_else(|| gone(g))?;
            let p = place(*v);
            if !p.iter().all(|c| c.is_finite()) {
                return Err(format!("{} of '{}' is not a finite point - re-pick it", g.feature, g.entity));
            }
            Ok(MateDatum::Point(p))
        }
        FeatureSel::Face(fid) => {
            let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(|| gone(g))?;
            if f.kind == feature::FaceKind::Degenerate {
                return Err(format!("{} of '{}' is degenerate (no usable plane) - pick another face", g.feature, g.entity));
            }
            let tris = model::facets_of(world, &e).ok_or_else(|| gone(g))?;
            if face_area(&tris, &f.facets) <= MIN_FACE_AREA_MM2 {
                return Err(format!("{} of '{}' has no area - pick a face with a surface", g.feature, g.entity));
            }
            let n = outward_normal(world, g, local)?
                .ok_or_else(|| format!("{} of '{}' is not flat - pick a flat face or a vertex", g.feature, g.entity))?;
            let c = facets_centroid(&tris, f.facets.iter().map(|&i| i as usize))
                .ok_or_else(|| format!("{} of '{}' has no area - pick a face with a surface", g.feature, g.entity))?;
            let wn = quat_rotate_point(pose.rot, Point3::new(n[0], n[1], n[2]));
            let normal = [wn.x, wn.y, wn.z];
            let at = place(c);
            if !at.iter().chain(&normal).all(|v| v.is_finite()) {
                return Err(format!("{} of '{}' has no usable plane - re-pick it", g.feature, g.entity));
            }
            Ok(MateDatum::Plane { at, normal })
        }
        FeatureSel::Patch(_) => {
            Err(format!("{} of '{}' is curved - pick a flat face or a vertex", g.feature, g.entity))
        }
        FeatureSel::Chain(_) => {
            Err(format!("{} of '{}' is an edge - pick a flat face or a vertex", g.feature, g.entity))
        }
    }
}

/// The summed facet area of a face (non-finite facets weigh nothing).
fn face_area(tris: &[crate::geometry::Triangle], facets: &[u32]) -> f64 {
    facets
        .iter()
        .filter_map(|&i| tris.get(i as usize))
        .map(|t| {
            let n = (t.b - t.a).cross(t.c - t.a);
            0.5 * n.dot(n).sqrt()
        })
        .filter(|a| a.is_finite())
        .sum()
}
