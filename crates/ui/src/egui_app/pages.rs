//! M-c: the property pages, graduated from the spike (S3) verbatim - the
//! dispatcher on the shell's ui target plus the five 2D operation pages, the
//! height law, and the setup pages (stock / work offsets / tool / model).
//!
//! These pages graduated from the spike verbatim; the imgui panels they
//! mirrored (operations.rs/setup.rs, plus the property-page code in
//! app.rs) retired at M-g, so the law they shared now lives in
//! `crate::operations` (height wording, retypes, warning folds - called
//! through, never restated) and here. Same fields, same notes and echoes,
//! same command shapes - every change rides `state.pending` /
//! the command methods, never an ad-hoc field poke (AGENTS.md, "Everything
//! Is a Command"). The M-c pages arrived as one scroll (parity first); the
//! named post-M-g carry-over landed here: the operation page is now the
//! specced named sections (SECTIONS - geometry, heights, passes, leads and
//! links, feeds) as a tab bar, each field keeping its within-section
//! order, the open tab transient pane state that queues nothing. Two
//! deliberate translations, both from the go/no-go
//! report (S4):
//!
//! * `InputDouble` becomes `DragValue` (divergence D5): egui has no
//!   InputDouble, and DragValue edits + commits with no custom state -
//!   click to type, drag to step. The typed-entry and drag paths hit the
//!   same field the imgui widget did; `tests.rs` proves both.
//! * The warning wall renders through `crate::operations::fold_warnings`
//!   (the M1.5.4 display law, reused, not restated).
//!
//! One string-level fix rides here: imgui's `##id` anchors ("Apply##stk")
//! showed only their head, but egui 0.36 has no `##` convention - it would
//! paint the anchor literally. Every such label is now plain ("Apply"),
//! which is what imgui displayed and therefore what parity means; egui's
//! auto-ids are unique per allocation, so the anchors were never needed.
//! (The spike's page code carried them; this step's column-model fix for
//! the browser was the same class of graduation-time repair.)
//!
//! The height rows keep M1.5.4's stacked law: `From` combo on its row (with
//! the Work-offset slot beside it), then `Offset`/`Value` with its resolved
//! `(= Z …)` echo - the pair that overflowed imgui's narrow pane now simply
//! stacks, because egui rows wrap nothing.
//!
//! `retype` (the type-change draft rule) is shared with the ribbon's Create
//! cells from `panes` - one law, one implementation.
//!
//! Provenance: our own port, specced from imgui operations.rs/setup.rs
//! (our code) and the egui vendored sources; no HSMWorks material involved.

use egui::{
    Align2, Color32, ComboBox, DragValue, FontId, Id, Label, Pos2, ScrollArea, Sense, Shape,
    Stroke, TextEdit, Ui, Vec2, Window,
};

use hsmworks_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use hsmworks_app::machining2d::{ARC_FIT_TOL, Lead, PocketStrategy, Side};
use hsmworks_app::model;
use hsmworks_app::operation::{self, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef};
use hsmworks_app::schema::{Operation, STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools;
use hsmworks_app::units::{self, Unit};
use hsmworks_app::verify::Warning;


use crate::actions;
use crate::operations::{
    fold_warnings, height_echo, offset_label, side_names, strategy_names, unresolved_note,
};
use crate::state::{CommandKind, ErrorScope, GeoPick, State, UiTarget, quat_yaw_deg};

use super::panes::{TYPES, kind_name, retype};
use super::theme::Palette;

/// Tool kind labels, same order as `ToolKind` (mirrors the tool page).
const TOOL_KINDS: [&str; 3] = ["End mill", "Ball nose", "Bull nose"];
/// Lead in/out combo items (mirrors operations.rs's LEADS).
const LEADS: [&str; 3] = ["None", "Linear", "Arc"];

/// The pane's amber for a chip whose feature id no longer resolves.
const AMBER: Color32 = Color32::from_rgb(0xB8, 0xB8, 0xC8);
/// The status-line red for a refused command (imgui's [1.0,0.35,0.35]).
const RED: Color32 = Color32::from_rgb(0xFF, 0x59, 0x59);
/// The note gray imgui's geometry notes use ([0.62,0.66,0.72]).
const NOTE: Color32 = Color32::from_rgb(0x9E, 0xA8, 0xB8);

/// The specced named sections of the operation page (ROADMAP M1.4: "tabs
/// and named sections (geometry, heights, passes, leads and links, feeds)
/// instead of long stacks"), in tab-bar order. The guard indices in
/// `params_fields` are these same numbers - one placement law, table and
/// guards read each other.
const SECTIONS: [&str; 5] = ["Geometry", "Heights", "Passes", "Leads & Links", "Feeds"];
const SEC_GEO: usize = 0;
const SEC_HEIGHTS: usize = 1;
const SEC_PASSES: usize = 2;
const SEC_LEADS: usize = 3;
const SEC_FEEDS: usize = 4;

/// Which sections an operation kind actually has fields (or the pick law)
/// for: Geometry/Heights/Feeds always; Passes except Trace (Trace has no
/// stepover or leave); Leads & Links only Contour - it is the kind with
/// lead moves and tabs. The tab bar shows exactly this: no dead tabs.
fn has_section(kind: OpType, s: usize) -> bool {
    match s {
        SEC_GEO | SEC_HEIGHTS | SEC_FEEDS => true,
        SEC_PASSES => !matches!(kind, OpType::Trace),
        SEC_LEADS => matches!(kind, OpType::Contour),
        _ => false,
    }
}

// --------------------------------------------------------------- properties

/// The dock's properties pane: dispatch on the shell's ui target, exactly
/// like operations.rs's contents() dispatcher.
pub fn properties(ui: &mut Ui, state: &mut State, _pal: &Palette) {
    let t = state.pane_target();
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match t {
                UiTarget::None => operation_page(ui, state, None),
                UiTarget::Operation(n) => operation_page(ui, state, Some(n.as_str())),
                UiTarget::Tool(n) => tool_page(ui, state, &n),
                UiTarget::Stock => {
                    state.sync_setup_ui();
                    ui.heading("Stock");
                    ui.separator();
                    stock_page(ui, state);
                    setup_error(ui, state);
                }
                UiTarget::WcsSlot(i) => {
                    state.sync_setup_ui();
                    let name = WCS_SLOTS.get(i).copied().unwrap_or("?");
                    ui.heading(format!("Work offset {name}"));
                    ui.separator();
                    wcs_page(ui, state, i);
                    setup_error(ui, state);
                }
                UiTarget::Model(n) => model_page(ui, state, n),
            }
        });
}

fn setup_error(ui: &mut Ui, state: &State) {
    if let Some(msg) = state.error_for(ErrorScope::Setup).map(str::to_owned) {
        ui.colored_label(RED, msg);
    }
}

// ------------------------------------------------------------ operation page

