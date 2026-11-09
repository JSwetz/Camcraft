//! Tool commands: add, edit and delete tool entities. Validation is all
//! at construction (AGENTS.md: malformed input never enters the history);
//! `apply` is a pure world write so redo replays identically.
//!
//! Provenance: our own command model; identity by name, snapshot-and-
//! restore inverses, like every sibling command.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{Location, STOCK_NAME, WCS_SLOTS, ToolBody, ToolDef};
use crate::tools::Tool;

pub(super) fn reserved(world: &World, name: &str) -> Option<CommandError> {
    if name.is_empty() {
        return Some(CommandError::Invalid("tool name must not be empty".into()));
    }
    // Setup furniture names (M3.2 A): the legacy `Stock`/`G54`..`G59`,
    // every declared setup's own names, and the `SetupN/...` convention
    // so a tool cannot squat on the entity a future setup will lazily
    // spawn.
    let taken = name == STOCK_NAME
        || WCS_SLOTS.contains(&name)
        || crate::setup::is_furniture_shape(name)
        || crate::setup::names(world).iter().any(|s| {
                crate::setup::stock_entity_name(world, Some(s)) == name
                    || crate::setup::wcs_entity_name(world, Some(s)) == name
            });
    if taken {
        return Some(CommandError::Invalid(format!(
            "'{name}' is reserved for setup furniture"
        )));
    }
    None
}

/// Create a tool entity with the given definition.
pub struct AddTool {
    name: String,
    tool: Tool,
}

impl AddTool {
    pub fn new(world: &World, name: &str, tool: Tool) -> Result<Self, CommandError> {
        if let Some(err) = reserved(world, name) {
            return Err(err);
        }
        if !tool.is_sane() {
            return Err(CommandError::Invalid(format!(
                "tool parameters must be finite and positive: {tool:?}"
            )));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        Ok(AddTool {
            name: name.to_string(),
            tool,
        })
    }
}

impl Command for AddTool {
    fn apply(&self, world: &World) {
        let entity = match model::find_by_name(world, &self.name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.name),
        };
        entity
            .entity_view(world)
            .set(ToolDef { tool: self.tool.clone(), number: None });
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}

/// Replace a tool's definition (the tool library's edit path). Operations
/// that reference the tool read the new numbers and derive Stale from the
/// stamp mismatch - no cascade writes here.
pub struct ChangeTool {
    name: String,
    prev: Tool,
    next: Tool,
}

impl ChangeTool {
    pub fn new(world: &World, name: &str, next: Tool) -> Result<Self, CommandError> {
        if !next.is_sane() {
            return Err(CommandError::Invalid(format!(
                "tool parameters must be finite and positive: {next:?}"
            )));
        }
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        // The tool law (M10.1 B): a retype that would leave an operation
        // holding a tool its family cannot cut with is refused by name.
        if let Some(msg) =
            crate::operation::tool_gate::retype_refusal(world, name, prev.kind, next.kind)
        {
            return Err(CommandError::Invalid(msg));
        }
        // #368: an edit that would stand the tool's body over its flutes
        // (or retype it to a lathe insert) is refused by name.
        if let Some(body) = crate::tool_body::body_of(world, &entity) {
            crate::tool_body::fits(&next, &body)
                .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        }
        Ok(ChangeTool {
            name: name.to_string(),
            prev,
            next,
        })
    }
}

impl Command for ChangeTool {
    // #221: the number is the tool table's, not the geometry's - a
    // ChangeTool (either way) keeps whatever number the tool has now.
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            let number = model::tool_number_of(world, &entity);
            entity
                .entity_view(world)
                .set(ToolDef { tool: self.next.clone(), number });
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            let number = model::tool_number_of(world, &entity);
            entity
                .entity_view(world)
                .set(ToolDef { tool: self.prev.clone(), number });
        }
    }
}

/// Delete a tool entity; the snapshot restores it identically, undoable
/// like every other persistent object.
pub struct DeleteTool {
    name: String,
    tool: Tool,
    /// #221: the tool's number, restored with it on undo.
    number: Option<u32>,
    /// #368: its modelled body, restored with it on undo.
    body: Option<ToolBody>,
    location: Location,
}

impl DeleteTool {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        Ok(DeleteTool {
            name: name.to_string(),
            tool,
            number: model::tool_number_of(world, &entity),
            body: crate::tool_body::body_of(world, &entity),
            location: model::transform_of(world, &entity).unwrap_or_default(),
        })
    }
}

impl Command for DeleteTool {
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
        entity
            .entity_view(world)
            .set(ToolDef { tool: self.tool.clone(), number: self.number });
        crate::tool_body::set_body(world, &entity, self.body.as_ref());
    }
}
