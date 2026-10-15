//! The Operations pane: one docked context pane showing the page of
//! whatever the browser (or menu, or viewport pick) pointed at - the
//! operation's parameters, a tool, the stock, a work offset, or a model.
//!
//! Provenance: our own pane, specced from public CAM context-property
//! pages; parameter names follow the published machining vocabulary
//! (stepover, stock to leave, climb/conventional, lead in/out, tabs).
//!
//! Like every panel this one never touches the world: it edits the
//! `State` drafts and queues `CommandKind`s. The world's components are
//! the truth, refetched into the drafts by the `sync_*_ui` methods, and
//! routing goes through `State::pane_target` - a target whose entity
//! vanished (undo, delete) retargets the pane on its own, so it can never
//! edit a ghost. Status and warnings are computed on read, so a page
//! cannot display a stale claim about a stale path.

use std::borrow::Cow;

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::geo2d::Point2;
use hsmworks_app::machining2d::{Lead, PocketStrategy};
use hsmworks_app::model;
use hsmworks_app::operation::{self, OpParams, OpStatus, SrcRef};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::catalog;

use crate::browser::kind_name;
use crate::form;
use crate::setup;
use crate::state::{CommandKind, ErrorScope, GeoPick, State, UiTarget};

/// Radio/combo order = `OpType` discriminants (Face, Contour, Pocket, Slot,
/// Trace); `kind_index` and `retype` must agree with this list.
const TYPES: [&str; 5] = ["Face", "2D Contour", "2D Pocket", "Slot", "Trace"];
const LEADS: [&str; 3] = ["None", "Linear", "Arc"];
const STRATEGIES: [&str; 2] = ["Zigzag", "Waterline"];
const TOOL_KINDS: [&str; 3] = ["End mill", "Ball nose", "Bull nose"];

pub fn operations_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("operations", "Operations") else {
        return;
    };
    // Persistent docked pane: it is always part of the layout (the ini is
    // not saved, so the layout is rebuilt from code every launch anyway);
    // what changes is its page, via `State::pane_target`.
    ui.window(key.label("Operations")).build(|| {
        contents(ui, state);
    });
}

fn contents(ui: &Ui, state: &mut State) {
    let target = state.pane_target();
    match target {
        UiTarget::None => operation_page(ui, state, None),
        UiTarget::Operation(name) => operation_page(ui, state, Some(name.as_str())),
        UiTarget::Tool(name) => tool_page(ui, state, &name),
        UiTarget::Stock => {
            state.sync_setup_ui();
            ui.text("Stock");
            setup::stock_page(ui, state);
            setup_error(ui, state);
        }
        UiTarget::WcsSlot(i) => {
            state.sync_setup_ui();
            ui.text(format!(
                "Work offset {}",
                WCS_SLOTS.get(i).copied().unwrap_or("?")
            ));
            setup::wcs_page(ui, state, i);
            setup_error(ui, state);
        }
        UiTarget::Model(name) => model_page(ui, state, &name),
    }
}

/// A refusal from the stock/offset commands belongs to whoever is looking
/// at a setup page; elsewhere it rides the status strip.
fn setup_error(ui: &Ui, state: &State) {
    if let Some(msg) = state.error_for(ErrorScope::Setup) {
        ui.text_colored([1.0, 0.35, 0.35, 1.0], msg);
    }
}

