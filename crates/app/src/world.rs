//! World construction: one blessed entry point so schema registration can
//! never be forgotten in a new binary.

use flecs_ecs::prelude::World;

use crate::schema;

pub fn new_world() -> World {
    let world = World::new();
    schema::register_schema(&world);
    world
}
