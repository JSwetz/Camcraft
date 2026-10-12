//! Entity helpers over the schema components. The only place outside
//! commands/systems that touches flecs gets/sets, so the UI layer can stay
//! inside its layering rules (ui -> app -> flecs).
//!
//! `Entity` in flecs_ecs is a bare id; every verb goes through an
//! `EntityView` borrowed from the world, which is why each helper takes
//! `world` explicitly.

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{Aabb, IDENTITY_Q, Point3, Triangle, normalize_quat, transform_point};
use crate::schema::{Facets, Location, Name, Selected, StockDef, Wcs};
use crate::stock::StockShape;

/// Create a named model entity at the origin with no geometry yet.
#[must_use]
pub fn spawn(world: &World, name: &str) -> Entity {
    *world
        .entity()
        .set(Name(name.to_string()))
        .set(Location::default())
}

pub fn set_facets(world: &World, entity: &Entity, tris: Vec<Triangle>) {
    entity.entity_view(world).set(Facets { tris });
}

/// Set the translation half of the placement, keeping any existing rotation.
pub fn set_location(world: &World, entity: &Entity, p: Point3) {
    let rot = entity
        .entity_view(world)
        .try_get::<&Location>(|l| l.rot)
        .unwrap_or(IDENTITY_Q);
    entity.entity_view(world).set(Location { pos: p, rot });
}

/// Set the full rigid placement. Normalizes the quaternion at this single
/// write chokepoint: a zero or non-finite `rot` (a hand-edited file, a bad
/// parameter) becomes identity with a logged diagnostic, never NaN geometry
/// (Prime Directive).
pub fn set_transform(world: &World, entity: &Entity, mut loc: Location) {
    match normalize_quat(loc.rot) {
        Some(q) => loc.rot = q,
        None => {
            eprintln!(
                "model: rotation {:?} is not a usable quaternion; using identity",
                loc.rot
            );
            loc.rot = IDENTITY_Q;
        }
    }
    entity.entity_view(world).set(loc);
}

/// Delete an entity; Rust-owned components free via flecs's drop hooks.
pub fn remove(world: &World, entity: &Entity) {
    entity.entity_view(world).destruct();
}

#[must_use]
pub fn facets_of(world: &World, entity: &Entity) -> Option<Vec<Triangle>> {
    entity.entity_view(world).try_get::<&Facets>(|f| f.tris.clone())
}

/// Translation only - the common read for position-oriented code. Use
/// [`transform_of`] when orientation matters (render, pick, serialization).
#[must_use]
pub fn location_of(world: &World, entity: &Entity) -> Option<Point3> {
    entity.entity_view(world).try_get::<&Location>(|l| l.pos)
}

#[must_use]
pub fn transform_of(world: &World, entity: &Entity) -> Option<Location> {
    entity.entity_view(world).try_get::<&Location>(|l| *l)
}

/// Resolve an entity by its `Name` component (commands reference entities by
/// name so they stay invertible across undo/redo entity re-creation).
#[must_use]
pub fn find_by_name(world: &World, name: &str) -> Option<Entity> {
    let mut found = None;
    world.each_entity::<&Name>(|e, n| {
        if n.0 == name {
            found = Some(*e);
        }
    });
    found
}

/// All named entities, sorted by name - the serializer's iteration order.
#[must_use]
pub fn named_entities(world: &World) -> Vec<(String, Entity)> {
    let mut out: Vec<(String, Entity)> = Vec::new();
    world.each_entity::<&Name>(|e, n| {
        out.push((n.0.clone(), *e));
    });
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The named entities that are models - the user's parts. Stock and WCS
/// entities are setup furniture and live in the Setup pane, so the tree and
/// "fit to models" must skip them.
#[must_use]
pub fn model_entities(world: &World) -> Vec<(String, Entity)> {
    named_entities(world)
        .into_iter()
        .filter(|(_, e)| !is_stock(world, e) && !is_wcs(world, e))
        .collect()
}

#[must_use]
pub fn is_stock(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&StockDef>(|_| ()).is_some()
}

#[must_use]
pub fn is_wcs(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).has(Wcs)
}

/// The stock parameters of this entity, if it is the stock solid.
#[must_use]
pub fn stock_def_of(world: &World, entity: &Entity) -> Option<StockShape> {
    entity.entity_view(world).try_get::<&StockDef>(|s| s.shape.clone())
}

/// World-space bounding box of all model geometry. Stock and WCS are excluded
/// on purpose: fitting stock to a set that contains the stock would grow
/// forever on repeated Fit.
#[must_use]
pub fn solid_world_aabb(world: &World) -> Option<Aabb> {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut any = false;
    for (_, entity) in model_entities(world) {
        let Some(tris) = facets_of(world, &entity) else {
            continue;
        };
        let loc = transform_of(world, &entity).unwrap_or_default();
        for t in tris {
            for p in [t.a, t.b, t.c] {
                let p = transform_point(p, loc.pos, loc.rot);
                for axis in 0..3 {
                    let v = [p.x, p.y, p.z][axis];
                    if v.is_finite() {
                        min[axis] = min[axis].min(v);
                        max[axis] = max[axis].max(v);
                        any = true;
                    }
                }
            }
        }
    }
    any.then(|| Aabb {
        min: Point3::new(min[0], min[1], min[2]),
        max: Point3::new(max[0], max[1], max[2]),
    })
}

/// Names of all currently-selected entities.
#[must_use]
pub fn selected_names(world: &World) -> Vec<String> {
    named_entities(world)
        .into_iter()
        .filter(|(_, e)| e.entity_view(world).has(Selected))
        .map(|(name, _)| name)
        .collect()
}

pub fn is_selected(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).has(Selected)
}

/// Destruct every named entity (new/open document). Rust-owned components
/// free through flecs's drop hooks.
pub fn clear_named(world: &World) {
    for (_, entity) in named_entities(world) {
        remove(world, &entity);
    }
}
