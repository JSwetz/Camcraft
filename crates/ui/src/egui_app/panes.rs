//! The egui shell's pane layer, graduated from the spike's panels.rs -
//! every pane a faithful re-implementation of its imgui original
//! (`crates/ui/src/{ribbon,browser}.rs`): same strings, same command
//! shapes, same enabled rules. The strips are painted (super::painted)
//! because that is the law the spike proved; the property pages moved to
//! `pages` with M-c, leaving this module the keyboard layer and the two
//! painted panes.
//!
//! Layering holds (AGENTS.md): world access is `hsmworks_app`'s public API
//! only, and every change goes through `state.pending` / the command
//! methods, never an ad-hoc field poke. The whole-pane law
//! (docs/spike-egui-go-no-go.md) forbids porting a pane by halves.

use egui::{Key, Label, Rect, RichText, Sense, Ui, Vec2};

use hsmworks_app::machining2d::{Lead, PocketStrategy, Side};
use hsmworks_app::model;
use hsmworks_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::tools::ToolKind;

use crate::state::{CommandKind, OpUi, State, UiTarget};

use super::painted;
use super::theme::Palette;

/// The operation kinds in panel order (mirrors operations.rs's TYPES).
/// Shared with pages.rs: the Type combo there and the ribbon tips here
/// quote the same table.
pub(crate) const TYPES: [&str; 5] = ["Face", "2D Contour", "2D Pocket", "Slot", "Trace"];

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
/// toggled, label and tip. The table is the whole group layout
/// (ribbon.rs's, command for command).
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

fn tip_of(c: Cmd) -> String {
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

/// The property-pane target. Ephemeral UI state (AGENTS.md): immediate,
/// not queued - the imgui shell's actions::target, same field, same law.
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

fn run_cmd(c: Cmd, state: &mut State) {
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
            let r = Rect::from_min_size(egui::Pos2::new(x, cell_top), Vec2::new(CELL_W, CELL_H));
            let id = base.with((gi, ci));
            let resp = painted::cell(ui, id, r, &text_of(*c), pal, enabled(*c, state), toggled(*c, state));
            let fired = resp.clicked() && enabled(*c, state);
            let cell = *c;
            resp.on_hover_text(tip_of(cell));
            if fired {
                run_cmd(cell, state);
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

/// The status strip (bottom panel): the shell's one-line feedback, dimmer,
/// with the chrome the painted ground gives it. Since M-d this is the
/// file-op outcome line too ("saved …", "open failed: …" - State::run
/// writes them all), while command refusals read twice by design: here for
/// everyone, and on their own pane through `error_for` (M-c). Startup is a
/// plain empty document; an untouched status reads "ready".
pub fn status(ui: &mut Ui, state: &State, pal: &Palette) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    let text = if state.status.is_empty() { "ready" } else { state.status.as_str() };
    ui.horizontal(|ui| {
        ui.add(Label::new(RichText::new(text).weak()).selectable(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("egui shell (M-d) - imgui stays default until M-g")
                    .color(pal.text_faint)
                    .small(),
            );
        });
    });
    ui.allocate_rect(rect, Sense::hover());
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
    depth: usize,
    leaf: Leaf,
) {
    rows.push(painted::Row { label, suffix, kind, selected, hidden, depth });
    leaves.push(leaf);
}

pub(crate) fn kind_name(k: ToolKind) -> &'static str {
    match k {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
    }
}

