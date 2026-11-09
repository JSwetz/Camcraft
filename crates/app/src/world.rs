//! World construction: one blessed entry point so schema registration can
//! never be forgotten in a new binary.

use flecs_ecs::prelude::World;

use crate::schema;

pub fn new_world() -> World {
    // #321: a new document starts with no carried body cached.
    crate::rest::clear_carried_cache();
    let world = World::new();
    schema::register_schema(&world);
    world
}
