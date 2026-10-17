//! M-c: the property pages, graduated from the spike (S3) verbatim - the
//! dispatcher on the shell's ui target plus the five 2D operation pages, the
//! height law, and the setup pages (stock / work offsets / tool / model).
//!
//! This is the mirror law of `operations.rs` and `setup.rs` (the imgui
//! originals): same fields, same order, same notes and echoes, same command
//! shapes - every change rides `state.pending` / the command methods, never
//! an ad-hoc field poke (AGENTS.md, "Everything Is a Command"). Two
//! deliberate translations, both from the go/no-go report (S4):
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

use egui::{Color32, ComboBox, DragValue, Label, ScrollArea, TextEdit, Ui};

use hsmworks_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use hsmworks_app::machining2d::{ARC_FIT_TOL, Lead, PocketStrategy, Side};
use hsmworks_app::model;
use hsmworks_app::operation::{self, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef};
use hsmworks_app::schema::{Operation, STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools;
use hsmworks_app::verify::Warning;


use crate::operations::{fold_warnings, side_names, strategy_names};
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

    // Geometry.
    ui.separator();
    ui.heading("Geometry");
    // Viewport pick mode - transient view state like the drafts this page
    // edits: the mode itself is not undoable; what a Face/Edge click
    // changes is (M-e: the imgui radio row, ported, because the egui
    // viewport now honors geo_pick exactly like the imgui one).
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
    let notes: Vec<String> =
        operation::selection_notes(&state.world, &state.op_ui.geo.clone(), &state.op_ui.params.clone());
    for note in notes {
        ui.colored_label(NOTE, note);
    }
    if state.op_ui.src_kind == 1 && !state.op_ui.src_name.is_empty() {
        let n = state.op_ui.src_name.clone();
        ui.label(format!("Whole of '{n}' - the first pick replaces this."));
    } else if state.op_ui.src_kind != 2 {
        ui.label("Whole stock contour (default) - picks bound the operation.");
    }

    // Status.
    if exists {
        ui.separator();
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

    // Parameters.
    params_fields(ui, state);
    if let Err(w) = operation::resolve_params(&state.world, &state.op_ui.params.clone()) {
        ui.label(format!("{w}; set the source up or switch the reference"));
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

/// The params block for all five kinds, mirrored verbatim from
/// operations.rs's params_fields (same fields, same order, same notes).
/// Numbers are DragValue instead of InputDouble (divergence D5, S4 report):
/// egui has no InputDouble; DragValue edits + commits with no custom state.
fn params_fields(ui: &mut Ui, state: &mut State) {
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
            field(ui, "Stepover (Y spacing)");
            num(ui, stepover);
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
                num(ui, stepdown.get_or_insert(1.0));
            }
            field(ui, "Feed (mm/min)");
            num(ui, feed);
            field(ui, "Plunge feed (mm/min)");
            num(ui, plunge_feed);
            ui.checkbox(climb, "Climb milling");
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
            field(ui, "Stock to leave");
            num(ui, stock_to_leave);
            field(ui, "Clearance Z");
            height_editor(ui, &state.world, clearance_z, "contour.clearance");
            field(ui, "Retract Z (between passes)");
            height_editor(ui, &state.world, retract_z, "contour.retract");
            if let (Some(rz), Some(cz)) = (retract_z.z(&state.world), clearance_z.z(&state.world))
                && rz > cz
            {
                ui.label(format!("above clearance - the hop travels at {cz:.3} (min of the two)"));
            }
            field(ui, "Top Z");
            height_editor(ui, &state.world, top_z, "contour.top");
            field(ui, "Bottom Z");
            height_editor(ui, &state.world, bottom_z, "contour.bottom");
            field(ui, "Stepdown");
            num(ui, stepdown);
            field(ui, "Feed (mm/min)");
            num(ui, feed);
            field(ui, "Plunge feed (mm/min)");
            num(ui, plunge_feed);
            ui.checkbox(climb, "Climb milling");
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
                    num(ui, length);
                }
                Lead::Arc { radius } => {
                    field(ui, "Lead arc radius");
                    num(ui, radius);
                }
                Lead::None => {}
            }
            field(ui, "Tolerance");
            num(ui, tolerance);
            field(ui, "Tab count");
            let mut tabs = f64::from(*tab_count);
            num(ui, &mut tabs);
            *tab_count = tabs.clamp(0.0, 99.0).round().max(0.0) as u32;
            field(ui, "Tab width");
            num(ui, tab_width);
            field(ui, "Tab height");
            num(ui, tab_height);
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
            field(ui, "Stepover");
            num(ui, stepover);
            field(ui, "Stock to leave");
            num(ui, stock_to_leave);
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
                if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                    && rz > cz
                {
                    ui.label(format!("above clearance - the hop travels at {cz:.3} (min of the two)"));
                }
            }
            field(ui, "Top Z");
            height_editor(ui, &state.world, top_z, "pocket.top");
            field(ui, "Bottom Z");
            height_editor(ui, &state.world, bottom_z, "pocket.bottom");
            field(ui, "Stepdown");
            num(ui, stepdown);
            field(ui, "Feed (mm/min)");
            num(ui, feed);
            field(ui, "Plunge feed (mm/min)");
            num(ui, plunge_feed);
            ui.checkbox(climb, "Climb milling");
            let mut st_idx = match strategy {
                PocketStrategy::Zigzag => 0,
                PocketStrategy::Waterline => 1,
                PocketStrategy::Auto => 2,
            };
            field(ui, "Strategy");
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
            field(ui, "A X");
            num(ui, &mut a.x);
            field(ui, "A Y");
            num(ui, &mut a.y);
            field(ui, "B X");
            num(ui, &mut b.x);
            field(ui, "B Y");
            num(ui, &mut b.y);
            field(ui, "Slot width");
            num(ui, width);
            field(ui, "Stock to leave");
            num(ui, stock_to_leave);
            field(ui, "Clearance Z");
            height_editor(ui, &state.world, clearance_z, "slot.clearance");
            field(ui, "Top Z");
            height_editor(ui, &state.world, top_z, "slot.top");
            field(ui, "Bottom Z");
            height_editor(ui, &state.world, bottom_z, "slot.bottom");
            field(ui, "Stepdown");
            num(ui, stepdown);
            field(ui, "Stepover");
            num(ui, stepover);
            field(ui, "Feed (mm/min)");
            num(ui, feed);
            field(ui, "Plunge feed (mm/min)");
            num(ui, plunge_feed);
            ui.checkbox(climb, "Climb milling");
        }
        OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, stepdown, feed, plunge_feed, climb } => {
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
                if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                    && rz > cz
                {
                    ui.label(format!("above clearance - the hop travels at {cz:.3} (min of the two)"));
                }
            }
            field(ui, "Top Z");
            height_editor(ui, &state.world, top_z, "trace.top");
            field(ui, "Bottom Z");
            height_editor(ui, &state.world, bottom_z, "trace.bottom");
            field(ui, "Stepdown");
            num(ui, stepdown);
            field(ui, "Feed (mm/min)");
            num(ui, feed);
            field(ui, "Plunge feed (mm/min)");
            num(ui, plunge_feed);
            ui.checkbox(climb, "Climb milling");
        }
    }
}

