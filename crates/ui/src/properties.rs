//! Properties pane: the parameter page of an operation, and the form that
//! creates one.
//!
//! Provenance: our own pane, specced from public CAM property-manager
//! pages; parameter names follow the published machining vocabulary
//! (stepover, stock to leave, climb/conventional, lead in/out, tabs).
//!
//! Like every panel this one never touches the world: it edits
//! `State::op_ui` and queues `CommandKind`s. The world's `Operation`
//! component is the truth, refetched into the draft by `sync_ops_ui`;
//! status (never/ok/STALE) and warnings are computed on read, so the page
//! cannot display a stale claim about a stale path.

use std::borrow::Cow;

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::geo2d::Point2;
use hsmworks_app::machining2d::{Lead, PocketStrategy};
use hsmworks_app::model;
use hsmworks_app::operation::{self, OpParams, OpStatus, SrcRef};

use crate::state::{CommandKind, ErrorScope, State};

/// Radio/combo order = `OpType` discriminants (Face, Contour, Pocket, Slot,
/// Trace); `kind_index` and `retype` must agree with this list.
const TYPES: [&str; 5] = ["Face", "2D Contour", "2D Pocket", "Slot", "Trace"];
const LEADS: [&str; 3] = ["None", "Linear", "Arc"];
const STRATEGIES: [&str; 2] = ["Zigzag", "Waterline"];

pub fn properties_panel(ui: &Ui, state: &mut State) {
    if !state.show_properties {
        return;
    }
    let Ok(key) = WindowKey::new("operations", "Operations") else {
        return;
    };
    // The `opened` flag flips false when the user closes the dock tab; the
    // CAM menu reopens the pane by setting `show_properties` again.
    let mut open = state.show_properties;
    ui.window(key.label("Operations")).opened(&mut open).build(|| {
        contents(ui, state);
    });
    state.show_properties = open;
}

fn contents(ui: &Ui, state: &mut State) {
    state.sync_ops_ui();
    let target = state.op_ui.name.clone();
    let live = model::find_by_name(&state.world, &target)
        .filter(|e| model::is_operation(&state.world, e))
        .and_then(|e| model::operation_of(&state.world, &e));
    let exists = live.is_some();

    if exists {
        ui.text(format!("Operation: {target}"));
    } else {
        // Typing the name is not an event; it rides the next Add command.
        let _ = ui.input_text("Name##op_name", &mut state.op_ui.name);
    }

    // Type: swapping the draft's variant, carrying over the shared numbers.
    let mut kind_idx = kind_index(state.op_ui.params.kind());
    if ui.combo("Type##op_kind", &mut kind_idx, TYPES.as_slice(), |t| {
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
    if ui.combo("Tool##op_tool", &mut t_idx, &tools, |t| Cow::Borrowed(t.as_str()))
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
        ui.text_wrapped("No tools in the document - add one in the Tool Library.");
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
    if ui.combo("Source##op_src", &mut s_idx, src_items.as_slice(), |s| {
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
            Some("No tool to cut with yet - add one in the Tool Library.")
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
            ui.input_double("Stepover (Y spacing)##f_so", stepover);
            ui.input_double("Clearance Z##f_clr", clearance_z);
            ui.input_double("Cut Z##f_cut", cut_z);
            ui.input_double("Feed (mm/min)##f_feed", feed);
            ui.input_double("Plunge feed (mm/min)##f_pl", plunge_feed);
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
            ui.input_double("Stock to leave##c_stl", stock_to_leave);
            ui.input_double("Clearance Z##c_clr", clearance_z);
            ui.input_double("Retract Z (between passes)##c_ret", retract_z);
            ui.input_double("Top Z##c_top", top_z);
            ui.input_double("Bottom Z##c_bot", bottom_z);
            ui.input_double("Stepdown##c_sd", stepdown);
            ui.input_double("Feed (mm/min)##c_feed", feed);
            ui.input_double("Plunge feed (mm/min)##c_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
            let mut lead_idx = match lead {
                Lead::None => 0,
                Lead::Linear { .. } => 1,
                Lead::Arc { .. } => 2,
            };
            if ui.combo("Lead in/out##c_lead", &mut lead_idx, LEADS.as_slice(), |t| {
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
                    ui.input_double("Lead length##c_lead_len", length);
                }
                Lead::Arc { radius } => {
                    ui.input_double("Lead arc radius##c_lead_r", radius);
                }
                Lead::None => {}
            }
            ui.input_double("Tolerance##c_tol", tolerance);
            let mut tabs = f64::from(*tab_count);
            ui.input_double("Tab count##c_tabs", &mut tabs);
            *tab_count = tabs.clamp(0.0, 99.0).round().max(0.0) as u32;
            ui.input_double("Tab width##c_tabw", tab_width);
            ui.input_double("Tab height##c_tabh", tab_height);
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
            ui.input_double("Stepover##p_so", stepover);
            ui.input_double("Stock to leave##p_stl", stock_to_leave);
            ui.input_double("Clearance Z##p_clr", clearance_z);
            ui.input_double("Top Z##p_top", top_z);
            ui.input_double("Bottom Z##p_bot", bottom_z);
            ui.input_double("Stepdown##p_sd", stepdown);
            ui.input_double("Feed (mm/min)##p_feed", feed);
            ui.input_double("Plunge feed (mm/min)##p_pl", plunge_feed);
            ui.checkbox("Climb milling", climb);
            let mut st_idx = match strategy {
                PocketStrategy::Zigzag => 0,
                PocketStrategy::Waterline => 1,
            };
            if ui.combo("Strategy##p_strat", &mut st_idx, STRATEGIES.as_slice(), |t| {
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
            ui.input_double("A X##s_ax", &mut a.x);
            ui.input_double("A Y##s_ay", &mut a.y);
            ui.input_double("B X##s_bx", &mut b.x);
            ui.input_double("B Y##s_by", &mut b.y);
            ui.input_double("Slot width##s_w", width);
            ui.input_double("Stock to leave##s_stl", stock_to_leave);
            ui.input_double("Clearance Z##s_clr", clearance_z);
            ui.input_double("Top Z##s_top", top_z);
            ui.input_double("Bottom Z##s_bot", bottom_z);
            ui.input_double("Stepdown##s_sd", stepdown);
            ui.input_double("Stepover##s_so", stepover);
            ui.input_double("Feed (mm/min)##s_feed", feed);
            ui.input_double("Plunge feed (mm/min)##s_pl", plunge_feed);
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
            ui.input_double("Clearance Z##t_clr", clearance_z);
            ui.input_double("Top Z##t_top", top_z);
            ui.input_double("Bottom Z##t_bot", bottom_z);
            ui.input_double("Stepdown##t_sd", stepdown);
            ui.input_double("Feed (mm/min)##t_feed", feed);
            ui.input_double("Plunge feed (mm/min)##t_pl", plunge_feed);
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
