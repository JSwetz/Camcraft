//! The unsaved-changes prompt (#107): a modal over the shell while a New,
//! Open or Quit waits on `state.unsaved_prompt`. The three buttons answer
//! through the command queue (`actions::resolve_unsaved`); Escape or a
//! click on the backdrop is Cancel.
//!
//! Provenance: our own layout, specced from common desktop-editor
//! save-on-close prompts; egui's `Modal` container. No HSMWorks material.

use egui::{Id, Modal};

use crate::actions;
use crate::state::State;
use crate::unsaved::{UnsavedChoice, held_verb};

pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let Some(held) = state.unsaved_prompt.as_ref() else {
        return;
    };
    let verb = held_verb(held);
    let name = state
        .project_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map_or_else(|| String::from("Untitled"), |n| n.to_string_lossy().into_owned());
    let mut choice = None;
    let response = Modal::new(Id::new("unsaved-prompt")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        ui.heading("Unsaved changes");
        ui.label(format!("Save the changes to {name} before {verb}?"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                choice = Some(UnsavedChoice::Save);
            }
            if ui.button("Don't Save").clicked() {
                choice = Some(UnsavedChoice::DontSave);
            }
            if ui.button("Cancel").clicked() {
                choice = Some(UnsavedChoice::Cancel);
            }
        });
    });
    if choice.is_none() && response.should_close() {
        choice = Some(UnsavedChoice::Cancel);
    }
    if let Some(choice) = choice {
        actions::resolve_unsaved(state, choice);
    }
}
