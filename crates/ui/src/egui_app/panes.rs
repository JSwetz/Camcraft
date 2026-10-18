//! The egui shell's pane layer, graduated from the spike's panels.rs -
//! every pane a faithful re-implementation of its imgui original (the
//! retired ribbon.rs/browser.rs): same strings, same command shapes, same
//! enabled rules. The strips are painted (super::painted) because that is
//! the law the spike proved; the property pages moved to `pages` with M-c,
//! leaving this module the keyboard layer and the two painted panes. Since
//! M-g the bundled Tabler glyphs ride both strips as egui textures
//! (super::icons), with the painted vector marks as their no-GPU twins -
//! the retired imgui icons module folded into this stack, not replaced.
//!
//! Layering holds (AGENTS.md): world access is `hsmworks_app`'s public API
//! only, and every change goes through `state.pending` / the command
//! methods, never an ad-hoc field poke. The whole-pane law
//! (docs/spike-egui-go-no-go.md) forbids porting a pane by halves.
//!
//! Since M-f the surfaces here stopped forking their own command logic: the
//! ribbon cells, browser rows and keyboard shortcuts all fire through the
//! shared `crate::actions` layer - the same one the imgui ribbon/browser/menu
//! call - so each effect lives exactly once across both shells. `Cmd::Paths`
//! stays a direct field flip (view state, mirroring imgui ribbon.rs) and the
//! browser pushes its `Select` alongside `actions::target` (the tree law).

use egui::{Key, Label, Rect, RichText, Sense, Ui, Vec2};

use hsmworks_app::model;
use hsmworks_app::operation::{self, OpParams, OpType};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::tools::ToolKind;

use crate::actions;
use crate::state::{CommandKind, DisplayMode, State, UiTarget};

use super::icons::{Icon, IconSet};
use super::painted;
use super::theme::Palette;

/// The operation kinds in panel order (mirrors operations.rs's TYPES).
/// Shared with pages.rs: the Type combo there and the ribbon tips here
/// quote the same table.
pub(crate) const TYPES: [&str; 6] = ["Face", "2D Contour", "2D Pocket", "Slot", "Trace", "Drill"];

// ---------------------------------------------------------------- shortcuts

/// The shell's keyboard layer: history keys always, destructive and file
/// keys only while no text field owns focus (imgui's io.WantTextInput
/// guard - a dialog must never spawn mid-type). Every arm fires through
/// `crate::actions`, the same entry the ribbon uses.
pub fn shortcuts(ui: &mut Ui, state: &mut State) {
    let typing = ui.memory(|m| m.focused().is_some());
    let (modifiers, n, o, s, q, z, y, del, f) = ui.input(|i| {
        (
            i.modifiers,
            i.key_pressed(Key::N),
            i.key_pressed(Key::O),
            i.key_pressed(Key::S),
            i.key_pressed(Key::Q),
            i.key_pressed(Key::Z),
            i.key_pressed(Key::Y),
            i.key_pressed(Key::Delete),
            i.key_pressed(Key::F),
        )
    });
    let cmd = modifiers.command;
    if cmd && z && !modifiers.shift && !modifiers.alt {
        actions::undo(state);
    }
    if cmd && y {
        // Ctrl+Y and Ctrl+Shift+Y both redo (imgui law).
        actions::redo(state);
    }
    if cmd && !typing {
        // The File tips' keys (ribbon.rs strings): N new, O a model,
        // Shift+O a project, S save. Shift is what tells the two opens
        // apart, so it must be tested, not ignored (imgui law).
        if n {
            actions::new_document(state);
        }
        if o && !modifiers.shift {
            actions::open_model_dialog(state);
        }
        if o && modifiers.shift {
            actions::open_project(state);
        }
        if s {
            actions::save_project(state);
        }
        if q {
            // The window close button (CloseRequested) always quit; since
            // M-g it has a keyboard sibling. imgui's only quit surface was
            // its File menu, which retired with the imgui shell - Ctrl+Q
            // keeps that capability on the shared law (Quit -> request_quit
            // -> exit_requested -> el.exit()).
            actions::quit(state);
        }
    }
    if del && !typing {
        actions::delete_selected(state);
    }
    if f && !typing && !cmd && !modifiers.alt {
        actions::frame_all(state);
    }
}