fn operation_page(ui: &mut Ui, state: &mut State, editing: Option<&str>) {
    state.sync_ops_ui();
    match editing {
        Some(name) => {
            ui.heading(format!("Operation: {name}"));
        }
        None => {
            // Typing the name is not an event; it rides the next Add command.
            ui.label("Name");
            ui.add(TextEdit::singleline(&mut state.op_ui.name).desired_width(f32::INFINITY));
        }
    }
    let target = state.op_ui.name.clone();
    let live: Option<Operation> = model::find_by_name(&state.world, &target)
        .as_ref()
        .filter(|e| model::is_operation(&state.world, e))
        .and_then(|e| model::operation_of(&state.world, e));
    let exists = live.is_some();
    ui.separator();

    // Type.
    let cur_kind_idx = kind_index(state.op_ui.params.kind());
    ui.label("Type");
    let mut kind = cur_kind_idx;
    combo(ui, "op_type", &mut kind, &TYPES);
    if kind != cur_kind_idx {
        let cur = state.op_ui.params.clone();
        state.op_ui.params = retype(&cur, kind.min(4));
    }

    // Tool.
    let mut tools = state.tool_names();
    if !state.op_ui.tool.is_empty() && !tools.contains(&state.op_ui.tool) {
        tools.push(state.op_ui.tool.clone());
    } else if state.op_ui.tool.is_empty() && let Some(first) = tools.first() {
        state.op_ui.tool = first.clone();
    }
    ui.label("Tool");
    if tools.is_empty() {
        ui.label("No tools in the document - the Tools page has a catalog.");
    } else {
        let mut t_idx = tools
            .iter()
            .position(|t| *t == state.op_ui.tool)
            .unwrap_or(0);
        let before = t_idx;
        let names: Vec<String> = tools.clone();
        combo_owned(ui, "op_tool", &mut t_idx, &names);
        if t_idx != before {
            state.op_ui.tool = tools[t_idx.min(tools.len() - 1)].clone();
            if exists {
                state.pending.push_back(CommandKind::EditOperationLinks {
                    name: target.clone(),
                    tool: Some(state.op_ui.tool.clone()),
                    src: None,
                });
            }
        }
    }

    // Source.
    let models: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| {
            !model::is_stock(&state.world, e)
                && !model::is_tool(&state.world, e)
                && !model::is_operation(&state.world, e)
        })
        .map(|(n, _)| n)
        .collect();
    ui.label("Source");
    let mut src_items: Vec<String> = vec!["Stock".to_owned()];
    src_items.extend(models.iter().cloned());
    let mut s_idx: usize = if state.op_ui.src_kind != 0 && !state.op_ui.src_name.is_empty() {
        match models.iter().position(|m| *m == state.op_ui.src_name) {
            Some(p) => 1 + p,
            None => {
                state.op_ui.src_kind = 0;
                state.op_ui.src_name.clear();
                0
            }
        }
    } else {
        0
    };
    if state.op_ui.src_kind == 2 {
        s_idx = 0;
    }
    let before = s_idx;
    combo_owned(ui, "op_src", &mut s_idx, &src_items);
    if s_idx != before {
        if s_idx == 0 {
            state.op_ui.src_kind = 0;
            state.op_ui.src_name.clear();
        } else {
            state.op_ui.src_kind = 1;
            state.op_ui.src_name = models[s_idx - 1].clone();
        }
        if exists {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.clone(),
                tool: None,
                src: Some(state.op_ui.src()),
            });
        }
    }

    // Status above the tab bar (it sat below the Geometry block in the
    // single scroll): a STALE line and the warning fold must stay visible
    // whichever section is open.
    if exists {
        let ent = model::find_by_name(&state.world, &target);
        let (moves, warnings): (usize, Vec<Warning>) = ent
            .as_ref()
            .and_then(|e| model::toolpath_of(&state.world, e))
            .map(|tp| (tp.path.len(), tp.warnings))
            .unwrap_or((0, Vec::new()));
        let status_line = match ent.as_ref().map(|e| operation::status_of(&state.world, e)) {
            Some(operation::OpStatus::Never) | None => "Status: never generated".to_owned(),
            Some(operation::OpStatus::Ok) => format!("Status: OK - {moves} move(s) cached"),
            Some(operation::OpStatus::Stale) => {
                format!("Status: STALE - inputs moved since generation ({moves} move(s) cached)")
            }
        };
        ui.label(status_line);
        // The M1.5.4 fold, reused: one line per warning kind, not a wall.
        for line in fold_warnings(&warnings) {
            ui.label(line);
        }
    }
    ui.separator();

    // The named sections (M1.4's tabbing of M-c's single scroll): one tab
    // per specced group, SECTIONS order; a kind shows only the sections it
    // has fields for - no dead tabs. The open tab is transient pane state
    // (`State::op_tab`), never a command, never saved: switching it queues
    // nothing, and every edit rides its own tab exactly as it did in the
    // scroll (hidden guards write no params).
    let kind = state.op_ui.params.kind();
    let want = state.op_tab.min(SECTIONS.len() - 1);
    let mut tab = if has_section(kind, want) { want } else { SEC_GEO };
    ui.horizontal_wrapped(|ui| {
        for (i, name) in SECTIONS.iter().enumerate() {
            if !has_section(kind, i) {
                continue;
            }
            // Wrapped, not one row: the properties pane is ~22% wide and
            // five full names overflow it - an off-pane tab would take
            // clicks meant for the viewport (M1.4's pane-width law).
            if ui.selectable_label(tab == i, *name).clicked() {
                state.op_tab = i;
                tab = i;
            }
        }
    });
    ui.separator();

    // The Geometry section's pick block (params' geometry fields follow in
    // params_fields). Viewport pick mode - transient view state like the
    // drafts this page edits: the mode itself is not undoable; what a
    // Face/Edge click changes is (M-e: the radio row, because the egui
    // viewport honors geo_pick exactly like the imgui one did).
    if tab == SEC_GEO {
        ui.horizontal(|ui| {
            ui.label("Click selects:");
            let mut pick_mode = match state.geo_pick {
                GeoPick::Model => 0,
                GeoPick::Face => 1,
                GeoPick::Edge => 2,
            };
            ui.radio_value(&mut pick_mode, 0, "Models");
            ui.radio_value(&mut pick_mode, 1, "Faces");
            ui.radio_value(&mut pick_mode, 2, "Edges");
            state.geo_pick = match pick_mode {
                1 => GeoPick::Face,
                2 => GeoPick::Edge,
                _ => GeoPick::Model,
            };
        });
        ui.label("Click in the viewport - Ctrl adds; Alt in Edge mode takes one edge.");
        if state.op_ui.src_kind == 2 {
            // Hovering a row lights that feature in the viewport (screen-space
            // amber, the shared interact law) - so the chip carries its GeoSel.
            let chips: Vec<(String, bool, GeoSel)> = state
                .geo_chips()
                .into_iter()
                .map(|c| (c.label, c.missing, c.sel))
                .collect();
            for (i, (label, missing, sel)) in chips.into_iter().enumerate() {
                let remove = ui
                    .horizontal(|ui| {
                        let resp = if missing {
                            ui.colored_label(AMBER, &label)
                        } else {
                            ui.label(&label)
                        };
                        if resp.hovered() {
                            state.geo_hover = Some(sel);
                        }
                        ui.small_button("×").clicked()
                    })
                    .inner;
                if remove {
                    let mut rest: Vec<GeoSel> = state.op_ui.geo.clone();
                    if i < rest.len() {
                        rest.remove(i);
                        if rest.is_empty() {
                            state.op_ui.geo.clear();
                            state.op_ui.src_kind = 0;
                            state.pending.push_back(CommandKind::EditOperationLinks {
                                name: target.clone(),
                                tool: None,
                                src: Some(SrcRef::Stock),
                            });
                        } else {
                            state.op_ui.geo = rest.clone();
                            state.pending.push_back(CommandKind::SetOperationGeometry {
                                name: target.clone(),
                                items: rest,
                            });
                        }
                    }
                }
            }
            if ui.small_button("Clear").clicked() {
                state.pending.push_back(CommandKind::EditOperationLinks {
                    name: target.clone(),
                    tool: None,
                    src: Some(SrcRef::Stock),
                });
            }
        }
        let notes: Vec<String> = operation::selection_notes(
            &state.world,
            &state.op_ui.geo.clone(),
            &state.op_ui.params.clone(),
        );
        for note in notes {
            ui.colored_label(NOTE, note);
        }
        if state.op_ui.src_kind == 1 && !state.op_ui.src_name.is_empty() {
            let n = state.op_ui.src_name.clone();
            ui.label(format!("Whole of '{n}' - the first pick replaces this."));
        } else if state.op_ui.src_kind != 2 {
            ui.label("Whole stock contour (default) - picks bound the operation.");
        }
    }

    // Parameters, placed on the section tabs.
    params_fields(ui, state, tab);
    // The whole-panel unresolved line is operations.rs's law, same as the
    // retired imgui page's.
    if let Some(note) = unresolved_note(&state.world, &state.op_ui.params) {
        ui.label(note);
    }
    ui.separator();

    // Footer.
    if exists {
        let unchanged = live.as_ref().is_some_and(|o| o.params == state.op_ui.params);
        ui.horizontal(|ui| {
            let apply = ui.add_enabled(!unchanged, egui::Button::new("Apply Parameters"));
            if apply.clicked() {
                state.apply_operation_params(&target);
            }
            if ui.button("Generate").clicked() {
                state.generate_operation(&target);
            }
            if ui.button("Delete").clicked() {
                state.pending.push_back(CommandKind::DeleteOperation(target.clone()));
            }
        });
        if unchanged {
            ui.label("Apply is a no-op until a parameter differs from the operation.");
        }
    } else {
        let name_ok = !state.op_ui.name.trim().is_empty();
        let dup = name_ok && model::find_by_name(&state.world, &state.op_ui.name.clone()).is_some();
        let tool_ok = !state.op_ui.tool.trim().is_empty()
            && model::find_by_name(&state.world, &state.op_ui.tool.clone())
                .is_some_and(|e| model::is_tool(&state.world, &e));
        let mut block = String::new();
        if !name_ok {
            block = "Name the operation first.".into();
        } else if dup {
            block = "That name is already in the document.".into();
        } else if !tool_ok {
            block = "No tool to cut with yet - add one from the Tools page.".into();
        }
        let name = state.op_ui.name.trim().to_owned();
        let tool = state.op_ui.tool.clone();
        let src = state.op_ui.src();
        let params = state.op_ui.params.clone();
        let add = ui.add_enabled(block.is_empty(), egui::Button::new("Add Operation"));
        if add.clicked() {
            state.pending.push_back(CommandKind::CreateOperation {
                name: name.clone(),
                tool,
                src,
                params,
            });
            state.pending.push_back(CommandKind::Select(Some(name.clone())));
            state.ui_target = UiTarget::Operation(name);
        }
        if !block.is_empty() {
            ui.horizontal(|ui| {
                ui.label(block);
            });
        }
        if let Some(msg) = state.error_for(ErrorScope::Operation).map(str::to_owned) {
            ui.colored_label(RED, msg);
        }
    }
}

