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
//! links, feeds) stacked as one pane per kind (M9.1 B: the tab bar and its
//! transient open-tab state retired; each field keeps its within-section
//! order under its group header). Two
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
//! The kind is not a page field (M9.1): the operation page has no Type
//! combo - the button that made the operation decided what it is, and the
//! browser row's right-click is the only retype verb. Every surface that
//! retypes (the quick-adds, the menu's command) rides `crate::operations`'
//! one law, so they cannot disagree about what a retype preserves.
//!
//! Provenance: our own port, specced from imgui operations.rs/setup.rs
//! (our code) and the egui vendored sources; no HSMWorks material involved.

use egui::{
    Align, Align2, Color32, ComboBox, DragValue, FontId, Id, Label, Pos2, ScrollArea, Sense, Shape,
    Stroke, TextEdit, Ui, Vec2, Window,
};

use camcraft_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use camcraft_app::machine;
use camcraft_app::machining2d::{ARC_FIT_TOL, Lead, PocketStrategy, Side};
use camcraft_app::model;
use camcraft_app::operation::{
    self, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef, Tilt,
};
use camcraft_app::posting;
use camcraft_app::commands::theme_of;
use camcraft_app::schema::{Operation, PostRef, ThemeRef, WCS_SLOTS};
use camcraft_app::stock::StockShape;
use camcraft_app::tools;
use camcraft_app::units::{self, Unit};
use camcraft_app::verify::Warning;

use crate::actions;
use crate::interact;
use crate::operations::{
    cycle_index, cycle_names, fold_warnings, height_echo, offset_label, side_names, strategy_names,
    strategy3d_names, unresolved_note,
};
use crate::state::{
    CommandKind, ErrorScope, FixtureUi, GeoPick, ModelUi, PostView, State, UiTarget, quat_yaw_deg,
};

use super::panes::kind_name;
use super::theme::Tokens;

/// Tool kind labels, same order as `ToolKind` (mirrors the tool page).
const TOOL_KINDS: [&str; 11] = [
    "End mill",
    "Ball nose",
    "Bull nose",
    "Drill",
    "Tap",
    "Bore",
    "Turning insert",
    "Grooving blade",
    "Threading insert",
    "Form gauge",
    "Chamfer mill",
];
/// Lead in/out combo items - the whole `Lead` vocabulary, index order
/// matching the combo-arm mapping in the contour page.
const LEADS: [&str; 4] = ["None", "Linear", "Arc", "Tangential"];
/// Descent combo items - the whole `Entry3D` vocabulary, index order
/// matching the combo-arm mapping in the 3-D pocket's passes section.
const ENTRY_KINDS: [&str; 3] = ["Plunge", "Ramp", "Helix"];
/// The 3-D finish kinematics, in the order the combo maps them.
const FINISH_PATTERNS: [&str; 15] = [
    "Waterline",
    "Parallel",
    "Parallel - multi",
    "Scallop",
    "Radial",
    "Spiral",
    "Contour",
    "Project",
    "Steep and shallow",
    "Flat",
    "Pencil",
    "Corner",
    "Ramp",
    "Morphed spiral",
    "Geodesic",
];

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
pub(crate) const SECTIONS: [&str; 6] = ["Geometry", "Heights", "Passes", "Leads & Links", "Feeds", "Machine"];
const SEC_GEO: usize = 0;
const SEC_HEIGHTS: usize = 1;
const SEC_PASSES: usize = 2;
const SEC_LEADS: usize = 3;
const SEC_FEEDS: usize = 4;
const SEC_MACHINE: usize = 5;

/// Which sections an operation kind actually has fields (or the pick law)
/// for: Geometry/Heights/Feeds always; Passes except Trace (Trace has no
/// stepover or leave); Leads & Links only Contour - it is the kind with
/// lead moves and tabs; Machine only Part - the bar feeder's signals
/// are the one lifecycle group no cutting tab names (M5.2 G.2). The
/// page stacks exactly these groups: no dead tabs, and none to click
/// (M9.1 B).
pub(crate) fn has_section(kind: OpType, s: usize) -> bool {
    match s {
        SEC_GEO | SEC_HEIGHTS | SEC_FEEDS => true,
        // Trace cuts no passes; a drill's Passes are the cycle's own -
        // peck depth and bottom dwell, the numbers that shape its hops.
        // The engrave twin is Trace's shape verbatim, and the chamfer
        // ride's floor is DERIVED from width and angle - neither has a
        // stepover or a leave to show (M2.4 B).
        SEC_PASSES => !matches!(kind, OpType::Trace | OpType::Engrave | OpType::Chamfer2D),
        SEC_LEADS => matches!(kind, OpType::Contour),
        SEC_MACHINE => matches!(kind, OpType::Part),
        _ => false,
    }
}

// --------------------------------------------------------------- properties

/// The dock's properties pane: dispatch on the shell's ui target, exactly
/// like operations.rs's contents() dispatcher.
pub fn properties(ui: &mut Ui, state: &mut State, _pal: &Tokens) {
    let t = state.pane_target();
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match t {
            UiTarget::None => no_operation_page(ui, state),
            UiTarget::Operation(n) => operation_page(ui, state, n.as_str()),
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
            UiTarget::Model(n) => {
                state.sync_setup_ui();
                model_page(ui, state, n);
                setup_error(ui, state);
            }
            UiTarget::Setups => {
                state.sync_setup_ui();
                ui.heading("Setups");
                ui.separator();
                setups_page(ui, state);
                setup_error(ui, state);
            }
        });
}

fn setup_error(ui: &mut Ui, state: &State) {
    if let Some(msg) = state.error_for(ErrorScope::Setup).map(str::to_owned) {
        ui.colored_label(RED, msg);
    }
}

// ------------------------------------------------------------ operation page

/// The empty state of the properties pane when nothing is targeted and
/// nothing is selected (M9.1): the create-everything form is retired -
/// the kind comes from the button you pressed, so this page points at
/// the doors that press it. No dropdown stands between a click and a
/// kind's own pane.
fn no_operation_page(ui: &mut Ui, _state: &mut State) {
    ui.heading("Operations");
    ui.separator();
    ui.label("No operation to show.");
    ui.label("Click one in the browser, or pick a kind from the ribbon's family doors - the new operation opens its own page straight away.");
}

fn operation_page(ui: &mut Ui, state: &mut State, name: &str) {
    state.sync_ops_ui();
    ui.heading(format!("Operation: {name}"));
    let target = name.to_owned();
    let live: Option<Operation> = model::find_by_name(&state.world, &target)
        .as_ref()
        .filter(|e| model::is_operation(&state.world, e))
        .and_then(|e| model::operation_of(&state.world, e));
    let exists = live.is_some();
    // The heights' context is the operation's declared setup, never the
    // active cursor: what the page echoes is what that operation's
    // generate will see.
    let live_setup = live.as_ref().and_then(|o| o.setup.as_deref());
    ui.separator();

    // The kind is not a field: what you are machining is decided by the
    // button that made the operation (M9.1); changing it is the op's
    // context-menu verb, one command, never a dropdown in its own page.

    // Tool.
    let mut tools = state.tool_names();
    if !state.op_ui.tool.is_empty() && !tools.contains(&state.op_ui.tool) {
        tools.push(state.op_ui.tool.clone());
    } else if state.op_ui.tool.is_empty()
        && let Some(first) = tools.first()
    {
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

    // Setup membership (M3.2 A): which context this operation machines
    // in. Moving is never a stamp edit - SetOperationSetup keeps the
    // stamp and lets the resolved-Z digest say whether the op went Stale.
    ui.label("Setup");
    let setups = camcraft_app::setup::names(&state.world);
    if setups.is_empty() {
        ui.label("Global stock and offsets - this document has no setups yet.");
    } else {
        let cur = live
            .as_ref()
            .and_then(|o| o.setup.clone())
            .or_else(|| setups.first().cloned());
        let mut s_idx = setups
            .iter()
            .position(|n| Some(n) == cur.as_ref())
            .unwrap_or(0);
        let before = s_idx;
        combo_owned(ui, "op_setup", &mut s_idx, &setups);
        if s_idx != before {
            let n = setups[s_idx.min(setups.len() - 1)].clone();
            state.pending.push_back(CommandKind::SetOperationSetup {
                name: target.clone(),
                setup: Some(n),
            });
        }
    }

    // Status above the section stack (it sat below the Geometry block in
    // the M-c scroll, and above the tab bar after M1.4): a STALE line and
    // the warning fold ride the top of the pane, always in view.
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
                // M3.1 C: say which input moved, not just that one did -
                // the reason is a live question against the world.
                let reason = ent
                    .as_ref()
                    .and_then(|e| operation::stale_reason(&state.world, e))
                    .unwrap_or_else(|| "inputs moved since generation".to_owned());
                format!("Status: STALE - {reason} ({moves} move(s) cached)")
            }
        };
        ui.label(status_line);
        // The M1.5.4 fold, reused: one line per warning kind, not a wall.
        for line in fold_warnings(&warnings) {
            ui.label(line);
        }
    }
    ui.separator();

    // One pane per kind (M9.1 B): the sections no longer fold into tabs -
    // every group this kind has fields for stacks under its own header, in
    // SECTIONS order, fields within a group in the order the tabs already
    // showed them. A dead tab becomes impossible by construction (there is
    // no tab), and the operation's whole field set is visible in one
    // scroll. The open-tab field was view state (never a command, never
    // saved); it retires with the bar. params_fields keeps its per-section
    // guards and is simply asked once per visible group - the guards were
    // written so no field renders outside its own section's call, which is
    // what makes this loop exact.
    let kind = state.op_ui.params.kind();
    for (sec, name) in SECTIONS.iter().enumerate() {
        if !has_section(kind, sec) {
            continue;
        }
        if sec != SEC_GEO {
            ui.add_space(6.0);
        }
        ui.strong(*name);
        if sec == SEC_GEO {
            // The Geometry group leads with the viewport pick block.
            // Viewport pick mode - transient view state like the drafts
            // this page edits: the mode itself is not undoable; what a
            // Face/Edge click changes is (M-e: the radio row, because the
            // egui viewport honors geo_pick exactly like the imgui one did).
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
                // Hovering a row lights that feature in the viewport
                // (screen-space amber, the shared interact law) - so the
                // chip carries its GeoSel.
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
                live_setup,
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
        params_fields(ui, state, sec);
        ui.separator();
    }
    // The whole-panel unresolved line is operations.rs's law, same as the
    // retired imgui page's.
    if let Some(note) = unresolved_note(&state.world, live_setup, &state.op_ui.params) {
        ui.label(note);
    }
    ui.separator();

    // Footer. Creation is not a page event: the ribbon's button makes the
    // operation (kind-previewed name, one command + a Select); this page
    // edits the operation it was opened on.
    let unchanged = live
        .as_ref()
        .is_some_and(|o| o.params == state.op_ui.params);
    ui.horizontal(|ui| {
        let apply = ui.add_enabled(!unchanged, egui::Button::new("Apply Parameters"));
        if apply.clicked() {
            state.apply_operation_params(&target);
        }
        if ui.button("Generate").clicked() {
            state.generate_operation(&target);
        }
        if ui.button("Delete").clicked() {
            state
                .pending
                .push_back(CommandKind::DeleteOperation(target.clone()));
        }
    });
    if unchanged {
        ui.label("Apply is a no-op until a parameter differs from the operation.");
    }
    if let Some(msg) = state.error_for(ErrorScope::Operation).map(str::to_owned) {
        ui.colored_label(RED, msg);
    }
}

// --------------------------------------------------------------- param rows

