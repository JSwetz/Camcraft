//! Entity helpers over the schema components. The only place outside
//! commands/systems that touches flecs gets/sets, so the UI layer can stay
//! inside its layering rules (ui -> app -> flecs).
//!
//! `Entity` in flecs_ecs is a bare id; every verb goes through an
//! `EntityView` borrowed from the world, which is why each helper takes
//! `world` explicitly.

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::geometry::{IDENTITY_Q, Point3, Triangle, normalize_quat};
use crate::schema::{Facets, Location, Name, Selected};

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