fn kind_index(kind: OpType) -> usize {
    match kind {
        OpType::Face => 0,
        OpType::Contour => 1,
        OpType::Pocket => 2,
        OpType::Slot => 3,
        OpType::Trace => 4,
    }
}

// --------------------------------------------------------------- param rows

/// The params block for all five kinds - operations.rs's fields, notes and
/// echoes, unchanged in content and in within-section order, now placed on
/// the section tabs: each field renders when, and only when, its section
/// is the open tab (SECTIONS/has_section are the placement law; the guards
/// below read them as indices). Numbers are DragValue instead of
/// InputDouble (divergence D5, S4 report): egui has no InputDouble;
/// DragValue edits + commits with no custom state.
fn params_fields(ui: &mut Ui, state: &mut State, tab: usize) {
    let u = state.units();
    let mut side_note: Option<String> = None;
    let mut auto_note: Option<String> = None;
    let nm = state.op_ui.name.clone();
    let op = model::find_by_name(&state.world, &nm)
        .as_ref()
        .and_then(|e| model::operation_of(&state.world, e));
    if let OpParams::Contour { side: Side::Auto, .. } = &state.op_ui.params {
        let resolved = op
            .as_ref()
            .and_then(|o| operation::contour_auto_side(&state.world, &o.src, &state.op_ui.params.clone()));
        side_note = Some(match resolved {
            Some(true) => "Auto -> Inside: the picked rings bound air, so the metal is outside them.".into(),
            Some(false) => "Auto -> Outside: the picked rings bound material.".into(),
            None => "Auto picks the side from the selection at generate.".into(),
        });
    }
    if let OpParams::Pocket { strategy: PocketStrategy::Auto, stepover, .. } = &state.op_ui.params {
        let pick = "Auto picks the shape from the pocket's outline at generate.".to_owned();
        auto_note = Some(match op.as_ref() {
            Some(o) => {
                let region = operation::region_of(&state.world, o).unwrap_or_default();
                let diameter = model::find_by_name(&state.world, &o.tool)
                    .as_ref()
                    .and_then(|t| model::tool_of(&state.world, t))
                    .map_or(6.0, |t| t.diameter);
                let outer: Vec<hsmworks_app::geo2d::Point2> = region
                    .loops()
                    .iter()
                    .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
                    .map(|l| l.points().to_vec())
                    .unwrap_or_default();
                if outer.is_empty() {
                    pick
                } else {
                    match PocketStrategy::Auto.resolve(&outer, *stepover, diameter, ARC_FIT_TOL) {
                        PocketStrategy::Waterline => {
                            "Auto -> Waterline: the outline is round and the rings reach every band.".into()
                        }
                        _ => "Auto -> Zigzag: rings would not suit this outline.".into(),
                    }
                }
            }
            None => pick,
        });
    }

    match &mut state.op_ui.params {
        OpParams::Face { stepover, clearance_z, cut_z, top_z, stepdown, feed, plunge_feed, climb } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(ui, &state.world, clearance_z, "face.clearance");
                field(ui, "Cut Z");
                height_editor(ui, &state.world, cut_z, "face.cut");
                field(ui, "Depth staging");
                let mut staged = top_z.is_some();
                if ui.checkbox(&mut staged, "Stage down to Cut Z").changed() {
                    if staged {
                        let seed = Height::of(cut_z.reference, cut_z.offset + 1.0);
                        *top_z = Some(seed);
                        *stepdown = Some(1.0);
                    } else {
                        *top_z = None;
                        *stepdown = None;
                    }
                }
                if staged {
                    field(ui, "Top Z (first cut level)");
                    let seed = Height::of(cut_z.reference, cut_z.offset + 1.0);
                    height_editor(ui, &state.world, top_z.get_or_insert(seed), "face.top");
                    field(ui, "Stepdown");
                    num(ui, u, stepdown.get_or_insert(1.0));
                }
            }
            if tab == SEC_PASSES {
                field(ui, "Stepover (Y spacing)");
                num(ui, u, stepover);
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
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
            side,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
        } => {
            if tab == SEC_GEO {
                field(ui, "Side");
                let mut s_idx = match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                };
                combo(ui, "c_side", &mut s_idx, side_names());
                *side = match s_idx {
                    1 => Side::Inside,
                    2 => Side::Outside,
                    _ => Side::Auto,
                };
                if let Some(note) = &side_note {
                    ui.label(note.clone());
                }
                field(ui, "Tolerance");
                num(ui, u, tolerance);
            }
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(ui, &state.world, clearance_z, "contour.clearance");
                field(ui, "Retract Z (between passes)");
                height_editor(ui, &state.world, retract_z, "contour.retract");
                if let (Some(rz), Some(cz)) =
                    (retract_z.z(&state.world), clearance_z.z(&state.world))
                    && rz > cz
                {
                    ui.label(format!(
                        "above clearance - the hop travels at {} (min of the two)", units::len(cz, u, 3)
                    ));
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, top_z, "contour.top");
                field(ui, "Bottom Z");
                height_editor(ui, &state.world, bottom_z, "contour.bottom");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
            }
            if tab == SEC_PASSES {
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
            }
            if tab == SEC_LEADS {
                field(ui, "Lead in/out");
                let mut l_idx = match lead {
                    Lead::None => 0,
                    Lead::Linear { .. } => 1,
                    Lead::Arc { .. } => 2,
                };
                let before = l_idx;
                combo(ui, "c_lead", &mut l_idx, &LEADS);
                if l_idx != before {
                    *lead = match l_idx {
                        1 => Lead::Linear { length: 8.0 },
                        2 => Lead::Arc { radius: 5.0 },
                        _ => Lead::None,
                    };
                }
                match lead {
                    Lead::Linear { length } => {
                        field(ui, "Lead length");
                        num(ui, u, length);
                    }
                    Lead::Arc { radius } => {
                        field(ui, "Lead arc radius");
                        num(ui, u, radius);
                    }
                    Lead::None => {}
                }
                field(ui, "Tab count");
                let mut tabs = f64::from(*tab_count);
                num_plain(ui, &mut tabs);
                *tab_count = tabs.clamp(0.0, 99.0).round().max(0.0) as u32;
                field(ui, "Tab width");
                num(ui, u, tab_width);
                field(ui, "Tab height");
                num(ui, u, tab_height);
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
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
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(ui, &state.world, clearance_z, "pocket.clearance");
                field(ui, "Retract plane");
                let mut hop = retract_z.is_some();
                if ui.checkbox(&mut hop, "Hop below clearance").changed() {
                    *retract_z = if hop {
                        Some(Height::of(top_z.reference, top_z.offset + 1.0))
                    } else {
                        None
                    };
                }
                if let Some(r) = retract_z {
                    field(ui, "Retract Z");
                    height_editor(ui, &state.world, r, "pocket.retract");
                    if let (Some(rz), Some(cz)) =
                        (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)", units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, top_z, "pocket.top");
                field(ui, "Bottom Z");
                height_editor(ui, &state.world, bottom_z, "pocket.bottom");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
            }
            if tab == SEC_PASSES {
                field(ui, "Stepover");
                num(ui, u, stepover);
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                field(ui, "Strategy");
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                };
                let names = strategy_names();
                let before = st_idx;
                combo(ui, "p_strat", &mut st_idx, names);
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Auto,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if let Some(note) = &auto_note {
                    ui.label(note.clone());
                }
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            clearance_z,
            retract_z: _,
            top_z,
            bottom_z,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
        } => {
            if tab == SEC_GEO {
                field(ui, "A X");
                num(ui, u, &mut a.x);
                field(ui, "A Y");
                num(ui, u, &mut a.y);
                field(ui, "B X");
                num(ui, u, &mut b.x);
                field(ui, "B Y");
                num(ui, u, &mut b.y);
                field(ui, "Slot width");
                num(ui, u, width);
            }
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(ui, &state.world, clearance_z, "slot.clearance");
                field(ui, "Top Z");
                height_editor(ui, &state.world, top_z, "slot.top");
                field(ui, "Bottom Z");
                height_editor(ui, &state.world, bottom_z, "slot.bottom");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
            }
            if tab == SEC_PASSES {
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                field(ui, "Stepover");
                num(ui, u, stepover);
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, stepdown, feed, plunge_feed, climb } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(ui, &state.world, clearance_z, "trace.clearance");
                field(ui, "Retract plane");
                let mut hop = retract_z.is_some();
                if ui.checkbox(&mut hop, "Hop below clearance").changed() {
                    *retract_z = if hop {
                        Some(Height::of(top_z.reference, top_z.offset + 1.0))
                    } else {
                        None
                    };
                }
                if let Some(r) = retract_z {
                    field(ui, "Retract Z");
                    height_editor(ui, &state.world, r, "trace.retract");
                    if let (Some(rz), Some(cz)) =
                        (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)", units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, top_z, "trace.top");
                field(ui, "Bottom Z");
                height_editor(ui, &state.world, bottom_z, "trace.bottom");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
    }
}