/// The operation page: live operation when `editing` names one, else the
/// creation form (typed name, `Add` pre-validated at the click site).
fn operation_page(ui: &Ui, state: &mut State, editing: Option<&str>) {
    state.sync_ops_ui();
    match editing {
        Some(name) => ui.text(format!("Operation: {name}")),
        None => {
            // Typing the name is not an event; it rides the next Add command.
            form::field(ui, "Name");
            let _ = ui.input_text("##op_name", &mut state.op_ui.name);
        }
    }
    let target = state.op_ui.name.clone();
    let live = model::find_by_name(&state.world, &target)
        .filter(|e| model::is_operation(&state.world, e))
        .and_then(|e| model::operation_of(&state.world, &e));
    let exists = live.is_some();

    // Type: swapping the draft's variant, carrying over the shared numbers.
    let mut kind_idx = kind_index(state.op_ui.params.kind());
    form::field(ui, "Type");
    if ui.combo("##op_kind", &mut kind_idx, TYPES.as_slice(), |t| {
        Cow::Borrowed(*t)
    }) {
        let top = stock_top(&state.world);
        let cur = state.op_ui.params.clone();
        state.op_ui.params = retype(&cur, kind_idx.min(4), top);
    }

    // Tool: an existing operation retargets immediately (a discrete event,
    // like the WCS slot switch); the new-op form just updates the draft.
    // A combo renders each item as a Selectable at its popup root, and
    // ImGui asserts on an empty label there ("empty ID at the root of a
    // window") - so a draft that names no tool (the fresh form, before any
    // selection resyncs it) must never join the list; it adopts the first
    // tool instead, so the preview and a later Add say the same thing. With
    // no tools at all the combo is inert and the hint below explains.
    let mut tools = state.tool_names();
    if !state.op_ui.tool.is_empty() && !tools.contains(&state.op_ui.tool) {
        tools.push(state.op_ui.tool.clone());
    } else if state.op_ui.tool.is_empty()
        && let Some(first) = tools.first()
    {
        state.op_ui.tool = first.clone();
    }
    let mut t_idx = tools
        .iter()
        .position(|t| *t == state.op_ui.tool)
        .unwrap_or(0);
    form::field(ui, "Tool");
    if ui.combo("##op_tool", &mut t_idx, &tools, |t| Cow::Borrowed(t.as_str()))
        && let Some(name) = tools.get(t_idx)
    {
        state.op_ui.tool = name.clone();
        if exists {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.clone(),
                tool: Some(name.clone()),
                src: None,
            });
        }
    }
    if tools.is_empty() {
        ui.text_wrapped("No tools in the document - the Tools page has a catalog.");
    }

    // Source: Stock solid or a model entity (discrete, as above).
    let models: Vec<String> = model::model_entities(&state.world)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let mut src_items = vec![String::from("Stock")];
    src_items.extend(models.iter().cloned());
    // A source model that disappeared under the draft (undo, delete) reads
    // as plain Stock until re-picked: the combo never lies.
    let mut s_idx = match state.op_ui.src_name.clone() {
        n if state.op_ui.src_kind != 0 && !n.is_empty() => match models.iter().position(|m| *m == n)
        {
            Some(p) => 1 + p,
            None => {
                state.op_ui.src_kind = 0;
                state.op_ui.src_name.clear();
                0
            }
        },
        _ => 0,
    };
    form::field(ui, "Source");
    if state.op_ui.src_kind == 2 {
        // Feature picks are not a choice this combo could express, and
        // resting it on "Stock" would lie about the operation. The
        // Geometry section below owns the source; Clear hands it back.
        ui.text("Feature picks");
    } else if ui.combo("##op_src", &mut s_idx, src_items.as_slice(), |s| {
        Cow::Borrowed(s.as_str())
    }) {
        let src = if s_idx == 0 {
            state.op_ui.src_kind = 0;
            state.op_ui.src_name.clear();
            SrcRef::Stock
        } else {
            let name = models[s_idx - 1].clone();
            state.op_ui.src_kind = 1;
            state.op_ui.src_name = name.clone();
            SrcRef::Entity(name)
        };
        if exists {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.clone(),
                tool: None,
                src: Some(src),
            });
        }
    }

    // Geometry: the boundary this operation machines, and the mode that
    // fills it. Between Source and Parameters, the shape of reference 04.
    ui.separator();
    ui.text("Geometry");
    // Viewport pick mode - transient view state like the drafts this page
    // edits: the mode itself is not undoable; what a Face/Edge click
    // changes is (it queues a geometry pick for this operation, Ctrl adds
    // to the list, Alt in Edge mode narrows a chain to the one segment
    // under the cursor).
    ui.text("Click selects:");
    ui.same_line();
    let mut pick_mode = match state.geo_pick {
        GeoPick::Model => 0,
        GeoPick::Face => 1,
        GeoPick::Edge => 2,
    };
    ui.radio_button_int("Models", &mut pick_mode, 0);
    ui.same_line();
    ui.radio_button_int("Faces", &mut pick_mode, 1);
    ui.same_line();
    ui.radio_button_int("Edges", &mut pick_mode, 2);
    state.geo_pick = match pick_mode {
        1 => GeoPick::Face,
        2 => GeoPick::Edge,
        _ => GeoPick::Model,
    };
    ui.text_wrapped("Click in the viewport - Ctrl adds; Alt in Edge mode takes one edge.");
    if state.op_ui.src_kind == 2 {
        // One chip per pick, in list order; each row ends in the x that
        // takes it out. Removal queues the reduced list (the world's
        // order, mirrored - never a re-sort), and the last chip leaving
        // reverts the source to Stock: an empty selection is not a
        // thing SrcRef can hold, and never was.
        let chips = state.geo_chips();
        for (i, chip) in chips.iter().enumerate() {
            if i > 0 {
                let avail = ui.content_region_avail()[0];
                let need = ui.calc_text_size(chip.label.as_str())[0] + 26.0;
                if avail < need {
                    ui.new_line();
                } else {
                    ui.same_line();
                }
            }
            if chip.missing {
                ui.text_colored([1.0, 0.72, 0.20, 1.0], &chip.label);
            } else {
                ui.text(&chip.label);
            }
            ui.same_line();
            let del = format!("×##geo_del_{i}");
            if ui.small_button(&del) {
                let mut rest = state.op_ui.geo.clone();
                rest.remove(i);
                state.pending.push_back(if rest.is_empty() {
                    CommandKind::EditOperationLinks {
                        name: target.clone(),
                        tool: None,
                        src: Some(SrcRef::Stock),
                    }
                } else {
                    CommandKind::SetOperationGeometry { name: target.clone(), items: rest }
                });
            }
        }
        ui.same_line();
        if ui.button("Clear##geo") {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.clone(),
                tool: None,
                src: Some(SrcRef::Stock),
            });
        }
    } else if state.op_ui.src_kind == 1 && !state.op_ui.src_name.is_empty() {
        ui.text_wrapped(format!("Whole of '{}' - the first pick replaces this.", state.op_ui.src_name));
    } else {
        ui.text_wrapped("Whole stock contour (default) - picks bound the operation.");
    }

    // Status, cached-path summary and verification warnings.
    if exists
        && let Some(entity) = model::find_by_name(&state.world, &target)
    {
        let status = operation::status_of(&state.world, &entity);
        let (moves, warnings) = model::toolpath_of(&state.world, &entity)
            .map(|tp| (tp.path.len(), tp.warnings))
            .unwrap_or((0, Vec::new()));
        ui.text(match status {
            OpStatus::Never => "Status: never generated".to_string(),
            OpStatus::Ok => format!("Status: OK - {moves} move(s) cached"),
            OpStatus::Stale => {
                format!("Status: STALE - inputs moved since generation ({moves} move(s) cached)")
            }
        });
        for w in &warnings {
            ui.text_wrapped(format!("warning: {w}"));
        }
    }

    ui.separator();
    params_fields(ui, state);
    ui.separator();

    if exists {
        let unchanged = live.as_ref().is_some_and(|op| op.params == state.op_ui.params);
        if ui.button("Apply Parameters") && !unchanged {
            state.pending.push_back(CommandKind::EditOperationParams {
                name: target.clone(),
                params: state.op_ui.params.clone(),
            });
        }
        ui.same_line();
        if ui.button("Generate") {
            state.pending.push_back(CommandKind::GenerateOperation(target.clone()));
        }
        ui.same_line();
        if ui.button("Delete##op") {
            state.pending.push_back(CommandKind::DeleteOperation(target.clone()));
        }
        if unchanged {
            ui.text_wrapped("Apply is a no-op until a parameter differs from the operation.");
        }
    } else {
        // Pre-validate at the click site (the command still validates -
        // defense in depth): a dead-looking button was the bug report.
        let name = state.op_ui.name.clone();
        let tool = state.op_ui.tool.clone();
        let block = if name.trim().is_empty() {
            Some("Name the operation first.")
        } else if model::find_by_name(&state.world, &name).is_some() {
            Some("That name is already in the document.")
        } else if tool.trim().is_empty()
            || !model::find_by_name(&state.world, &tool)
                .is_some_and(|e| model::is_tool(&state.world, &e))
        {
            Some("No tool to cut with yet - add one from the Tools page.")
        } else {
            None
        };
        let dis = ui.begin_disabled_with_cond(block.is_some());
        if ui.button("Add Operation") {
            let src = state.op_ui.src();
            let params = state.op_ui.params.clone();
            state.pending.push_back(CommandKind::CreateOperation {
                name,
                tool,
                src,
                params,
            });
        }
        dis.end();
        if let Some(why) = block {
            ui.same_line();
            ui.text_wrapped(why);
        }
    }
    if let Some(msg) = state.error_for(ErrorScope::Operation) {
        ui.text_colored([1.0, 0.35, 0.35, 1.0], msg);
    }
}

