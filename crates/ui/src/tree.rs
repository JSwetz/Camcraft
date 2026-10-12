//! Entity tree panel: lists named models, click = `Select` command.
//! Provenance: our own pane, specced from public CAM object-manager trees.

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::model;

use crate::state::{CommandKind, State};

pub fn models_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("models", "Models") else {
        return;
    };
    ui.window(key.label("Models")).build(|| {
        let entities = model::named_entities(&state.world);
        if entities.is_empty() {
            ui.text_wrapped(
                "No models yet.\n\nFile > Open Model (Ctrl+O)\nor drag a file onto the window.\n\n\
                 In the viewport: click selects, middle-drag orbits, right-drag pans, \
                 wheel zooms toward the cursor, F frames all.",
            );
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
