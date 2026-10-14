//! Browser tree: every object in the document, one persistent pane.
//! Provenance: our own pane, specced from public CAM browser trees.
//!
//! The tree is the document's outline, not a filter of it: Setup (stock and
//! the G5x offsets), Models, Tools and Operations each get a group. A leaf
//! click aims the context pane (`UiTarget`) and, for world entities,
//! mirrors a `Select` - the target is view state, the selection is the
//! command (AGENTS.md). Tool leaves deliberately push no `Select`: a tool
//! node must not arm the Del key against the delete path models use.
//!
//! Status badges are computed on read (`operation::status_of`) - never
//! stored - so a line cannot lie about the world.

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::model;
use hsmworks_app::operation::{self, OpStatus};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::tools::ToolKind;

use crate::actions;
use crate::state::{CommandKind, State, UiTarget};

pub fn browser_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("browser", "Browser") else {
        return;
    };
    // Persistent docked pane: always in the layout, what changes is what
    // the open groups hold.
    ui.window(key.label("Browser")).build(|| {
        contents(ui, state);
    });
}

/// Shared with the Operations pane's tool page for its summary line.
pub(crate) fn kind_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
    }
}

fn contents(ui: &Ui, state: &mut State) {
    // View toggles, not commands (AGENTS.md ephemeral UI state, like the
    // camera): the world-origin axes and the non-selected toolpaths.
    ui.checkbox("Show world origin axes", &mut state.show_world_axes);
    ui.checkbox("Show all toolpaths", &mut state.show_all_paths);

    let target = state.pane_target();
    setup_group(ui, state, &target);
    models_group(ui, state, &target);
    tools_group(ui, state, &target);
    operations_group(ui, state, &target);
}

fn setup_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    let Some(_node) = ui.tree_node_config("Setup###browser_setup").default_open(true).push() else {
        return;
    };
    // The stock page doubles as the create-stock form, so the node is a
    // destination whether or not the entity exists yet.
    let active = *target == UiTarget::Stock;
    if ui.selectable_config("Stock###browser_stock").selected(active).build() {
        actions::target(state, UiTarget::Stock);
        if model::find_by_name(&state.world, STOCK_NAME).is_some() {
            state.pending.push_back(CommandKind::Select(Some(STOCK_NAME.to_string())));
        }
    }
    for (i, slot) in WCS_SLOTS.iter().enumerate() {
        let active = *target == UiTarget::WcsSlot(i);
        let placed = model::find_by_name(&state.world, slot).is_some();
        let label = if placed {
            (*slot).to_string()
        } else {
            format!("{slot}  -")
        };
        if ui
            .selectable_config(label.as_str())
            .selected(active)
            .build()
        {
            actions::target(state, UiTarget::WcsSlot(i));
            if placed {
                state.pending.push_back(CommandKind::Select(Some((*slot).to_string())));
            }
        }
    }
}

fn models_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    let Some(_node) = ui.tree_node_config("Models###browser_models").default_open(true).push() else {
        return;
    };
    // Setup furniture (stock, offsets) and CAM entities are filtered out of
    // the model list by `model_entities` itself.
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
        let active = *target == UiTarget::Model(name.clone()) || selected;
        if ui.selectable_config(name.as_str()).selected(active).build() {
            actions::target(state, UiTarget::Model(name.clone()));
            state.pending.push_back(CommandKind::Select(Some(name)));
        }
    }
}

fn tools_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    let Some(_node) = ui.tree_node_config("Tools###browser_tools").default_open(true).push() else {
        return;
    };
    let tools: Vec<(String, hsmworks_app::tools::Tool)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(&state.world, e))
        .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
        .collect();
    if tools.is_empty() {
        ui.text_wrapped("None yet - the Tool page has a catalog to add from.");
    }
    for (name, tool) in tools {
        let active = *target == UiTarget::Tool(name.clone());
        let label = format!("{}  Ø{} {}", name, tool.diameter, kind_name(tool.kind));
        if ui.selectable_config(label.as_str()).selected(active).build() {
            // Target only: no Select, so browsing tools never arms Del.
            actions::target(state, UiTarget::Tool(name));
        }
    }
}

fn operations_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    let Some(_node) = ui.tree_node_config("Operations###browser_ops").default_open(true).push() else {
        return;
    };
    let names: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .map(|(name, _)| name)
        .collect();
    if names.is_empty() {
        ui.text_wrapped(
            "No operations yet.\n\nAdd a tool (Tools above), then\nCAM > New Operation.",
        );
        return;
    }
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
        let active = *target == UiTarget::Operation(name.clone());
        if ui.selectable_config(label.as_str()).selected(active).build() {
            actions::target(state, UiTarget::Operation(name.clone()));
            state.pending.push_back(CommandKind::Select(Some(name)));
        }
    }
}
