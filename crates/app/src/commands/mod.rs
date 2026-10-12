//! The command pattern: every persistent world mutation is an invertible
//! command, and the history is the single source of truth for undo/redo
//! (AGENTS.md). Parsing and validation happen at command *construction*, so
//! malformed input never enters the history ("validate early").

pub mod delete_entity;
pub mod import_model;
pub mod select_entity;

use flecs_ecs::prelude::World;

pub use delete_entity::DeleteEntity;
pub use import_model::ImportModel;
pub use select_entity::SelectEntity;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("no entity named '{0}'")]
    NotFound(String),
    #[error("an entity named '{0}' already exists")]
    NameConflict(String),
    #[error(transparent)]
    Import(#[from] crate::geometry::ImportError),
}

pub trait Command {
    /// Forward effect.
    fn apply(&self, world: &World);
    /// Exact inverse of `apply`.
    fn invert(&self, world: &World);
}

/// Linear undo/redo over executed commands. Executing a fresh command clears
/// the redo stack (standard history semantics).
#[derive(Default)]
pub struct History {
    done: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl History {
    pub fn execute(&mut self, world: &World, command: impl Command + 'static) {
        command.apply(world);
        self.done.push(Box::new(command));
        self.redo.clear();
    }

    pub fn undo(&mut self, world: &World) -> bool {
        match self.done.pop() {
            Some(command) => {
                command.invert(world);
                self.redo.push(command);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, world: &World) -> bool {
        match self.redo.pop() {
            Some(command) => {
                command.apply(world);
                self.done.push(command);
                true
            }
            None => false,
        }
    }

    /// Run a command without touching either stack. Selection is a command
    /// (AGENTS.md) but is deliberately not on the undo timeline.
    pub fn execute_ephemeral(world: &World, command: impl Command) {
        command.apply(world);
    }

    /// Drop all history; used when a document replaces the current one.
    pub fn clear(&mut self) {
        self.done.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.done.is_empty() && self.redo.is_empty()
    }
}