fn field(ui: &mut Ui, label: &str) {
    ui.add_space(3.0);
    ui.label(label);
}

fn num(ui: &mut Ui, v: &mut f64) {
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
    let vlabel = if matches!(h.reference, HeightReference::Fixed) {
        "Value"
    } else {
        "Offset"
    };
    ui.horizontal(|ui| {
        ui.label(vlabel);
        ui.add(DragValue::new(&mut h.offset).speed(0.1));
        let echo = match h.z(world) {
            Some(z) => format!("(= Z {z:.3})"),
            None => format!(
                "(unresolved: {})",
                operation::missing_source(world, h.reference).unwrap_or("source missing")
            ),
        };
        ui.add(Label::new(egui::RichText::new(echo).weak()).selectable(false));
    });
}

// --------------------------------------------------------------- stock page

fn stock_page(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        for (i, name) in ["Box", "Cylinder", "From model"].iter().enumerate() {
            if ui.radio_value(&mut state.stock_ui.shape, i as i32, *name).changed() {}
        }
    });
    let shape = state.stock_ui.shape;
    if shape == 1 {
        field(ui, "Diameter");
        num(ui, &mut state.stock_ui.diameter);
        field(ui, "Height");
        num(ui, &mut state.stock_ui.height);
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
        num(ui, &mut state.stock_ui.dx);
        field(ui, "Width (Y)");
        num(ui, &mut state.stock_ui.dy);
        field(ui, "Height (Z)");
        num(ui, &mut state.stock_ui.dz);
    }
    ui.add_space(6.0);
    field(ui, "X center");
    num(ui, &mut state.stock_ui.x);
    field(ui, "Y center");
    num(ui, &mut state.stock_ui.y);
    field(ui, "Z bottom");
    num(ui, &mut state.stock_ui.z);
    field(ui, "Rotation (yaw deg)");
    num(ui, &mut state.stock_ui.yaw_deg);
    ui.add_space(6.0);
    field(ui, "Fit margin");
    num(ui, &mut state.stock_ui.fit_margin);
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
    if state.wcs_ui.slot != slot {
        load_wcs_slot(state, slot);
    }
    field(ui, "X origin");
    num(ui, &mut state.wcs_ui.x);
    field(ui, "Y origin");
    num(ui, &mut state.wcs_ui.y);
    field(ui, "Z origin");
    num(ui, &mut state.wcs_ui.z);
    field(ui, "Tilt about X (roll)");
    num(ui, &mut state.wcs_ui.roll_deg);
    field(ui, "Tilt about Y (pitch)");
    num(ui, &mut state.wcs_ui.pitch_deg);
    field(ui, "Turn about Z (yaw)");
    num(ui, &mut state.wcs_ui.yaw_deg);
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
                "{name}: {:.2}, {:.2}, {:.2}  yaw {:.1}",
                loc.pos.x,
                loc.pos.y,
                loc.pos.z,
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