/// The params block for all five kinds - operations.rs's fields, notes and
/// echoes, unchanged in content and in within-section order, placed by
/// section: each field renders when, and only when, its own section is the
/// one being asked (SECTIONS/has_section are the placement law; the guards
/// below read them as indices). The page walks the sections a kind has and
/// calls this once per group, so the guards' exclusivity is what makes a
/// field appear exactly once per pane (M9.1 B). Numbers are DragValue
/// instead of InputDouble (divergence D5, S4 report): egui has no
/// InputDouble; DragValue edits + commits with no custom state.
/// The roughing twins' shared pane (M2.4 A): the pocket's fields minus
/// the Strategy combo - the kind IS the strategy, so the choice that
/// would undo the name is not offered. `tag` scopes the height-editor
/// ids ("adaptive"/"offset"); the engagement door rides along as an
/// Option so the law "a section header renders exactly when its kind
/// has fields" stays one code path for both catalog names.
#[allow(clippy::too_many_arguments)]
fn roughing2d_pane(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    op_ctx: Option<&str>,
    u: Unit,
    tab: usize,
    tag: &str,
    stepover: &mut f64,
    stock_to_leave: &mut f64,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    stepdown: &mut f64,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    entry: &mut camcraft_app::machining3d::Entry3D,
    helix_radius: &mut f64,
    max_engagement: Option<&mut f64>,
    tilt: &mut Option<Tilt>,
) {
    if tab == SEC_HEIGHTS {
        orientation_editor(ui, u, tilt);
        field(ui, "Clearance Z");
        height_editor(ui, world, op_ctx, clearance_z, &format!("{tag}.clearance"));
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
            height_editor(ui, world, op_ctx, r, &format!("{tag}.retract"));
            if let (Some(rz), Some(cz)) = (r.z(world), clearance_z.z(world))
                && rz > cz
            {
                ui.label(format!(
                    "above clearance - the hop travels at {} (min of the two)",
                    units::len(cz, u, 3)
                ));
            }
        }
        field(ui, "Top Z");
        height_editor(ui, world, op_ctx, top_z, &format!("{tag}.top"));
        field(ui, "Bottom Z");
        height_editor(ui, world, op_ctx, bottom_z, &format!("{tag}.bottom"));
        field(ui, "Stepdown");
        num(ui, u, stepdown);
    }
    if tab == SEC_PASSES {
        field(ui, "Stepover");
        num(ui, u, stepover);
        field(ui, "Stock to leave");
        num(ui, u, stock_to_leave);
        match max_engagement {
            Some(m) => {
                field(ui, "Max engagement (deg)");
                num_plain(ui, m);
                ui.label("rings step by tool_r x (1 - cos a): 45 deg bites about 15% of the diameter");
            }
            None => {
                ui.label("concentric rings from the boundary inward, one stepover apart");
            }
        }
        field(ui, "Entry");
        let mut e_idx = match entry {
            camcraft_app::machining3d::Entry3D::Plunge => 0,
            camcraft_app::machining3d::Entry3D::Ramp => 1,
            camcraft_app::machining3d::Entry3D::Helix => 2,
        };
        let before = e_idx;
        combo(ui, &format!("{tag}_entry"), &mut e_idx, &ENTRY_KINDS);
        if e_idx != before {
            *entry = match e_idx {
                1 => camcraft_app::machining3d::Entry3D::Ramp,
                2 => camcraft_app::machining3d::Entry3D::Helix,
                _ => camcraft_app::machining3d::Entry3D::Plunge,
            };
        }
        if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
            field(ui, "Helix lead radius");
            num(ui, u, helix_radius);
            ui.label("a plunge stands in wherever a helix of this radius cannot fit");
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

fn params_fields(ui: &mut Ui, state: &mut State, tab: usize) {
    let u = state.units();
    let mut side_note: Option<String> = None;
    let mut auto_note: Option<String> = None;
    let mut cycle_note: Option<String> = None;
    let nm = state.op_ui.name.clone();
    // Height echoes resolve in the editing operation's declared setup
    // context (M3.2 A); an operation that does not exist yet - a draft
    // name - resolves `None`, the first setup, what CreateOperation will
    // declare for it if the cursor is untouched.
    let op_ctx: Option<String> = model::find_by_name(&state.world, &nm)
        .and_then(|e| model::operation_of(&state.world, &e))
        .and_then(|o| o.setup);
    if let OpParams::Drill {
        cycle: camcraft_app::drilling::CycleChoice::Auto,
        holes,
        cross,
        ..
    } = &state.op_ui.params
    {
        // What Auto answers for, in the tool's and first hole's own
        // numbers - the same call generation makes (`auto_kind`), never
        // a panel-side guess that could drift from it. A radial list
        // keeps its note to generation (M5.2 F2): the kernel's depth
        // starts at the R standoff, which the panel would only guess.
        let radial = !cross.is_empty();
        cycle_note = match (
            model::find_by_name(&state.world, &state.op_ui.tool)
                .as_ref()
                .and_then(|t| model::tool_of(&state.world, t)),
            if radial { None } else { holes.first() },
        ) {
            (Some(t), Some(h)) => {
                let depth = h.entry_z - h.floor_z;
                let pick = camcraft_app::drilling::auto_kind(t.kind, depth, t.diameter);
                let name = match pick {
                    camcraft_app::cl::CycleKind::Spot => "Spot",
                    camcraft_app::cl::CycleKind::Drill => "Drill",
                    camcraft_app::cl::CycleKind::Peck => "Peck",
                    camcraft_app::cl::CycleKind::Deep => "Deep (G83)",
                    camcraft_app::cl::CycleKind::Tap => "Tap",
                    camcraft_app::cl::CycleKind::Bore => "Bore",
                    camcraft_app::cl::CycleKind::BackBore => "Back bore",
                };
                Some(format!(
                    "Auto -> {name} for this tool and a depth of {}",
                    units::len(depth, units::of(&state.world), 3)
                ))
            }
            _ => Some("Auto picks the cycle from the tool and each hole's depth.".into()),
        };
    }
    let mut detect_holes = false;
    let op = model::find_by_name(&state.world, &nm)
        .as_ref()
        .and_then(|e| model::operation_of(&state.world, e));
    if let OpParams::Contour {
        side: Side::Auto, ..
    } = &state.op_ui.params
    {
        let resolved = op.as_ref().and_then(|o| {
            operation::contour_auto_side(
                &state.world,
                o.setup.as_deref(),
                &o.src,
                &state.op_ui.params.clone(),
            )
        });
        side_note = Some(match resolved {
            Some(true) => {
                "Auto -> Inside: the picked rings bound air, so the metal is outside them.".into()
            }
            Some(false) => "Auto -> Outside: the picked rings bound material.".into(),
            None => "Auto picks the side from the selection at generate.".into(),
        });
    }
    if let OpParams::Pocket {
        strategy: PocketStrategy::Auto,
        stepover,
        ..
    } = &state.op_ui.params
    {
        let pick = "Auto picks the shape from the pocket's outline at generate.".to_owned();
        auto_note = Some(match op.as_ref() {
            Some(o) => {
                let region = operation::region_of(&state.world, o).unwrap_or_default();
                let diameter = model::find_by_name(&state.world, &o.tool)
                    .as_ref()
                    .and_then(|t| model::tool_of(&state.world, t))
                    .map_or(6.0, |t| t.diameter);
                let outer: Vec<camcraft_app::geo2d::Point2> = region
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

    // Which Trace-shape name the arm below renders: the engrave twin
    // IS Trace's field shape (M2.4 B), so they share one arm and only
    // the height-editor ids switch. Read before the match's borrow.
    let engrave = matches!(&state.op_ui.params, OpParams::Engrave { .. });

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
            tilt,
        } => {
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "face.clearance",
                );
                field(ui, "Cut Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), cut_z, "face.cut");
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
                    height_editor(
                        ui,
                        &state.world,
                        op_ctx.as_deref(),
                        top_z.get_or_insert(seed),
                        "face.top",
                    );
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
            entry,
            helix_radius,
            tilt,
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
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "contour.clearance",
                );
                field(ui, "Retract Z (between passes)");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    retract_z,
                    "contour.retract",
                );
                if let (Some(rz), Some(cz)) =
                    (retract_z.z(&state.world), clearance_z.z(&state.world))
                    && rz > cz
                {
                    ui.label(format!(
                        "above clearance - the hop travels at {} (min of the two)",
                        units::len(cz, u, 3)
                    ));
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "contour.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "contour.bottom",
                );
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
                    Lead::Tangential { .. } => 3,
                };
                let before = l_idx;
                combo(ui, "c_lead", &mut l_idx, &LEADS);
                if l_idx != before {
                    *lead = match l_idx {
                        1 => Lead::Linear { length: 8.0 },
                        2 => Lead::Arc { radius: 5.0 },
                        3 => Lead::Tangential { radius: 5.0 },
                        _ => Lead::None,
                    };
                }
                match lead {
                    Lead::Linear { length } => {
                        field(ui, "Lead length");
                        num(ui, u, length);
                    }
                    Lead::Arc { radius } | Lead::Tangential { radius } => {
                        // One label for both arc radii: the widgets can
                        // never coexist (the draft holds one lead), and
                        // the number is the same thing - the arc's
                        // radius.
                        field(ui, "Lead arc radius");
                        num(ui, u, radius);
                    }
                    Lead::None => {}
                }
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "c_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                    ui.label("a plunge stands in wherever a helix of this radius cannot fit");
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
            entry,
            helix_radius,
            max_engagement,
            tilt,
        } => {
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "pocket.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "pocket.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "pocket.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "pocket.bottom",
                );
                field(ui, "Stepdown");
                num(ui, u, stepdown);
            }
            if tab == SEC_PASSES {
                field(ui, "Stepover");
                num(ui, u, stepover);
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                field(ui, "Strategy");
                // M5.1 H: the 2-D combo offers Adaptive beside the 3-D
                // one's - the law runs on the plane now, so hiding the
                // choice would make a file that carries it display a
                // strategy it is not cutting.
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                };
                let names = strategy_names();
                let before = st_idx;
                combo(ui, "p_strat", &mut st_idx, names);
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if matches!(strategy, PocketStrategy::Adaptive) {
                    field(ui, "Max engagement (deg)");
                    num_plain(ui, max_engagement);
                    ui.label("rings step by tool_r x (1 - cos a): 45 deg bites about 15% of the diameter");
                }
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "p_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                    ui.label("a plunge stands in wherever a helix of this radius cannot fit");
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
        // The roughing twins (M2.4 A): the pocket's pane with the
        // Strategy combo retired - the kind IS the strategy, so the
        // choice that would undo the name is not offered. Adaptive
        // shows the engagement door; Offset has none to show.
        OpParams::Adaptive2D {
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
            entry,
            helix_radius,
            max_engagement,
            tilt,
        } => roughing2d_pane(
            ui,
            &state.world,
            op_ctx.as_deref(),
            u,
            tab,
            "adaptive",
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
            entry,
            helix_radius,
            Some(max_engagement),
            tilt,
        ),
        OpParams::Offset2D {
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
            entry,
            helix_radius,
            tilt,
        } => roughing2d_pane(
            ui,
            &state.world,
            op_ctx.as_deref(),
            u,
            tab,
            "offset",
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
            entry,
            helix_radius,
            None,
            tilt,
        ),
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
            entry,
            helix_radius,
            tilt,
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
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "slot.clearance",
                );
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "slot.top");
                field(ui, "Bottom Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), bottom_z, "slot.bottom");
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
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "s_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                    ui.label("the channel's own width certifies the orbit - a full-width slot has no room and plunges instead");
                }
            }
        }
        // The engrave twin (M2.4 B): Trace's fields verbatim under the
        // catalog name - the arm is shared exactly because a second
        // pane body could only fork the equivalence.
        OpParams::Trace {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tilt,
        }
        | OpParams::Engrave {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tilt,
        } => {
            let tg = if engrave { "engrave" } else { "trace" };
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    &format!("{tg}.clearance"),
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "trace.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "trace.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "trace.bottom",
                );
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
        // The chamfer ride (M2.4 B): the edge is picked like Trace's
        // chains, the two authored numbers state the cone's line, and
        // the floor is DERIVED from them - so there is no bottom_z or
        // stepdown to edit (Passes is out by has_section) and the pane
        // reads the corner Z back so the derived number is visible,
        // never merely implied.
        OpParams::Chamfer2D {
            width,
            angle_deg,
            clearance_z,
            retract_z,
            top_z,
            feed,
            plunge_feed,
            climb,
            side,
            tolerance,
            tilt,
        } => {
            if tab == SEC_GEO {
                field(ui, "Chamfer width");
                num(ui, u, width);
                field(ui, "Chamfer angle (deg)");
                num_plain(ui, angle_deg);
                ui.label("the face's slope off the top plane; the depth follows as width x tan(angle) - authored here, never second-guessed by a depth field");
                field(ui, "Side");
                let mut s_idx = match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                };
                combo(ui, "ch_side", &mut s_idx, side_names());
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
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "chamfer.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "chamfer.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "chamfer.top");
                if let Some(tz) = top_z.z(&state.world) {
                    let corner = tz - *width * angle_deg.to_radians().tan();
                    ui.label(format!(
                        "the corner stands at {} - derived: width x tan(angle) below the top plane",
                        units::len(corner, u, 3)
                    ));
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
        // The circular ladder (M2.4 B): the pocket family about an
        // AUTHORED centre - geometry typed like Slot's endpoints, no
        // pick to disagree with the circles; heights and passes are the
        // pocket's shape, and `vault` stands the floor on the dome law
        // instead of at Bottom Z.
        OpParams::Circular {
            center,
            radius,
            vault,
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
            entry,
            helix_radius,
            tilt,
        } => {
            if tab == SEC_GEO {
                field(ui, "Center X");
                num(ui, u, &mut center.x);
                field(ui, "Center Y");
                num(ui, u, &mut center.y);
                field(ui, "Radius");
                num(ui, u, radius);
                ui.checkbox(vault, "Vault - dome floor");
                ui.label("flat floors stand at Bottom Z; the vault sinks each ring by (r/R)^2 of the Top-to-Bottom span - a parabolic reflector, every ring a circle the spindle simply holds");
            }
            if tab == SEC_HEIGHTS {
                orientation_editor(ui, u, tilt);
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "circ.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "circ.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "circ.top");
                field(ui, "Bottom Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), bottom_z, "circ.bottom");
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
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "circ_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                }
            }
        }
        OpParams::Drill {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            cycle,
            peck,
            dwell,
            feed,
            plunge_feed,
            holes,
            cross,
        } => {
            if tab == SEC_GEO {
                field(ui, "Holes");
                if holes.is_empty() && cross.is_empty() {
                    ui.label("none yet - Detect holes reads them from the model");
                }
                for (i, h) in holes.iter().enumerate() {
                    let floor = if h.through {
                        "through".to_owned()
                    } else {
                        format!("floor {}", units::len(h.floor_z, u, 3))
                    };
                    ui.label(format!(
                        "{}  X {}  Y {}  Ø{}  entry {}  {floor}",
                        i + 1,
                        units::len(h.xy.x, u, 3),
                        units::len(h.xy.y, u, 3),
                        units::len(2.0 * h.radius, u, 3),
                        units::len(h.entry_z, u, 3),
                    ));
                }
                // The radial half of the list (M5.2 F2): a cross hole
                // states its mouth in world XYZ, its plunge by the
                // station's axis, and its floor as a travel INWARD
                // from the mouth - the mouth is the surface it opens
                // on, not a Z plane.
                for (i, h) in cross.iter().enumerate() {
                    let station = if h.dir.x != 0.0 { "X" } else { "Y" };
                    let floor = if h.through {
                        "through".to_owned()
                    } else {
                        format!("depth {}", units::len(-h.floor_d, u, 3))
                    };
                    ui.label(format!(
                        "R{}  ({}, {}, {})  Ø{}  plunge {station}  {floor}",
                        holes.len() + i + 1,
                        units::len(h.origin.x, u, 3),
                        units::len(h.origin.y, u, 3),
                        units::len(h.origin.z, u, 3),
                        units::len(2.0 * h.radius, u, 3),
                    ));
                }
                // Edge-driven, one recognize pass per click (never per
                // paint frame): the list it finds replaces the draft's,
                // and the command lands so undo restores the old one.
                if ui.button("Detect holes").clicked() {
                    detect_holes = true;
                }
            }
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "drill.clearance",
                );
                field(ui, "R plane (feed starts here)");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "drill.top");
                field(ui, "Retract plane (between holes)");
                let mut hop = retract_z.is_some();
                if ui.checkbox(&mut hop, "Hop below clearance").changed() {
                    *retract_z = if hop {
                        Some(Height::of(top_z.reference, top_z.offset + 1.0))
                    } else {
                        None
                    };
                }
                if let Some(r) = retract_z {
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "drill.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Depth");
                let mut over = bottom_z.is_some();
                if ui.checkbox(&mut over, "Override the hole floors").changed() {
                    *bottom_z = if over {
                        Some(Height::of(top_z.reference, top_z.offset - 1.0))
                    } else {
                        None
                    };
                }
                match bottom_z {
                    Some(b) => {
                        height_editor(ui, &state.world, op_ctx.as_deref(), b, "drill.bottom")
                    }
                    None => {
                        ui.label("every hole stops at its own floor from the model");
                    }
                }
            }
            if tab == SEC_PASSES {
                field(ui, "Cycle");
                let mut c_idx = cycle_index(cycle);
                let before = c_idx;
                combo(ui, "d_cycle", &mut c_idx, cycle_names());
                if c_idx != before {
                    *cycle = match c_idx {
                        1 => camcraft_app::drilling::CycleChoice::Spot,
                        2 => camcraft_app::drilling::CycleChoice::Drill,
                        3 => camcraft_app::drilling::CycleChoice::Peck,
                        4 => camcraft_app::drilling::CycleChoice::Tap,
                        5 => camcraft_app::drilling::CycleChoice::Bore,
                        6 => camcraft_app::drilling::CycleChoice::BackBore,
                        _ => camcraft_app::drilling::CycleChoice::Auto,
                    };
                }
                if let Some(note) = &cycle_note {
                    ui.label(note.clone());
                }
                field(ui, "Peck depth");
                num(ui, u, peck);
                ui.label("0 lets the cycle size itself: 3 x dia chip-break, 1 x dia full-retract");
                field(ui, "Dwell at bottom (s)");
                num(ui, u, dwell);
                ui.label("seconds held at the bottom; honored on the cycles that dwell");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
            }
        }
        // The standalone boring ride (M2.4 C): the drill's hole-list
        // shape - the same Geometry list, the same R-plane/Depth
        // promise - but its Passes are the pocket family's (a leave,
        // a tolerance for the ring's chords) and its ladder is the
        // stepdown's, spoken on one circle per hole.
        OpParams::Bore {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            tolerance,
            holes,
        } => {
            if tab == SEC_GEO {
                field(ui, "Holes");
                if holes.is_empty() {
                    ui.label("none yet - Detect holes reads them from the model");
                }
                for (i, h) in holes.iter().enumerate() {
                    let floor = if h.through {
                        "through".to_owned()
                    } else {
                        format!("floor {}", units::len(h.floor_z, u, 3))
                    };
                    ui.label(format!(
                        "{}  X {}  Y {}  Ø{}  entry {}  {floor}",
                        i + 1,
                        units::len(h.xy.x, u, 3),
                        units::len(h.xy.y, u, 3),
                        units::len(2.0 * h.radius, u, 3),
                        units::len(h.entry_z, u, 3),
                    ));
                }
                if ui.button("Detect holes").clicked() {
                    detect_holes = true;
                }
                ui.label("a stepped hole is declined by name at generate - this ride wants one finished wall per hole");
            }
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "bore.clearance",
                );
                field(ui, "Retract plane (between holes)");
                let mut hop = retract_z.is_some();
                if ui.checkbox(&mut hop, "Hop below clearance").changed() {
                    *retract_z = if hop {
                        Some(Height::of(top_z.reference, top_z.offset + 1.0))
                    } else {
                        None
                    };
                }
                if let Some(r) = retract_z {
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "bore.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "R plane (feed starts here)");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "bore.top");
                field(ui, "Depth");
                let mut over = bottom_z.is_some();
                if ui.checkbox(&mut over, "Override the hole floors").changed() {
                    *bottom_z = if over {
                        Some(Height::of(top_z.reference, top_z.offset - 1.0))
                    } else {
                        None
                    };
                }
                match bottom_z {
                    Some(b) => {
                        height_editor(ui, &state.world, op_ctx.as_deref(), b, "bore.bottom")
                    }
                    None => {
                        ui.label("every hole stops at its own floor from the model");
                    }
                }
                field(ui, "Stepdown");
                num(ui, u, stepdown);
            }
            if tab == SEC_PASSES {
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the ring is chords to this tolerance - the same fit law the pocket walls keep");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // The helical thread mill (M2.4 C): the hole list again - the
        // ride is one helix per hole - and its Passes says the only
        // thing the drill's cycle had no word for: how deep each
        // ring's infeed goes. The pitch lives on the TOOL; the pane
        // says so in one line, because a silent number the tool owns
        // is how staleness hides (the stamp's law).
        OpParams::Thread {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            infeed,
            feed,
            plunge_feed,
            climb,
            holes,
        } => {
            if tab == SEC_GEO {
                field(ui, "Holes");
                if holes.is_empty() {
                    ui.label("none yet - Detect holes reads them from the model");
                }
                for (i, h) in holes.iter().enumerate() {
                    let floor = if h.through {
                        "through".to_owned()
                    } else {
                        format!("floor {}", units::len(h.floor_z, u, 3))
                    };
                    ui.label(format!(
                        "{}  X {}  Y {}  Ø{}  entry {}  {floor}",
                        i + 1,
                        units::len(h.xy.x, u, 3),
                        units::len(h.xy.y, u, 3),
                        units::len(2.0 * h.radius, u, 3),
                        units::len(h.entry_z, u, 3),
                    ));
                }
                if ui.button("Detect holes").clicked() {
                    detect_holes = true;
                }
            }
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "thread.clearance",
                );
                field(ui, "Retract plane (between holes)");
                let mut hop = retract_z.is_some();
                if ui.checkbox(&mut hop, "Hop below clearance").changed() {
                    *retract_z = if hop {
                        Some(Height::of(top_z.reference, top_z.offset + 1.0))
                    } else {
                        None
                    };
                }
                if let Some(r) = retract_z {
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "thread.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "R plane (feed starts here)");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "thread.top");
                field(ui, "Depth");
                let mut over = bottom_z.is_some();
                if ui.checkbox(&mut over, "Override the hole floors").changed() {
                    *bottom_z = if over {
                        Some(Height::of(top_z.reference, top_z.offset - 1.0))
                    } else {
                        None
                    };
                }
                match bottom_z {
                    Some(b) => {
                        height_editor(ui, &state.world, op_ctx.as_deref(), b, "thread.bottom")
                    }
                    None => {
                        ui.label("every hole stops at its own floor from the model");
                    }
                }
            }
            if tab == SEC_PASSES {
                field(ui, "Radial infeed per pass");
                num(ui, u, infeed);
                ui.label("rings from the tangent start to the finished wall; the pitch is the TOOL's number, not a parameter here");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        OpParams::Pocket3D {
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
            entry,
            helix_radius,
            rest_rough,
            max_engagement,
            feed_opt,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "pocket3d.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "pocket3d.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "pocket3d.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "pocket3d.bottom",
                );
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
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                };
                let names = strategy3d_names();
                let before = st_idx;
                combo(ui, "p3_strat", &mut st_idx, names);
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if matches!(strategy, PocketStrategy::Adaptive) {
                    field(ui, "Max engagement (deg)");
                    num_plain(ui, max_engagement);
                    ui.label("rings step by tool_r x (1 - cos a): 45 deg bites about 15% of the diameter");
                }
                ui.label("levels come from the mesh: each shelf cuts around the material standing above it");
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "p3_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                    ui.label("a plunge stands in wherever a helix of this radius cannot fit");
                }
                ui.checkbox(rest_rough, "Rest roughing");
                ui.label("a closing ladder, half the step, cuts what the levels leave standing");
                ui.checkbox(feed_opt, "Feed optimization");
                ui.label("heavier bites slow toward the programmed feed (never faster); the sim measures each move");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        OpParams::Finish3D {
            pattern,
            stepover,
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            ramp,
            min_step,
            ramp_angle,
            entry_ramp,
            steep_stepover,
            pencil_reference,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "finish3d.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "finish3d.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "finish3d.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "finish3d.bottom",
                );
                ui.label("Top is the first ring's plane, Bottom the last level's");
            }
            if tab == SEC_PASSES {
                field(ui, "Pattern");
                let mut pat_idx = match pattern {
                    camcraft_app::finishing3d::FinishPattern::Waterline => 0,
                    camcraft_app::finishing3d::FinishPattern::Parallel => 1,
                    camcraft_app::finishing3d::FinishPattern::ParallelMulti => 2,
                    camcraft_app::finishing3d::FinishPattern::Scallop => 3,
                    camcraft_app::finishing3d::FinishPattern::Radial => 4,
                    camcraft_app::finishing3d::FinishPattern::Spiral => 5,
                    camcraft_app::finishing3d::FinishPattern::Contour => 6,
                    camcraft_app::finishing3d::FinishPattern::Project => 7,
                    camcraft_app::finishing3d::FinishPattern::SteepShallow => 8,
                    camcraft_app::finishing3d::FinishPattern::Flat => 9,
                    camcraft_app::finishing3d::FinishPattern::Pencil => 10,
                    camcraft_app::finishing3d::FinishPattern::Corner => 11,
                    camcraft_app::finishing3d::FinishPattern::Ramp => 12,
                    camcraft_app::finishing3d::FinishPattern::MorphSpiral => 13,
                    camcraft_app::finishing3d::FinishPattern::Geodesic => 14,
                };
                let before = pat_idx;
                combo(ui, "f3_pattern", &mut pat_idx, &FINISH_PATTERNS);
                if pat_idx != before {
                    *pattern = match pat_idx {
                        1 => camcraft_app::finishing3d::FinishPattern::Parallel,
                        2 => camcraft_app::finishing3d::FinishPattern::ParallelMulti,
                        3 => camcraft_app::finishing3d::FinishPattern::Scallop,
                        4 => camcraft_app::finishing3d::FinishPattern::Radial,
                        5 => camcraft_app::finishing3d::FinishPattern::Spiral,
                        6 => camcraft_app::finishing3d::FinishPattern::Contour,
                        7 => camcraft_app::finishing3d::FinishPattern::Project,
                        8 => camcraft_app::finishing3d::FinishPattern::SteepShallow,
                        9 => camcraft_app::finishing3d::FinishPattern::Flat,
                        10 => camcraft_app::finishing3d::FinishPattern::Pencil,
                        11 => camcraft_app::finishing3d::FinishPattern::Corner,
                        12 => camcraft_app::finishing3d::FinishPattern::Ramp,
                        13 => camcraft_app::finishing3d::FinishPattern::MorphSpiral,
                        14 => camcraft_app::finishing3d::FinishPattern::Geodesic,
                        _ => camcraft_app::finishing3d::FinishPattern::Waterline,
                    };
                }
                ui.label(
                    "Waterline: tangent rings only. The others march the flat ground first - Parallel along the axes (Multi both, Scallop the diagonals), Radial as chords through the centre, Spiral as one in-winding curve, Contour as concentric loops of the model's shadow, Project as its lifted silhouette - then the rings take the flanks. Steep and shallow draws that split as a border: the rings keep their hands off shallow ground, and the flanks answer to their own spacing. Flat rides the floors only, one pass per shrink. Pencil and Corner read the bends instead of the slope: each rides the line of centres tangent to both faces of every concave corner, Corner with the op's own ball and Pencil with a fatter reference ball, cleaning only what that pen leaves standing. Ramp is Parallel with the sloped entries always on. The morphed spiral keeps one in-winding curve but spaces its turns by the ground's own cosine under them. Geodesic rings start at the model's open edge and march outward by distance ALONG the surface, so one loop can run floor, wall and rim together; a closed shell with no open edge is refused by name.",
                );
                if !matches!(pattern, camcraft_app::finishing3d::FinishPattern::Waterline) {
                    field(ui, "Stepover");
                    let mut over = stepover.is_some();
                    if ui.checkbox(&mut over, "Set line spacing").changed() {
                        *stepover = if over {
                            // The scallop law on the reference R3
                            // ball - sqrt(8 r tol) - as a starting
                            // figure; the field is then free.
                            Some((24.0 * tolerance.max(1e-9)).sqrt())
                        } else {
                            None
                        };
                    }
                    if let Some(s) = stepover {
                        num(ui, u, s);
                        ui.label("flat-ground spacing; on a slope it follows the terrain's cosine");
                    } else {
                        ui.label(
                            "from tolerance and tool by the scallop law, times the slope's cosine",
                        );
                    }
                    // M2.5 C honesty: Geodesic carries no chord march
                    // to floor, and rides neither chain-ramp nor
                    // entry-ramp law - every loop enters by its own
                    // certified plunge. Ramp IS the sloped laws,
                    // always on, so the checkboxes would only lie.
                    let geodesic_pat = matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::Geodesic
                    );
                    let ramp_pat = matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::Ramp
                    );
                    if !geodesic_pat {
                    field(ui, "Minimum step");
                    let mut floored = min_step.is_some();
                    if ui
                        .checkbox(&mut floored, "Floor the march's refinement")
                        .changed()
                    {
                        *min_step = if floored {
                            // A tenth of the march's own coarse step
                            // - the scallop law's 2·√(r·tol) on the
                            // reference R3 ball: the refinement
                            // stays alive, it just stops here.
                            Some(0.1 * (12.0 * tolerance.max(1e-9)).sqrt())
                        } else {
                            None
                        };
                    }
                    if let Some(s) = min_step {
                        num(ui, u, s);
                        ui.label("no cut segment shorter than this; run edges still land where the material says");
                    } else {
                        ui.label("the march's own resolution floor");
                    }
                    }
                    if ramp_pat {
                        ui.label(
                            "the diagonal chains between lines and the ramped ring entries are this pattern's whole story - always on, whatever the boxes below would say",
                        );
                    } else if !geodesic_pat {
                        ui.checkbox(ramp, "Ramp between lines");
                        ui.label(
                            "one diagonal feed chains each line's end to the next where the ground allows; where it doesn't, the hop stands",
                        );
                        if *ramp {
                            field(ui, "Ramp angle");
                            num_plain(ui, ramp_angle);
                            ui.label("the steepest descent a chain may carry; past it a link is a plunge in a diagonal coat, and the hop holds the line");
                        }
                        ui.checkbox(entry_ramp, "Ramp ring entries");
                        ui.label(
                            "on flanks steeper than the split, a ring's first contact slides in along its contact normal instead of dropping; where the normal cannot certify the way in, the plunge stands and the path says why",
                        );
                    } else {
                        ui.label(
                            "geodesic rings ride the surface, not the ladder: each run enters by its own certified plunge, and the sloped-entry laws keep their hands off",
                        );
                    }
                    if matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::SteepShallow
                    ) {
                        field(ui, "Steep stepover");
                        let mut authored = steep_stepover.is_some();
                        if ui.checkbox(&mut authored, "Space the rings").changed() {
                            // The scallop law's own figure on the
                            // reference R3 ball - 2 x sqrt(r tol) -
                            // as a starting value; the field is then
                            // free. The envelope verdict, not the
                            // number, is what keeps the promise.
                            *steep_stepover = if authored {
                                Some((12.0 * tolerance.max(1e-9)).sqrt())
                            } else {
                                None
                            };
                        }
                        if let Some(s) = steep_stepover {
                            num(ui, u, s);
                            ui.label("the flank's own level spacing - state it and the ladder obeys; the cusp it leaves is still measured, never assumed");
                        } else {
                            ui.label("from tolerance and tool by the scallop law, 2 x sqrt(r x tolerance)");
                        }
                    }
                    if matches!(
                        pattern,
                        camcraft_app::finishing3d::FinishPattern::Pencil
                    ) {
                        field(ui, "Reference ball");
                        let mut named = pencil_reference.is_some();
                        if ui.checkbox(&mut named, "Name a fatter pen").changed() {
                            // Four times the reference R3 ball's own
                            // diameter - a pen a band can hide under
                            // is no reference; the field is then free.
                            *pencil_reference = if named { Some(24.0) } else { None };
                        }
                        if let Some(d) = pencil_reference {
                            num(ui, u, d);
                            ui.label("the pen whose leftovers this op cleans - it must be fatter than the op's ball, and the path names it if it is not");
                        } else {
                            ui.label("no pen named: every concave corner whose locus rests on the solid is cleaned");
                        }
                    }
                }
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the finish budget: levels drop by 2 x sqrt(tool radius x tolerance), so the cusp under the ball stays within it");
                ui.label("stepdown follows from tolerance and tool - it is not a second knob");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // The 5-axis twins (M5.1 stage C). The heights, feeds and the
        // scallop law are the finish's words verbatim - the paths ARE
        // the finish's paths with a stamped pose. There is no Pattern
        // combo: the kind is the choice of kinematic (contour = the
        // ring ladder, parallel = the raster), and the new content is
        // the axis trio the `multiaxis` law reads.
        OpParams::Contour5D {
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "contour5d.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "contour5d.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "contour5d.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "contour5d.bottom",
                );
                ui.label("Top is the first ring's plane, Bottom the last level's");
            }
            if tab == SEC_PASSES {
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the finish budget: levels drop by 2 x sqrt(tool radius x tolerance), so the cusp under the ball stays within it");
                ui.label("the rings are the waterline's; the leaning spindle is what makes this a 5-axis op");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        OpParams::Parallel5D {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "parallel5d.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "parallel5d.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "parallel5d.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "parallel5d.bottom",
                );
                ui.label("Top is the first line's plane, Bottom the last one's");
            }
            if tab == SEC_PASSES {
                field(ui, "Stepover");
                let mut over = stepover.is_some();
                if ui.checkbox(&mut over, "Set line spacing").changed() {
                    *stepover = if over {
                        Some((24.0 * tolerance.max(1e-9)).sqrt())
                    } else {
                        None
                    };
                }
                if let Some(s) = stepover {
                    num(ui, u, s);
                    ui.label("flat-ground spacing; on a slope it follows the terrain's cosine");
                } else {
                    ui.label(
                        "from tolerance and tool by the scallop law, times the slope's cosine",
                    );
                }
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the finish budget: lines are spaced and levels drop from it, so the cusp under the ball stays within it");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // Flow (M5.1 I): the parallel's panel, field for field - the
        // direction the lines follow is read off the model, so the
        // only difference is the wording that says so.
        OpParams::Flow5D {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "flow5d.clearance",
                );
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
                    height_editor(ui, &state.world, op_ctx.as_deref(), r, "flow5d.retract");
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "flow5d.top");
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "flow5d.bottom",
                );
                ui.label("Top is the first line's plane, Bottom the last one's");
            }
            if tab == SEC_PASSES {
                field(ui, "Stepover");
                let mut over = stepover.is_some();
                if ui.checkbox(&mut over, "Set line spacing").changed() {
                    *stepover = if over {
                        Some((24.0 * tolerance.max(1e-9)).sqrt())
                    } else {
                        None
                    };
                }
                if let Some(s) = stepover {
                    num(ui, u, s);
                    ui.label("flat-ground spacing; on a slope it follows the terrain's cosine");
                } else {
                    ui.label(
                        "from tolerance and tool by the scallop law, times the slope's cosine",
                    );
                }
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the finish budget: lines are spaced and levels drop from it, so the cusp under the ball stays within it");
                ui.label("the lines follow the surface's own least-curving direction; flat ground is cut in straight passes");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // The drive-curve contour (M5.1 J): flow's panel minus the
        // stepover - the PASSES page has no pattern to author,
        // because the GEOMETRY page's picked chains ARE the path.
        OpParams::MultiAxisContour {
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "multi_axis_contour.clearance",
                );
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
                    height_editor(
                        ui,
                        &state.world,
                        op_ctx.as_deref(),
                        r,
                        "multi_axis_contour.retract",
                    );
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "multi_axis_contour.top",
                );
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "multi_axis_contour.bottom",
                );
                ui.label("the machining window: a picked chain standing outside it is refused by name, never clipped");
            }
            if tab == SEC_PASSES {
                ui.label("no pattern - the picked edge chains are the path, machined where they stand; see Geometry");
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the verify budget: it authors nothing here - the envelope scan measures the chain-riding path against it");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // The swarf family (M5.1 stage D): the raster's panel, because
        // the path is the raster run along the ruling. Two arms like
        // the twins' - the ladder's extra word has no name for the
        // plain kind to bind - and one shared body, `swarf_editor`.
        OpParams::Swarf {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        } => swarf_editor(
            ui,
            &state.world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            None,
        ),
        OpParams::AdvancedSwarf {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            passes,
        } => swarf_editor(
            ui,
            &state.world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            Some(passes),
        ),
        // The rotary twins (M5.1 stage E): the swarf family's panel
        // worded for the TURN - the axis is a reading of the model, so
        // the trio stands in for it where the ruling's name would
        // otherwise be. Two arms, one body: the contour kind has no
        // spacing field for the or-pattern to name, so the arms hand
        // it in as data, the twins' law.
        OpParams::RotaryContour {
            tolerance,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        } => rotary_editor(
            ui,
            &state.world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            None,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        ),
        OpParams::RotaryParallel {
            tolerance,
            stepover,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        } => rotary_editor(
            ui,
            &state.world,
            op_ctx.as_deref(),
            u,
            tab,
            tolerance,
            Some(stepover),
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        ),
        // The wrap ladder (M5.1 stage E2): the pocket's panel worded
        // for the TURN - stepdown infeeds radii, stepover spaces
        // arc-length around the bar - and the twins' pose trio. No
        // retract row anywhere: the kernel's rapids ride the skin by
        // law, and the panel says so where the field would stand.
        OpParams::RotaryOffset {
            tolerance,
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
            entry,
            helix_radius,
            max_engagement,
            lead_deg,
            lag_deg,
            max_tilt_deg,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "rotary_offset.clearance",
                );
                field(ui, "Top Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "rotary_offset.top",
                );
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "rotary_offset.bottom",
                );
                ui.label("the world planes map onto stations along the fitted axis - the ladder roughs the bar only between them (inert on a bar lying flat)");
                field(ui, "Stepdown (radial)");
                num(ui, u, stepdown);
                ui.label("the radial infeed: what one wrap level peels off the skin");
            }
            if tab == SEC_PASSES {
                field(ui, "Stepover");
                num(ui, u, stepover);
                ui.label("spacing around the bar, in developed arc length - the wrap's own ruler");
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                ui.label("the shelf the finishing kind is asked to take off");
                field(ui, "Strategy");
                let mut st_idx = match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                };
                let names = strategy3d_names();
                let before = st_idx;
                combo(ui, "ro_strat", &mut st_idx, names);
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if matches!(strategy, PocketStrategy::Adaptive) {
                    field(ui, "Max engagement (deg)");
                    num_plain(ui, max_engagement);
                    ui.label("rings step by tool_r x (1 - cos a): 45 deg bites about 15% of the diameter");
                }
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "ro_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                    ui.label("a plunge stands in wherever a helix of this radius cannot fit");
                }
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label(
                    "the verify budget: the swept envelope is judged against the model within it",
                );
                rotary_axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // The stamped ladder (M5.1 stage E3): the pocket's rows verbatim
        // - no rest-roughing or feed-optimization switches, because the
        // kernel runs neither over a stamp - plus the tolerance (verify
        // budget) and the cone the poses live inside.
        OpParams::MultiAxisRough {
            tolerance,
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
            entry,
            helix_radius,
            max_engagement,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "multiaxis_rough.clearance",
                );
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
                    height_editor(
                        ui,
                        &state.world,
                        op_ctx.as_deref(),
                        r,
                        "multiaxis_rough.retract",
                    );
                    if let (Some(rz), Some(cz)) = (r.z(&state.world), clearance_z.z(&state.world))
                        && rz > cz
                    {
                        ui.label(format!(
                            "above clearance - the hop travels at {} (min of the two)",
                            units::len(cz, u, 3)
                        ));
                    }
                }
                field(ui, "Top Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "multiaxis_rough.top",
                );
                field(ui, "Bottom Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "multiaxis_rough.bottom",
                );
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
                    PocketStrategy::Adaptive => 2,
                    PocketStrategy::Auto => 3,
                };
                let names = strategy3d_names();
                let before = st_idx;
                combo(ui, "ma_strat", &mut st_idx, names);
                if st_idx != before {
                    *strategy = match st_idx {
                        1 => PocketStrategy::Waterline,
                        2 => PocketStrategy::Adaptive,
                        3 => PocketStrategy::Auto,
                        _ => PocketStrategy::Zigzag,
                    };
                }
                if matches!(strategy, PocketStrategy::Adaptive) {
                    field(ui, "Max engagement (deg)");
                    num_plain(ui, max_engagement);
                    ui.label("rings step by tool_r x (1 - cos a): 45 deg bites about 15% of the diameter");
                }
                ui.label("levels come from the mesh: each shelf cuts around the material standing above it");
                field(ui, "Entry");
                let mut e_idx = match entry {
                    camcraft_app::machining3d::Entry3D::Plunge => 0,
                    camcraft_app::machining3d::Entry3D::Ramp => 1,
                    camcraft_app::machining3d::Entry3D::Helix => 2,
                };
                let before = e_idx;
                combo(ui, "ma_entry", &mut e_idx, &ENTRY_KINDS);
                if e_idx != before {
                    *entry = match e_idx {
                        1 => camcraft_app::machining3d::Entry3D::Ramp,
                        2 => camcraft_app::machining3d::Entry3D::Helix,
                        _ => camcraft_app::machining3d::Entry3D::Plunge,
                    };
                }
                if matches!(entry, camcraft_app::machining3d::Entry3D::Helix) {
                    field(ui, "Helix lead radius");
                    num(ui, u, helix_radius);
                    ui.label("a plunge stands in wherever a helix of this radius cannot fit");
                }
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label(
                    "the verify budget: the swept envelope is judged against the model within it",
                );
                ui.label("the ladder is move for move the 3-D pocket's; the spindle adds a pose where a wall or corner needs one, and stands vertical where a floor does not ask");
                axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
                shank_avoid_editor(ui, shank_avoid);
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.checkbox(climb, "Climb milling");
            }
        }
        // Turning face (M5.2 A): the ladder runs at axial stations, so
        // the heights trio is clearance / stock top / the plane to face
        // - the words match the generator's refusals, one truth. The
        // passes tab carries only the radial stepdown (the ridge law:
        // the kernel refuses past twice the insert nose) and the
        // tolerance; there is no stepover to show - the passes spiral
        // through the axis by construction - and no climb: a lathe cut
        // runs one way.
        OpParams::Facing {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "facing.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "facing.top");
                field(ui, "Facing Plane Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), bottom_z, "facing.face");
                ui.label("the bar faces the spindle: stock is read between the facing plane and the part top; the ladder steps down from the top, one stepdown per rung");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
                ui.label("axial infeed per rung - the kernel refuses past twice the insert nose radius, the ridge it would leave");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label(
                    "the verify budget: the swept envelope is judged against the model within it",
                );
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the descent between rungs travels in air at the plunge feed");
            }
        }
        // Profile turning (M5.2 B): the window trio names the bar
        // the rungs ride, not a plane they face toward; the passes
        // tab carries the RADIAL stepdown (the same ridge law read
        // as a shell width - twice the insert nose at the most) and
        // the leave the finish takes off. No stepover, no climb:
        // the ride IS the pattern and the work spins one way.
        OpParams::ProfileRough {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stock_to_leave,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "profile_rough.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "profile_rough.top",
                );
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "profile_rough.bottom",
                );
                ui.label("the rungs ride the bar between these heights; the window's ends are where the tool plunges in and slides out");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
                ui.label("radial shell one rung eats - the kernel refuses past twice the insert nose radius, the sheet the next rung in would miss");
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                ui.label(
                    "the wall the last rung stands off the skin - the finish pass takes it off",
                );
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the chord budget the silhouette and each rung's ride settle at, and the envelope the verdict measures - one number, both sides");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the radial lead onto each rung feeds at the plunge number; along the rung, at the feed");
            }
        }
        OpParams::ProfileFinish {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "profile_finish.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "profile_finish.top",
                );
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "profile_finish.bottom",
                );
                ui.label("one ride of the nose along the bar's offset, bottom of the window up to top - the whole finish is one pass");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the chord budget the ride settles at, and the envelope the verdict measures - the surface left stands within it");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the radial lead in and slide out feed at the plunge number; along the profile, at the feed");
            }
        }
        // Grooving rough (M5.2 C): the blade eats a BAND, so the
        // heights are the window's axial ends - between them the
        // kerfs tile one blade width apart, floor depth read off
        // the model's own silhouette. No stepover: the tile step IS
        // the width. No climb: the work spins one way.
        OpParams::GrooveRough {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stock_to_leave,
            feed: _,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "groove_rough.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "groove_rough.top",
                );
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "groove_rough.bottom",
                );
                ui.label("the band's axial ends: the kerfs tile between these heights, one blade width apart, and the window must be at least a blade wide");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
                ui.label("radial infeed per rung - each center's ladder steps from the window's shell down to the leave line under it");
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                ui.label("the skin the floor stands off - the finish groove takes it off");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the chord budget the band settles at, and the meridian the verdict measures the swept blade against");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the blade's only feed is radial, at this number; the step from center to center and every retract travel rapid at the bar's widest radius - the Feed the parameter carries is inert here, so the page says nothing with it (the tool page's law, M5.2 C)");
            }
        }
        // Grooving finish (M5.2 C): one full-depth plunge per center,
        // leave-free - the band walked once, at depth.
        OpParams::GrooveFinish {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed: _,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "groove_finish.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    top_z,
                    "groove_finish.top",
                );
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "groove_finish.bottom",
                );
                ui.label("one plunge per center between these heights, to the model's own floor - the whole finish is one walk of the band");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the chord budget the band settles at, and the meridian the verdict measures - the groove left stands within it");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("one full-depth plunge at this feed per center, to the model's floor - everything else on this path is a rapid");
            }
        }
        // Parting (M5.2 C): one center, the plane the part is cut
        // off at. The kerf's column comes from the model - the
        // ladder falls through the bar's full height and the last
        // rung stands ON the axis - so the page names just the
        // plane and the clearance; the Top of Part the parameter
        // carries shapes nothing here and stays unshown.
        OpParams::Part {
            tolerance,
            clearance_z,
            stepdown,
            feed: _,
            plunge_feed,
            bottom_z,
            top_z: _,
            bar_load,
            bar_advance,
            bar_unload,
            bar_transfer,
            inspect_tol,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "part.clearance",
                );
                field(ui, "Parting Plane Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), bottom_z, "part.plane");
                ui.label("where a part gets cut off from the bar it was turned on: the blade stands one center deep at this plane and its kerf falls through the model's whole column");
                field(ui, "Stepdown");
                num(ui, u, stepdown);
                ui.label("radial infeed per rung of the ladder down through the bar - the last rung severs, tip on the axis");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the chord budget the kerf band settles at; through-axis cuts are the cut, not a gouge - the verdict reads the separation");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the ladder down the column plunges at this feed; every retract off the kerf is a rapid back to the clear radius at the same station");
            }
            // The bar feeder's signals (M5.2 G.2): they shape no
            // toolpath and enter no sim - they are what the POSTED
            // program says around the cut. One program cuts one
            // part, so the honest frame is load, then cut, then
            // unload; the advance is the bar's next bite, measured
            // from wherever the stub left the axis.
            if tab == SEC_MACHINE {
                ui.checkbox(bar_load, "Load bar before the cut");
                if *bar_load {
                    field(ui, "Bar advance");
                    num(ui, u, bar_advance);
                    ui.label("how far the feeder pushes the bar past the previous part's stub - 0 says the act alone, no measured advance");
                }
                ui.checkbox(bar_unload, "Unload part after the cut");
                ui.label("the signals post as stream events around this section - a script voices them for the machine, M-code or comment, and silence posts silence");
                ui.checkbox(bar_transfer, "Transfer to sub-spindle");
                ui.label("the handoff step (M5.3 C): after the last cut, the part moves to the machine's second spindle - check the program gate, it names a machine that states no second spindle");
                field(ui, "Probe budget (mm)");
                num(ui, u, inspect_tol);
                ui.label("the measurement ask (M5.3 D): 0 asks no probe; above 0 the program posts its PROBE intent and the post measures the part as machined - every blade's cut composed into one - against this bar; what stands above the model past it holds the post and names the pass to re-run (0.1 um is the least any probe measures)");
            }
        }
        // Threading (M5.2 D): the wave the model already carries, cut
        // one turn per pitch. The insert's V is the tool's own two
        // numbers (width and included angle) and never appears here -
        // the page names the thread, the tool names the cutter.
        OpParams::Threading {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            pitch,
            depth,
            infeed,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "threading.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "threading.top");
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "threading.bottom",
                );
                ui.label("the helix runs between these heights, one turn per pitch - the whole thread walks past the bar's floor in the model");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("the chord budget the helix settles at, and the play the verdict measures between insert and modeled flank");
                field(ui, "Pitch");
                num(ui, u, pitch);
                ui.label("axis travel per turn - the thread's own number, and the lattice the passes walk along");
                field(ui, "Depth");
                num(ui, u, depth);
                ui.label("radial depth of the groove, crest to root - the deepest rung stands this far under the turned skin");
                field(ui, "Infeed");
                num(ui, u, infeed);
                ui.label("radial growth per pass of the growing wave; the last pass reaches full depth and the turns between them are the spring pass");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                ui.label("the synchronized helix cuts at this feed - the spindle turns once as the axis rises one pitch");
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the lead-in arc swings into the first turn at this feed; every pass after the first rides the ladder in rapids");
            }
        }
        // Chamfering (M5.2 E): the ride is the profile finish's, so
        // the page names the window, the promise and the feeds - and
        // the angle is a PROMISE, spent at the door: the model must
        // already carry this cone within the tolerance, or the
        // generation says so by name.
        OpParams::Chamfering {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            angle_deg,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "chamfer.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "chamfer.top");
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "chamfer.bottom",
                );
                ui.label("the two stations the chamfered corner spans - the ride walks the face between them and nowhere outside");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("how far the model may stand off the promised cone before the op refuses - and the chord budget the ride settles at");
                field(ui, "Angle from cylinder (deg)");
                num_plain(ui, angle_deg);
                ui.label("the slope the drawing states, measured off the cylinder - the MODEL owns which way the face falls, and its own line is what the tool rides");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                ui.label("the nose tracks the modeled face at this feed");
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the radial lead-in and slide-out at the window ends ride this feed");
            }
        }
        // Taper turning (M5.2 E): the diameters state the cone the
        // same way the angle does - a promise the model keeps or
        // the op names. Equal diameters are the straight turn.
        OpParams::TaperTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            bottom_dia,
            top_dia,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "taper.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "taper.top");
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "taper.bottom",
                );
                ui.label(
                    "the stations the taper runs between - the diameters stand at these planes",
                );
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("how far the model may stand off the authored cone before the op refuses - and the chord budget the ride settles at");
                field(ui, "Bottom Diameter");
                num(ui, u, bottom_dia);
                ui.label("the cone's size at the window's low station");
                field(ui, "Top Diameter");
                num(ui, u, top_dia);
                ui.label(
                    "and at its high one - equal diameters state the straight turn of a cylinder",
                );
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                ui.label("the nose tracks the modeled cone at this feed");
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the radial lead-in and slide-out at the window ends ride this feed");
            }
        }
        // Multi-pass turning (M5.2 E): the rough page and the finish
        // page's numbers in one program, plus the ride count - each
        // spring pass takes about half of what the last one
        // sprang, and the last of them lands ON the profile.
        OpParams::MultiPassTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stock_to_leave,
            passes,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "multipass.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "multipass.top");
                field(ui, "Bottom of Part Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    bottom_z,
                    "multipass.bottom",
                );
                ui.label("the length of bar the ladder eats and the rides finish, station window and all (M5.2 B's law, one program long)");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label(
                    "the chord budget every ride settles at, and the play the verdict measures",
                );
                field(ui, "Stepdown");
                num(ui, u, stepdown);
                ui.label("radial shell each rung eats - never more than twice the nose, the ladder's own door");
                field(ui, "Stock to leave");
                num(ui, u, stock_to_leave);
                ui.label(
                    "what the last rung stands off the skin for the finishing rides to share out",
                );
                field(ui, "Finishing passes");
                let mut n = f64::from(*passes);
                num_plain(ui, &mut n);
                *passes = n.clamp(1.0, 64.0).round().max(1.0) as u32;
                ui.label("spring passes after the rough - each removes about half of what the last sprang back; the last one is the profile finish itself");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                ui.label("rung and ride both track the offset at this feed");
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the radial plunge into each block's start, and the slide out of its end");
            }
        }
        // Form turning (M5.2 E2): the page names the window and the
        // budget and nothing else - there is no depth to author,
        // because the model states it, and no crown to author,
        // because the gauge is TOOL steel the tool page edits.
        OpParams::FormTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "form.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "form.top");
                field(ui, "Bottom of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), bottom_z, "form.bottom");
                ui.label("the stations the formed surfaces stand between - a valley outside the window is another op's promise");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("how far a floor may stand off the model's valley before the verdict speaks, and the lattice the door reads at");
                ui.label("the crown's arc and the band's width ride the mounted form gauge - the tool page states them");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                ui.label("the spindle's pace, stated for the sheet - a gauge reads no distance off the skin");
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the sink to each crown's depth is this kind's whole cut, and it rides this feed");
            }
        }
        // Turning boring (M5.2 F): the page names the window and the
        // budget and nothing else - the wall is in the model and the
        // size is TOOL steel, the ground nose the tool page edits.
        OpParams::BoringTurning {
            tolerance,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } => {
            if tab == SEC_HEIGHTS {
                field(ui, "Clearance Z");
                height_editor(
                    ui,
                    &state.world,
                    op_ctx.as_deref(),
                    clearance_z,
                    "bore.clearance",
                );
                field(ui, "Top of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), top_z, "bore.top");
                field(ui, "Bottom of Part Z");
                height_editor(ui, &state.world, op_ctx.as_deref(), bottom_z, "bore.bottom");
                ui.label("the length of wall the bar rides - the bar enters along the axis, so the window must stand inside one open hole");
            }
            if tab == SEC_PASSES {
                field(ui, "Tolerance");
                num(ui, u, tolerance);
                ui.label("how far the nose's ride may stand off the wall before the verdict speaks, and the chord budget the wall is sampled at");
                ui.label("the ground nose that holds the hole to size rides the mounted boring bar - the tool page states it");
            }
            if tab == SEC_FEEDS {
                field(ui, &format!("Feed ({})", u.feed_suffix()));
                num_feed(ui, u, feed);
                ui.label("the wall is walked tangentially, station by station, at this feed");
                field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
                num_feed(ui, u, plunge_feed);
                ui.label("the bar's lead-out: the radial move from the axis onto the wall, and the slide back off it");
            }
        }
    }
    if detect_holes {
        state.detect_holes(&nm);
    }
}

