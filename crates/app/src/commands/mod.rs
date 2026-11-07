//! The command pattern: every persistent world mutation is an invertible
//! command, and the history is the single source of truth for undo/redo
//! (AGENTS.md). Parsing and validation happen at command *construction*, so
//! malformed input never enters the history ("validate early").

// M13.2: workpiece material + the feeds-and-speeds suggestion.
pub mod cutting_data;
pub mod delete_entity;
pub mod fixture;
pub mod import_model;
pub mod load_machine;
pub mod move_model;
pub mod operation;
pub mod post;
// M13.11: mesh repair (weld + fill holes) as one undo unit over UpdateModel.
pub mod repair_model;
pub mod select_entity;
pub mod set_stock;
pub mod setup;
pub mod set_units;
// M6.1 C: the document's theme choice, `SelectPost`'s twin.
pub mod theme;
// M13.4: the viewer's colour-vision remap, `SetTheme`'s twin.
pub mod color_vision;
pub mod set_wcs;
pub mod tool;
pub mod update_model;

use flecs_ecs::prelude::World;

pub use cutting_data::{ApplyCuttingData, SetWorkMaterial};
pub use delete_entity::DeleteEntity;
pub use fixture::{
    AddFixtureModel, DeleteFixture, MoveFixture, SetFixtureFixed, TagFixture, UntagFixture,
};
pub use import_model::ImportModel;
pub use load_machine::LoadMachine;
pub use move_model::MoveModel;
pub use operation::{
    ChangeOperationKind, ClearToolpath, CreateOperation, DeleteOperation, EditOperationLinks,
    EditOperationParams,
    GenerateOperation, PasteOperation, RegenerateAll, RenameOperation, ReorderOperation, SetOperationGeometry,
};
pub use post::SelectPost;
pub use repair_model::RepairModel;
pub use select_entity::SelectEntity;
pub use set_stock::SetStock;
pub use setup::{ActivateSetup, CreateSetup, DeleteSetup, SetOperationSetup};
pub use set_units::SetUnits;
pub use theme::{theme_of, theme_set, SetTheme};
pub use color_vision::{color_vision_of, color_vision_set, SetColorVision};
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
///
/// The history also knows whether the document has unsaved changes (#107):
/// `saved` is the undo depth at the last save or load. The document is
/// clean exactly when the current depth equals it - so undoing back to the
/// saved point is clean again, and redoing past it is dirty again. A fresh
/// command executed while the saved point sits on the redo stack discards
/// that state for good, so the saved point becomes unreachable (`None`).
pub struct History {
    done: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
    saved: Option<usize>,
}

impl Default for History {
    fn default() -> Self {
        Self { done: Vec::new(), redo: Vec::new(), saved: Some(0) }
    }
}

impl History {
    pub fn execute(&mut self, world: &World, command: impl Command + 'static) {
        if self.saved.is_some_and(|depth| depth > self.done.len()) {
            self.saved = None;
        }
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
    /// The replacing document (blank or loaded) starts clean.
    pub fn clear(&mut self) {
        self.done.clear();
        self.redo.clear();
        self.saved = Some(0);
    }

    /// Record the current position as the saved state (after a save or a
    /// load succeeded).
    pub fn mark_clean(&mut self) {
        self.saved = Some(self.done.len());
    }

    /// True when the document differs from the last save or load: the
    /// history position is not the recorded saved position.
    pub fn is_dirty(&self) -> bool {
        self.saved != Some(self.done.len())
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