/// The tool page: the document tools list moved to the browser; this page
/// edits the targeted tool, or (with an empty name target - "Tool
/// Library...", the Tools group header) shows a new draft plus the
/// starter catalog to add from.
fn tool_page(ui: &Ui, state: &mut State, name: &str) {
    // Bridge the target into the draft before syncing: the identity rule
    // in `sync_tool_ui` is keyed by the draft's name, so a node click
    // arriving as a new target has to land in the draft first.
    if !name.is_empty() && state.tool_ui.name != name {
        state.tool_ui.name = name.to_string();
    }
    state.sync_tool_ui();

    if state.tool_ui.exists {
        ui.text(format!("Tool: {}", state.tool_ui.name));
    } else {
        // Typing the name is not an event; it rides the next Add command.
        form::field(ui, "Name");
        let _ = ui.input_text("##tool_name", &mut state.tool_ui.name);
    }
    let mut k = state.tool_ui.kind.min(2);
    form::field(ui, "Type");
    if ui.combo("##tool_kind", &mut k, TOOL_KINDS.as_slice(), |t| {
        Cow::Borrowed(*t)
    }) {
        state.tool_ui.kind = k;
    }
    form::field(ui, "Diameter");
    ui.input_double("##t_d", &mut state.tool_ui.diameter);
    form::field(ui, "Corner radius");
    ui.input_double("##t_cr", &mut state.tool_ui.corner_r);
    form::field(ui, "Flute length");
    ui.input_double("##t_fl", &mut state.tool_ui.flute_len);
    form::field(ui, "Shank diameter");
    ui.input_double("##t_sd", &mut state.tool_ui.shank_d);
    form::field(ui, "Shank length");
    ui.input_double("##t_sl", &mut state.tool_ui.shank_len);
    form::field(ui, "Holder diameter");
    ui.input_double("##t_hd", &mut state.tool_ui.holder_d);
    form::field(ui, "Holder length");
    ui.input_double("##t_hl", &mut state.tool_ui.holder_len);
    form::field(ui, "Material group");
    let _ = ui.input_text("##t_mat", &mut state.tool_ui.material_group);
    form::field(ui, "Spindle rpm");
    ui.input_double("##t_rpm", &mut state.tool_ui.rpm);
    form::field(ui, "Feed (mm/min)");
    ui.input_double("##t_feed", &mut state.tool_ui.feed);

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
        // silently, which read as a dead Add button. A name typed this
        // frame has not synced yet, so check the world directly.
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
        // A new draft must also release the named-tool target, or next
        // frame's bridge would reload the old tool over it.
        state.ui_target = UiTarget::Tool(String::new());
        state.tool_ui = state.new_tool_draft();
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool) {
        ui.text_colored([1.0, 0.35, 0.35, 1.0], msg);
    }

    ui.separator();
    ui.text("Catalog");
    for (name, tool) in catalog() {
        ui.text(format!("{}  Ø{} {}", name, tool.diameter, kind_name(tool.kind)));
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

/// Read-only model info. The node click already made it the selection, so
/// Del is live; this page explains what the model is used for.
fn model_page(ui: &Ui, state: &State, name: &str) {
    ui.text(format!("Model: {name}"));
    let Some(entity) = model::find_by_name(&state.world, name) else {
        ui.text_wrapped("It is gone from the document.");
        return;
    };
    let tris = model::facets_of(&state.world, &entity).map_or(0, |t| t.len());
    ui.text(format!("{tris} triangle(s)"));
    if let Some(b) = model::entity_world_aabb(&state.world, &entity) {
        ui.text(format!(
            "{:.1} x {:.1} x {:.1} mm",
            b.max.x - b.min.x,
            b.max.y - b.min.y,
            b.max.z - b.min.z
        ));
    }
    let users: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .filter_map(|(op_name, e)| {
            let op = model::operation_of(&state.world, &e)?;
            match &op.src {
                SrcRef::Entity(n) if n == name => Some(op_name),
                _ => None,
            }
        })
        .collect();
    if users.is_empty() {
        ui.text_wrapped("No operation uses it as a source.");
    } else {
        ui.text_wrapped(format!("Source for: {}", users.join(", ")));
    }
    if let Some(stock) = model::find_by_name(&state.world, STOCK_NAME)
        && let Some(StockShape::FromModel { source }) = model::stock_def_of(&state.world, &stock)
        && source == name
    {
        ui.text("It is also the stock solid (Setup > Stock).");
    }
    ui.text_wrapped("Selected now: Del removes it (undo restores).");
}

/// The parameter fields of the draft's current variant. Edits go to the
/// draft; the world sees them only through `EditOperationParams`.
fn params_fields(ui: &Ui, state: &mut State) {
    match &mut state.op_ui.params {
        OpParams::Face {
            stepover,
            clearance_z,
            cut_z,
            feed,
            plunge_feed,
            climb,
        } => {
            form::field(ui, "Stepover (Y spacing)");
            ui.input_double("##f_so", stepover);
            form::field(ui, "Clearance Z");
            ui.input_double("##f_clr", clearance_z);
            form::field(ui, "Cut Z");
            ui.input_double("##f_cut", cut_z);
            form::field(ui, "Feed (mm/min)");
            ui.input_double("##f_feed", feed);
            form::field(ui, "Plunge feed (mm/min)");
            ui.input_double("##f_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
        }
        OpParams::Contour {
            stock_to_leave,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
        } => {
            form::field(ui, "Stock to leave");
            ui.input_double("##c_stl", stock_to_leave);
            form::field(ui, "Clearance Z");
            ui.input_double("##c_clr", clearance_z);
            form::field(ui, "Retract Z (between passes)");
            ui.input_double("##c_ret", retract_z);
            form::field(ui, "Top Z");
            ui.input_double("##c_top", top_z);
            form::field(ui, "Bottom Z");
            ui.input_double("##c_bot", bottom_z);
            form::field(ui, "Stepdown");
            ui.input_double("##c_sd", stepdown);
            form::field(ui, "Feed (mm/min)");
            ui.input_double("##c_feed", feed);
            form::field(ui, "Plunge feed (mm/min)");
            ui.input_double("##c_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
            let mut lead_idx = match lead {
                Lead::None => 0,
                Lead::Linear { .. } => 1,
                Lead::Arc { .. } => 2,
            };
            form::field(ui, "Lead in/out");
            if ui.combo("##c_lead", &mut lead_idx, LEADS.as_slice(), |t| {
                Cow::Borrowed(*t)
            }) {
                *lead = match lead_idx {
                    1 => Lead::Linear { length: 8.0 },
                    2 => Lead::Arc { radius: 5.0 },
                    _ => Lead::None,
                };
            }
            match lead {
                Lead::Linear { length } => {
                    form::field(ui, "Lead length");
                    ui.input_double("##c_lead_len", length);
                }
                Lead::Arc { radius } => {
                    form::field(ui, "Lead arc radius");
                    ui.input_double("##c_lead_r", radius);
                }
                Lead::None => {}
            }
            form::field(ui, "Tolerance");
            ui.input_double("##c_tol", tolerance);
            form::field(ui, "Tab count");
            let mut tabs = f64::from(*tab_count);
            ui.input_double("##c_tabs", &mut tabs);
            *tab_count = tabs.clamp(0.0, 99.0).round().max(0.0) as u32;
            form::field(ui, "Tab width");
            ui.input_double("##c_tabw", tab_width);
            form::field(ui, "Tab height");
            ui.input_double("##c_tabh", tab_height);
        }
        OpParams::Pocket {
            stepover,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
        } => {
            form::field(ui, "Stepover");
            ui.input_double("##p_so", stepover);
            form::field(ui, "Stock to leave");
            ui.input_double("##p_stl", stock_to_leave);
            form::field(ui, "Clearance Z");
            ui.input_double("##p_clr", clearance_z);
            form::field(ui, "Top Z");
            ui.input_double("##p_top", top_z);
            form::field(ui, "Bottom Z");
            ui.input_double("##p_bot", bottom_z);
            form::field(ui, "Stepdown");
            ui.input_double("##p_sd", stepdown);
            form::field(ui, "Feed (mm/min)");
            ui.input_double("##p_feed", feed);
            form::field(ui, "Plunge feed (mm/min)");
            ui.input_double("##p_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
            let mut st_idx = match strategy {
                PocketStrategy::Zigzag => 0,
                PocketStrategy::Waterline => 1,
            };
            form::field(ui, "Strategy");
            if ui.combo("##p_strat", &mut st_idx, STRATEGIES.as_slice(), |t| {
                Cow::Borrowed(*t)
            }) {
                *strategy = if st_idx == 1 {
                    PocketStrategy::Waterline
                } else {
                    PocketStrategy::Zigzag
                };
            }
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
        } => {
            form::field(ui, "A X");
            ui.input_double("##s_ax", &mut a.x);
            form::field(ui, "A Y");
            ui.input_double("##s_ay", &mut a.y);
            form::field(ui, "B X");
            ui.input_double("##s_bx", &mut b.x);
            form::field(ui, "B Y");
            ui.input_double("##s_by", &mut b.y);
            form::field(ui, "Slot width");
            ui.input_double("##s_w", width);
            form::field(ui, "Stock to leave");
            ui.input_double("##s_stl", stock_to_leave);
            form::field(ui, "Clearance Z");
            ui.input_double("##s_clr", clearance_z);
            form::field(ui, "Top Z");
            ui.input_double("##s_top", top_z);
            form::field(ui, "Bottom Z");
            ui.input_double("##s_bot", bottom_z);
            form::field(ui, "Stepdown");
            ui.input_double("##s_sd", stepdown);
            form::field(ui, "Stepover");
            ui.input_double("##s_so", stepover);
            form::field(ui, "Feed (mm/min)");
            ui.input_double("##s_feed", feed);
            form::field(ui, "Plunge feed (mm/min)");
            ui.input_double("##s_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
        }
        OpParams::Trace {
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
        } => {
            form::field(ui, "Clearance Z");
            ui.input_double("##t_clr", clearance_z);
            form::field(ui, "Top Z");
            ui.input_double("##t_top", top_z);
            form::field(ui, "Bottom Z");
            ui.input_double("##t_bot", bottom_z);
            form::field(ui, "Stepdown");
            ui.input_double("##t_sd", stepdown);
            form::field(ui, "Feed (mm/min)");
            ui.input_double("##t_feed", feed);
            form::field(ui, "Plunge feed (mm/min)");
            ui.input_double("##t_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
        }
    }
}

fn kind_index(kind: operation::OpType) -> usize {
    match kind {
        operation::OpType::Face => 0,
        operation::OpType::Contour => 1,
        operation::OpType::Pocket => 2,
        operation::OpType::Slot => 3,
        operation::OpType::Trace => 4,
    }
}

/// Top of the stock solid: the natural default for a `Top Z`.
fn stock_top(world: &flecs_ecs::prelude::World) -> f64 {
    model::stock_world_aabb(world).map_or(0.0, |b| b.max.z)
}

/// Swap the draft's parameter variant, carrying over every shared number
/// so re-typing an operation does not throw away feeds and heights.
fn retype(cur: &OpParams, kind: usize, top: f64) -> OpParams {
    let feed = feed_of(cur);
    let plunge = plunge_of(cur);
    let climb = climb_of(cur);
    let clearance = cur.clearance_z();
    let stepover = stepover_of(cur).unwrap_or(3.0);
    let stepdown = stepdown_of(cur).unwrap_or(5.0);
    let bottom = match bottom_of(cur) {
        Some(b) if b < top => b,
        _ => top - 5.0,
    };
    match kind {
        1 => OpParams::Contour {
            stock_to_leave: 0.0,
            clearance_z: clearance,
            retract_z: top + 10.0,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            feed,
            plunge_feed: plunge,
            climb,
            lead: Lead::Linear { length: 8.0 },
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 5.0,
            tab_height: 1.0,
        },
        2 => OpParams::Pocket {
            stepover,
            stock_to_leave: 0.0,
            clearance_z: clearance,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            feed,
            plunge_feed: plunge,
            climb,
            strategy: PocketStrategy::Zigzag,
        },
        3 => OpParams::Slot {
            a: Point2::new(-20.0, 0.0),
            b: Point2::new(20.0, 0.0),
            width: 4.0,
            stock_to_leave: 0.0,
            clearance_z: clearance,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            stepover,
            feed,
            plunge_feed: plunge,
            climb,
        },
        4 => OpParams::Trace {
            clearance_z: clearance,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            feed,
            plunge_feed: plunge,
            climb,
        },
        _ => OpParams::Face {
            stepover,
            clearance_z: clearance,
            cut_z: top,
            feed,
            plunge_feed: plunge,
            climb,
        },
    }
}

/// Parameter set a ribbon quick-add starts an operation of `kind` with:
/// the draft defaults, retyped onto the stock top - the same numbers the
/// creation form would have prefilled.
pub(crate) fn quick_params(world: &flecs_ecs::prelude::World, kind: usize) -> OpParams {
    retype(&crate::state::OpUi::default().params, kind, stock_top(world))
}

fn feed_of(p: &OpParams) -> f64 {
    match p {
        OpParams::Face { feed, .. }
        | OpParams::Contour { feed, .. }
        | OpParams::Pocket { feed, .. }
        | OpParams::Slot { feed, .. }
        | OpParams::Trace { feed, .. } => *feed,
    }
}

fn plunge_of(p: &OpParams) -> f64 {
    match p {
        OpParams::Face { plunge_feed, .. }
        | OpParams::Contour { plunge_feed, .. }
        | OpParams::Pocket { plunge_feed, .. }
        | OpParams::Slot { plunge_feed, .. }
        | OpParams::Trace { plunge_feed, .. } => *plunge_feed,
    }
}

fn climb_of(p: &OpParams) -> bool {
    match p {
        OpParams::Face { climb, .. }
        | OpParams::Contour { climb, .. }
        | OpParams::Pocket { climb, .. }
        | OpParams::Slot { climb, .. }
        | OpParams::Trace { climb, .. } => *climb,
    }
}

fn stepover_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Face { stepover, .. }
        | OpParams::Pocket { stepover, .. }
        | OpParams::Slot { stepover, .. } => Some(*stepover),
        _ => None,
    }
}

fn stepdown_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Contour { stepdown, .. }
        | OpParams::Pocket { stepdown, .. }
        | OpParams::Slot { stepdown, .. }
        | OpParams::Trace { stepdown, .. } => Some(*stepdown),
        _ => None,
    }
}

fn bottom_of(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Contour { bottom_z, .. }
        | OpParams::Pocket { bottom_z, .. }
        | OpParams::Slot { bottom_z, .. }
        | OpParams::Trace { bottom_z, .. } => Some(*bottom_z),
        _ => None,
    }
}