/// The axis trio both 5-axis kinds carry (M5.1 stage C). The law lives
/// in `camcraft_app::multiaxis::tilt`: lead leans along the feed, lag
/// leans across it, and the max tilt is the cone half-angle the
/// spindle may open from the surface normal - past it the commanded
/// direction folds onto the cone's edge. All three are degrees, so
/// they ride `num_plain`: the unit is a lens on lengths and feeds,
/// never on angles.
fn axis_editor(ui: &mut Ui, lead_deg: &mut f64, lag_deg: &mut f64, max_tilt_deg: &mut f64) {
    field(ui, "Tool lead angle (deg)");
    num_plain(ui, lead_deg);
    ui.label("lean with the feed, off the surface normal - tips the shank away from the wall ahead of the cut");
    field(ui, "Tool lag angle (deg)");
    num_plain(ui, lag_deg);
    ui.label("lean across the feed (side tilt) - clears the wall the tool has just cut");
    field(ui, "Max tilt (deg)");
    num_plain(ui, max_tilt_deg);
    ui.label("the cone the spindle may open from the normal; where the surface demands more, the lean folds onto the cone's edge");
}

/// Stage F's one switch (M5.1): where the posed axis would drive the
/// shank into the part, the spindle tilts to the nearest clear angle
/// inside the max-tilt cone - the shortest travel from the pose it
/// held, so neighboring cuts do not swing the head back and forth.
/// The ball's envelope is rotation-invariant, so this can only ever
/// change which way the spindle stands, never where the cut lands;
/// and where no angle inside the cone clears, the operation refuses
/// by name rather than post a path the verifier would warn on.
fn shank_avoid_editor(ui: &mut Ui, shank_avoid: &mut bool) {
    field(ui, "Shank avoidance");
    ui.checkbox(shank_avoid, "Avoid shank collisions");
    ui.label("tilt the spindle off a leaning pose that would rub the shank; positions never move, and a wall no tilt clears refuses the job");
}

