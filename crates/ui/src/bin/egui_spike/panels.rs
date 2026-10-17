//! S2: the shell's panes on egui - ribbon, browser tree, property pages.
//!
//! This module is the mirror law of the spike: every pane here is a faithful
//! re-implementation of the imgui original (`crates/ui/src/{ribbon,browser,
//! operations,setup}.rs`) - same strings, same command shapes, same notes and
//! echoes - with two deliberate divergences, each listed in the go/no-go
//! report (S4): the strips are painted (painted.rs) because the spike must
//! prove the draw-list port, and feature picking / the WCS anchor grid are
//! out of scope for a property-pane spike (no viewport pick path yet).
//!
//! Layering holds: world access is `hsmworks_app`'s public API only, and
//! every change goes through `state.pending` / the command methods, never an
//! ad-hoc field poke (AGENTS.md "Everything Is a Command").

use egui::{Color32, ComboBox, DragValue, Key, Label, Rect, ScrollArea, Sense, TextEdit, Ui, Vec2};

use hsmworks_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use hsmworks_app::machining2d::{ARC_FIT_TOL, Lead, PocketStrategy, Side};
use hsmworks_app::model;
use hsmworks_app::operation::{self, GeoSel, Height, HeightReference, OpParams, OpType, SrcRef};
use hsmworks_app::schema::{Operation, STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::{self, ToolKind};
use hsmworks_app::verify::Warning;

use hsmworks_ui::operations::fold_warnings;
use hsmworks_ui::state::{CommandKind, ErrorScope, State, UiTarget};

use super::painted;
use super::theme::Palette;

/// The operation kinds in panel order (mirrors operations.rs's TYPES).
const TYPES: [&str; 5] = ["Face", "2D Contour", "2D Pocket", "Slot", "Trace"];
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

// ---------------------------------------------------------------- shortcuts

/// The shell's keyboard layer: history keys always, destructive keys only
/// while no text field owns focus (imgui's io.WantTextInput guard).
pub fn shortcuts(ui: &mut Ui, state: &mut State) {
    let typing = ui.memory(|m| m.focused().is_some());
    let (modifiers, z, y, del, f) = ui.input(|i| {
        (
            i.modifiers,
            i.key_pressed(Key::Z),
            i.key_pressed(Key::Y),
            i.key_pressed(Key::Delete),
            i.key_pressed(Key::F),
        )
    });
    let cmd = modifiers.command;
    if cmd && z && !modifiers.shift && !modifiers.alt {
        state.pending.push_back(CommandKind::Undo);
    }
    if cmd && y {
        // Ctrl+Y and Ctrl+Shift+Y both redo (imgui law).
        state.pending.push_back(CommandKind::Redo);
    }
    if del && !typing {
        state.pending.push_back(CommandKind::DeleteSelected);
    }
    if f && !typing && !cmd && !modifiers.alt {
        state.pending.push_back(CommandKind::FrameAll);
    }
}

// -------------------------------------------------------------------- ribbon

/// One ribbon cell: command identity plus the state it reads for enabled,
/// toggled, label and tip. The table is the whole group layout.
#[derive(Clone, Copy)]
enum Cmd {
    New,
    Model,
    Project,
    Save,
    Stock,
    Origin,
    Tools,
    Create(usize),
    Undo,
    Redo,
    Run,
    Delete,
    Frame,
    Paths,
}

const GROUPS: [(&str, &[Cmd]); 5] = [
    ("File", &[Cmd::New, Cmd::Model, Cmd::Project, Cmd::Save]),
    ("Setup", &[Cmd::Stock, Cmd::Origin, Cmd::Tools]),
    ("Create", &[Cmd::Create(0), Cmd::Create(1), Cmd::Create(2), Cmd::Create(3), Cmd::Create(4)]),
    ("Modify", &[Cmd::Undo, Cmd::Redo, Cmd::Run, Cmd::Delete]),
    ("Inspect", &[Cmd::Frame, Cmd::Paths]),
];

const CELL_W: f32 = 56.0;
const CELL_H: f32 = 24.0;
const GAP: f32 = 4.0;
const PAD_H: f32 = 8.0;

fn label_of(c: Cmd) -> &'static str {
    match c {
        Cmd::New => "New",
        Cmd::Model => "Model",
        Cmd::Project => "Project",
        Cmd::Save => "Save",
        Cmd::Stock => "Stock",
        Cmd::Origin => "Origin",
        Cmd::Tools => "Tools",
        Cmd::Create(_) => "op", // formatted by the group row; ribbon shows TYPES
        Cmd::Undo => "Undo",
        Cmd::Redo => "Redo",
        Cmd::Run => "Run",
        Cmd::Delete => "Delete",
        Cmd::Frame => "Frame",
        Cmd::Paths => "Paths",
    }
}