fn field(ui: &mut Ui, label: &str) {
    ui.add_space(3.0);
    ui.label(label);
}

fn num(ui: &mut Ui, u: Unit, v: &mut f64) {
    num_prec(ui, u, v, 4);
}

/// The unit-lens widget law (M1.1): in mm mode this is the untouched
/// historical `DragValue` (speed 0.1, default format), so every pre-inch
/// string, test and snapshot keeps its bits. In inch mode the widget
/// works on a display copy - `units::from_display` puts the world value
/// back on the micron lattice, so a typed `0.5` stores exactly `12.7`.
/// The drag speed divides by 25.4 so one pixel moves the same *millimeter*
/// distance either way - the unit is a lens, not a gearbox. egui's smart
/// aim rounds a drag toward the nicest number of the SHOWN unit within a
/// one-pixel radius; because of that speed law the radius is the same
/// ±0.1 mm in both modes, so the assist is unit-fair (0.355 in is exactly
/// what a machinist dragging near it wants).
fn num_prec(ui: &mut Ui, u: Unit, v: &mut f64, prec: usize) {
    if u == Unit::Millimeters {
        ui.add(DragValue::new(v).speed(0.1));
    } else {
        let mut d = units::to_display(*v, u);
        let resp = ui.add(
            DragValue::new(&mut d)
                .speed(0.1 / 25.4)
                .custom_formatter(move |n, _| format!("{n:.prec$}")),
        );
        if resp.changed() {
            *v = units::from_display(d, u);
        }
    }
}

/// Feed rates (mm/min stored, in/min shown): same lens, coarser text.
fn num_feed(ui: &mut Ui, u: Unit, v: &mut f64) {
    num_prec(ui, u, v, 2);
}

/// Degrees, rpm, counts: no lens - these were never millimeters.
fn num_plain(ui: &mut Ui, v: &mut f64) {
    ui.add(DragValue::new(v).speed(0.1));
}

