//! All Flecs component declarations live here (AGENTS.md layering rule).
//!
//! Provenance: our own domain model; no HSMWorks internals involved.
//!
//! Components are native Rust types owned by the ECS: `Vec`/`String` fields
//! are freed by flecs_ecs's registered drop hooks when an entity is deleted
//! or a component removed. Per-type arenas are a later (M4.3-era) perf
//! optimization, not current policy.

use flecs_ecs::prelude::*;

use crate::geometry::{Point3, Triangle};

/// Human-unique name of a persistent entity. The project serializer and
/// name-based command lookup both key on this.
#[derive(Component, Debug)]
pub struct Name(pub String);

/// Placement of a model entity. Translation-only for now; upgrade to a full
/// rigid transform lands with M1.2 stock orientation.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Location {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Triangulated geometry of a model. Rust-owned storage: freed on delete.
#[derive(Component, Debug)]
pub struct Facets {
    pub tris: Vec<Triangle>,
}

/// Registers every schema component with a fresh world. Called by
/// `world::new_world()` so every binary (and every test) gets identical
/// registration.
pub fn register_schema(world: &World) {
    world.component::<Name>();
    world.component::<Location>();
    world.component::<Facets>();
}

#[must_use]
pub fn point_of(loc: &Location) -> Point3 {
    Point3::new(loc.x, loc.y, loc.z)
}