fn text_of(c: Cmd) -> String {
    match c {
        Cmd::Create(i) => match i {
            0 => "Face",
            1 => "Contour",
            2 => "Pocket",
            3 => "Slot",
            _ => "Trace",
        }
        .to_owned(),
        other => label_of(other).to_owned(),
    }
}

fn tip_of(c: Cmd, _state: &State) -> String {
    match c {
        Cmd::New => "New document".into(),
        Cmd::Model => "Import a model\tCtrl+M".into(),
        Cmd::Project => "Open a project\tCtrl+O".into(),
        Cmd::Save => "Save the project\tCtrl+S".into(),
        Cmd::Stock => "Edit the stock".into(),
        Cmd::Origin => "Edit the work offsets".into(),
        Cmd::Tools => "Tools".into(),
        Cmd::Create(i) => format!("Add a {} operation", TYPES[i]),
        Cmd::Undo => "Undo\tCtrl+Z".into(),
        Cmd::Redo => "Redo\tCtrl+Y".into(),
        Cmd::Run => "Regenerate the selected operation".into(),
        Cmd::Delete => "Delete the selection\tDel".into(),
        Cmd::Frame => "Frame the model\tF".into(),
        Cmd::Paths => "Show every operation's path".into(),
    }
}

fn enabled(c: Cmd, state: &State) -> bool {
    match c {
        Cmd::Undo => state.history.can_undo(),
        Cmd::Redo => state.history.can_redo(),
        Cmd::Run => state.selected_operation().is_some(),
        Cmd::Delete => state.has_selection(),
        Cmd::Create(_) => !state.tool_names().is_empty(),
        _ => true,
    }
}

fn toggled(c: Cmd, state: &State) -> bool {
    match c {
        Cmd::Paths => state.show_all_paths,
        _ => false,
    }
}

fn target(state: &mut State, t: UiTarget) {
    state.ui_target = t;
}

fn add_operation(state: &mut State, kind: usize) {
    // Mirrors actions.rs: the new op cuts with the newest tool in the
    // document; no tools, no operation - and say so.
    let Some(tool) = state.tool_names().pop() else {
        state.status = "cannot add an operation: no tool in the document".to_owned();
        return;
    };
    let mut n = model::named_entities(&state.world)
        .iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .count()
        + 1;
    let mut name = format!("Operation{n}");
    while model::find_by_name(&state.world, &name).is_some() {
        n += 1;
        name = format!("Operation{n}");
    }
    state.pending.push_back(CommandKind::CreateOperation {
        name: name.clone(),
        tool,
        src: SrcRef::Stock,
        params: quick_params(kind),
    });
    state.pending.push_back(CommandKind::Select(Some(name.clone())));
    state.ui_target = UiTarget::Operation(name);
}

fn generate_selected(state: &mut State) {
    if let Some(name) = state.selected_operation() {
        state.generate_operation(&name);
    }
}

