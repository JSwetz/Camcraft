//! Entity tree panel: lists models and operations, click = `Select` command.
//! Provenance: our own pane, specced from public CAM object-manager trees.
//!
//! Stock and work offsets are setup furniture, not models: they are edited
//! in the Setup pane and deliberately filtered out here (`model_entities`).
//! Tools and operations have their own sections and components; they are
//! filtered out of the model list the same way (`is_tool`/`is_operation`).

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::model;
use hsmworks_app::operation::{self, OpStatus};

use crate::state::{CommandKind, State};

pub fn models_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("models", "Models") else {
        return;
    };
    ui.window(key.label("Models")).build(|| {
        // View toggle, not a command (AGENTS.md ephemeral UI state, like the
        // camera): hides the dimmed world origin axes - the coordinate
        // system a model lands on - while setting up G5x offsets. Real work
        // offset triads are setup, not furniture, and always show.
        ui.checkbox("Show world origin axes", &mut state.show_world_axes);
        // Also view state, not a command: which cached paths are on screen.
        ui.checkbox("Show all toolpaths", &mut state.show_all_paths);
        ui.separator();
        ui.text("Models");
        let entities = model::model_entities(&state.world);
        if entities.is_empty() {
            ui.text_wrapped(
                "No models yet.\n\nFile > Open Model (Ctrl+O)\nor drag a file onto the window.\n\n\
                 In the viewport: click selects, middle-drag orbits, right-drag pans, \
                 wheel zooms toward the cursor, F frames all.",
            );
        }
        for (name, entity) in entities {
            let selected = model::is_selected(&state.world, &entity);
            if ui.selectable_config(&name).selected(selected).build() && !selected {
                state.pending.push_back(CommandKind::Select(Some(name)));
            }
        }
        ui.separator();
        ui.text("Operations");
        operations_section(ui, state);
    });
}

/// One line per operation: name, type, derived status, warning count.
/// Status is recomputed on read (`operation::status_of`) - it is never
/// stored, so it cannot lie about the world (AGENTS.md).
fn operations_section(ui: &Ui, state: &mut State) {
    let names: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .map(|(name, _)| name)
        .collect();
    if names.is_empty() {
        ui.text_wrapped(
            "No operations yet.\n\nAdd a tool in the Tool Library, then create\n\
             operations in the Properties panel.",
        );
        return;
    }
    let selected = state.selected_operation();
    for name in names {
        let Some(entity) = model::find_by_name(&state.world, &name) else {
            continue;
        };
        let kind = model::operation_of(&state.world, &entity)
            .map(|op| op.params.kind().to_string())
            .unwrap_or_default();
        let tag = match operation::status_of(&state.world, &entity) {
            OpStatus::Never => "never",
            OpStatus::Ok => "ok",
            OpStatus::Stale => "STALE",
        };
        let warns = model::toolpath_of(&state.world, &entity)
            .map_or(0, |tp| tp.warnings.len());
        let warn_text = if warns == 1 {
            String::from("  1 warning")
        } else if warns > 1 {
            format!("  {warns} warnings")
        } else {
            String::new()
        };
        let label = format!("{name}  {kind}  {tag}{warn_text}");
        let is_sel = selected.as_deref() == Some(name.as_str());
        if ui.selectable_config(&label).selected(is_sel).build() && !is_sel {
            state.pending.push_back(CommandKind::Select(Some(name)));
        }
    }
    if let Some(name) = state.selected_operation() {
        if ui.button("Generate##tree") {
            state.pending.push_back(CommandKind::GenerateOperation(name.clone()));
        }
        ui.same_line();
        if ui.button("Delete##tree") {
            state.pending.push_back(CommandKind::DeleteOperation(name));
        }
    }
}