/// The swarf family's panel (M5.1 stage D): the raster's sections
/// worded for the ruling, in one body both kinds share. `passes` is
/// Some only for the ladder - the or-pattern cannot name a field one
/// variant lacks, so the arms hand their extra word in as data.
#[allow(clippy::too_many_arguments)] // flat panel args, same shape as `cell`
fn swarf_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    u: Unit,
    tab: usize,
    tolerance: &mut f64,
    stepover: &mut Option<f64>,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    lead_deg: &mut f64,
    lag_deg: &mut f64,
    max_tilt_deg: &mut f64,
    passes: Option<&mut u32>,
) {
    if tab == SEC_HEIGHTS {
        field(ui, "Clearance Z");
        height_editor(ui, world, setup, clearance_z, "swarf.clearance");
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
            height_editor(ui, world, setup, r, "swarf.retract");
            if let (Some(rz), Some(cz)) = (r.z(world), clearance_z.z(world))
                && rz > cz
            {
                ui.label(format!(
                    "above clearance - the hop travels at {} (min of the two)",
                    units::len(cz, u, 3)
                ));
            }
        }
        field(ui, "Top Z");
        height_editor(ui, world, setup, top_z, "swarf.top");
        field(ui, "Bottom Z");
        height_editor(ui, world, setup, bottom_z, "swarf.bottom");
        ui.label("Top is the first line's plane, Bottom the last one's");
    }
    if tab == SEC_PASSES {
        field(ui, "Stepover");
        let mut over = stepover.is_some();
        if ui.checkbox(&mut over, "Set line spacing").changed() {
            *stepover = if over {
                Some((24.0 * tolerance.max(1e-9)).sqrt())
            } else {
                None
            };
        }
        if let Some(s) = stepover {
            num(ui, u, s);
            ui.label("spacing ACROSS the ruling; along it the lines never part - the surface's own direction spaces them");
        } else {
            ui.label("from tolerance and tool by the scallop law, across the ruling");
        }
        // The ladder's rung count, the rung-count law: a plain number
        // clamped into the generator's named range at the draft, so
        // no panel edit ever holds a 0 or a 9. Named "Pass count" - in
        // the stacked pane a field reading exactly "Passes" would echo
        // its own group header (M9.1 B named this by pinning headers).
        if let Some(passes) = passes {
            field(ui, "Pass count");
            let mut n = f64::from(*passes);
            num_plain(ui, &mut n);
            *passes = n.clamp(1.0, 4.0).round() as u32;
            ui.label("coarse-to-fine runs: each rough pass relaxes the tolerance fourfold (and doubles the spacing), the last cuts to the value set below");
        }
        axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        field(ui, "Tolerance");
        num(ui, u, tolerance);
        ui.label("the finish budget: lines are spaced from it, so the cusp along the flank stays within it");
    }
    if tab == SEC_FEEDS {
        field(ui, &format!("Feed ({})", u.feed_suffix()));
        num_feed(ui, u, feed);
        field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
        num_feed(ui, u, plunge_feed);
        ui.checkbox(climb, "Climb milling");
    }
}