fn run(c: Cmd, state: &mut State) {
    match c {
        Cmd::New => state.pending.push_back(CommandKind::NewDocument),
        Cmd::Model => state.pending.push_back(CommandKind::OpenModelDialog),
        Cmd::Project => state.pending.push_back(CommandKind::OpenProjectDialog),
        Cmd::Save => state.pending.push_back(CommandKind::SaveProjectDialog),
        Cmd::Stock => target(state, UiTarget::Stock),
        Cmd::Origin => {
            let slot = state.wcs_ui.slot.min(WCS_SLOTS.len() - 1);
            target(state, UiTarget::WcsSlot(slot));
        }
        Cmd::Tools => target(state, UiTarget::Tool(String::new())),
        Cmd::Create(i) => add_operation(state, i),
        Cmd::Undo => state.pending.push_back(CommandKind::Undo),
        Cmd::Redo => state.pending.push_back(CommandKind::Redo),
        Cmd::Run => generate_selected(state),
        Cmd::Delete => state.pending.push_back(CommandKind::DeleteSelected),
        Cmd::Frame => state.pending.push_back(CommandKind::FrameAll),
        Cmd::Paths => state.show_all_paths = !state.show_all_paths,
    }
}

/// The ribbon strip: painted cells over a fixed-height top panel. Group
/// captions and hairline dividers match imgui's layout math (CELL_W/GAP/
/// PAD_H from ribbon.rs), the cells themselves are painted::cell.
pub fn ribbon(ui: &mut Ui, state: &mut State, pal: &Palette) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    let cell_top = rect.top() + 3.0;
    let caption_y = rect.top() + 31.0;
    let mut x = rect.left() + PAD_H;
    let base = ui.id().with("ribbon");
    for (gi, (title, cells)) in GROUPS.iter().enumerate() {
        let start = x;
        for (ci, c) in cells.iter().enumerate() {
            let r = Rect::from_min_size(
                egui::Pos2::new(x, cell_top),
                Vec2::new(CELL_W, CELL_H),
            );
            let id = base.with((gi, ci));
            let resp = painted::cell(ui, id, r, &text_of(*c), pal, enabled(*c, state), toggled(*c, state));
            let fired = resp.clicked() && enabled(*c, state);
            let cell = *c;
            resp.on_hover_text(tip_of(cell, state));
            if fired {
                run(cell, state);
            }
            x += CELL_W + GAP;
        }
        x -= GAP;
        let mid = (start + x) * 0.5;
        painted::caption(ui.painter(), mid, caption_y, title, pal);
        if gi + 1 < GROUPS.len() {
            x += GAP + 2.0;
            painted::divider(ui.painter(), x - (GAP + 2.0) / 2.0 - 1.0, rect, pal);
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}

/// The status strip (bottom panel): the shell's one-line feedback, dimmer.
pub fn status(ui: &mut Ui, state: &State, pal: &Palette) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    ui.add(Label::new(egui::RichText::new(state.status.as_str()).weak()).selectable(false));
}

// ------------------------------------------------------------------ browser

/// What a browser row's click means. Parallel to the painted Row list.
#[derive(Clone)]
enum Leaf {
    Section,
    Stock,
    Wcs(usize),
    Model(String),
    Tool(String),
    Op(String),
}

/// Append one tree row and its parallel leaf.
#[allow(clippy::too_many_arguments)]
fn push_row(
    rows: &mut Vec<painted::Row>,
    leaves: &mut Vec<Leaf>,
    label: String,
    suffix: String,
    kind: painted::Kind,
    selected: bool,
    hidden: bool,
    has_path: bool,
    stale: bool,
    depth: usize,
    leaf: Leaf,
) {
    rows.push(painted::Row { label, suffix, kind, selected, hidden, has_path, stale, depth });
    leaves.push(leaf);
}

fn kind_name(k: ToolKind) -> &'static str {
    match k {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
    }
}

fn toggle_visibility(list: &mut Vec<String>, name: &str) {
    match list.iter().position(|x| x == name) {
        Some(i) => {
            list.remove(i);
        }
        None => list.push(name.to_owned()),
    }
}

