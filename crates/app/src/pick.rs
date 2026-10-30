//! Ray picking over named models: the viewport click target.
//!
//! Provenance: the Möller–Trumbore ray/triangle intersection, reimplemented
//! from the published algorithm (Möller & Trumbore, J. Graphics Tools 1997);
//! no third-party code.

use flecs_ecs::prelude::World;

use crate::geometry::{
    Point3, Triangle, quat_conjugate, quat_rotate_point, transform_inverse_point, transform_point,
};
use crate::model;

/// Nearest named solid whose facet sits along `origin + t*dir`, `t > 0`.
/// Wrapper over [`first_hit_f`] that keeps only the entity name.
#[must_use]
pub fn first_hit(world: &World, origin: [f64; 3], dir: [f64; 3]) -> Option<String> {
    first_hit_f(world, origin, dir, |_| false).map(|(name, _, _)| name)
}

/// The full pick: nearest entity NAME, the index of the facet it hit (into
/// `model::facets_of` for that entity - the same order `world_facets`
/// transforms, so a viewer can light up exactly the triangle the ray hit),
/// and the world-space hit point. `skip` names entities the ray passes
/// through as if they weren't there - browser-eye-hidden models are
/// display-off, so picking what sits behind them is what the user sees.
///
/// Double-sided; degenerate or non-finite geometry is skipped, never fatal
/// (Prime Directive). Entities' rigid placements are honored by transforming
/// the ray into each entity's local frame.
///
/// Models win at any depth; stock is a fallback. The translucent box is
/// always in the way, and clicking "through the stock at the model" is what
/// the CAM user means (the roadmap's stock-is-pickable promise: click stock
/// where nothing is behind it). WCS entities never carry facets, so the
/// facet test already excludes them from picking.
#[must_use]
pub fn first_hit_f(
    world: &World,
    origin: [f64; 3],
    dir: [f64; 3],
    skip: impl Fn(&str) -> bool,
) -> Option<(String, usize, [f64; 3])> {
    if !origin.iter().all(|v| v.is_finite()) || !dir.iter().all(|v| v.is_finite()) {
        return None;
    }
    let o = Point3::new(origin[0], origin[1], origin[2]);
    let d = Point3::new(dir[0], dir[1], dir[2]);
    let mut best_model: Option<(f64, String, usize, [f64; 3])> = None;
    let mut best_stock: Option<(f64, String, usize, [f64; 3])> = None;
    for (name, entity) in model::named_entities(world) {
        if skip(&name) {
            continue;
        }
        let Some(tris) = model::facets_of(world, &entity) else {
            continue;
        };
        // One inverse rigid transform of the ray per entity, instead of one
        // shift per triangle. Rotations are unit quats, so distances along
        // the local ray equal world distances: depths stay comparable.
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let o_local = transform_inverse_point(o, loc.pos, loc.rot);
        let d_local = quat_rotate_point(quat_conjugate(loc.rot), d);
        if !(o_local.is_finite() && d_local.is_finite()) {
            continue;
        }
        let bucket = if model::is_stock(world, &entity) {
            &mut best_stock
        } else {
            &mut best_model
        };
        for (idx, t) in tris.iter().enumerate() {
            let Some(hit_t) = ray_triangle(*t, o_local, d_local) else {
                continue;
            };
            if bucket.as_ref().is_none_or(|(b, _, _, _)| hit_t < *b) {
                // Local hit point, pushed back through the entity's rigid
                // placement into the frame the viewer draws in.
                let hit_local = Point3::new(
                    o_local.x + d_local.x * hit_t,
                    o_local.y + d_local.y * hit_t,
                    o_local.z + d_local.z * hit_t,
                );
                let p = transform_point(hit_local, loc.pos, loc.rot);
                *bucket = Some((hit_t, name.clone(), idx, [p.x, p.y, p.z]));
            }
        }
    }
    best_model
        .or(best_stock)
        .map(|(_, name, facet, xyz)| (name, facet, xyz))
}

/// Ray/triangle crossing in the entity's local frame; the ray direction need
/// not be normalized. Returns t along the ray.
fn ray_triangle(facet: Triangle, origin: Point3, dir: Point3) -> Option<f64> {
    let (a, b, c) = (facet.a, facet.b, facet.c);
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return None;
    }
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 || !det.is_finite() {
        return None; // parallel or degenerate facet
    }
    let s = origin - a;
    let inv = 1.0 / det;
    let u = s.dot(p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if !(v >= -1e-9 && u + v <= 1.0 + 1e-9) {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t.is_finite() && t > 1e-9 {
        Some(t)
    } else {
        None
    }
}