/// The rotary twins' panel (M5.1 stage E): the swarf body worded for
/// the turn. `stepover` is Some only for the parallel kind - the
/// contour's stations drop by the scallop law alone, and an or-pattern
/// cannot name a field one variant lacks, so the arms hand their
/// spacing in as data.
#[allow(clippy::too_many_arguments)] // flat panel args, same shape as `swarf_editor`
fn rotary_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    u: Unit,
    tab: usize,
    tolerance: &mut f64,
    stepover: Option<&mut Option<f64>>,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    lead_deg: &mut f64,
    lag_deg: &mut f64,
    max_tilt_deg: &mut f64,
) {
    if tab == SEC_HEIGHTS {
        field(ui, "Clearance Z");
        height_editor(ui, world, setup, clearance_z, "rotary.clearance");
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
            height_editor(ui, world, setup, r, "rotary.retract");
            if let (Some(rz), Some(cz)) = (r.z(world), clearance_z.z(world))
                && rz > cz
            {
                ui.label(format!(
                    "above clearance - the hop travels at {} (min of the two)",
                    units::len(cz, u, 3)
                ));
            }
        }
        field(ui, "Top Z");
        height_editor(ui, world, setup, top_z, "rotary.top");
        field(ui, "Bottom Z");
        height_editor(ui, world, setup, bottom_z, "rotary.bottom");
        ui.label("the world planes map onto stations along the fitted axis - Top where the run starts, Bottom where it ends");
    }
    if tab == SEC_PASSES {
        if let Some(stepover) = stepover {
            field(ui, "Stepover");
            let mut over = stepover.is_some();
            if ui.checkbox(&mut over, "Set line spacing").changed() {
                *stepover = if over {
                    Some((24.0 * tolerance.max(1e-9)).sqrt())
                } else {
                    None
                };
            }
            if let Some(s) = stepover {
                num(ui, u, s);
                ui.label("spacing AROUND the turn; along the axis the lines never part - the cylinder's own direction spaces them");
            } else {
                ui.label("from tolerance and tool by the scallop law, around the turn");
            }
        }
        rotary_axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        field(ui, "Tolerance");
        num(ui, u, tolerance);
        ui.label("the finish budget: stations and lines follow from it, so the cusp under the ball stays within it");
    }
    if tab == SEC_FEEDS {
        field(ui, &format!("Feed ({})", u.feed_suffix()));
        num_feed(ui, u, feed);
        field(ui, &format!("Plunge feed ({})", u.feed_suffix()));
        num_feed(ui, u, plunge_feed);
        ui.checkbox(climb, "Climb milling");
    }
}

