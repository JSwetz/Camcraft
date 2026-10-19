//! One entry point per command: what every surface (ribbon, tree,
//! shortcuts) fires, so no surface forks command-queue logic of its own.
//! The menu surface retired with the imgui shell at M-g; two menu-only
//! helpers (`new_operation`, `delete_operation`) left with it - the
//! ribbon's Create/Delete cells already cover both effects through this
//! same module.
//!
//! Provenance: our own module, the standard "actions layer" pattern of CAM
//! shells. The rule it exists to enforce: a surface never pushes a
//! `CommandKind` directly and never writes world state - it calls one of
//! these. Enabling/disabling stays with the surface (it knows what the
//! user can act on), but the effect lives here exactly once.

use hsmworks_app::operation::SrcRef;

use crate::operations::quick_params;
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

/// Post every operation to one NC program (M1.7b). The dialog arm picks
/// the path; the gate itself - generated, fresh, warning-free or refused
/// by name - lives once in the app layer (hsmworks_app::posting).
pub(crate) fn post_program(state: &mut State) {
    state.pending.push_back(CommandKind::PostProgramDialog);
}

/// Close the app through the same queue as everything else (the window
/// button and, since M-g, Ctrl+Q both land here; the imgui File menu that
/// used to was retired with that shell).
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

/// View: look at the current target along a world direction - the
/// view cube's face/edge/corner law resolving to `Camera::toward`
/// (ephemeral, like the camera itself - no command; `target` and `dist`
/// survive, so orienting never pans or zooms).
pub(crate) fn orient_view(state: &mut State, d: [f64; 3]) {
    state.cam = state.cam.toward(d);
}

/// View: the house isometric (yaw 0.9, pitch 0.5) - what `FrameAll`
/// lands on, spelled as an orientation for the nav bar.
pub(crate) fn view_iso(state: &mut State) {
    state.cam = state.cam.oriented(0.9, 0.5);
}

/// Point the context panes at `t`. Pure view state: the panes read it
/// through `State::pane_target`, which validates it against the world each
/// frame. Surfaces that also change what the world highlights (tree nodes,
/// viewport picks) push a `Select` alongside; a bare retarget never does.
pub(crate) fn target(state: &mut State, t: UiTarget) {
    state.ui_target = t;
}

/// Open the tool editor window (M1.4) on `name` - the named tool through
/// the same sync law the panes use, so the fields (and the live preview)
/// start at the world's numbers; the empty name opens a numbered blank
/// draft as the creation form. View state like `target`: opening itself
/// queues nothing; closing is the window's ✕ or the ghost law in
/// `State::drain_actions`, and only the form's buttons ever push
/// Add/Change/Delete.
pub(crate) fn open_tool_editor(state: &mut State, name: &str) {
    if name.is_empty() {
        let draft = state.new_tool_draft();
        state.tool_ui = draft;
    } else {
        state.tool_ui.name = name.to_owned();
        state.sync_tool_ui();
    }
    state.tool_editor = Some(name.to_owned());
}

/// Quick-add an operation of `kind` (index into the `OpType` order the
/// Create group buttons follow): name it past the existing ones, give it
/// the newest tool, cut the stock with retyped default params, then make
/// it both selected and targeted so the Operations pane lands on it.
/// Without a tool the refusal is loud on the status line - the ribbon
/// disables the button too, but every surface shares this one effect.
pub(crate) fn add_operation(state: &mut State, kind: usize) {
    let Some(tool) = state.tool_names().pop() else {
        state.status = String::from("cannot add an operation: no tool in the document");
        return;
    };
    let mut n = state.operation_count() + 1;
    let name = loop {
        let candidate = format!("Operation{n}");
        if hsmworks_app::model::find_by_name(&state.world, &candidate).is_none() {
            break candidate;
        }
        n += 1;
    };
    let params = quick_params(kind);
    state.pending.push_back(CommandKind::CreateOperation {
        name: name.clone(),
        tool,
        src: SrcRef::Stock,
        params,
    });
    state.pending.push_back(CommandKind::Select(Some(name.clone())));
    state.ui_target = UiTarget::Operation(name);
}

/// Generate the selected operation, if any - surfaces gate the menu item
/// on the same question. Routes through the same arm as the pane button,
/// so a visible dirty draft is applied before generating from here too.
pub(crate) fn generate_selected(state: &mut State) {
    if let Some(name) = state.selected_operation() {
        state.generate_operation(&name);
    }
}
