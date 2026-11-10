//! World construction: one blessed entry point so schema registration can
//! never be forgotten in a new binary.
//!
//! Provenance: our own wrapper over flecs_ecs; the first-world race note
//! below comes from reading the vendored flecs C source (MIT), #379.

use flecs_ecs::core::ecs_os_api;
use flecs_ecs::prelude::World;
use std::sync::Once;

use crate::schema;

/// Every world goes through here: no other code may call `World::new`
/// (a world made elsewhere on another thread would bring back #379).
pub fn new_world() -> World {
    init_flecs_os_api();
    // #321: a new document starts with no carried body cached.
    crate::rest::clear_carried_cache();
    let world = World::new();
    schema::register_schema(&world);
    world
}

/// Fill in flecs' process-global OS API (malloc, free, abort, the clock)
/// once, before any world exists.
///
/// flecs fills that struct in without a lock on the first world.
/// `flecs_ecs::ecs_os_api::ensure_initialized` claims it under a mutex,
/// but writes it after the mutex is released. Meanwhile `ecs_mini` (the
/// C side of `World::new`) copies the struct and writes it back. So a
/// second thread building its first world at that moment could copy a
/// half-written struct. Then `ecs_mini` aborted (ECS_MISSING_OS_API) or
/// called a NULL hook (SIGSEGV). Under load, the test binaries that
/// build worlds on many threads crashed this way about once in 170
/// runs (#379).
///
/// Once the first initialisation has finished, flecs marks the API
/// initialised. After that, a world's construction only reads it (the
/// write-back is refused). So finishing it once here, while every
/// other caller waits, closes the race. `ensure_initialized` can panic
/// only if its own mutex is poisoned, and nothing else takes that mutex.
fn init_flecs_os_api() {
    static FLECS_OS_API: Once = Once::new();
    FLECS_OS_API.call_once(ecs_os_api::ensure_initialized);
}
