//! Operation lifecycle commands: create a new operation entity, delete one with its cache.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::cl::Toolpath as ClPath;
use crate::commands::{Command, CommandError};
use crate::model;
use crate::operation::{OpParams, SrcRef};
use crate::schema::{Location, Operation, Toolpath};
use crate::verify::Warning;

use super::gates::{area_gate, check_tool, family_gate, heights_gate, tool_gate, valid_src};

/// Create an operation entity. Deliberately does not generate: a new
/// operation reads "never generated" until `GenerateOperation` runs, the
/// same lifecycle the tree and status surface are built around.
pub struct CreateOperation {
    name: String,
    operation: Operation,
}

impl CreateOperation {
    pub fn new(
        world: &World,
        name: &str,
        tool: &str,
        src: SrcRef,
        params: OpParams,
    ) -> Result<Self, CommandError> {
        if name.is_empty() {
            return Err(CommandError::Invalid("operation name must not be empty".into()));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        check_tool(world, tool)?;
        // The new operation JOINS the active setup (M3.2 A) - declared at
        // creation, never inferred later, so undo replays membership
        // exactly. With no setups the name is `None`: the implicit-setup
        // law makes that the first/legacy context byte-identically.
        let setup = crate::setup::active(world);
        valid_src(world, setup.as_deref(), &src)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        // Ordering needs resolved heights, and the world is in hand right
        // here: an operation that would cut upward is refused at creation
        // (AGENTS.md: validate early), not deep in the generator - and
        // so is a From: Selection height its source cannot answer.
        heights_gate(world, setup.as_deref(), &src, &params)?;
        if let Some(msg) = crate::operation::side_sane(&params, &src) {
            return Err(CommandError::Invalid(msg));
        }
        family_gate(world, &params)?;
        tool_gate(world, &params, tool)?;
        area_gate(world, setup.as_deref(), &src, &params)?;
        Ok(CreateOperation {
            name: name.to_string(),
            operation: Operation {
                tool: tool.to_string(),
                src,
                params,
                stamp: None,
                setup,
                area: Default::default(),
            },
        })
    }
}

impl Command for CreateOperation {
    fn apply(&self, world: &World) {
        let entity = match model::find_by_name(world, &self.name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.name),
        };
        entity.entity_view(world).set(self.operation.clone());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}

/// Delete an operation with its cached path; the snapshot restores both.
pub struct DeleteOperation {
    name: String,
    operation: Operation,
    cache: Option<(ClPath, Vec<Warning>)>,
    location: Location,
}

impl DeleteOperation {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let operation = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let cache = model::toolpath_of(world, &entity).map(|t| (t.path, t.warnings));
        Ok(DeleteOperation {
            name: name.to_string(),
            operation,
            cache,
            location: model::transform_of(world, &entity).unwrap_or_default(),
        })
    }
}

impl Command for DeleteOperation {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }

    fn invert(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        entity.entity_view(world).set(self.operation.clone());
        if let Some((path, warnings)) = &self.cache {
            entity.entity_view(world).set(Toolpath {
                path: path.clone(),
                warnings: warnings.clone(),
            });
        }
    }
}