/// The axis trio, worded for the rotary twins: the leans pose off the
/// RADIAL, not the surface normal - the wall of a turn answers for the
/// whole ring at once, and the law lives in
/// `camcraft_motion::multiaxis::rotary` (M5.1 stage E).
fn rotary_axis_editor(ui: &mut Ui, lead_deg: &mut f64, lag_deg: &mut f64, max_tilt_deg: &mut f64) {
    field(ui, "Tool lead angle (deg)");
    num_plain(ui, lead_deg);
    ui.label("lean with the feed, off the radial from the fitted axis - tips the shank away from the wall ahead of the cut");
    field(ui, "Tool lag angle (deg)");
    num_plain(ui, lag_deg);
    ui.label(
        "lean across the feed (side tilt), off the radial - clears the wall the tool has just cut",
    );
    field(ui, "Max tilt (deg)");
    num_plain(ui, max_tilt_deg);
    ui.label("the cone the spindle may open from the radial; where the stance demands more, the lean folds onto the cone's edge");
}

pub(crate) fn field(ui: &mut Ui, label: &str) {
    ui.add_space(3.0);
    ui.label(label);
}

pub(crate) fn num(ui: &mut Ui, u: Unit, v: &mut f64) {
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
pub(crate) fn num_feed(ui: &mut Ui, u: Unit, v: &mut f64) {
    num_prec(ui, u, v, 2);
}

/// Degrees, rpm, counts: no lens - these were never millimeters.
pub(crate) fn num_plain(ui: &mut Ui, v: &mut f64) {
    ui.add(DragValue::new(v).speed(0.1));
}

/// The 3+2 index editor (M5.1 stage B): the checkbox arms or disarms
/// a tilt, and the fields edit the two RS-274 letters (degrees - the
/// A/B of [`camcraft_app::cl`]'s pose convention, C left to the table)
/// and the plane anchor (mm). It writes the draft, like every other
/// field here, so the existing EditOperationParams commit path makes
/// the edit undoable. The panel never normalizes and never refuses:
/// the door in [`Tilt::is_sane`] owns the bounds, and the mounted
/// machine's reach is generation's question, answered by name there.
fn orientation_editor(ui: &mut Ui, u: Unit, tilt: &mut Option<Tilt>) {
    let mut armed = tilt.is_some();
    if ui.checkbox(&mut armed, "Tilted plane (3+2)").changed() {
        *tilt = if armed {
            Some(Tilt {
                a_deg: 0.0,
                b_deg: 0.0,
                origin: [0.0; 3],
            })
        } else {
            None
        };
    }
    if let Some(t) = tilt.as_mut() {
        field(ui, "Tool axis A (deg)");
        num_plain(ui, &mut t.a_deg);
        field(ui, "Tool axis B (deg)");
        num_plain(ui, &mut t.b_deg);
        field(ui, "Plane origin X");
        num(ui, u, &mut t.origin[0]);
        field(ui, "Plane origin Y");
        num(ui, u, &mut t.origin[1]);
        field(ui, "Plane origin Z");
        num(ui, u, &mut t.origin[2]);
    }
}

/// Combo over `&[&str]` with a usize model (imgui's combo idiom).
pub(crate) fn combo(ui: &mut Ui, id: &str, idx: &mut usize, names: &[&str]) {
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
fn height_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    h: &mut Height,
    tag: &str,
) {
    ui.push_id(tag, |ui| {
        height_editor_inner(ui, world, setup, h);
    });
}

fn height_editor_inner(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    h: &mut Height,
) {
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
        ui.add(
            Label::new(egui::RichText::new(height_echo(world, setup, h)).weak()).selectable(false),
        );
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
            if ui
                .radio_value(&mut state.stock_ui.shape, i as i32, *name)
                .changed()
            {}
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
    // The stock pane edits the ACTIVE setup's stock entity (M3.2 A):
    // with no setups in the document this resolves to the old global
    // "Stock", so the legacy behavior is the degenerate case, not a
    // branch. sync_setup_ui already mirrored this entity's numbers.
    let active = camcraft_app::setup::active(&state.world);
    let stock_name = camcraft_app::setup::stock_entity_name(&state.world, active.as_deref());
    let has_stock = model::find_by_name(&state.world, &stock_name)
        .is_some_and(|e| model::is_stock(&state.world, &e));
    if let Some(s) = active.as_deref() {
        ui.label(format!("Stock of setup {s}."));
    }
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            let s = if shape == 1 {
                StockShape::Cylinder { diameter, height }
            } else if shape == 2 {
                StockShape::FromModel {
                    source: source.clone(),
                }
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
            state
                .pending
                .push_back(CommandKind::StockFit { margin, shape });
        }
        if has_stock && ui.button("Delete").clicked() {
            state
                .pending
                .push_back(CommandKind::DeleteByName(stock_name.clone()));
        }
    });
    ui.add_space(10.0);
    ui.separator();
    machine_section(ui, state);
    ui.add_space(10.0);
    ui.separator();
    post_section(ui, state);
    ui.add_space(10.0);
    ui.separator();
    theme_section(ui, state);
}

/// The document's theme (M6.1 C): setup furniture, read live off the
/// world singleton like the machine and post sections beside it - no
/// mirror to go stale, the label is exactly the reference the command
/// wrote. The combo offers the two built-ins and the discovered
/// `assets/themes/*.json` files BY NAME (the `post_registry` law: an
/// index into a file list would silently re-point the document when a
/// file arrives), and "Load theme" reaches any `.json` through the
/// file-pick law. The look itself lands on the next frame edge, in
/// `State::sync_theme` - the click writes the reference, never a
/// table - so undo puts the previous theme back exactly.
fn theme_section(ui: &mut Ui, state: &mut State) {
    let current = theme_of(&state.world);
    ui.label("Theme");
    ui.label(crate::state::theme_label(&state.world));
    ui.horizontal(|ui| {
        let mut pick: Option<CommandKind> = None;
        egui::ComboBox::from_id_salt("theme_switcher")
            .selected_text(state.theme_selection_text())
            .show_ui(ui, |ui| {
                for (label, cmd) in state.theme_options() {
                    let on = match (&cmd, &current) {
                        (CommandKind::SelectTheme(ThemeRef::Dark), ThemeRef::Dark) => true,
                        (
                            CommandKind::SelectTheme(ThemeRef::Named(n)),
                            ThemeRef::Named(m),
                        ) => n == m,
                        (
                            CommandKind::SelectTheme(ThemeRef::File(p)),
                            ThemeRef::File(q),
                        ) => p == q,
                        _ => false,
                    };
                    if ui.selectable_label(on, &label).clicked() {
                        pick = Some(cmd);
                    }
                }
            });
        if let Some(cmd) = pick {
            state.pending.push_back(cmd);
        }
        if matches!(current, ThemeRef::File(_)) && ui.button("Built-in").clicked() {
            state
                .pending
                .push_back(CommandKind::SelectTheme(ThemeRef::Dark));
        }
    });
}

/// The document's post script (M3.3 A/B): setup furniture, read live
/// off the world singleton like the machine section above. The label
/// is exactly what the post resolves - one truth, so a pane can never
/// advertise a script the gate did not run. The combo offers the five
/// shipped posts BY NAME (an index would silently re-point the
/// document when the registry grows or reorders); loading a `.cps` of
/// your own and returning to the default are commands too - undo puts
/// the previous post back.
fn post_section(ui: &mut Ui, state: &mut State) {
    let current = posting::post_of(&state.world);
    ui.label("Post");
    ui.label(posting::post_label(&state.world));
    ui.horizontal(|ui| {
        let names: Vec<&str> = posting::BUILTINS.iter().map(|(n, _)| *n).collect();
        let sel: Option<usize> = match &current {
            PostRef::Builtin => Some(0),
            PostRef::Named(n) => names.iter().position(|m| *m == n.as_str()),
            PostRef::File(_) => None,
        };
        let text = match sel {
            Some(i) => names[i].to_owned(),
            None => "custom".to_owned(),
        };
        let mut pick: Option<String> = None;
        egui::ComboBox::from_id_salt("post_registry")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(sel == Some(i), *n).clicked() {
                        pick = Some((*n).to_owned());
                    }
                }
            });
        if let Some(name) = pick {
            state.pending.push_back(CommandKind::SelectPostNamed(name));
        }
        if ui.button("Load post (.cps)").clicked() {
            state.pending.push_back(CommandKind::SelectPostDialog);
        }
        if matches!(current, PostRef::File(_)) && ui.button("Built-in").clicked() {
            state.pending.push_back(CommandKind::SelectPostBuiltin);
        }
    });
}