/// Combo over `&[&str]` with a usize model (imgui's combo idiom).
fn combo(ui: &mut Ui, id: &str, idx: &mut usize, names: &[&str]) {
    let cur = names
        .get((*idx).min(names.len().saturating_sub(1)))
        .copied()
        .unwrap_or("");
    ComboBox::from_id_salt(id)
        .selected_text(cur)
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(idx, i, *n);
            }
        });
}

/// Same, for owned name lists (tool/source lists built per frame).
fn combo_owned(ui: &mut Ui, id: &str, idx: &mut usize, names: &[String]) {
    let cur: String = names
        .get((*idx).min(names.len().saturating_sub(1)))
        .cloned()
        .unwrap_or_default();
    ComboBox::from_id_salt(id)
        .selected_text(cur)
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(idx, i, n.as_str());
            }
        });
}

/// The From/Offset pair, two stacked rows (M1.5.4's fix, mirrored): the kind
/// combo - with the Work-offset slot beside it - then the value with its
/// resolved-Z or unresolved echo.
/// `tag` scopes the widgets' egui ids: the combos salt through
/// `make_persistent_id`, which ignores position, so five height editors in
/// one panel would otherwise share one id per combo (the duplicate-id
/// badges the first S2 dump caught). Each call site passes its section name.
fn height_editor(ui: &mut Ui, world: &flecs_ecs::prelude::World, h: &mut Height, tag: &str) {
    ui.push_id(tag, |ui| {
        height_editor_inner(ui, world, h);
    });
}

fn height_editor_inner(ui: &mut Ui, world: &flecs_ecs::prelude::World, h: &mut Height) {
    let u = units::of(world);
    let mut idx = h.reference.kind_index().min(5);
    let before = idx;
    ui.horizontal(|ui| {
        ui.label("From");
        combo(ui, "h_kind", &mut idx, &HeightReference::KIND_LABELS);
    });
    if idx != before {
        h.reference = match idx {
            0 => HeightReference::Fixed,
            1 => HeightReference::TopOfStock,
            2 => HeightReference::BottomOfStock,
            3 => HeightReference::TopOfModel,
            4 => HeightReference::BottomOfModel,
            _ => HeightReference::WorkOffset(h.reference.wcs_slot().min(WCS_SLOTS.len() - 1)),
        };
    }
    if matches!(h.reference, HeightReference::WorkOffset(_)) {
        let mut slot = h.reference.wcs_slot().min(WCS_SLOTS.len() - 1);
        ui.horizontal(|ui| {
            ui.label("WCS");
            combo(ui, "h_wcs", &mut slot, &WCS_SLOTS);
        });
        h.reference = HeightReference::WorkOffset(slot.min(WCS_SLOTS.len() - 1));
    }
    // Both strings are operations.rs's law (the retired imgui height row
    // said the same words); the page only lays them out.
    ui.horizontal(|ui| {
        ui.label(offset_label(h.reference));
        num(ui, u, &mut h.offset);
        ui.add(Label::new(egui::RichText::new(height_echo(world, h)).weak()).selectable(false));
    });
}

// --------------------------------------------------------------- stock page

fn stock_page(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    // The document's unit lens, straight off the world singleton - the
    // click queues SetUnits; the numbers below re-render from the world
    // next pass, so there is no mirror to keep in step.
    ui.horizontal(|ui| {
        ui.label("Units");
        for cand in [Unit::Millimeters, Unit::Inches] {
            if ui.radio(u == cand, cand.label()).clicked() && u != cand {
                state.pending.push_back(CommandKind::SetUnits(cand));
            }
        }
    });
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        for (i, name) in ["Box", "Cylinder", "From model"].iter().enumerate() {
            if ui.radio_value(&mut state.stock_ui.shape, i as i32, *name).changed() {}
        }
    });
    let shape = state.stock_ui.shape;
    if shape == 1 {
        field(ui, "Diameter");
        num(ui, u, &mut state.stock_ui.diameter);
        field(ui, "Height");
        num(ui, u, &mut state.stock_ui.height);
    } else if shape == 2 {
        let models: Vec<String> = model::named_entities(&state.world)
            .into_iter()
            .filter(|(_, e)| {
                !model::is_stock(&state.world, e)
                    && !model::is_tool(&state.world, e)
                    && !model::is_operation(&state.world, e)
            })
            .map(|(n, _)| n)
            .collect();
        field(ui, "Source");
        if models.is_empty() {
            ui.label("No models to take stock from.");
        } else {
            let mut idx = models
                .iter()
                .position(|m| *m == state.stock_ui.source)
                .unwrap_or(models.len() - 1);
            let before = idx;
            combo_owned(ui, "stk_src", &mut idx, &models);
            if idx != before {
                state.stock_ui.source = models[idx.min(models.len() - 1)].clone();
            }
        }
    } else {
        field(ui, "Length (X)");
        num(ui, u, &mut state.stock_ui.dx);
        field(ui, "Width (Y)");
        num(ui, u, &mut state.stock_ui.dy);
        field(ui, "Height (Z)");
        num(ui, u, &mut state.stock_ui.dz);
    }
    ui.add_space(6.0);
    field(ui, "X center");
    num(ui, u, &mut state.stock_ui.x);
    field(ui, "Y center");
    num(ui, u, &mut state.stock_ui.y);
    field(ui, "Z bottom");
    num(ui, u, &mut state.stock_ui.z);
    field(ui, "Rotation (yaw deg)");
    num_plain(ui, &mut state.stock_ui.yaw_deg);
    ui.add_space(6.0);
    field(ui, "Fit margin");
    num(ui, u, &mut state.stock_ui.fit_margin);
    let shape = state.stock_ui.shape;
    let (dx, dy, dz) = (state.stock_ui.dx, state.stock_ui.dy, state.stock_ui.dz);
    let (diameter, height) = (state.stock_ui.diameter, state.stock_ui.height);
    let source = state.stock_ui.source.clone();
    let pos = [state.stock_ui.x, state.stock_ui.y, state.stock_ui.z];
    let yaw = state.stock_ui.yaw_deg;
    let margin = state.stock_ui.fit_margin;
    let has_stock =
        model::find_by_name(&state.world, STOCK_NAME).is_some_and(|e| model::is_stock(&state.world, &e));
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            let s = if shape == 1 {
                StockShape::Cylinder { diameter, height }
            } else if shape == 2 {
                StockShape::FromModel { source: source.clone() }
            } else {
                StockShape::Box { dx, dy, dz }
            };
            state.pending.push_back(CommandKind::SetStock {
                shape: s,
                pos,
                yaw_deg: yaw,
            });
        }
        if ui.button("Fit to Models").clicked() {
            state.pending.push_back(CommandKind::StockFit { margin, shape });
        }
        if has_stock && ui.button("Delete").clicked() {
            state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.to_owned()));
        }
    });
}

// ------------------------------------------------------------------ wcs page

