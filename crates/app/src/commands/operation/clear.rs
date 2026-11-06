//! Clear an operation's generated toolpath while keeping the operation itself.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the generate/delete siblings.

use flecs_ecs::prelude::{ComponentId, World};

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{Operation, Toolpath};

/// Throw away an operation's cached path - the "clear the generated
/// result, keep my setup" verb that Delete used to be mistaken for. The
/// operation's definition (tool, source, params, area, setup) is kept
/// verbatim; its stamp drops with the cache, so it reads "never
/// generated" until the next Generate. The constructor snapshots both
/// halves, so undo puts the exact cache bytes and stamp back - no
/// re-run, no drift. An operation with nothing cached is refused by
/// name: an undo entry that changes nothing reads as a dead Ctrl+Z.
pub struct ClearToolpath {
    name: String,
    prev_operation: Operation,
    prev_cache: Toolpath,
}

impl ClearToolpath {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev_operation = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let prev_cache = model::toolpath_of(world, &entity).ok_or_else(|| {
            CommandError::Invalid(format!("'{name}' has no generated toolpath to clear"))
        })?;
        Ok(ClearToolpath { name: name.to_string(), prev_operation, prev_cache })
    }
}

impl Command for ClearToolpath {
    fn apply(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return; // gone underneath us; nothing to clear
        };
        entity
            .entity_view(world)
            .set(Operation { stamp: None, ..self.prev_operation.clone() });
        // Data components are removed by id (the value form is for tags).
        entity.entity_view(world).remove(Toolpath::entity_id(world));
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        entity.entity_view(world).set(self.prev_operation.clone());
        entity.entity_view(world).set(self.prev_cache.clone());
    }
}