/// The document's machine (M2.3 C, group completed by M3.3 stage C):
/// setup furniture read live off the world singleton - no mirror to go
/// stale. The summary is exactly what the file stated; a blank is a
/// `?`, never an invented ceiling. The library combo picks a shipped
/// machine BY NAME, "Load other" reaches any `.mch` through the file
/// law, and "Edit" opens the draft editor; all three land on the
/// same `LoadMachine` command, so every change here is undoable
/// furniture, not a field poke. The kind line follows the DECLARED
/// kind where the machine states one, the axis inventory otherwise.
fn machine_section(ui: &mut Ui, state: &mut State) {
    let (path, m) = machine::of(&state.world);
    ui.label("Machine");
    ui.horizontal(|ui| {
        ui.label("Library");
        let names: Vec<&str> = posting::MACHINES.iter().map(|(n, _)| *n).collect();
        let sel: Option<usize> = names.iter().position(|n| path == posting::builtin_path(n));
        let text = match sel {
            Some(i) => names[i].to_owned(),
            None => "other".to_owned(),
        };
        let mut pick: Option<String> = None;
        egui::ComboBox::from_id_salt("machine_library")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(sel == Some(i), *n).clicked() {
                        pick = Some((*n).to_owned());
                    }
                }
            });
        if let Some(name) = pick {
            state
                .pending
                .push_back(CommandKind::LoadBuiltinMachine(name));
        }
    });
    match &m {
        Some(m) => {
            ui.label(m.summary());
            ui.label(format!("{} workspace", machine::kind(Some(m)).label()));
            if !path.is_empty() {
                ui.label(format!("loaded from {path}"));
            }
            ui.horizontal(|ui| {
                if ui.button("Load other").clicked() {
                    state.pending.push_back(CommandKind::LoadMachineDialog);
                }
                if ui.button("Edit").clicked() {
                    state.pending.push_back(CommandKind::EditMachineDialog);
                }
                if ui.button("Clear").clicked() {
                    state.pending.push_back(CommandKind::ClearMachine);
                }
            });
        }
        None => {
            ui.label("No machine loaded - the checker states, it cannot judge.");
            if ui.button("Load machine (.mch)").clicked() {
                state.pending.push_back(CommandKind::LoadMachineDialog);
            }
        }
    }
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
    // Box-point anchoring (feedback 5, revised): the reference box is an
    // explicit choice - stock or model - and the nine points of the chosen
    // level can be clicked in the viewport as markers instead of hunting
    // through the button grid. The buttons and the markers share
    // `interact::anchor_points`, so what you click, what highlights, and
    // what places are the same points by construction. This section is the
    // imgui pane's origin picker (setup.rs, de272ca/067f3aa), restored:
    // `interact.rs` kept the markers and the viewport pick law through the
    // M-g retirement, but the widgets here died un-ported, leaving the pick
    // mode unreachable (the crosses only render while `pick_anchor` is
    // set). Behavior specced from published CAM setup practice; the grid
    // and its math are ours.
    const SOURCES: [&str; 2] = ["Stock box", "Model bounds"];
    field(ui, "Anchor on");
    ComboBox::from_id_salt("wcs_src")
        .selected_text(SOURCES[state.wcs_ui.anchor_source.min(1)])
        .show_ui(ui, |ui| {
            for (i, s) in SOURCES.iter().enumerate() {
                ui.selectable_value(&mut state.wcs_ui.anchor_source, i, *s);
            }
        });
    match interact::anchor_reference(state) {
        Some((_, what)) => {
            let chosen = SOURCES[state.wcs_ui.anchor_source.min(1)];
            if !chosen.eq_ignore_ascii_case(what) {
                ui.label(format!("No {chosen} defined - anchoring on {what}."));
            }
            ui.horizontal(|ui| {
                for (i, name) in ["Bottom", "Middle", "Top"].iter().enumerate() {
                    ui.radio_value(&mut state.wcs_ui.anchor_z, i, *name);
                }
            });
            ui.checkbox(&mut state.wcs_ui.pick_anchor, "Pick markers in viewport");
            if state.wcs_ui.pick_anchor {
                ui.label("Click a yellow marker in the viewport to set the origin.");
            }
            let pts = interact::anchor_points(state);
            for row in 0..3usize {
                ui.horizontal(|ui| {
                    for col in 0..3usize {
                        let k = row * 3 + col;
                        let code = pts.get(k).map_or("?", |(_, name)| *name);
                        let label = if state.wcs_ui.anchor_xy == k {
                            format!("[{code}]")
                        } else {
                            code.to_owned()
                        };
                        if ui.button(label).clicked()
                            && let Some((pos, _)) = pts.get(k)
                        {
                            state.wcs_ui.anchor_xy = k;
                            state.wcs_ui.x = pos[0];
                            state.wcs_ui.y = pos[1];
                            state.wcs_ui.z = pos[2];
                            apply_wcs_drafts(state);
                        }
                    }
                });
            }
        }
        None => {
            ui.label("Import a model or set stock to anchor the origin on a point.");
        }
    }

    ui.add_space(6.0);
    // The slot WORDS are always the RS-274 literals (law f: a posted G55
    // is a G55); only the entity behind each follows the setup.
    let wcs_ctx = camcraft_app::setup::active(&state.world);
    ui.label(match wcs_ctx.as_deref() {
        Some(s) => format!("Placed offsets in setup {s}:"),
        None => "Placed offsets:".to_owned(),
    });
    for (i, name) in WCS_SLOTS.iter().enumerate() {
        let line = match camcraft_app::setup::wcs_entity(&state.world, wcs_ctx.as_deref(), i)
            .and_then(|e| model::transform_of(&state.world, &e))
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

/// Queue the Model page's placement draft as a `MoveModel` (M5.2 G) -
/// the `apply_wcs_drafts` shape, one command kind over.
fn apply_model_draft(state: &mut State) {
    state.pending.push_back(CommandKind::MoveModel {
        name: state.model_ui.name.clone(),
        pos: [state.model_ui.x, state.model_ui.y, state.model_ui.z],
        roll_deg: state.model_ui.roll_deg,
        pitch_deg: state.model_ui.pitch_deg,
        yaw_deg: state.model_ui.yaw_deg,
    });
}

/// Quarter-turn the drafted part about a WORLD axis and write: the
/// `turn_wcs` law (the turn premultiplies the draft's rotation), on
/// the page that owns a full placement editor.
fn turn_model(state: &mut State, axis: usize, sign: f64) {
    let q = quat_from_euler(
        state.model_ui.roll_deg.to_radians(),
        state.model_ui.pitch_deg.to_radians(),
        state.model_ui.yaw_deg.to_radians(),
    );
    let turn = quat_from_axis_angle(axis, sign * 90.0f64.to_radians());
    let (r, p, y) = quat_to_euler(quat_mul(turn, q));
    state.model_ui.roll_deg = r.to_degrees();
    state.model_ui.pitch_deg = p.to_degrees();
    state.model_ui.yaw_deg = y.to_degrees();
    apply_model_draft(state);
}

// --------------------------------------------------------------- setups page

/// The setups list (M3.2 A): the contexts a clamping creates. Clicking a
/// row activates it (undoable - it is the ActiveSetup command), New
/// creates the next named setup, optionally carrying the current one's
/// stock and placed offsets across (the shop's datum transfer, the
/// re-clamp law). The Stock and Work-offset pages always edit the ACTIVE
/// row's furniture, so this page manages context, never geometry.
fn setups_page(ui: &mut Ui, state: &mut State) {
    let names = camcraft_app::setup::names(&state.world);
    let active = camcraft_app::setup::active(&state.world);
    if names.is_empty() {
        ui.label(
            "No setups in this document: the one global stock and the \
             G54-G59 offsets act as the first setup.",
        );
        ui.add_space(6.0);
        if ui.button("Create first setup").clicked() {
            state.pending.push_back(CommandKind::CreateSetup {
                name: None,
                datum_from: None,
            });
        }
        ui.separator();
        // A document with no setups still machines steel - the
        // workholding section and the tool list belong to the implicit
        // context too.
        fixtures_section(ui, state, None);
        ui.separator();
        tool_list_section(ui, state, None);
        sheet_footer(ui, state);
        return;
    }
    for name in &names {
        let is_active = active.as_deref() == Some(name.as_str());
        ui.horizontal(|ui| {
            if ui.selectable_label(is_active, name).clicked() && !is_active {
                state
                    .pending
                    .push_back(CommandKind::ActivateSetup(Some(name.clone())));
            }
            if is_active {
                ui.label("(active)");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Delete").clicked() {
                    state
                        .pending
                        .push_back(CommandKind::DeleteSetup(name.clone()));
                }
            });
        });
        if let Some(def) = camcraft_app::setup::def_of(&state.world, name) {
            let placed = (0..WCS_SLOTS.len())
                .filter(|i| {
                    camcraft_app::setup::wcs_entity(&state.world, Some(name.as_str()), *i).is_some()
                })
                .count();
            let mut line = format!("    stock {}, offsets placed {placed}", def.stock);
            if !def.datum_from.is_empty() {
                line.push_str(&format!(", datum from {}", def.datum_from));
            }
            ui.label(line);
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.setups_ui.datum, "Carry stock and offsets over");
        if ui.button("New setup").clicked() {
            let datum_from = if state.setups_ui.datum {
                active.clone()
            } else {
                None
            };
            state.pending.push_back(CommandKind::CreateSetup {
                name: None,
                datum_from,
            });
            state.setups_ui.datum = false;
        }
    });
    ui.separator();
    fixtures_section(ui, state, active.as_deref());
    ui.separator();
    tool_list_section(ui, state, active.as_deref());
    sheet_footer(ui, state);
    ui.label("The Stock and Work-offset pages edit the active setup's furniture.");
}

/// The tool list of one setup context (M3.2 stage C): what goes into
/// the spindle here, in change order, with the shop's time law behind
/// the estimate. Read-only by law - tools live in the Tool Library and
/// operations pick them; this page only reports what that wiring
/// implies, so there is no draft and no command here.
fn tool_list_section(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let u = state.units();
    let rows = camcraft_app::tool_list::for_setup(&state.world, ctx);
    if rows.is_empty() {
        ui.label("Tool list: no generated operation in this setup yet.");
        return;
    }
    ui.label("Tool list for this setup (in change order):");
    for row in &rows {
        ui.label(format!(
            "    {}  {} op{}  {} change{}  cut {}  rapid {}  est {} min",
            row.tool,
            row.ops.len(),
            if row.ops.len() == 1 { "" } else { "s" },
            row.changes,
            if row.changes == 1 { "" } else { "s" },
            units::len(row.cut_mm, u, 2),
            units::len(row.rapid_mm, u, 2),
            camcraft_app::setup_sheet::minutes(row.minutes),
        ));
    }
    ui.label(format!(
        "    setup total: est {} min",
        camcraft_app::setup_sheet::minutes(camcraft_app::tool_list::setup_minutes(
            &state.world,
            ctx,
        )),
    ));
}

/// The setup-sheet export row (M3.2 stage D). Writing a sheet is a
/// read of the document plus a file write - like posting - so the
/// button queues the dialog arm and nothing touches the history.
fn sheet_footer(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        if ui.button("Export setup sheets…").clicked() {
            state.pending.push_back(CommandKind::SetupSheetDialog);
        }
        ui.label("(one page per setup, text)");
    });
}

/// The workholding section of the Setups page (M3.2 stage B): the
/// solids the collision law sees in this context. A fixture is a plain
/// model wearing the marker - the rows are that model, placed by the
/// same `MoveFixture` any part uses - and the machine table is the one
/// row nobody can edit: it is derived prose, what the `.mch` says the
/// table is, never an entity. Import rides the model worker (no parse
/// in a command constructor) and tags on landing.
fn fixtures_section(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let u = state.units();
    let fixtures = camcraft_app::fixture::tagged(&state.world, ctx);
    let table = camcraft_app::fixture::table_box(&state.world);
    // A draft whose row vanished (deleted or untagged, here or via
    // undo) retires with it - the pane never edits a ghost.
    if !state.fixture_ui.name.is_empty()
        && !fixtures.iter().any(|(n, _)| n == &state.fixture_ui.name)
    {
        state.fixture_ui = FixtureUi::default();
    }
    ui.label(if fixtures.is_empty() && table.is_none() {
        "No workholding modeled: the collision checks see stock and part only."
    } else {
        "Workholding in this setup's collision checks:"
    });
    for (name, entity) in fixtures {
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        let is_draft = state.fixture_ui.name == name;
        ui.horizontal(|ui| {
            if ui.selectable_label(is_draft, &name).clicked() {
                state.fixture_ui = FixtureUi {
                    name: name.clone(),
                    x: loc.pos.x,
                    y: loc.pos.y,
                    z: loc.pos.z,
                    yaw_deg: quat_yaw_deg(loc.rot),
                };
            }
            ui.label(format!(
                "{}, {}, {}  yaw {:.1}",
                units::len(loc.pos.x, u, 2),
                units::len(loc.pos.y, u, 2),
                units::len(loc.pos.z, u, 2),
                quat_yaw_deg(loc.rot)
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Delete").clicked() {
                    state
                        .pending
                        .push_back(CommandKind::DeleteFixture(name.clone()));
                }
                if ui.small_button("Unmark").clicked() {
                    state
                        .pending
                        .push_back(CommandKind::UntagFixture(name.clone()));
                }
            });
        });
        if is_draft {
            ui.horizontal(|ui| {
                ui.label("    place");
                num(ui, u, &mut state.fixture_ui.x);
                num(ui, u, &mut state.fixture_ui.y);
                num(ui, u, &mut state.fixture_ui.z);
                ui.label("yaw");
                num_plain(ui, &mut state.fixture_ui.yaw_deg);
                if ui.small_button("Apply").clicked() {
                    state.pending.push_back(CommandKind::MoveFixture {
                        name: name.clone(),
                        pos: [state.fixture_ui.x, state.fixture_ui.y, state.fixture_ui.z],
                        yaw_deg: state.fixture_ui.yaw_deg,
                    });
                }
            });
        }
    }
    if let Some(t) = table {
        ui.label(format!(
            "Machine/Table  span {} x {}  top Z {}  (from the machine file)",
            units::len(t.max.x - t.min.x, u, 1),
            units::len(t.max.y - t.min.y, u, 1),
            units::len(t.max.z, u, 2),
        ));
    }
    ui.horizontal(|ui| {
        if ui.button("Import fixture…").clicked() {
            state.pending.push_back(CommandKind::ImportFixtureDialog);
        }
        // The other way in: a model already imported - select its row
        // in the browser and mark it. Offered only for exactly one
        // selected model, so the button never guesses its target.
        let sel: Vec<String> = model::selected_names(&state.world)
            .into_iter()
            .filter(|n| {
                model::find_by_name(&state.world, n)
                    .is_some_and(|e| model::is_model(&state.world, &e))
            })
            .collect();
        if sel.len() == 1 && ui.button(format!("Mark {} as fixture", sel[0])).clicked() {
            state.pending.push_back(CommandKind::TagFixture {
                name: sel[0].clone(),
                setup: ctx.map(str::to_owned),
            });
        }
    });
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
        if let Some(t) =
            model::find_by_name(&state.world, &nm).and_then(|e| model::tool_of(&state.world, &e))
        {
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
            .horizontal(|ui| (ui.button("Edit").clicked(), ui.button("Delete").clicked()))
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
                ui.label(format!(
                    "{}  Ø{} {}",
                    cname,
                    ct.diameter,
                    kind_name(ct.kind)
                ));
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
/// M1.7d: the G-code viewer. The program one successful post wrote,
/// line-numbered and monospaced, with the scroll cursor (the line at
/// the window's middle) published through `PostView::line` for the
/// viewport's amber segment (`shell::gcode_segment`). It shows the
/// retained text, not a re-read of the file: text and positions came
/// from the same engine run, so a highlighted line always highlights
/// the move that line posts, even if something edits the file after.
pub(crate) fn gcode_window(ctx: &egui::Context, state: &mut State) {
    let Some(view) = state.post_view.as_mut() else {
        return;
    };
    if !view.open {
        return; // closed by ✖ or a document switch: allocate nothing
    }
    let stem = view
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program")
        .to_owned();
    let mut open = view.open;
    // Disjoint field borrows: the closure writes `line` and reads
    // `program` at once, and `open` is a copy resolved after the pass.
    let PostView {
        program,
        line,
        blocks,
        trace,
        ..
    } = view;
    // The trace pane's selection is DERIVED (the block owning the
    // scroll cursor), never stored - the one cursor law the viewer
    // already obeys, applied to a second view of the same evidence.
    let owning = |ln: usize| -> Option<usize> {
        blocks
            .iter()
            .position(|b| ln >= b.first && ln < b.last.max(b.first + 1))
    };
    Window::new(format!("G-code - {stem}"))
        .id(Id::new("gcode-viewer"))
        .open(&mut open)
        .default_size([560.0, 400.0])
        .show(ctx, |ui| {
            let jump = std::cell::Cell::new(None::<usize>);
            ui.horizontal(|ui| {
                // Stage D: the debugging trace, beside the program it
                // explains. Off by default - the plain viewer draws
                // exactly as it always did.
                ui.checkbox(trace, "trace");
            });
            let trace_on = *trace;
            let sel = owning(*line);
            // Trace off: the listing is the window's direct child -
            // byte-for-byte the geometry every cursor-law test was
            // written against. Trace on: egui 0.36's Sides, sized to
            // the listing's left (300 px) and the rest to the
            // program. (A `horizontal` of `allocate_ui` panes was
            // tried first and is wrong inside an auto-shrinking
            // Window: the child measures available_width as its
            // desired width, the window grows to satisfy it, and the
            // next frame's cursor law reads a rectangle it never
            // wrote. Sides asks each child only for a row during
            // sizing, so the window's rect stays the user's.)
            if trace_on {
                let h = ui.available_height();
                egui::containers::Sides::new()
                    .shrink_right()
                    .height(h)
                    .show(
                        ui,
                        |ui| {
                            ui.allocate_ui(Vec2::new(300.0, h), |ui| {
                                ScrollArea::vertical()
                                    .id_salt("gcode-trace-list")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for (i, b) in blocks.iter().enumerate() {
                                            let txt = format!("{:>4}  {}", b.first + 1, b.label);
                                            let rt = egui::RichText::new(txt)
                                                .monospace()
                                                .size(11.0)
                                                .color(if sel == Some(i) {
                                                    ui.visuals().strong_text_color()
                                                } else {
                                                    ui.visuals().text_color()
                                                });
                                            let r = ui.add(
                                                Label::new(rt)
                                                    .selectable(false)
                                                    .sense(Sense::click()),
                                            );
                                            if r.clicked() {
                                                jump.set(Some(b.first));
                                            }
                                        }
                                    });
                            });
                        },
                        |ui| {
                            gcode_listing(
                                ui,
                                program.as_str(),
                                line,
                                blocks.as_slice(),
                                true,
                                &jump,
                            );
                        },
                    );
            } else {
                gcode_listing(ui, program.as_str(), line, blocks.as_slice(), false, &jump);
            }
        });
    view.open = open;
}