fn wcs_page(ui: &mut Ui, state: &mut State, slot: usize) {
    let u = state.units();
    if state.wcs_ui.slot != slot {
        load_wcs_slot(state, slot);
    }
    field(ui, "X origin");
    num(ui, u, &mut state.wcs_ui.x);
    field(ui, "Y origin");
    num(ui, u, &mut state.wcs_ui.y);
    field(ui, "Z origin");
    num(ui, u, &mut state.wcs_ui.z);
    field(ui, "Tilt about X (roll)");
    num_plain(ui, &mut state.wcs_ui.roll_deg);
    field(ui, "Tilt about Y (pitch)");
    num_plain(ui, &mut state.wcs_ui.pitch_deg);
    field(ui, "Turn about Z (yaw)");
    num_plain(ui, &mut state.wcs_ui.yaw_deg);
    ui.add_space(6.0);
    ui.label("Quarter-turns about world axes (apply immediately):");
    const TURNS: [(&str, usize, f64); 6] = [
        ("X+", 0, 1.0),
        ("X-", 0, -1.0),
        ("Y+", 1, 1.0),
        ("Y-", 1, -1.0),
        ("Z+", 2, 1.0),
        ("Z-", 2, -1.0),
    ];
    for row in [0usize, 3] {
        ui.horizontal(|ui| {
            for (label, axis, sign) in TURNS[row..row + 3].iter() {
                if ui.button(format!("Turn {label}")).clicked() {
                    turn_wcs(state, *axis, *sign);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Level").clicked() {
            state.wcs_ui.roll_deg = 0.0;
            state.wcs_ui.pitch_deg = 0.0;
            state.wcs_ui.yaw_deg = 0.0;
            apply_wcs_drafts(state);
        }
        if ui.button("Set Origin").clicked() {
            apply_wcs_drafts(state);
        }
    });
    ui.add_space(6.0);
    ui.label("Placed offsets:");
    for (i, name) in WCS_SLOTS.iter().enumerate() {
        let line = match model::find_by_name(&state.world, name)
            .as_ref()
            .and_then(|e| model::transform_of(&state.world, e))
        {
            Some(loc) => format!(
                "{name}: {}, {}, {}  yaw {:.1}",
                units::len(loc.pos.x, u, 2),
                units::len(loc.pos.y, u, 2),
                units::len(loc.pos.z, u, 2),
                quat_yaw_deg(loc.rot)
            ),
            None => format!("{name}: -"),
        };
        ui.label(line);
        let _ = i;
    }
}

fn load_wcs_slot(state: &mut State, slot: usize) {
    state.wcs_ui.slot = slot.min(WCS_SLOTS.len() - 1);
    // The lib's own sync (state.rs sync_setup_ui) mirrors the world into
    // every wcs_ui field, `placed` included - that flag is pub(crate) and
    // not writable from a pane anyway. Same behavior, one source.
    state.sync_setup_ui();
}

fn apply_wcs_drafts(state: &mut State) {
    let slot = *WCS_SLOTS
        .get(state.wcs_ui.slot.min(WCS_SLOTS.len() - 1))
        .unwrap_or(&WCS_SLOTS[0]);
    state.pending.push_back(CommandKind::SetWcs {
        slot: slot.to_string(),
        pos: [state.wcs_ui.x, state.wcs_ui.y, state.wcs_ui.z],
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

fn turn_wcs(state: &mut State, axis: usize, sign: f64) {
    let q = quat_from_euler(
        state.wcs_ui.roll_deg.to_radians(),
        state.wcs_ui.pitch_deg.to_radians(),
        state.wcs_ui.yaw_deg.to_radians(),
    );
    let turn = quat_from_axis_angle(axis, sign * 90.0f64.to_radians());
    let (r, p, y) = quat_to_euler(quat_mul(turn, q));
    state.wcs_ui.roll_deg = r.to_degrees();
    state.wcs_ui.pitch_deg = p.to_degrees();
    state.wcs_ui.yaw_deg = y.to_degrees();
    apply_wcs_drafts(state);
}

// ----------------------------------------------------------------- tool page

/// The dock's tool surface is the LIBRARY (M1.4): what a tool in the
/// document is, in plain text, plus the verbs that point at it. The field
/// list moved to `tool_editor_window` - deliberately NO field widgets
/// live here, so every labeled field resolves in exactly one surface
/// (the a11y-query law the tests ride on).
fn tool_page(ui: &mut Ui, state: &mut State, name: &str) {
    let u = state.units();
    if !name.is_empty() && state.tool_ui.name != name {
        state.tool_ui.name = name.to_owned();
    }
    state.sync_tool_ui();
    // sync_tool_ui maintains `exists` from the world; the pane trusts it.
    let exists = state.tool_ui.exists;
    let nm = state.tool_ui.name.clone();
    if exists {
        ui.heading(format!("Tool: {nm}"));
        ui.separator();
        // The summary: the WORLD's numbers, not the draft's - what the
        // document holds right now, one line per fact.
        if let Some(t) = model::find_by_name(&state.world, &nm).and_then(|e| model::tool_of(&state.world, &e)) {
            ui.label(format!(
                "{}  Ø{}  corner r {}",
                kind_name(t.kind),
                units::len(t.diameter, u, 2),
                units::len(t.corner_r, u, 2)
            ));
            ui.label(format!(
                "cut {}  shank Ø{}×{}  holder Ø{}×{}",
                units::len(t.flute_len, u, 1),
                units::len(t.shank_d, u, 1),
                units::len(t.shank_len, u, 1),
                units::len(t.holder_d, u, 1),
                units::len(t.holder_len, u, 1)
            ));
            ui.label(format!(
                "{} rpm  {} {}  {}",
                t.rpm,
                units::feed(t.feed, u),
                u.feed_suffix(),
                t.material_group
            ));
        }
        ui.add_space(4.0);
        let (edit, del) = ui
            .horizontal(|ui| {
                (ui.button("Edit").clicked(), ui.button("Delete").clicked())
            })
            .inner;
        if del {
            state.pending.push_back(CommandKind::DeleteTool(nm.clone()));
        }
        if edit {
            actions::open_tool_editor(state, &nm);
        }
    } else {
        ui.heading("Tool library");
        ui.label("Pick a tool in the browser, or open the editor to create one.");
    }
    if ui.button("New tool…").clicked() {
        actions::open_tool_editor(state, "");
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
        ui.colored_label(RED, msg);
    }
    ui.separator();
    ui.heading("Catalog");
    for (cname, ct) in tools::catalog() {
        let taken = model::find_by_name(&state.world, cname).is_some();
        let clicked = ui
            .horizontal(|ui| {
                ui.label(format!("{}  Ø{} {}", cname, ct.diameter, kind_name(ct.kind)));
                if taken {
                    ui.label("(in document)");
                    false
                } else {
                    ui.button("Add").clicked()
                }
            })
            .inner;
        if clicked {
            state.pending.push_back(CommandKind::AddTool {
                name: cname.to_owned(),
                tool: ct,
            });
        }
    }
}

/// M1.4: the graphical tool editor - its own window, fields on the left,
/// a live to-scale side view on the right. An in-app `egui::Window`: the
/// shell has exactly one OS window and that law stands (RECOMMENDED.md).
/// Ephemeral subject like every pane: `state.tool_editor` says whether and
/// what it shows (None closed, Some(name) editing, Some("") a blank
/// draft); opening queues nothing, and the ghost law in
/// `State::drain_actions` closes it when the tool it names stops existing.
/// The numbers edit the shared `state.tool_ui` draft; only Apply/Add here
/// reach the world, through the same commands the dock's field list used
/// to carry. Delete stays a library verb (the dock owns it), which also
/// keeps every button label unique across the two surfaces.
pub(crate) fn tool_editor_window(ctx: &egui::Context, state: &mut State) {
    let u = state.units();
    let Some(_subject) = state.tool_editor.clone() else {
        return;
    };
    state.sync_tool_ui();
    let exists = state.tool_ui.exists;
    let title = if exists {
        format!("Tool: {}", state.tool_ui.name)
    } else {
        "New tool".to_owned()
    };
    let mut open = true;
    Window::new(title)
        .id(Id::new("tool-editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos([
            ctx.viewport_rect().center().x - 270.0,
            ctx.viewport_rect().center().y - 210.0,
        ])
        .show(ctx, |ui| {
            // One snapshot of the draft drives the picture, so fields and
            // preview cannot disagree mid-frame; the DragValues below still
            // edit the live draft, and the next frame's snapshot follows.
            let preview_tool = state.tool_ui.to_tool();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    if !exists {
                        // Typing the name is not an event; it rides the
                        // Add command at the bottom, same law as ever.
                        field(ui, "Name");
                        ui.add(
                            TextEdit::singleline(&mut state.tool_ui.name)
                                .desired_width(f32::INFINITY),
                        );
                    }
                    field(ui, "Type");
                    let mut k = state.tool_ui.kind.min(2);
                    combo(ui, "tool_kind", &mut k, &TOOL_KINDS);
                    state.tool_ui.kind = k.min(2);
                    field(ui, "Diameter");
                    num(ui, u, &mut state.tool_ui.diameter);
                    field(ui, "Corner radius");
                    num(ui, u, &mut state.tool_ui.corner_r);
                    field(ui, "Flute length");
                    num(ui, u, &mut state.tool_ui.flute_len);
                    field(ui, "Shank diameter");
                    num(ui, u, &mut state.tool_ui.shank_d);
                    field(ui, "Shank length");
                    num(ui, u, &mut state.tool_ui.shank_len);
                    field(ui, "Holder diameter");
                    num(ui, u, &mut state.tool_ui.holder_d);
                    field(ui, "Holder length");
                    num(ui, u, &mut state.tool_ui.holder_len);
                    field(ui, "Material group");
                    ui.add(
                        TextEdit::singleline(&mut state.tool_ui.material_group)
                            .desired_width(140.0),
                    );
                    field(ui, "Spindle rpm");
                    num_plain(ui, &mut state.tool_ui.rpm);
                    field(ui, &format!("Feed ({})", u.feed_suffix()));
                    num_feed(ui, u, &mut state.tool_ui.feed);
                });
                ui.vertical(|ui| {
                    draw_tool_preview(ui, &preview_tool, u);
                });
            });
            ui.separator();
            let name_now = state.tool_ui.name.clone();
            let block = if name_now.trim().is_empty() {
                "Name the tool first.".to_owned()
            } else if !exists && model::find_by_name(&state.world, &name_now).is_some() {
                "That name is already in the document.".to_owned()
            } else {
                String::new()
            };
            if exists {
                if ui.button("Apply").clicked() {
                    state.pending.push_back(CommandKind::ChangeTool {
                        name: name_now.clone(),
                        tool: state.tool_ui.to_tool(),
                    });
                }
            } else {
                let add = ui.add_enabled(block.is_empty(), egui::Button::new("Add tool"));
                if add.clicked() {
                    state.pending.push_back(CommandKind::AddTool {
                        name: name_now.trim().to_owned(),
                        tool: state.tool_ui.to_tool(),
                    });
                }
                if !block.is_empty() {
                    ui.label(block);
                }
            }
            if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
                ui.colored_label(RED, msg);
            }
        });
    if !open {
        state.tool_editor = None;
    }
}

