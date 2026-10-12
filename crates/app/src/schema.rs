//! All Flecs component declarations live here (AGENTS.md layering rule).
//!
//! Provenance: our own domain model; no HSMWorks internals involved.
//!
//! Components are native Rust types owned by the ECS: `Vec`/`String` fields
//! are freed by flecs_ecs's registered drop hooks when an entity is deleted
//! or a component removed. Per-type arenas are a later (M4.3-era) perf
//! optimization, not current policy.

use flecs_ecs::prelude::*;

use crate::geometry::{IDENTITY_Q, Point3, Triangle};

/// Human-unique name of a persistent entity. The project serializer and
/// name-based command lookup both key on this.
#[derive(Component, Debug)]
pub struct Name(pub String);

/// Rigid placement of a model entity: world position plus orientation as a
/// unit quaternion (x, y, z, w). The UI exposes yaw only for now; the data
/// model carries full orientation so stock/fixture alignment later is an
/// editor change, not a schema change. Write it through `model::set_transform`,
/// which guarantees the quaternion is unit.
#[derive(Component, Debug, Clone, Copy)]
pub struct Location {
    pub pos: Point3,
    pub rot: [f64; 4],
}

impl Default for Location {
    /// Manual: a derived `Default` would store the zero quaternion, which is
    /// not a rotation.
    fn default() -> Self {
        Location {
            pos: Point3::ZERO,
            rot: IDENTITY_Q,
        }
    }
}

/// Triangulated geometry of a model. Rust-owned storage: freed on delete.
#[derive(Component, Debug)]
pub struct Facets {
    pub tris: Vec<Triangle>,
}

/// Selection marker (tag). Only `SelectEntity` writes it (AGENTS.md: changes
/// go through commands); the tree and viewport read it to highlight.
#[derive(Component, Debug)]
pub struct Selected;

/// Registers every schema component with a fresh world. Called by
/// `world::new_world()` so every binary (and every test) gets identical
/// registration.
pub fn register_schema(world: &World) {
    world.component::<Name>();
    world.component::<Location>();
    world.component::<Facets>();
    world.component::<Selected>();
}