/// The whole tree, built exactly like browser.rs: four sections,
/// per-group selection/visibility rules, the suffix column mirrored as
/// `suffix` (kind, cache tag, warning count - the one status channel the
/// column model lays out). Sections show a faint placeholder row when
/// empty, in place of imgui's help paragraph (the tree paints rows, not
/// wrapped text).
fn browser_rows(state: &State) -> (Vec<painted::Row>, Vec<Leaf>) {
    let mut rows = Vec::new();
    let mut leaves = Vec::new();
    let t = state.pane_target();

    push_row(&mut rows, &mut leaves, "Setup".into(), String::new(), painted::Kind::Section, false, false, 0, Leaf::Section);
    push_row(
        &mut rows,
        &mut leaves,
        STOCK_NAME.to_owned(),
        String::new(),
        painted::Kind::Stock,
        matches!(t, UiTarget::Stock),
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
            1,
            Leaf::Wcs(i),
        );
    }

    push_row(&mut rows, &mut leaves, "Models".into(), String::new(), painted::Kind::Section, false, false, 0, Leaf::Section);
    let entities = model::model_entities(&state.world);
    if entities.is_empty() {
        push_row(&mut rows, &mut leaves, "(no models yet)".into(), String::new(), painted::Kind::Section, false, true, 1, Leaf::Section);
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
            1,
            Leaf::Model(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Tools".into(), String::new(), painted::Kind::Section, false, false, 0, Leaf::Section);
    let tools: Vec<(String, hsmworks_app::tools::Tool)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(&state.world, e))
        .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
        .collect();
    if tools.is_empty() {
        push_row(&mut rows, &mut leaves, "(no tools yet)".into(), String::new(), painted::Kind::Section, false, true, 1, Leaf::Section);
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
            1,
            Leaf::Tool(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Operations".into(), String::new(), painted::Kind::Section, false, false, 0, Leaf::Section);
    let names: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .map(|(name, _)| name)
        .collect();
    if names.is_empty() {
        push_row(&mut rows, &mut leaves, "(no operations yet)".into(), String::new(), painted::Kind::Section, false, true, 1, Leaf::Section);
    }
    for name in names {
        let Some(entity) = model::find_by_name(&state.world, &name) else {
            continue;
        };
        let kind = model::operation_of(&state.world, &entity)
            .map(|op| op.params.kind().to_string())
            .unwrap_or_default();
        let tag = match operation::status_of(&state.world, &entity) {
            operation::OpStatus::Never => "never",
            operation::OpStatus::Ok => "ok",
            operation::OpStatus::Stale => "STALE",
        };
        let warns = model::toolpath_of(&state.world, &entity).map_or(0, |tp| tp.warnings.len());
        let warn_text = match warns {
            0 => String::new(),
            1 => "  1 warning".to_owned(),
            n => format!("  {n} warnings"),
        };
        let hidden = state.hidden_paths.contains(&name);
        push_row(
            &mut rows,
            &mut leaves,
            name.clone(),
            format!("{kind}  {tag}{warn_text}"),
            painted::Kind::Op,
            matches!(&t, UiTarget::Operation(n) if *n == name),
            hidden,
            1,
            Leaf::Op(name),
        );
    }
    (rows, leaves)
}

/// The browser pane: the two global view toggles, then the painted tree
/// with per-row click/eye dispatch identical to browser.rs.
pub fn browser(ui: &mut Ui, state: &mut State, pal: &Palette) {
    ui.checkbox(&mut state.show_world_axes, "Show world origin axes");
    ui.checkbox(&mut state.show_all_paths, "Show all toolpaths");
    ui.separator();
    let (rows, leaves) = browser_rows(state);
    let rect = Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(ui.available_width(), rows.len() as f32 * painted::ROW_H),
    );
    let hit = painted::tree(ui, rect, &rows, pal);
    if let Some(ix) = hit.eye {
        match leaves[ix].clone() {
            Leaf::Model(n) => State::toggle_visibility(&mut state.hidden_models, &n),
            Leaf::Op(n) => State::toggle_visibility(&mut state.hidden_paths, &n),
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
            // Target only: no Select, so browsing tools never arms Del.
            Leaf::Tool(n) => target(state, UiTarget::Tool(n)),
            Leaf::Op(n) => {
                target(state, UiTarget::Operation(n.clone()));
                state.pending.push_back(CommandKind::Select(Some(n)));
            }
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}

// --------------------------------------------------------------- parameters

/// The parameters' type-change law, graduated with the ribbon's Create
/// cells (it is what a new operation is built from; actions.rs mirrors
/// it). Carries every field across, the way the imgui panel's kind combo
/// does; M-c's property pages reuse the same function, so creation and
/// editing cannot disagree on what a retype preserves.
pub(crate) fn retype(cur: &OpParams, kind: usize) -> OpParams {
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

/// The defaults for a brand-new operation of kind `kind` (actions.rs
/// mirror: a default draft, retyped).
fn quick_params(kind: usize) -> OpParams {
    retype(&OpUi::default().params, kind)
}

