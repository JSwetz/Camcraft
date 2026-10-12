//! Entity tree panel: lists named models, click = `Select` command.
//! Provenance: our own pane, specced from public CAM object-manager trees.

use dear_imgui_rs::Ui;
use hsmworks_app::model;

use crate::state::{CommandKind, State};

pub fn models_panel(ui: &Ui, state: &mut State) {
    ui.window("Models").build(|| {
        let entities = model::named_entities(&state.world);
        if entities.is_empty() {
            ui.text_wrapped("No models. File > Open Model (Ctrl+O)");
            return;
        }
        for (name, entity) in entities {
            let selected = model::is_selected(&state.world, &entity);
            if ui.selectable_config(&name).selected(selected).build() && !selected {
                state.pending.push_back(CommandKind::Select(Some(name)));
            }
        }
    });
}
