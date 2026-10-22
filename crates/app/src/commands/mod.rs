//! The command pattern: every persistent world mutation is an invertible
//! command, and the history is the single source of truth for undo/redo
//! (AGENTS.md). Parsing and validation happen at command *construction*, so
//! malformed input never enters the history ("validate early").

pub mod delete_entity;
pub mod import_model;
pub mod load_machine;
pub mod operation;
pub mod select_entity;
pub mod set_stock;
pub mod set_units;
pub mod set_wcs;
pub mod tool;
pub mod update_model;

use flecs_ecs::prelude::World;

pub use delete_entity::DeleteEntity;
pub use import_model::ImportModel;
pub use load_machine::LoadMachine;
pub use operation::{
    CreateOperation, DeleteOperation, EditOperationLinks, EditOperationParams, GenerateOperation,
    RegenerateAll, SetOperationGeometry,
};
pub use select_entity::SelectEntity;
pub use set_stock::SetStock;
pub use set_units::SetUnits;
pub use set_wcs::SetWcsOrigin;
pub use tool::{AddTool, ChangeTool, DeleteTool};
pub use update_model::UpdateModel;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("no entity named '{0}'")]
    NotFound(String),
    #[error("an entity named '{0}' already exists")]
    NameConflict(String),
    #[error("invalid parameters: {0}")]
    Invalid(String),
    #[error(transparent)]
    Import(#[from] crate::geometry::ImportError),
    #[error("'{0}' import is not supported yet; import a STEP, STL, or OBJ file instead")]
    UnsupportedType(String),
    #[error("could not generate the toolpath: {0}")]
    Machining(String),
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