fn tool_page(ui: &mut Ui, state: &mut State, name: &str) {
    if !name.is_empty() && state.tool_ui.name != name {
        state.tool_ui.name = name.to_owned();
    }
    state.sync_tool_ui();
    let tu = &mut state.tool_ui;
    let nm = tu.name.clone();
    // sync_tool_ui maintains `exists` from the world; the pane trusts it.
    let exists = tu.exists;
    if exists {
        ui.heading(format!("Tool: {nm}"));
    } else {
        // Typing the name is not an event; it rides the next Add command.
        ui.label("Name");
        ui.add(TextEdit::singleline(&mut tu.name).desired_width(f32::INFINITY));
    }
    ui.separator();
    field(ui, "Type");
    let mut k = tu.kind.min(2);
    combo(ui, "tool_kind", &mut k, &TOOL_KINDS);
    tu.kind = k.min(2);
    field(ui, "Diameter");
    num(ui, &mut tu.diameter);
    field(ui, "Corner radius");
    num(ui, &mut tu.corner_r);
    field(ui, "Flute length");
    num(ui, &mut tu.flute_len);
    field(ui, "Shank diameter");
    num(ui, &mut tu.shank_d);
    field(ui, "Shank length");
    num(ui, &mut tu.shank_len);
    field(ui, "Holder diameter");
    num(ui, &mut tu.holder_d);
    field(ui, "Holder length");
    num(ui, &mut tu.holder_len);
    field(ui, "Material group");
    ui.add(TextEdit::singleline(&mut tu.material_group).desired_width(140.0));
    field(ui, "Spindle rpm");
    num(ui, &mut tu.rpm);
    field(ui, "Feed (mm/min)");
    num(ui, &mut tu.feed);
    ui.add_space(6.0);
    let name_now = tu.name.clone();
    let tool = tu.to_tool();
    let block = if name_now.trim().is_empty() {
        "Name the tool first.".to_owned()
    } else if !exists && model::find_by_name(&state.world, &name_now).is_some() {
        "That name is already in the document.".to_owned()
    } else {
        String::new()
    };
    if exists {
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                state.pending.push_back(CommandKind::ChangeTool {
                    name: name_now.clone(),
                    tool: state.tool_ui.to_tool(),
                });
            }
            if ui.button("Delete").clicked() {
                state.pending.push_back(CommandKind::DeleteTool(name_now.clone()));
            }
        });
    } else {
        let add = ui.add_enabled(block.is_empty(), egui::Button::new("Add"));
        if add.clicked() {
            state.pending.push_back(CommandKind::AddTool {
                name: name_now.trim().to_owned(),
                tool,
            });
        }
        if !block.is_empty() {
            ui.label(block);
        }
    }
    if ui.button("New").clicked() {
        let draft = state.new_tool_draft();
        state.ui_target = UiTarget::Tool(String::new());
        state.tool_ui = draft;
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

// ---------------------------------------------------------------- model page

fn model_page(ui: &mut Ui, state: &mut State, name: String) {
    ui.heading(format!("Model: {name}"));
    let Some(entity) = model::find_by_name(&state.world, &name) else {
        ui.label("It is gone from the document.");
        return;
    };
    let tris = model::facets_of(&state.world, &entity).map_or(0, |f| f.len());
    ui.label(format!("{tris} triangle(s)"));
    if let Some(a) = model::entity_world_aabb(&state.world, &entity) {
        ui.label(format!(
            "{:.1} x {:.1} x {:.1} mm",
            a.max.x - a.min.x,
            a.max.y - a.min.y,
            a.max.z - a.min.z
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