/// The live preview: the tool drawn to scale from `tools::Tool::profile`,
/// the same band stack the collision checker sweeps, so what you see is
/// what `verify` would collide.
///
/// Pure state→paint: nothing here queues a command; it renders the draft
/// the fields beside it are editing. Tip at the bottom (z = 0 upward),
/// equal aspect both axes - "to scale" means to scale. A half-typed draft
/// renders a stub and a word, never a NaN.
fn draw_tool_preview(ui: &mut Ui, t: &tools::Tool, u: Unit) {
    const SIDE: f32 = 260.0;
    const PAD: f32 = 18.0;
    let (resp, painter) = ui.allocate_painter(Vec2::splat(SIDE), Sense::hover());
    let rect = resp.rect;
    let axis_x = rect.center().x;
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 31));
    let outline = t.profile();
    let r_max = outline.iter().map(|q| q.1).fold(0.0f64, f64::max);
    let z_max = outline.iter().map(|q| q.0).fold(0.0f64, f64::max);
    if r_max <= 0.0 || z_max <= 0.0 || !r_max.is_finite() || !z_max.is_finite() {
        painter.line_segment(
            [
                Pos2::new(axis_x, rect.min.y + PAD),
                Pos2::new(axis_x, rect.max.y - PAD),
            ],
            Stroke::new(1.0, NOTE),
        );
        painter.text(
            Pos2::new(axis_x, rect.center().y),
            Align2::CENTER_CENTER,
            "enter dimensions",
            FontId::proportional(11.0),
            NOTE,
        );
        return;
    }
    let base_y = rect.max.y - PAD;
    let scale = (((rect.width() - 2.0 * PAD) / (r_max as f32 * 2.0))
        .min((rect.height() - 2.0 * PAD) / z_max as f32))
        .max(f32::EPSILON);
    let at = |z: f64, r: f64| -> Pos2 {
        Pos2::new(
            axis_x + (r as f32 * scale),
            base_y - (z as f32 * scale),
        )
    };
    // The axis first, so everything steel-colored lies over it.
    painter.line_segment(
        [
            Pos2::new(axis_x, rect.min.y + 6.0),
            Pos2::new(axis_x, rect.max.y - 6.0),
        ],
        Stroke::new(1.0, Color32::from_rgb(70, 74, 84)),
    );
    let zc = t.flute_len.max(0.0);
    let zs = zc + t.shank_len.max(0.0);
    let steel = [
        Color32::from_rgb(168, 178, 192), // cutting band
        Color32::from_rgb(122, 130, 142), // shank
        Color32::from_rgb(86, 92, 104),   // holder
    ];
    // Band fills, one quad per outline pair: each z-monotone symmetric
    // slice is convex, so it fills honestly - the whole stepped silhouette
    // is not (concave at every band step) and egui fills convex only.
    // Radius steps (z0 == z1) carry no area; the slice above paints them.
    for w in outline.windows(2) {
        let (z0, r0) = w[0];
        let (z1, r1) = w[1];
        if !(z0.is_finite() && z1.is_finite() && r0.is_finite() && r1.is_finite()) || z1 <= z0 {
            continue;
        }
        let zm = (z0 + z1) * 0.5;
        let band = (usize::from(zm >= zc) + usize::from(zm >= zs)).min(2);
        painter.add(Shape::convex_polygon(
            vec![at(z0, -r0), at(z0, r0), at(z1, r1), at(z1, -r1)],
            steel[band],
            Stroke::NONE,
        ));
    }
    // The outline as one closed polyline (a stroke has no convexity rule):
    // right side up, then the mirrored left side back down. Axis points
    // (r = 0) appear on the right only - the mirror skips them so the tip
    // and top close with a single straight segment.
    let mut pts: Vec<Pos2> = Vec::with_capacity(outline.len() * 2);
    for &(z, r) in &outline {
        pts.push(at(z, r));
    }
    for &(z, r) in outline.iter().filter(|q| q.1 > 0.0).rev() {
        pts.push(at(z, -r));
    }
    painter.add(Shape::closed_line(
        pts,
        Stroke::new(1.0, Color32::from_rgb(214, 220, 228)),
    ));
    painter.text(
        Pos2::new(axis_x, rect.max.y - 4.0),
        Align2::CENTER_BOTTOM,
        format!("Ø {}", units::len(t.diameter, u, 2)),
        FontId::proportional(10.0),
        NOTE,
    );
    painter.text(
        Pos2::new(rect.min.x + 6.0, rect.min.y + 6.0),
        Align2::LEFT_TOP,
        format!("{} {} overall", units::len(z_max, u, 1), u.suffix()),
        FontId::proportional(10.0),
        NOTE,
    );
}