/// The program listing: every line, uniformly `row_h` tall, with the
/// scroll cursor law the viewer is tested against (the middle row of
/// the clip rect IS the published line). Drawn as the window's direct
/// child or to the right of the trace panel - one geometry, two
/// hosts. While the trace is open, lines outside the block under
/// study rest weak; a row of the clicked block centers itself, so the
/// cursor law reads the selection back next frame - the pane stores
/// no second truth about the scroll.
fn gcode_listing(
    ui: &mut Ui,
    program: &str,
    line: &mut usize,
    blocks: &[camcraft_app::post::TraceBlock],
    trace_on: bool,
    jump: &std::cell::Cell<Option<usize>>,
) {
    ScrollArea::vertical()
        .id_salt("gcode-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The scroll offset is read, not stored: egui lays the
            // inner ui out at `-offset`, so the clip rect's center
            // against the content top names the middle row - that row
            // is the sync cursor. Uniform rows (one monospace label
            // per line, spacing fixed) are what make the division
            // exact.
            let n = program.lines().count();
            let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
            ui.spacing_mut().item_spacing.y = 0.0;
            let top = ui.min_rect().min.y;
            let vis = ui.clip_rect();
            *line = (((vis.center().y - top) / row_h).max(0.0) as usize).min(n.saturating_sub(1));
            let want = trace_on
                .then(|| {
                    blocks
                        .iter()
                        .position(|b| *line >= b.first && *line < b.last.max(b.first + 1))
                        .map(|bi| (blocks[bi].first, blocks[bi].last))
                })
                .flatten();
            let width = ui.available_width();
            for (i, ln) in program.lines().enumerate() {
                let label = format!("{:>4}  {}", i + 1, ln);
                let mut rt = egui::RichText::new(label).monospace();
                if let Some((a, b)) = want
                    && !(i >= a && i < b.max(a + 1))
                {
                    rt = rt.weak();
                }
                // add_sized pins every row to exactly row_h, so the
                // cursor arithmetic above stays exact over a long
                // program instead of drifting a row.
                let resp = ui.add_sized([width, row_h], Label::new(rt).selectable(false));
                if jump.get() == Some(i) {
                    resp.scroll_to_me(Some(Align::Center));
                    jump.set(None);
                }
            }
        });
}

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
                    let before = state.tool_ui.kind;
                    let mut k = state.tool_ui.kind.min(TOOL_KINDS.len() - 1);
                    combo(ui, "tool_kind", &mut k, &TOOL_KINDS);
                    state.tool_ui.kind = k.min(TOOL_KINDS.len() - 1);
                    if state.tool_ui.kind != before {
                        // A draft inherited from a mill carries every
                        // specialty field inert (0.0); moving to a kind
                        // that READS them seeds the standard value, so
                        // flipping the combo alone never leaves the Add
                        // button refusing on a half-built tool.
                        if state.tool_ui.kind == 3
                            && (state.tool_ui.tip_angle.is_nan() || state.tool_ui.tip_angle <= 60.0)
                        {
                            state.tool_ui.tip_angle = 118.0;
                        }
                        if state.tool_ui.kind == 4
                            && (state.tool_ui.pitch.is_nan() || state.tool_ui.pitch <= 0.0)
                        {
                            state.tool_ui.pitch = 1.0;
                        }
                        if state.tool_ui.kind == 6
                            && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
                        {
                            state.tool_ui.nose_radius = 0.8;
                        }
                        // The boring bar's nose (M5.2 F): the same
                        // corner circle, read by the turning kind now -
                        // seed it the way the insert's row is, so a
                        // combo flip alone never leaves the bore op
                        // refusing at the kernel's point-tip door.
                        if state.tool_ui.kind == 5
                            && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
                        {
                            state.tool_ui.nose_radius = 0.4;
                        }
                        if state.tool_ui.kind == 7
                            && (state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0)
                        {
                            state.tool_ui.width = 2.0;
                        }
                        // The threading V (M5.2 D): crest width 0.866 x
                        // the seed op's pitch 2.0, and the 60-degree
                        // metric/unified flank angle - the two numbers
                        // the generator fits against the thread it cuts.
                        if state.tool_ui.kind == 8 {
                            if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
                                state.tool_ui.width = 1.732;
                            }
                            if state.tool_ui.tip_angle.is_nan()
                                || !(0.0..180.0).contains(&state.tool_ui.tip_angle)
                            {
                                state.tool_ui.tip_angle = 60.0;
                            }
                        }
                        // The form gauge (M5.2 E2): the house row's
                        // 4-wide band on a 6-radius crown - wide
                        // enough to state a valley, narrow enough to
                        // stand inside twice its own arc.
                        if state.tool_ui.kind == 9 {
                            if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
                                state.tool_ui.width = 4.0;
                            }
                            if state.tool_ui.nose_radius.is_nan()
                                || state.tool_ui.nose_radius <= 0.0
                            {
                                state.tool_ui.nose_radius = 6.0;
                            }
                        }
                        // The chamfer mill (M2.4 B): the shop's 90 -
                        // the supplement of the 45-degree face the
                        // catalog's chamfer seed stands at.
                        if state.tool_ui.kind == 10
                            && (state.tool_ui.tip_angle.is_nan()
                                || !(0.0..180.0).contains(&state.tool_ui.tip_angle))
                        {
                            state.tool_ui.tip_angle = 90.0;
                        }
                    }
                    field(ui, "Diameter");
                    num(ui, u, &mut state.tool_ui.diameter);
                    // Kind-read fields (the inert-unless-the-kind-reads-them
                    // law): a point angle means nothing on an end mill, a
                    // pitch means nothing on a drill - show each only where
                    // it feeds a generator or the collision profile.
                    if state.tool_ui.kind == 3 {
                        field(ui, "Point angle (deg)");
                        num_plain(ui, &mut state.tool_ui.tip_angle);
                        ui.label("cone tip, 60 to 180; standard twist drill is 118");
                    }
                    if state.tool_ui.kind == 4 {
                        field(ui, "Thread pitch");
                        num(ui, u, &mut state.tool_ui.pitch);
                        ui.label("distance advanced per turn - M6 is 1.0, M8 is 1.25");
                    }
                    if state.tool_ui.kind == 6 {
                        field(ui, "Insert nose radius");
                        num(ui, u, &mut state.tool_ui.nose_radius);
                        ui.label("the corner circle ground into the tip - it IS the cutting ball");
                    }
                    if state.tool_ui.kind == 5 {
                        field(ui, "Bar nose radius");
                        num(ui, u, &mut state.tool_ui.nose_radius);
                        ui.label("the ground tip circle the bar rides the hole wall on (M5.2 F) - inert on drilling cycles, which size by the slide");
                    }
                    if state.tool_ui.kind == 7 {
                        field(ui, "Blade width");
                        num(ui, u, &mut state.tool_ui.width);
                        ui.label("the cutting edge along the axis - the kerf's width IS the tool");
                    }
                    if state.tool_ui.kind == 8 {
                        field(ui, "Blade width");
                        num(ui, u, &mut state.tool_ui.width);
                        ui.label("the V's crest width across the axis - 0.866 x the pitch it threads, and it must fit inside that pitch");
                        field(ui, "Form angle (deg)");
                        num_plain(ui, &mut state.tool_ui.tip_angle);
                        ui.label("the included angle of the V's flanks - 60 for the metric and unified forms, 55 for Whitworth");
                    }
                    if state.tool_ui.kind == 9 {
                        field(ui, "Band width");
                        num(ui, u, &mut state.tool_ui.width);
                        ui.label("the crown's ground edge across the axis - it must stand inside twice the crown radius");
                        field(ui, "Crown radius");
                        num(ui, u, &mut state.tool_ui.nose_radius);
                        ui.label("the arc of the ground edge in the work's meridian - the sagitta the verdict bends by");
                    }
                    if state.tool_ui.kind == 10 {
                        field(ui, "Cone angle (deg)");
                        num_plain(ui, &mut state.tool_ui.tip_angle);
                        ui.label("the included angle of the taper - 180 minus twice the chamfer's slope, so 90 cuts a 45-degree face");
                    }
                    if state.tool_ui.kind != 4 {
                        field(ui, "Corner radius");
                        num(ui, u, &mut state.tool_ui.corner_r);
                    }
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
        Pos2::new(axis_x + (r as f32 * scale), base_y - (z as f32 * scale))
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
    // Placement (M5.2 G): the M3.2 flip grew its two missing angles.
    // The draft mirrors the world whenever the page moves to another
    // model; typing edits the draft, Apply (or a turn button) writes
    // through `MoveModel` - the numbers on screen and the geometry in
    // the viewport are the same Location, one write apart.
    if state.model_ui.name != name {
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        let (roll, pitch, yaw) = quat_to_euler(loc.rot);
        state.model_ui = ModelUi {
            name: name.clone(),
            x: loc.pos.x,
            y: loc.pos.y,
            z: loc.pos.z,
            roll_deg: roll.to_degrees(),
            pitch_deg: pitch.to_degrees(),
            yaw_deg: yaw.to_degrees(),
        };
    }
    ui.add_space(6.0);
    field(ui, "X");
    num(ui, u, &mut state.model_ui.x);
    field(ui, "Y");
    num(ui, u, &mut state.model_ui.y);
    field(ui, "Z");
    num(ui, u, &mut state.model_ui.z);
    field(ui, "Tilt about X (roll)");
    num_plain(ui, &mut state.model_ui.roll_deg);
    field(ui, "Tilt about Y (pitch)");
    num_plain(ui, &mut state.model_ui.pitch_deg);
    field(ui, "Turn about Z (yaw)");
    num_plain(ui, &mut state.model_ui.yaw_deg);
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
                    turn_model(state, *axis, *sign);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            apply_model_draft(state);
        }
    });
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
    if model::stock_def_of(&state.world, &entity)
        .is_some_and(|s| matches!(s, StockShape::FromModel { source } if source == name))
    {
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
    use camcraft_app::geometry::{Point3, transform_point};

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

    fn location(state: &State, slot: &str) -> camcraft_app::schema::Location {
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
            [(p.x - 10.0), (p.y + 1.0), p.z]
                .iter()
                .all(|v| v.abs() < 1e-9),
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
        assert!(
            (dot.abs() - 1.0).abs() < 1e-9,
            "closed the circle: {:?}",
            loc.rot
        );
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
            [(p.x - 5.0), p.y, (p.z - 1.0)]
                .iter()
                .all(|v| v.abs() < 1e-9),
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