// -------------------------------------------------------------------- ribbon

/// One ribbon cell: command identity plus the state it reads for enabled,
/// toggled, label and tip. The table is the whole group layout
/// (ribbon.rs's, command for command).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cmd {
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
    (
        "Create",
        &[
            Cmd::Create(0),
            Cmd::Create(1),
            Cmd::Create(2),
            Cmd::Create(3),
            Cmd::Create(4),
            Cmd::Create(5),
        ],
    ),
    ("Modify", &[Cmd::Undo, Cmd::Redo, Cmd::Run, Cmd::Delete]),
    ("Inspect", &[Cmd::Frame, Cmd::Paths]),
];

const CELL_W: f32 = 56.0;
/// Icon over caption, ribbon.rs's cell: ICON_PX + 2 + one line, so the
/// PNG and the painted fallback occupy the same box. The egui font's
/// 11.5 px label needs 35; the fold's one geometry change (RIBBON_H in
/// shell.rs follows it).
const CELL_H: f32 = 35.0;
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
            4 => "Trace",
            _ => "Drill",
        }
        .to_owned(),
        other => label_of(other).to_owned(),
    }
}

/// The tip table, imgui's strings verbatim (ribbon.rs:140-222) - the
/// test `tips_match_the_imgui_table` keeps it that way. Every advertised
/// key is live: `shortcuts` implements them all.
pub(super) fn tip_of(c: Cmd, state: &State) -> String {
    match c {
        Cmd::New => "New document\tCtrl+N".into(),
        Cmd::Model => "Open a mesh model\tCtrl+O".into(),
        Cmd::Project => "Open a project\tCtrl+Shift+O".into(),
        Cmd::Save => "Save the project\tCtrl+S".into(),
        Cmd::Stock => "Stock definition".into(),
        Cmd::Origin => "Work offset (WCS)".into(),
        Cmd::Tools => "Tool library".into(),
        Cmd::Create(i) => {
            // imgui's swap: the buttons sit disabled over an empty tool
            // list, and the tip says what to do about it (ribbon.rs:138).
            if state.tool_names().is_empty() {
                "No tool yet - add one (Setup > Tools)".into()
            } else if i == 0 {
                "Add a Face mill operation".into()
            } else {
                format!("Add a {} operation", TYPES[i])
            }
        }
        Cmd::Undo => "Undo\tCtrl+Z".into(),
        Cmd::Redo => "Redo\tCtrl+Y".into(),
        Cmd::Run => "Generate the selected operation".into(),
        Cmd::Delete => "Delete the selection\tDel".into(),
        Cmd::Frame => "Frame all\tF".into(),
        Cmd::Paths => "Show every cached toolpath".into(),
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

/// Command to bundled glyph: the same table the retired imgui icons.rs
/// store keyed (and ribbon.rs's `Item.icon` chose per cell), so the ribbon,
/// the browser's `spec.icon` slots and the store name one set of pixels.
fn icon_of(c: Cmd) -> Icon {
    match c {
        Cmd::New => Icon::New,
        Cmd::Model => Icon::OpenModel,
        Cmd::Project => Icon::OpenProject,
        Cmd::Save => Icon::Save,
        Cmd::Stock => Icon::Stock,
        Cmd::Origin => Icon::WorkOffset,
        Cmd::Tools => Icon::Tools,
        Cmd::Create(i) => match i {
            0 => Icon::Face,
            1 => Icon::Contour,
            2 => Icon::Pocket,
            3 => Icon::Slot,
            4 => Icon::Trace,
            _ => Icon::Drill,
        },
        Cmd::Undo => Icon::Undo,
        Cmd::Redo => Icon::Redo,
        Cmd::Run => Icon::Generate,
        Cmd::Delete => Icon::Delete,
        Cmd::Frame => Icon::FrameAll,
        Cmd::Paths => Icon::Toolpaths,
    }
}

/// The op row's glyph (browser.rs's `op_icon`); `Generate` stands in where
/// the operation component is gone but the entity still names itself -
/// the fallback imgui's eyes list refuses to leave blank.
fn op_icon(kind: OpType) -> Icon {
    match kind {
        OpType::Face => Icon::Face,
        OpType::Contour => Icon::Contour,
        OpType::Pocket => Icon::Pocket,
        OpType::Slot => Icon::Slot,
        OpType::Trace => Icon::Trace,
        OpType::Drill => Icon::Drill,
    }
}

/// The ribbon's one command path: every cell fires the shared actions
/// layer (`crate::actions`), so the effect lives there exactly once and
/// menu, tree and shortcut cannot fork a variant of it (actions.rs's
/// rule). `Cmd::Paths` stays a direct field flip - view state, not a
/// command, exactly imgui's run_toggle_paths (ribbon.rs:98).
fn run_cmd(c: Cmd, state: &mut State) {
    match c {
        Cmd::New => actions::new_document(state),
        Cmd::Model => actions::open_model_dialog(state),
        Cmd::Project => actions::open_project(state),
        Cmd::Save => actions::save_project(state),
        Cmd::Stock => actions::target(state, UiTarget::Stock),
        Cmd::Origin => {
            let slot = state.wcs_ui.slot.min(WCS_SLOTS.len() - 1);
            actions::target(state, UiTarget::WcsSlot(slot));
        }
        Cmd::Tools => actions::target(state, UiTarget::Tool(String::new())),
        Cmd::Create(i) => actions::add_operation(state, i),
        Cmd::Undo => actions::undo(state),
        Cmd::Redo => actions::redo(state),
        Cmd::Run => actions::generate_selected(state),
        Cmd::Delete => actions::delete_selected(state),
        Cmd::Frame => actions::frame_all(state),
        Cmd::Paths => state.show_all_paths = !state.show_all_paths,
    }
}

/// One laid-out ribbon cell: which command, where, and its id salt.
/// A flat index salt (was `(group, index)`) - the value only has to be
/// stable across frames for hover bookkeeping, and the flat list is
/// what the gesture tests can reach.
pub(super) struct Cell {
    pub cmd: Cmd,
    pub rect: Rect,
    pub salt: usize,
}

/// The ribbon's layout math, pure: cell rects, then the group captions
/// (centre x, title) and the divider x's between groups. `ribbon` draws
/// exactly this, and the tests click through it - one source of truth
/// for where each command lives. CELL_W/GAP/PAD_H are ribbon.rs's.
pub(super) fn ribbon_cells(strip: Rect) -> (Vec<Cell>, Vec<(f32, &'static str)>, Vec<f32>) {
    let mut cells = Vec::new();
    let mut captions = Vec::new();
    let mut dividers = Vec::new();
    // PAD_V (ribbon.rs): cells start 4 below the strip top.
    let cell_top = strip.top() + 4.0;
    let mut x = strip.left() + PAD_H;
    let mut salt = 0usize;
    for (gi, (title, group)) in GROUPS.iter().enumerate() {
        let start = x;
        for c in *group {
            cells.push(Cell {
                cmd: *c,
                rect: Rect::from_min_size(egui::Pos2::new(x, cell_top), Vec2::new(CELL_W, CELL_H)),
                salt,
            });
            salt += 1;
            x += CELL_W + GAP;
        }
        x -= GAP;
        captions.push(((start + x) * 0.5, *title));
        if gi + 1 < GROUPS.len() {
            x += GAP + 2.0;
            dividers.push(x - (GAP + 2.0) / 2.0 - 1.0);
        }
    }
    (cells, captions, dividers)
}

/// The ribbon strip: painted cells over a fixed-height top panel. Group
/// captions and hairline dividers match imgui's layout math, the cells
/// themselves are painted::cell - all from `ribbon_cells`'s geometry.
/// Since M-g each cell carries its bundled glyph (the retired imgui
/// ribbon's Item.icon table), or the painted fallback headless.
pub fn ribbon(
    ui: &mut Ui,
    state: &mut State,
    pal: &Palette,
    icons: Option<&IconSet>,
) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    let base = ui.id().with("ribbon");
    let (cells, captions, dividers) = ribbon_cells(rect);
    for cell in &cells {
        let c = cell.cmd;
        let resp = painted::cell(
            ui,
            base.with(cell.salt),
            cell.rect,
            &text_of(c),
            pal,
            enabled(c, state),
            toggled(c, state),
            icons.and_then(|s| s.texture(icon_of(c))),
        );
        let fired = resp.clicked() && enabled(c, state);
        resp.on_hover_text(tip_of(c, state));
        if fired {
            run_cmd(c, state);
        }
    }
    for (mid, title) in captions {
        painted::caption(ui.painter(), mid, rect.top() + 44.0, title, pal);
    }
    for dx in dividers {
        painted::divider(ui.painter(), dx, rect, pal);
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
                RichText::new("egui shell (M-g) - imgui retired")
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

/// Append one tree row and its parallel leaf. `icon` is browser.rs's
/// `spec.icon`: the bundled glyph for the row's subject, `None` only for
/// section headers and placeholders (where imgui drew no PNG either).
#[allow(clippy::too_many_arguments)]
fn push_row(
    rows: &mut Vec<painted::Row>,
    leaves: &mut Vec<Leaf>,
    label: String,
    suffix: String,
    kind: painted::Kind,
    icon: Option<Icon>,
    selected: bool,
    hidden: bool,
    depth: usize,
    leaf: Leaf,
) {
    rows.push(painted::Row { label, suffix, kind, icon, selected, hidden, depth });
    leaves.push(leaf);
}

pub(crate) fn kind_name(k: ToolKind) -> &'static str {
    match k {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
        ToolKind::Drill => "drill",
        ToolKind::Tap => "tap",
        ToolKind::Bore => "boring bar",
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

    push_row(&mut rows, &mut leaves, "Setup".into(), String::new(), painted::Kind::Section, None, false, false, 0, Leaf::Section);
    push_row(
        &mut rows,
        &mut leaves,
        STOCK_NAME.to_owned(),
        String::new(),
        painted::Kind::Stock,
        Some(Icon::Stock),
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
            Some(Icon::WorkOffset),
            matches!(t, UiTarget::WcsSlot(j) if j == i),
            false,
            1,
            Leaf::Wcs(i),
        );
    }

    push_row(&mut rows, &mut leaves, "Models".into(), String::new(), painted::Kind::Section, None, false, false, 0, Leaf::Section);
    let entities = model::model_entities(&state.world);
    if entities.is_empty() {
        push_row(&mut rows, &mut leaves, "(no models yet)".into(), String::new(), painted::Kind::Section, None, false, true, 1, Leaf::Section);
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
            Some(Icon::Model),
            matches!(&t, UiTarget::Model(n) if *n == name) || selected,
            hidden,
            1,
            Leaf::Model(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Tools".into(), String::new(), painted::Kind::Section, None, false, false, 0, Leaf::Section);
    let tools: Vec<(String, hsmworks_app::tools::Tool)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(&state.world, e))
        .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
        .collect();
    if tools.is_empty() {
        push_row(&mut rows, &mut leaves, "(no tools yet)".into(), String::new(), painted::Kind::Section, None, false, true, 1, Leaf::Section);
    }
    for (name, tool) in tools {
        push_row(
            &mut rows,
            &mut leaves,
            name.clone(),
            format!("Ø{} {}", tool.diameter, kind_name(tool.kind)),
            painted::Kind::Tool,
            Some(Icon::Tools),
            matches!(&t, UiTarget::Tool(n) if *n == name),
            false,
            1,
            Leaf::Tool(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Operations".into(), String::new(), painted::Kind::Section, None, false, false, 0, Leaf::Section);
    let names: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .map(|(name, _)| name)
        .collect();
    if names.is_empty() {
        push_row(&mut rows, &mut leaves, "(no operations yet)".into(), String::new(), painted::Kind::Section, None, false, true, 1, Leaf::Section);
    }
    for name in names {
        let Some(entity) = model::find_by_name(&state.world, &name) else {
            continue;
        };
        // imgui browser.rs's law: the op's own kind glyphs the row, and a
        // missing operation component falls back to Generate, never blank.
        let (kind, icon) = model::operation_of(&state.world, &entity)
            .map(|op| {
                let k = op.params.kind();
                (k.to_string(), op_icon(k))
            })
            .unwrap_or_else(|| (String::new(), Icon::Generate));
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
            Some(icon),
            matches!(&t, UiTarget::Operation(n) if *n == name),
            hidden,
            1,
            Leaf::Op(name),
        );
    }
    (rows, leaves)
}

/// The browser pane: the global view toggles, then the painted tree
/// with per-row click/eye dispatch identical to browser.rs.
pub fn browser(
    ui: &mut Ui,
    state: &mut State,
    pal: &Palette,
    icons: Option<&IconSet>,
) {
    ui.checkbox(&mut state.show_world_axes, "Show world origin axes");
    ui.checkbox(&mut state.show_all_paths, "Show all toolpaths");
    // The display mode rides the same law as the checkboxes: a direct
    // field flip (view state like the camera, never a command, never on
    // the ribbon - the M-f tip table is imgui's 18-cell law untouched).
    // Radios, not a combo: the same idiom the Geometry page uses for
    // `geo_pick`, and the whole choice is legible without a click.
    // Short names by pane-width law (the Geometry row's Models/Faces/Edges
    // precedent): the browser column clips anything longer.
    ui.horizontal(|ui| {
        ui.label("Solids");
        ui.radio_value(&mut state.display, DisplayMode::ShadedEdges, "Shaded");
        ui.radio_value(&mut state.display, DisplayMode::Wireframe, "Wireframe");
    });
    ui.separator();
    let (rows, leaves) = browser_rows(state);
    let rect = Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(ui.available_width(), rows.len() as f32 * painted::ROW_H),
    );
    let hit = painted::tree(ui, rect, &rows, pal, icons);
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
                actions::target(state, UiTarget::Stock);
                if model::find_by_name(&state.world, STOCK_NAME).is_some() {
                    state.pending.push_back(CommandKind::Select(Some(STOCK_NAME.to_owned())));
                }
            }
            Leaf::Wcs(i) => {
                actions::target(state, UiTarget::WcsSlot(i));
                if let Some(slot) = WCS_SLOTS.get(i)
                    && model::find_by_name(&state.world, slot).is_some()
                {
                    state.pending.push_back(CommandKind::Select(Some((*slot).to_owned())));
                }
            }
            Leaf::Model(n) => {
                actions::target(state, UiTarget::Model(n.clone()));
                state.pending.push_back(CommandKind::Select(Some(n)));
            }
            // Target only: no Select, so browsing tools never arms Del.
            Leaf::Tool(n) => actions::target(state, UiTarget::Tool(n)),
            Leaf::Op(n) => {
                actions::target(state, UiTarget::Operation(n.clone()));
                state.pending.push_back(CommandKind::Select(Some(n)));
            }
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}

// --------------------------------------------------------------- parameters

/// The parameters' type-change law. Since M-f this is a thin pointer to
/// `crate::operations::retype` - the SAME function `crate::actions::add_
/// operation` builds quick-adds with (through `quick_params`) and the
/// imgui panel retypes with. The ribbon's Create cells and pages.rs's
/// Type combo cannot disagree about what a retype preserves, because
/// there is exactly one of them. The former egui mirror of this law is
/// retired with M-f.
pub(crate) fn retype(cur: &OpParams, kind: usize) -> OpParams {
    crate::operations::retype(cur, kind)
}

