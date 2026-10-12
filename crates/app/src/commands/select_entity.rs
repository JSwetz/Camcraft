//! SelectEntity: the selection change command (AGENTS.md lists it explicitly).
//! Applied through `History::execute_ephemeral` - selection is not on the
//! undo timeline, but it is still a command and never an ad-hoc field poke.

use flecs_ecs::prelude::World;

use crate::commands::Command;
use crate::model;
use crate::schema::Selected;

pub struct SelectEntity {
    /// Entity to mark selected; `None` clears the selection.
    target: Option<String>,
    /// Names selected when the command was built, for an exact inverse.
    previous: Vec<String>,
}

impl SelectEntity {
    #[must_use]
    pub fn new(world: &World, target: Option<&str>) -> Self {
        SelectEntity {
            target: target.map(str::to_string),
            previous: model::selected_names(world),
        }
    }
}

impl Command for SelectEntity {
    fn apply(&self, world: &World) {
        set_selection(world, self.target.as_deref());
    }

    fn invert(&self, world: &World) {
        // Selection is not on the undo timeline; invert exists so the command
        // stays well-formed if that policy changes.
        for name in &self.previous {
            if let Some(entity) = model::find_by_name(world, name) {
                entity.entity_view(world).add(Selected);
            }
        }
    }
}

fn set_selection(world: &World, name: Option<&str>) {
    for (_, entity) in model::named_entities(world) {
        entity.entity_view(world).remove(Selected);
    }
    if let Some(name) = name
        && let Some(entity) = model::find_by_name(world, name)
    {
        entity.entity_view(world).add(Selected);
    }
}
