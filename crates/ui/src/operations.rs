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
use flecs_ecs::prelude::World;
use hsmworks_app::geo2d::Point2;
use hsmworks_app::machining2d::{ARC_FIT_TOL, Lead, PocketStrategy};
use hsmworks_app::model;
use hsmworks_app::operation::{self, Height, HeightReference, OpParams, OpStatus, SrcRef};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::catalog;
use hsmworks_app::verify::Warning;

use crate::browser::kind_name;
use crate::form;
use crate::setup;
use crate::state::{CommandKind, ErrorScope, GeoPick, State, UiTarget};

/// Radio/combo order = `OpType` discriminants (Face, Contour, Pocket, Slot,
/// Trace); `kind_index` and `retype` must agree with this list.
const TYPES: [&str; 5] = ["Face", "2D Contour", "2D Pocket", "Slot", "Trace"];
const LEADS: [&str; 3] = ["None", "Linear", "Arc"];
/// The Strategy combo's labels, in index order - one source for the
/// labels, the combo, and any test checking what the pane offers. Auto
/// sits beside the explicit choices; picking it stores the choice, and
/// [`hsmworks_app::machining2d::PocketStrategy::resolve`] answers which
/// path it cuts at generate.
pub fn strategy_names() -> &'static [&'static str] {
    &["Zigzag", "Waterline", "Auto"]
}
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

    // Type: an operation's kind is fixed at creation. A live operation
    // shows its kind as a label - swapping the variant in place replaced
    // one kind's work with another's and read as the op vanishing from
    // the list (hand-test report). Changing kinds means adding a new
    // operation. The combo survives on the creation form, where it swaps
    // the draft's variant and carries over the shared numbers.
    form::field(ui, "Type");
    if exists {
        ui.text(TYPES[kind_index(state.op_ui.params.kind())]);
    } else {
        let mut kind_idx = kind_index(state.op_ui.params.kind());
        if ui.combo("##op_kind", &mut kind_idx, TYPES.as_slice(), |t| {
            Cow::Borrowed(*t)
        }) {
            let cur = state.op_ui.params.clone();
            state.op_ui.params = retype(&cur, kind_idx.min(4));
        }
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
    // geo_hover is cleared once per frame by App::draw; this pane's rows
    // set it below, and only the row under the pointer this frame does.
    if state.op_ui.src_kind == 2 {
        // One row per pick, in list order - a readable descriptor of the
        // geometry, ending in the × that takes it out. Hovering a row
        // lights that feature in the viewport (screen-space amber, never
        // an overlay rebuild): the list answers "which one is that?".
        // Removal queues the reduced list (the world's order, mirrored -
        // never a re-sort), and the last row leaving reverts the source
        // to Stock: an empty selection is not a thing SrcRef can hold,
        // and never was.
        let chips = state.geo_chips();
        for (i, chip) in chips.iter().enumerate() {
            if chip.missing {
                ui.text_colored([1.0, 0.72, 0.20, 1.0], &chip.label);
            } else {
                ui.text(&chip.label);
            }
            if ui.is_item_hovered() {
                state.geo_hover = Some(chip.sel.clone());
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
            if i + 1 < chips.len() {
                ui.new_line();
            }
        }
        if ui.button("Clear##geo") {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.clone(),
                tool: None,
                src: Some(SrcRef::Stock),
            });
        }
        // What the picks resolve to beyond the raw list - runs stitched
        // into rings, lone runs reaching out to the stock boundary, a
        // region taken on the far side because the rings bound air.
        // Computed on read through the very walk the region takes, so a
        // note can never contradict the preview it describes.
        for note in operation::selection_notes(&state.world, &state.op_ui.geo, &state.op_ui.params)
        {
            ui.text_colored([0.62, 0.66, 0.72, 1.0], &note);
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
        for line in fold_warnings(&warnings) {
            ui.text_wrapped(line);
        }
    }

    ui.separator();
    params_fields(ui, state);
    // A height whose reference lost its source already says so on its
    // own row; this line binds the whole panel to it, because the
    // viewport has dropped that plane and Generate will refuse by name -
    // the pane must not read as "just edit the numbers" behind that.
    if let Some(note) = unresolved_note(&state.world, &state.op_ui.params) {
        ui.text_wrapped(note);
    }
    ui.separator();

    if exists {
        let unchanged = live.as_ref().is_some_and(|op| op.params == state.op_ui.params);
        if ui.button("Apply Parameters") {
            // The arms live on State (`apply_operation_params`) so the
            // queue and the no-op acknowledgment are testable without a
            // frame; the click always answers, changed or not.
            state.apply_operation_params(&target);
        }
        ui.same_line();
        if ui.button("Generate") {
            // One click, two honest commands: an unsaved draft is applied
            // first, so the path always comes from what the panel shows -
            // generating committed parameters under a visibly edited form
            // was the silent-wrong-answer bug. Arm on State, like Apply.
            state.generate_operation(&target);
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

/// One height, HSMWorks-style: *From* a reference, plus an *Offset* from
/// it - and the plane the pair currently resolves to, echoed so the
/// number on screen can never lie about where the tool goes. `Work
/// offset` grows a slot combo naming G54..G59. An unavailable source
/// says what is missing instead of showing a stale z; the viewport drops
/// that plane and generate refuses by name.
fn height_editor(ui: &Ui, id: &str, world: &World, h: &mut Height) {
    // Two stacked rows, never a same_line chain: the docked pane is ~18%
    // of the screen, and one row of full-width combo + input + echo put
    // everything past the combo outside the clip rect - the reference was
    // settable while its offset (the 25 in "stock top +25") and the echo
    // were invisible (M1.5.4 hand-test). Row 1 is the From picker, row 2
    // the number with the resolved plane beside it.
    // Sibling ids carry distinct `###`-free suffixes: the combo's source
    // string must never equal the input's (one action, one widget id -
    // the browser-row lesson of `7e95c31` applies inside a row too).
    let avail = ui.content_region_avail()[0].max(40.0);
    if matches!(h.reference, HeightReference::WorkOffset(_)) {
        // Slot picker shares row 1: override form::field's full width so
        // the combo leaves it room (SetNextItemWidth wins for one item).
        ui.set_next_item_width(avail * 0.6);
    }
    let mut kind = h.reference.kind_index().min(5);
    if ui.combo(
        format!("From{id}kind"),
        &mut kind,
        HeightReference::KIND_LABELS.as_slice(),
        |label| Cow::Borrowed(*label),
    ) {
        h.reference = match kind {
            0 => HeightReference::Fixed,
            1 => HeightReference::TopOfStock,
            2 => HeightReference::BottomOfStock,
            3 => HeightReference::TopOfModel,
            4 => HeightReference::BottomOfModel,
            _ => HeightReference::WorkOffset(h.reference.wcs_slot().min(WCS_SLOTS.len() - 1)),
        };
    }
    if let HeightReference::WorkOffset(slot_idx) = h.reference {
        ui.same_line();
        ui.set_next_item_width(avail * 0.35);
        let mut slot = slot_idx.min(WCS_SLOTS.len() - 1);
        if ui.combo(
            format!("{id}slot"),
            &mut slot,
            WCS_SLOTS.as_slice(),
            |name| Cow::Borrowed(*name),
        ) {
            h.reference = HeightReference::WorkOffset(slot);
        }
    }
    ui.text(offset_label(h.reference));
    ui.same_line();
    ui.set_next_item_width(avail * 0.45);
    ui.input_double(id, &mut h.offset);
    ui.same_line();
    ui.text_disabled(height_echo(world, h));
}

/// The row-2 caption: under `Fixed` the number *is* the world z, so do
/// not call it an offset from something (and under the other references
/// never pretend the box holds an absolute plane). Pure so a test can pin
/// which reference reads as which caption.
fn offset_label(reference: HeightReference) -> &'static str {
    if matches!(reference, HeightReference::Fixed) {
        "Value"
    } else {
        "Offset"
    }
}

/// What a height row echoes after its offset box: the plane it resolves
/// to, or - never a stale or guessed number - what is missing. Pure so
/// the tests can pin the exact wording; the draw code only labels it.
pub(crate) fn height_echo(world: &World, h: &Height) -> String {
    match h.z(world) {
        Some(z) => format!("(= Z {z:.3})"),
        None => format!(
            "(unresolved: {})",
            operation::missing_source(world, h.reference).unwrap_or("source missing")
        ),
    }
}

/// The pane-wide counterpart of the row echo: while any height fails to
/// resolve, one line tells the whole panel that the viewport dropped a
/// plane and Generate will refuse, naming the field and the reason (the
/// app's `resolve_params` error). `None` when everything resolves, so a
/// healthy panel shows no alarm.
pub(crate) fn unresolved_note(world: &World, params: &OpParams) -> Option<String> {
    operation::resolve_params(world, params)
        .err()
        .map(|why| format!("{why}; set the source up or switch the reference"))
}

/// The parameter fields of the draft's current variant. Edits go to the
/// draft; the world sees them only through `EditOperationParams`.
fn params_fields(ui: &Ui, state: &mut State) {
    // Computed up front: the match below keeps a mutable borrow of the
    // draft alive, and the Auto note reads the world, not the draft.
    let auto_note = auto_note(state);
    match &mut state.op_ui.params {
        OpParams::Face {
            stepover,
            clearance_z,
            cut_z,
            top_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
        } => {
            form::field(ui, "Stepover (Y spacing)");
            ui.input_double("##f_so", stepover);
            form::field(ui, "Clearance Z");
            height_editor(ui, "##f_clr", &state.world, clearance_z);
            form::field(ui, "Cut Z");
            height_editor(ui, "##f_cut", &state.world, cut_z);
            // Off is the legacy single skim; on stages from Top Z down,
            // prefilled one step above Cut Z in the same reference frame
            // for the user to retune.
            form::field(ui, "Depth staging");
            let mut staged = top_z.is_some();
            if ui.checkbox("Stage down to Cut Z", &mut staged) {
                let seed = Height::of(cut_z.reference, cut_z.offset + 1.0);
                if staged {
                    *top_z = Some(seed);
                    *stepdown = Some(1.0);
                } else {
                    *top_z = None;
                    *stepdown = None;
                }
            }
            if staged {
                form::field(ui, "Top Z (first cut level)");
                let seed = Height::of(cut_z.reference, cut_z.offset + 1.0);
                height_editor(ui, "##f_top", &state.world, top_z.get_or_insert(seed));
                form::field(ui, "Stepdown");
                ui.input_double("##f_sd", stepdown.get_or_insert(1.0));
            }
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
            height_editor(ui, "##c_clr", &state.world, clearance_z);
            form::field(ui, "Retract Z (between passes)");
            height_editor(ui, "##c_ret", &state.world, retract_z);
            if let (Some(r), Some(c)) = (retract_z.z(&state.world), clearance_z.z(&state.world))
                && r > c
            {
                ui.text_wrapped(format!(
                    "above clearance - the hop travels at {c:.3} (min of the two)"
                ));
            }
            form::field(ui, "Top Z");
            height_editor(ui, "##c_top", &state.world, top_z);
            form::field(ui, "Bottom Z");
            height_editor(ui, "##c_bot", &state.world, bottom_z);
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
            retract_z,
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
            height_editor(ui, "##p_clr", &state.world, clearance_z);
            // Off hops to clearance - the legacy shape; on lifts between
            // passes of one level to this plane, clamped at clearance.
            form::field(ui, "Retract plane");
            let mut hop = retract_z.is_some();
            if ui.checkbox("Hop below clearance", &mut hop) {
                *retract_z = if hop {
                    Some(Height::of(top_z.reference, top_z.offset + 1.0))
                } else {
                    None
                };
            }
            if let Some(r) = retract_z {
                form::field(ui, "Retract Z");
                height_editor(ui, "##p_ret", &state.world, r);
                if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                    && rz > cz
                {
                    ui.text_wrapped(format!(
                        "above clearance - the hop travels at {cz:.3} (min of the two)"
                    ));
                }
            }
            form::field(ui, "Top Z");
            height_editor(ui, "##p_top", &state.world, top_z);
            form::field(ui, "Bottom Z");
            height_editor(ui, "##p_bot", &state.world, bottom_z);
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
                PocketStrategy::Auto => 2,
            };
            form::field(ui, "Strategy");
            if ui.combo("##p_strat", &mut st_idx, strategy_names(), |t| {
                Cow::Borrowed(*t)
            }) {
                *strategy = match st_idx {
                    1 => PocketStrategy::Waterline,
                    2 => PocketStrategy::Auto,
                    _ => PocketStrategy::Zigzag,
                };
            }
            if let Some(note) = &auto_note {
                ui.text_wrapped(note.as_str());
            }
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            clearance_z,
            // The slot cuts one continuous serpentine per level, so there
            // is never a same-level lift to hop - the plane is accepted
            // and validated, but showing a dead control would be a lie.
            retract_z: _,
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
            height_editor(ui, "##s_clr", &state.world, clearance_z);
            form::field(ui, "Top Z");
            height_editor(ui, "##s_top", &state.world, top_z);
            form::field(ui, "Bottom Z");
            height_editor(ui, "##s_bot", &state.world, bottom_z);
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
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
        } => {
            form::field(ui, "Clearance Z");
            height_editor(ui, "##t_clr", &state.world, clearance_z);
            // Engraved lines cut their own groove, so a low hop between
            // loops of one level is safe here; off hops to clearance.
            form::field(ui, "Retract plane");
            let mut hop = retract_z.is_some();
            if ui.checkbox("Hop below clearance", &mut hop) {
                *retract_z = if hop {
                    Some(Height::of(top_z.reference, top_z.offset + 1.0))
                } else {
                    None
                };
            }
            if let Some(r) = retract_z {
                form::field(ui, "Retract Z");
                height_editor(ui, "##t_ret", &state.world, r);
                if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                    && rz > cz
                {
                    ui.text_wrapped(format!(
                        "above clearance - the hop travels at {cz:.3} (min of the two)"
                    ));
                }
            }
            form::field(ui, "Top Z");
            height_editor(ui, "##t_top", &state.world, top_z);
            form::field(ui, "Bottom Z");
            height_editor(ui, "##t_bot", &state.world, bottom_z);
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

/// The live decision behind an `Auto` pocket, quoting
/// [`PocketStrategy::resolve`] - the very law generation applies - so the
/// strategy never hides which shape the pocket will cut. `None` unless
/// the draft is a pocket set to Auto. A brand-new op has no geometry in
/// the world to ask yet; once its region resolves, the note names the
/// pick.
fn auto_note(state: &State) -> Option<String> {
    let OpParams::Pocket {
        strategy, stepover, ..
    } = &state.op_ui.params
    else {
        return None;
    };
    if *strategy != PocketStrategy::Auto {
        return None;
    }
    let stepover = *stepover;
    let pick = "Auto picks the shape from the pocket's outline at generate.";
    let entity = model::find_by_name(&state.world, &state.op_ui.name)?;
    let op = model::operation_of(&state.world, &entity)?;
    let region = operation::region_of(&state.world, &op)
        .unwrap_or_else(hsmworks_app::geo2d::Region::empty);
    let diameter = model::find_by_name(&state.world, &state.op_ui.tool)
        .and_then(|t| model::tool_of(&state.world, &t))
        .map_or(6.0, |tool| tool.diameter);
    let outer = region
        .loops()
        .iter()
        .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
        .map_or(&[][..], |l| l.points());
    if outer.is_empty() {
        return Some(pick.to_string());
    }
    Some(
        match PocketStrategy::Auto.resolve(outer, stepover, diameter, ARC_FIT_TOL) {
            PocketStrategy::Waterline => {
                "Auto -> Waterline: the outline is round and the rings reach every band.".to_string()
            }
            _ => "Auto -> Zigzag: rings would not suit this outline.".to_string(),
        },
    )
}

/// Swap the draft's parameter variant, carrying over every shared number
/// so re-typing an operation does not throw away feeds and heights.
///
/// Heights are references, not baked world numbers: a fresh op's Top is
/// the part's own top surface and its Bottom the first work offset (G54),
/// the planes the user actually points at - the complaint this replaces
/// was an "arbitrary bottom" computed from the stock. Carried-over
/// heights keep the reference they were saved with. Nothing resolves to
/// a `top - 5.0` anymore; Face cuts one skim at the stock top (`cut_z`
/// is a single plane, so a deeper reference would cut through the whole
/// stock in the default op - the same safe z retype produced before).
fn retype(cur: &OpParams, kind: usize) -> OpParams {
    let feed = feed_of(cur);
    let plunge = plunge_of(cur);
    let climb = climb_of(cur);
    let clearance = cur.clearance();
    let stepover = stepover_of(cur).unwrap_or(3.0);
    let stepdown = stepdown_of(cur).unwrap_or(5.0);
    let top = top_of(cur).unwrap_or_else(|| Height::of(HeightReference::TopOfModel, 0.0));
    let bottom = bottom_of(cur).unwrap_or_else(|| Height::of(HeightReference::WorkOffset(0), 0.0));
    let retract = retract_of(cur).unwrap_or_else(|| Height::of(HeightReference::TopOfStock, 10.0));
    match kind {
        1 => OpParams::Contour {
            stock_to_leave: 0.0,
            clearance_z: clearance,
            retract_z: retract,
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
            retract_z: None,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            feed,
            plunge_feed: plunge,
            climb,
            // New pockets let the shape come from the outline; explicit
            // choices on live operations are never retyped over.
            strategy: PocketStrategy::Auto,
        },
        3 => OpParams::Slot {
            a: Point2::new(-20.0, 0.0),
            b: Point2::new(20.0, 0.0),
            width: 4.0,
            stock_to_leave: 0.0,
            clearance_z: clearance,
            retract_z: None,
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
            retract_z: None,
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
            cut_z: Height::of(HeightReference::TopOfStock, 0.0),
            // A fresh face cuts one level; staging is opt-in in the
            // panel, never a surprise default.
            top_z: None,
            stepdown: None,
            feed,
            plunge_feed: plunge,
            climb,
        },
    }
}

/// Parameter set a ribbon quick-add starts an operation of `kind` with:
/// the draft defaults, retyped - the same heights the creation form
/// would have prefilled. No world argument: the seeds are references,
/// and generation resolves them against whatever the document holds.
pub(crate) fn quick_params(kind: usize) -> OpParams {
    retype(&crate::state::OpUi::default().params, kind)
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

fn bottom_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { bottom_z, .. }
        | OpParams::Pocket { bottom_z, .. }
        | OpParams::Slot { bottom_z, .. }
        | OpParams::Trace { bottom_z, .. } => Some(*bottom_z),
        _ => None,
    }
}

fn top_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Slot { top_z, .. }
        | OpParams::Trace { top_z, .. } => Some(*top_z),
        _ => None,
    }
}

/// The between-pass hop plane, present or not: Contour always stores one,
/// the other hopped kinds keep it in an option (None hops to clearance).
fn retract_of(p: &OpParams) -> Option<Height> {
    match p {
        OpParams::Contour { retract_z, .. } => Some(*retract_z),
        OpParams::Pocket { retract_z, .. }
        | OpParams::Slot { retract_z, .. }
        | OpParams::Trace { retract_z, .. } => *retract_z,
        _ => None,
    }
}

/// One line per warning kind, not one per colliding move. verify records
/// a ShankHit for every feed move the holder grazes stock on, so a deep
/// finishing pass used to flood the pane with a wall of near-identical
/// lines. The `Vec<Warning>` itself is the persisted cache format
/// (project `OperationDto.cache`) - the fold lives here, in the display
/// layer, and the full list still round-trips through the project file.
/// Groups keep first-appearance order; `ShankHit`'s two flavors (stock
/// vs part/island) group apart because they read differently and mean
/// different fixes.
pub fn fold_warnings(warnings: &[Warning]) -> Vec<String> {
    fn key(w: &Warning) -> (u8, bool) {
        match w {
            Warning::Crossing { .. } => (0, false),
            Warning::Gouge { .. } => (1, false),
            Warning::ShankHit { part, .. } => (2, *part),
            Warning::RapidInStock { .. } => (3, false),
            Warning::NonFinite { .. } => (4, false),
        }
    }
    fn last_move(w: &Warning) -> usize {
        match w {
            Warning::Crossing { moves } => moves[1],
            Warning::Gouge { at }
            | Warning::ShankHit { at, .. }
            | Warning::RapidInStock { at }
            | Warning::NonFinite { at } => *at,
        }
    }
    let mut lines: Vec<String> = Vec::new();
    let mut groups: Vec<((u8, bool), usize, usize)> = Vec::new();
    for w in warnings {
        match groups.iter_mut().find(|g| g.0 == key(w)) {
            Some(g) => {
                g.1 += 1;
                g.2 = last_move(w);
            }
            None => {
                groups.push((key(w), 1, last_move(w)));
                lines.push(format!("warning: {w}"));
            }
        }
    }
    for (line, (_, count, last)) in lines.iter_mut().zip(&groups) {
        let extra = count - 1;
        if extra > 0 {
            let noun = if extra == 1 { "move" } else { "moves" };
            line.push_str(&format!(" - repeated on {extra} more {noun} (to {last})"));
        }
    }
    lines
}

#[cfg(test)]
mod height_row_tests {
    //! M1.5.3 step 3: the wording of a height row and its pane-wide note.
    //! An unresolved reference must say which source is missing and where
    //! to fix it - never show a stale or guessed z.

    use super::*;
    use hsmworks_app::world::new_world;

    #[test]
    fn a_resolved_height_echoes_the_plane_it_lands_on() {
        let world = new_world();
        assert_eq!(height_echo(&world, &Height::fixed(-7.0)), "(= Z -7.000)");
        assert_eq!(height_echo(&world, &Height::fixed(25.0)), "(= Z 25.000)");
    }

    #[test]
    fn only_the_fixed_reference_calls_its_number_a_value() {
        // M1.5.4 row 2: every other reference is From + Offset; Fixed
        // holds the world z itself and must not be captioned "Offset".
        assert_eq!(offset_label(HeightReference::Fixed), "Value");
        for r in [
            HeightReference::TopOfStock,
            HeightReference::BottomOfStock,
            HeightReference::TopOfModel,
            HeightReference::BottomOfModel,
            HeightReference::WorkOffset(0),
        ] {
            assert_eq!(offset_label(r), "Offset", "{r:?} is a source");
        }
    }

    #[test]
    fn a_missing_source_names_itself_and_where_to_fix_it() {
        let world = new_world();
        assert_eq!(
            height_echo(&world, &Height::of(HeightReference::TopOfStock, 25.0)),
            "(unresolved: no stock yet - set it up in Setup > Stock)"
        );
        assert_eq!(
            height_echo(&world, &Height::of(HeightReference::BottomOfModel, 0.0)),
            "(unresolved: no model loaded)"
        );
        assert_eq!(
            height_echo(&world, &Height::of(HeightReference::WorkOffset(0), 0.0)),
            "(unresolved: that work offset has no origin yet - place it in Setup)"
        );
    }

    #[test]
    fn the_pane_note_appears_only_while_something_is_unresolved() {
        let world = new_world();
        let ok = OpParams::Face {
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(19.5),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        assert_eq!(unresolved_note(&world, &ok), None, "all fixed - no alarm");
        let broken = OpParams::Face {
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::of(HeightReference::BottomOfStock, 0.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        let note = unresolved_note(&world, &broken).expect("the stock is gone");
        assert!(note.contains("Cut Z"), "{note}");
        assert!(note.contains("no stock yet"), "{note}");
        assert!(note.ends_with("switch the reference"), "{note}");
    }

    #[test]
    fn fresh_depth_operations_default_to_the_part_and_g54() {
        // The contract behind issue 2: a new operation's Bottom rides the
        // selected work offset and its Top is the part itself - the
        // number the newcomer sees is where the material actually is,
        // never an arbitrary 5 under the stock top.
        for kind in [1usize, 2, 3, 4] {
            let p = quick_params(kind);
            assert_eq!(
                top_of(&p).map(|h| h.reference),
                Some(HeightReference::TopOfModel),
                "kind {kind} tops out at the model",
            );
            assert_eq!(
                bottom_of(&p).map(|h| (h.reference, h.offset)),
                Some((HeightReference::WorkOffset(0), 0.0)),
                "kind {kind} bottoms out at G54",
            );
            assert_eq!(
                p.clearance().reference,
                HeightReference::TopOfStock,
                "rapids stage above the stock",
            );
        }
        let OpParams::Face { cut_z, top_z, .. } = quick_params(0) else {
            panic!("kind 0 is a Face");
        };
        assert_eq!(
            cut_z.reference,
            HeightReference::TopOfStock,
            "a fresh face skims the stock top - a Bottom default would cut through everything",
        );
        assert!(top_z.is_none(), "staging is opt-in, never a surprise default");
    }
}

#[cfg(test)]
mod warning_fold_tests {
    //! M1.5.4 step 1: the display-side fold. The wall of identical
    //! "shank/holder at move N" lines was the hand-test complaint; the
    //! cache still stores every warning (file-format contract), the pane
    //! must not print every one.

    use super::fold_warnings;
    use hsmworks_app::verify::Warning;

    fn shank_run(from: usize, to: usize, part: bool) -> Vec<Warning> {
        (from..=to)
            .map(|at| Warning::ShankHit { at, part })
            .collect()
    }

    #[test]
    fn empty_and_singles_need_no_count() {
        assert!(fold_warnings(&[]).is_empty());
        assert_eq!(
            fold_warnings(&[Warning::Gouge { at: 7 }]),
            ["warning: move 7 cuts past the finish surface"]
        );
    }

    #[test]
    fn a_run_folds_to_its_first_line_plus_count() {
        let folded = fold_warnings(&shank_run(1131, 1146, false));
        assert_eq!(folded.len(), 1, "sixteen hits read as one line");
        assert_eq!(
            folded[0],
            "warning: shank/holder at move 1131 collides with the uncut stock \
             - repeated on 15 more moves (to 1146)"
        );
    }

    #[test]
    fn one_extra_repeat_reads_singular() {
        let folded = fold_warnings(&shank_run(4, 5, false));
        assert_eq!(
            folded[0],
            "warning: shank/holder at move 4 collides with the uncut stock \
             - repeated on 1 more move (to 5)"
        );
    }

    #[test]
    fn flavors_group_apart_and_order_is_first_appearance() {
        let mut w = shank_run(10, 12, false);
        w.push(Warning::ShankHit { at: 500, part: true });
        w.push(Warning::RapidInStock { at: 3 });
        w.push(Warning::ShankHit { at: 540, part: true });
        let folded = fold_warnings(&w);
        assert_eq!(folded.len(), 3);
        assert!(
            folded[0].starts_with("warning: shank/holder at move 10 collides with the uncut stock"),
            "{folded:?}"
        );
        // The island flavor leads with its own first hit, its run's end rides
        // the suffix, and the interleave does not reorder the kinds.
        assert_eq!(
            folded[1],
            "warning: shank/holder at move 500 collides with the part or an island \
             - repeated on 1 more move (to 540)"
        );
        assert_eq!(
            folded[2],
            "warning: rapid move 3 travels below the clearance plane inside stock"
        );
    }
}
