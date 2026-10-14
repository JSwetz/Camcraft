//! One entry point per command: what every surface (menu, ribbon, tree,
//! shortcuts) fires, so no surface forks command-queue logic of its own.
//!
//! Provenance: our own module, the standard "actions layer" pattern of CAM
//! shells. The rule it exists to enforce: a surface never pushes a
//! `CommandKind` directly and never writes world state - it calls one of
//! these. Enabling/disabling stays with the surface (it knows what the
//! user can act on), but the effect lives here exactly once.

use crate::state::{CommandKind, State, UiTarget};

/// File menu: ask for a mesh to import (the dialog itself runs in
/// `State::run`, never mid-frame).
pub(crate) fn open_model_dialog(state: &mut State) {
    state.pending.push_back(CommandKind::OpenModelDialog);
}

pub(crate) fn new_document(state: &mut State) {
    state.pending.push_back(CommandKind::NewDocument);
}

pub(crate) fn open_project(state: &mut State) {
    state.pending.push_back(CommandKind::OpenProjectDialog);
}

pub(crate) fn save_project(state: &mut State) {
    state.pending.push_back(CommandKind::SaveProjectDialog);
}

pub(crate) fn quit(state: &mut State) {
    state.pending.push_back(CommandKind::Quit);
}

pub(crate) fn undo(state: &mut State) {
    state.pending.push_back(CommandKind::Undo);
}

pub(crate) fn redo(state: &mut State) {
    state.pending.push_back(CommandKind::Redo);
}

pub(crate) fn delete_selected(state: &mut State) {
    state.pending.push_back(CommandKind::DeleteSelected);
}

/// View: reframe the camera on the whole document (ephemeral, like the
/// camera itself - no command).
pub(crate) fn frame_all(state: &mut State) {
    state.pending.push_back(CommandKind::FrameAll);
}

/// Point the context panes at `t`. Pure view state: the panes read it
/// through `State::pane_target`, which validates it against the world each
/// frame. Surfaces that also change what the world highlights (tree nodes,
/// viewport picks) push a `Select` alongside; a bare retarget never does.
pub(crate) fn target(state: &mut State, t: UiTarget) {
    state.ui_target = t;
}

/// Show the new-operation form: nothing targeted, and the selection
/// cleared so the panes' fallback can't resolve straight back onto some
/// other operation. The `Select(None)` is the command; the blanked target
/// is view state.
pub(crate) fn new_operation(state: &mut State) {
    state.ui_target = UiTarget::None;
    state.pending.push_back(CommandKind::Select(None));
}

/// Generate the selected operation, if any - surfaces gate the menu item
/// on the same question.
pub(crate) fn generate_selected(state: &mut State) {
    if let Some(name) = state.selected_operation() {
        state.pending.push_back(CommandKind::GenerateOperation(name));
    }
}

pub(crate) fn delete_operation(state: &mut State) {
    if let Some(name) = state.selected_operation() {
        state.pending.push_back(CommandKind::DeleteOperation(name));
    }
}
