//! SetToolNumber: give a tool its controller number, or clear it (#221).
//!
//! The number is what the post writes as `T..` / `H..`, matching the
//! machine's tool table. The command writes only `ToolDef::number` and its
//! inverse writes the previous value back, so a renumber is undoable and a
//! document change like any other. Provenance: our own command, mirroring
//! `ChangeTool`'s shape; specced from issue #221.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::ToolDef;
use crate::tool_numbers;

pub struct SetToolNumber {
    name: String,
    prev: Option<u32>,
    next: Option<u32>,
}

impl SetToolNumber {
    /// Set `name`'s number to `next` (`None` clears it). Refused: an unknown
    /// or non-tool name, a number outside 1-99999, a number another tool
    /// already has as its own, and a no-op.
    pub fn new(world: &World, name: &str, next: Option<u32>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        if model::tool_of(world, &entity).is_none() {
            return Err(CommandError::Invalid(format!("'{name}' is not a tool")));
        }
        if let Some(n) = next {
            if !tool_numbers::valid(n) {
                return Err(CommandError::Invalid(format!(
                    "tool number {n} is out of range - tool numbers run {}",
                    tool_numbers::RANGE_TEXT
                )));
            }
            let taken = model::named_entities(world).into_iter().find(|(other, e)| {
                other != name && model::tool_number_of(world, e) == Some(n)
            });
            if let Some((other, _)) = taken {
                return Err(CommandError::Invalid(format!(
                    "tool number {n} is already '{other}''s"
                )));
            }
        }
        let prev = model::tool_number_of(world, &entity);
        if prev == next {
            return Err(CommandError::Invalid(format!(
                "'{name}' already has that tool number"
            )));
        }
        Ok(Self {
            name: name.to_string(),
            prev,
            next,
        })
    }
}

fn write(world: &World, name: &str, number: Option<u32>) {
    if let Some(entity) = model::find_by_name(world, name)
        && let Some(tool) = model::tool_of(world, &entity)
    {
        entity.entity_view(world).set(ToolDef { tool, number });
    }
}

impl Command for SetToolNumber {
    fn apply(&self, world: &World) {
        write(world, &self.name, self.next);
    }

    fn invert(&self, world: &World) {
        write(world, &self.name, self.prev);
    }
}