/// The whole tree, built exactly like browser.rs: four sections, per-group
/// selection/visibility rules, the suffix column mirrored as `suffix`.
/// Divergences D1/D3 (S4 report): row icons become painted glyphs, and an
/// empty group shows one faint placeholder row instead of imgui's help
/// paragraph (the tree paints rows, not wrapped text).
fn browser_rows(state: &State) -> (Vec<painted::Row>, Vec<Leaf>) {
    let mut rows = Vec::new();
    let mut leaves = Vec::new();
    let t = state.pane_target();

    push_row(&mut rows, &mut leaves, "Setup".into(), String::new(), painted::Kind::Section, false, false, false, false, 0, Leaf::Section);
    push_row(
        &mut rows,
        &mut leaves,
        STOCK_NAME.to_owned(),
        String::new(),
        painted::Kind::Stock,
        matches!(t, UiTarget::Stock),
        false,
        false,
        false,
        1,
        Leaf::Stock,
    );
    for (i, slot) in WCS_SLOTS.iter().enumerate() {
        let placed = model::find_by_name(&state.world, slot).is_some();
        push_row(
            &mut rows,
            &mut leaves,
            (*slot).to_owned(),
            if placed { String::new() } else { "not placed".into() },
            painted::Kind::Wcs,
            matches!(t, UiTarget::WcsSlot(j) if j == i),
            false,
            false,
            false,
            1,
            Leaf::Wcs(i),
        );
    }

    push_row(&mut rows, &mut leaves, "Models".into(), String::new(), painted::Kind::Section, false, false, false, false, 0, Leaf::Section);
    let entities = model::model_entities(&state.world);
    if entities.is_empty() {
        push_row(&mut rows, &mut leaves, "(no models yet)".into(), String::new(), painted::Kind::Section, false, true, false, false, 1, Leaf::Section);
    }
    for (name, entity) in entities {
        let selected = model::is_selected(&state.world, &entity);
        let hidden = state.hidden_models.contains(&name);
        push_row(
            &mut rows,
            &mut leaves,
            name.clone(),
            if hidden { " (hidden)".into() } else { String::new() },
            painted::Kind::Model,
            matches!(&t, UiTarget::Model(n) if *n == name) || selected,
            hidden,
            false,
            false,
            1,
            Leaf::Model(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Tools".into(), String::new(), painted::Kind::Section, false, false, false, false, 0, Leaf::Section);
    let tools: Vec<(String, hsmworks_app::tools::Tool)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(&state.world, e))
        .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
        .collect();
    if tools.is_empty() {
        push_row(&mut rows, &mut leaves, "(no tools yet)".into(), String::new(), painted::Kind::Section, false, true, false, false, 1, Leaf::Section);
    }
    for (name, tool) in tools {
        push_row(
            &mut rows,
            &mut leaves,
            name.clone(),
            format!("Ø{} {}", tool.diameter, kind_name(tool.kind)),
            painted::Kind::Tool,
            matches!(&t, UiTarget::Tool(n) if *n == name),
            false,
            false,
            false,
            1,
            Leaf::Tool(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Operations".into(), String::new(), painted::Kind::Section, false, false, false, false, 0, Leaf::Section);
    let names: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .map(|(name, _)| name)
        .collect();
    if names.is_empty() {
        push_row(&mut rows, &mut leaves, "(no operations yet)".into(), String::new(), painted::Kind::Section, false, true, false, false, 1, Leaf::Section);
    }
    for name in names {
        let Some(entity) = model::find_by_name(&state.world, &name) else {
            continue;
        };
        let kind = model::operation_of(&state.world, &entity)
            .map(|op| op.params.kind().to_string())
            .unwrap_or_default();
        let (tag, stale) = match operation::status_of(&state.world, &entity) {
            operation::OpStatus::Never => ("never", false),
            operation::OpStatus::Ok => ("ok", false),
            operation::OpStatus::Stale => ("STALE", true),
        };
        let tp = model::toolpath_of(&state.world, &entity);
        let warns = tp.as_ref().map_or(0, |tp| tp.warnings.len());
        let warn_text = match warns {
            0 => String::new(),
            1 => "  1 warning".to_owned(),
            n => format!("  {n} warnings"),
        };
        let has_path = tp.as_ref().is_some_and(|tp| !tp.path.is_empty());
        let hidden = state.hidden_paths.contains(&name);
        push_row(
            &mut rows,
            &mut leaves,
            name.clone(),
            format!("{kind}  {tag}{warn_text}"),
            painted::Kind::Op,
            matches!(&t, UiTarget::Operation(n) if *n == name),
            hidden,
            has_path,
            stale,
            1,
            Leaf::Op(name),
        );
    }
    (rows, leaves)
}

/// The browser pane: the two global toggles, then the painted tree with
/// per-row click/eye dispatch identical to browser.rs.
pub fn browser(ui: &mut Ui, state: &mut State, pal: &Palette) {
    ui.checkbox(&mut state.show_world_axes, "Show world origin axes");
    ui.checkbox(&mut state.show_all_paths, "Show all toolpaths");
    ui.separator();
    let (rows, leaves) = browser_rows(state);
    let rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), rows.len() as f32 * painted::ROW_H));
    let hit = painted::tree(ui, rect, &rows, pal);
    if let Some(ix) = hit.eye {
        match leaves[ix].clone() {
            Leaf::Model(n) => toggle_visibility(&mut state.hidden_models, &n),
            Leaf::Op(n) => toggle_visibility(&mut state.hidden_paths, &n),
            _ => {}
        }
    }
    if let Some(ix) = hit.row {
        match leaves[ix].clone() {
            Leaf::Section => {}
            Leaf::Stock => {
                target(state, UiTarget::Stock);
                if model::find_by_name(&state.world, STOCK_NAME).is_some() {
                    state.pending.push_back(CommandKind::Select(Some(STOCK_NAME.to_owned())));
                }
            }
            Leaf::Wcs(i) => {
                target(state, UiTarget::WcsSlot(i));
                if let Some(slot) = WCS_SLOTS.get(i)
                    && model::find_by_name(&state.world, slot).is_some()
                {
                    state.pending.push_back(CommandKind::Select(Some((*slot).to_owned())));
                }
            }
            Leaf::Model(n) => {
                target(state, UiTarget::Model(n.clone()));
                state.pending.push_back(CommandKind::Select(Some(n)));
            }
            Leaf::Tool(n) => target(state, UiTarget::Tool(n)),
            Leaf::Op(n) => {
                target(state, UiTarget::Operation(n.clone()));
                state.pending.push_back(CommandKind::Select(Some(n)));
            }
        }
    }
    ui.allocate_rect(rect, Sense::hover());
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
        let mut t_idx = tools.iter().position(|t| *t == state.op_ui.tool).unwrap_or(0);
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
        .filter(|(_, e)| !model::is_stock(&state.world, e) && !model::is_tool(&state.world, e) && !model::is_operation(&state.world, e))
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
    ui.label("Click in the viewport - Ctrl adds; Alt in Edge mode takes one edge.");
    if state.op_ui.src_kind == 2 {
        let chips: Vec<(String, bool)> = state
            .geo_chips()
            .into_iter()
            .map(|c| (c.label, c.missing))
            .collect();
        for (i, (label, missing)) in chips.into_iter().enumerate() {
            let remove = ui
                .horizontal(|ui| {
                    if missing {
                        ui.colored_label(AMBER, label);
                    } else {
                        ui.label(label);
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
        if ui.small_button("Clear##geo").clicked() {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.clone(),
                tool: None,
                src: Some(SrcRef::Stock),
            });
        }
    }
    let notes: Vec<String> = operation::selection_notes(&state.world, &state.op_ui.geo.clone(), &state.op_ui.params.clone());
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
            if ui.button("Delete##op").clicked() {
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
            && model::find_by_name(&state.world, &state.op_ui.tool.clone()).is_some_and(|e| model::is_tool(&state.world, &e));
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
            state.pending.push_back(CommandKind::CreateOperation { name: name.clone(), tool, src, params });
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
    let op = model::find_by_name(&state.world, &nm).as_ref().and_then(|e| model::operation_of(&state.world, e));
    if let OpParams::Contour { side: Side::Auto, .. } = &state.op_ui.params {
        let resolved = op.as_ref().and_then(|o| operation::contour_auto_side(&state.world, &o.src, &state.op_ui.params.clone()));
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
                        PocketStrategy::Waterline => "Auto -> Waterline: the outline is round and the rings reach every band.".into(),
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
            combo(ui, "c_side", &mut s_idx, hsmworks_ui::operations::side_names());
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
            let names = hsmworks_ui::operations::strategy_names();
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
        OpParams::Slot { a, b, width, stock_to_leave, clearance_z, retract_z: _, top_z, bottom_z, stepdown, stepover, feed, plunge_feed, climb } => {
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
    let cur = names.get((*idx).min(names.len().saturating_sub(1))).copied().unwrap_or("");
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
    let cur: String = names.get((*idx).min(names.len().saturating_sub(1))).cloned().unwrap_or_default();
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
    let vlabel = if matches!(h.reference, HeightReference::Fixed) { "Value" } else { "Offset" };
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

/// Retype a draft's params to operation kind `kind`, keeping what the kinds
/// share (mirrors operations.rs's retype, same defaults).
fn retype(cur: &OpParams, kind: usize) -> OpParams {
    let (feed, plunge_feed, climb, clearance, stepover, stepdown, top, bottom, retract) = match cur {
        OpParams::Face { feed, plunge_feed, climb, clearance_z, stepover, .. } => (
            *feed,
            *plunge_feed,
            *climb,
            *clearance_z,
            Some(*stepover),
            None,
            None,
            None,
            None,
        ),
        OpParams::Contour { feed, plunge_feed, climb, clearance_z, retract_z, top_z, bottom_z, stepdown, .. } => (
            *feed,
            *plunge_feed,
            *climb,
            *clearance_z,
            None,
            Some(*stepdown),
            Some(*top_z),
            Some(*bottom_z),
            Some(*retract_z),
        ),
        OpParams::Pocket { feed, plunge_feed, climb, clearance_z, retract_z, top_z, bottom_z, stepdown, stepover, .. } => (
            *feed,
            *plunge_feed,
            *climb,
            *clearance_z,
            Some(*stepover),
            Some(*stepdown),
            Some(*top_z),
            Some(*bottom_z),
            *retract_z,
        ),
        OpParams::Slot { feed, plunge_feed, climb, clearance_z, retract_z, top_z, bottom_z, stepdown, stepover, .. } => (
            *feed,
            *plunge_feed,
            *climb,
            *clearance_z,
            Some(*stepover),
            Some(*stepdown),
            Some(*top_z),
            Some(*bottom_z),
            *retract_z,
        ),
        OpParams::Trace { feed, plunge_feed, climb, clearance_z, retract_z, top_z, bottom_z, stepdown, .. } => (
            *feed,
            *plunge_feed,
            *climb,
            *clearance_z,
            None,
            Some(*stepdown),
            Some(*top_z),
            Some(*bottom_z),
            *retract_z,
        ),
    };
    let top = top.unwrap_or(Height::of(HeightReference::TopOfModel, 0.0));
    let bottom = bottom.unwrap_or(Height::of(HeightReference::WorkOffset(0), 0.0));
    let retract = retract.unwrap_or(Height::of(HeightReference::TopOfStock, 10.0));
    let stepover = stepover.unwrap_or(3.0);
    let stepdown = stepdown.unwrap_or(5.0);
    match kind {
        1 => OpParams::Contour {
            stock_to_leave: 0.0,
            clearance_z: clearance,
            retract_z: retract,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            feed,
            plunge_feed,
            climb,
            side: Side::Auto,
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
            plunge_feed,
            climb,
            strategy: PocketStrategy::Auto,
        },
        3 => OpParams::Slot {
            a: hsmworks_app::geo2d::Point2::new(-20.0, 0.0),
            b: hsmworks_app::geo2d::Point2::new(20.0, 0.0),
            width: 4.0,
            stock_to_leave: 0.0,
            clearance_z: clearance,
            retract_z: None,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
        },
        4 => OpParams::Trace {
            clearance_z: clearance,
            retract_z: None,
            top_z: top,
            bottom_z: bottom,
            stepdown,
            feed,
            plunge_feed,
            climb,
        },
        _ => OpParams::Face {
            stepover,
            clearance_z: clearance,
            cut_z: Height::of(HeightReference::TopOfStock, 0.0),
            top_z: None,
            stepdown: None,
            feed,
            plunge_feed,
            climb,
        },
    }
}

/// The defaults for a brand-new operation of kind `kind` (actions.rs mirror:
/// a default draft, retyped).
fn quick_params(kind: usize) -> OpParams {
    retype(&hsmworks_ui::state::OpUi::default().params, kind)
}

// ---------------------------------------------------------------- stock page

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
            .filter(|(_, e)| !model::is_stock(&state.world, e) && !model::is_tool(&state.world, e) && !model::is_operation(&state.world, e))
            .map(|(n, _)| n)
            .collect();
        field(ui, "Source");
        if models.is_empty() {
            ui.label("No models to take stock from.");
        } else {
            let mut idx = models.iter().position(|m| *m == state.stock_ui.source).unwrap_or(models.len() - 1);
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
    let has_stock = model::find_by_name(&state.world, STOCK_NAME).is_some_and(|e| model::is_stock(&state.world, &e));
    ui.horizontal(|ui| {
        if ui.button("Apply##stk").clicked() {
            let s = if shape == 1 {
                StockShape::Cylinder { diameter, height }
            } else if shape == 2 {
                StockShape::FromModel { source: source.clone() }
            } else {
                StockShape::Box { dx, dy, dz }
            };
            state.pending.push_back(CommandKind::SetStock { shape: s, pos, yaw_deg: yaw });
        }
        if ui.button("Fit to Models").clicked() {
            state.pending.push_back(CommandKind::StockFit { margin, shape });
        }
        if has_stock && ui.button("Delete##stk").clicked() {
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
            for (i, (label, axis, sign)) in TURNS[row..row + 3].iter().enumerate() {
                if ui.button(format!("Turn {label}##wcs_turn{}", row + i)).clicked() {
                    turn_wcs(state, *axis, *sign);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Level##wcs_level").clicked() {
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
        let line = match model::find_by_name(&state.world, name).as_ref().and_then(|e| model::transform_of(&state.world, e)) {
            Some(loc) => format!(
                "{name}: {:.2}, {:.2}, {:.2}  yaw {:.1}",
                loc.pos.x,
                loc.pos.y,
                loc.pos.z,
                hsmworks_ui::state::quat_yaw_deg(loc.rot)
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
    // not writable from this bin anyway. Same behavior, one source.
    state.sync_setup_ui();
}

fn apply_wcs_drafts(state: &mut State) {
    let slot = *WCS_SLOTS.get(state.wcs_ui.slot.min(WCS_SLOTS.len() - 1)).unwrap_or(&WCS_SLOTS[0]);
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
            if ui.button("Apply##tool").clicked() {
                state.pending.push_back(CommandKind::ChangeTool {
                    name: name_now.clone(),
                    tool: state.tool_ui.to_tool(),
                });
            }
            if ui.button("Delete##tool").clicked() {
                state.pending.push_back(CommandKind::DeleteTool(name_now.clone()));
            }
        });
    } else {
        let add = ui.add_enabled(block.is_empty(), egui::Button::new("Add##tool"));
        if add.clicked() {
            state.pending.push_back(CommandKind::AddTool { name: name_now.trim().to_owned(), tool });
        }
        if !block.is_empty() {
            ui.label(block);
        }
    }
    if ui.button("New##tool").clicked() {
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
                    ui.button(format!("Add##cat_{cname}")).clicked()
                }
            })
            .inner;
        if clicked {
            state.pending.push_back(CommandKind::AddTool { name: cname.to_owned(), tool: ct });
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
    if model::stock_def_of(&state.world, &entity).is_some_and(|s| matches!(s, StockShape::FromModel { source } if source == name)) {
        ui.label("It is also the stock solid (Setup > Stock).");
    }
    ui.label("Selected now: Del removes it (undo restores).");
}
