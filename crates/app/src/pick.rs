//! Ray picking over named models: the viewport click target.
//!
//! Provenance: the Möller–Trumbore ray/triangle intersection, reimplemented
//! from the published algorithm (Möller & Trumbore, J. Graphics Tools 1997);
//! no third-party code.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle};
use crate::model;

/// Nearest named model whose facet sits along `origin + t*dir`, `t > 0`.
/// Double-sided; degenerate or non-finite geometry is skipped, never fatal
/// (Prime Directive).
#[must_use]
pub fn first_hit(world: &World, origin: [f64; 3], dir: [f64; 3]) -> Option<String> {
    if !origin.iter().all(|v| v.is_finite()) || !dir.iter().all(|v| v.is_finite()) {
        return None;
    }
    let o = Point3::new(origin[0], origin[1], origin[2]);
    let d = Point3::new(dir[0], dir[1], dir[2]);
    let mut best: Option<(f64, String)> = None;
    for (name, entity) in model::named_entities(world) {
        let Some(tris) = model::facets_of(world, &entity) else {
            continue;
        };
        let loc = model::location_of(world, &entity).unwrap_or(Point3::ZERO);
        for t in tris {
            let Some(hit_t) = ray_triangle(t, loc, o, d) else {
                continue;
            };
            if best.as_ref().is_none_or(|(b, _)| hit_t < *b) {
                best = Some((hit_t, name.clone()));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// Ray/triangle crossing in world space; the ray direction need not be
/// normalized. Returns t along the ray.
fn ray_triangle(facet: Triangle, off: Point3, origin: Point3, dir: Point3) -> Option<f64> {
    let a = shift(facet.a, off);
    let b = shift(facet.b, off);
    let c = shift(facet.c, off);
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

fn shift(p: Point3, o: Point3) -> Point3 {
    Point3::new(p.x + o.x, p.y + o.y, p.z + o.z)
}
