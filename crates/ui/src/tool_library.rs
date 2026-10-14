//! Tool library pane: the document's tool entities, a hand-typed tool
//! page, and the starter catalog to add from.
//!
//! Provenance: our own pane, specced from public CAM tool managers; the
//! catalog's sizes and numbers are ours (see `hsmworks_app::tools`).

use std::borrow::Cow;

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::model;
use hsmworks_app::tools::{ToolKind, catalog};

use crate::state::{CommandKind, ErrorScope, State, ToolUi};

const TOOL_KINDS: [&str; 3] = ["End mill", "Ball nose", "Bull nose"];

pub fn tools_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("tools", "Tool Library") else {
        return;
    };
    // Persistent docked pane, like the Operations one: always in the
    // layout, page selection lives in `state.tool_ui`, not in a flag.
    ui.window(key.label("Tool Library")).build(|| {
        contents(ui, state);
    });
}

fn contents(ui: &Ui, state: &mut State) {
    state.sync_tool_ui();

    ui.text("Document tools");
    let tools: Vec<(String, hsmworks_app::tools::Tool)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(&state.world, e))
        .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
        .collect();
    if tools.is_empty() {
        ui.text_wrapped("None yet - add one from the catalog below.");
    }
    for (name, tool) in &tools {
        let active = state.tool_ui.exists && state.tool_ui.name == *name;
        if ui.selectable_config(name.as_str()).selected(active).build() {
            // Click (select or re-click) loads the tool into the page below.
            state.tool_ui = ToolUi::from_tool(name, tool, true);
        }
        ui.same_line();
        ui.text(format!("Ø{}  {}", tool.diameter, kind_name(tool.kind)));
    }

    ui.separator();
    if state.tool_ui.exists && !state.tool_ui.name.is_empty() {
        ui.text(format!("Editing: {}", state.tool_ui.name));
    } else {
        // Typing the name is not an event; it rides the next Add command.
        let _ = ui.input_text("Name##tool_name", &mut state.tool_ui.name);
    }
    let mut k = state.tool_ui.kind.min(2);
    if ui.combo("Type##tool_kind", &mut k, TOOL_KINDS.as_slice(), |t| {
        Cow::Borrowed(*t)
    }) {
        state.tool_ui.kind = k;
    }
    ui.input_double("Diameter##t_d", &mut state.tool_ui.diameter);
    ui.input_double("Corner radius##t_cr", &mut state.tool_ui.corner_r);
    ui.input_double("Flute length##t_fl", &mut state.tool_ui.flute_len);
    ui.input_double("Shank diameter##t_sd", &mut state.tool_ui.shank_d);
    ui.input_double("Shank length##t_sl", &mut state.tool_ui.shank_len);
    ui.input_double("Holder diameter##t_hd", &mut state.tool_ui.holder_d);
    ui.input_double("Holder length##t_hl", &mut state.tool_ui.holder_len);
    let _ = ui.input_text("Material group##t_mat", &mut state.tool_ui.material_group);
    ui.input_double("Spindle rpm##t_rpm", &mut state.tool_ui.rpm);
    ui.input_double("Feed (mm/min)##t_feed", &mut state.tool_ui.feed);

    ui.spacing();
    if state.tool_ui.exists {
        if ui.button("Apply##tool") {
            let name = state.tool_ui.name.clone();
            let tool = state.tool_ui.to_tool();
            state.pending.push_back(CommandKind::ChangeTool { name, tool });
        }
        ui.same_line();
        if ui.button("Delete##tool") {
            let name = state.tool_ui.name.clone();
            state.pending.push_back(CommandKind::DeleteTool(name));
        }
    } else {
        // Pre-validate at the click site (the command still validates, as
        // defense in depth): an empty or taken name used to be refused
        // silently two panes away, which read as a dead Add button. A name
        // typed this frame has not synced yet, so check the world directly.
        let name = state.tool_ui.name.clone();
        let block = if name.trim().is_empty() {
            Some("Name the tool first.")
        } else if model::find_by_name(&state.world, &name).is_some() {
            Some("That name is already in the document.")
        } else {
            None
        };
        let dis = ui.begin_disabled_with_cond(block.is_some());
        if ui.button("Add##tool") {
            let tool = state.tool_ui.to_tool();
            state.pending.push_back(CommandKind::AddTool { name, tool });
        }
        dis.end();
        if let Some(why) = block {
            ui.same_line();
            ui.text_wrapped(why);
        }
    }
    ui.same_line();
    if ui.button("New##tool") {
        state.tool_ui = state.new_tool_draft();
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool) {
        ui.text_colored([1.0, 0.35, 0.35, 1.0], msg);
    }

    ui.separator();
    ui.text("Catalog");
    for (name, tool) in catalog() {
        ui.text(format!("{name}  Ø{}", tool.diameter));
        ui.same_line();
        let taken = model::find_by_name(&state.world, name).is_some();
        let dis = ui.begin_disabled_with_cond(taken);
        if ui.button(format!("Add##cat_{name}")) {
            state.pending.push_back(CommandKind::AddTool {
                name: name.to_string(),
                tool: tool.clone(),
            });
        }
        dis.end();
        if taken {
            ui.same_line();
            ui.text("(in document)");
        }
    }
}

fn kind_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
    }
}