// ---------------------------------------------------------------- model page

fn model_page(ui: &mut Ui, state: &mut State, name: String) {
    let u = state.units();
    ui.heading(format!("Model: {name}"));
    let Some(entity) = model::find_by_name(&state.world, &name) else {
        ui.label("It is gone from the document.");
        return;
    };
    let tris = model::facets_of(&state.world, &entity).map_or(0, |f| f.len());
    ui.label(format!("{tris} triangle(s)"));
    if let Some(a) = model::entity_world_aabb(&state.world, &entity) {
        ui.label(format!(
            "{} x {} x {} {}",
            units::len(a.max.x - a.min.x, u, 1),
            units::len(a.max.y - a.min.y, u, 1),
            units::len(a.max.z - a.min.z, u, 1),
            u.suffix()
        ));
    }
    let users: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .filter_map(|(n, e)| {
            model::operation_of(&state.world, &e).and_then(|op| match &op.src {
                SrcRef::Entity(s) if *s == name => Some(n),
                _ => None,
            })
        })
        .collect();
    if users.is_empty() {
        ui.label("No operation uses it as a source.");
    } else {
        ui.label(format!("Source for: {}", users.join(", ")));
    }
    if model::stock_def_of(&state.world, &entity).is_some_and(|s| {
        matches!(s, StockShape::FromModel { source } if source == name)
    }) {
        ui.label("It is also the stock solid (Setup > Stock).");
    }
    ui.label("Selected now: Del removes it (undo restores).");
}

#[cfg(test)]
mod turn_tests {
    //! The turn/Level buttons are commands of their own now: the world -
    //! and with it the viewport gizmo - moves on the press. Moved verbatim
    //! from the retired imgui setup.rs at M-g; the code under test is
    //! pages.rs's own `turn_wcs`/`apply_wcs_drafts`.

    use super::*;
    use hsmworks_app::geometry::{Point3, transform_point};

    fn place(state: &mut State, pos: [f64; 3], yaw_deg: f64) {
        state.pending.push_back(CommandKind::SetWcs {
            slot: "G54".to_string(),
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg,
        });
        state.drain_actions();
    }

    fn location(state: &State, slot: &str) -> hsmworks_app::schema::Location {
        let entity = model::find_by_name(&state.world, slot).expect("offset entity");
        model::transform_of(&state.world, &entity).expect("location")
    }

    #[test]
    fn a_turn_moves_the_world_not_just_the_drafts() {
        let mut state = State::new();
        place(&mut state, [10.0, 0.0, 0.0], 0.0);
        state.sync_setup_ui();
        turn_wcs(&mut state, 0, 1.0); // X+ quarter about the world X
        state.drain_actions();
        let loc = location(&state, "G54");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [10.0, 0.0, 0.0]);
        // Right-hand +90 about X takes +Z to -Y.
        let p = transform_point(Point3::new(0.0, 0.0, 1.0), loc.pos, loc.rot);
        assert!(
            [(p.x - 10.0), (p.y + 1.0), p.z].iter().all(|v| v.abs() < 1e-9),
            "{p:?}"
        );
    }

    #[test]
    fn four_turns_about_one_axis_return_to_the_start() {
        let mut state = State::new();
        place(&mut state, [0.0, 0.0, 0.0], 30.0);
        state.sync_setup_ui();
        for _ in 0..4 {
            turn_wcs(&mut state, 2, 1.0);
        }
        state.drain_actions();
        let loc = location(&state, "G54");
        // Same rotation as the 30-degree start: quaternion equal up to sign.
        let start = quat_from_euler(0.0, 0.0, 30.0f64.to_radians());
        let dot: f64 = start.iter().zip(loc.rot).map(|(a, b)| a * b).sum();
        assert!((dot.abs() - 1.0).abs() < 1e-9, "closed the circle: {:?}", loc.rot);
    }

    #[test]
    fn turning_an_unplaced_slot_applies_the_typed_position() {
        let mut state = State::new();
        state.wcs_ui.slot = 1; // G55
        state.wcs_ui.x = 5.0;
        turn_wcs(&mut state, 1, -1.0); // Y- quarter
        state.drain_actions();
        let loc = location(&state, "G55");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [5.0, 0.0, 0.0]);
        // Right-hand -90 about Y takes +X to +Z: the drafts, not just a
        // rotation, rode along.
        let p = transform_point(Point3::new(1.0, 0.0, 0.0), loc.pos, loc.rot);
        assert!(
            [(p.x - 5.0), p.y, (p.z - 1.0)].iter().all(|v| v.abs() < 1e-9),
            "{p:?}"
        );
    }

    /// M1.4: opening the editor is pure view state - the named path
    /// re-fetches the draft from the world (every field starts at the
    /// document's numbers, the law that lets the live preview be honest),
    /// the blank path starts a real draft numbered past what's taken, and
    /// neither queues a command.
    #[test]
    fn open_tool_editor_syncs_the_draft_from_the_world() {
        let mut state = State::new();
        let (name, tool) = tools::catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0600-4FL")
            .expect("test fixture: the catalog has it");
        state.pending.push_back(CommandKind::AddTool {
            name: name.to_owned(),
            tool: tool.clone(),
        });
        state.drain_actions();
        actions::open_tool_editor(&mut state, name);
        assert_eq!(state.tool_editor.as_deref(), Some(name));
        assert!(state.pending.is_empty(), "opening an editor queues nothing");
        assert!(state.tool_ui.exists);
        assert_eq!(state.tool_ui.name, name);
        assert_eq!(state.tool_ui.diameter, tool.diameter);
        assert_eq!(state.tool_ui.shank_d, tool.shank_d);
        assert_eq!(state.tool_ui.holder_len, tool.holder_len);
        // The blank path: "Tool2" - numbered past the one tool the
        // document now holds, with the starter's plausible numbers.
        actions::open_tool_editor(&mut state, "");
        assert_eq!(state.tool_editor.as_deref(), Some(""));
        assert_eq!(state.tool_ui.name, "Tool2");
        assert!(!state.tool_ui.exists);
        assert!(state.pending.is_empty());
    }
}


