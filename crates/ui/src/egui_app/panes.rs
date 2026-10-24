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
//! Layering holds (AGENTS.md): world access is `camcraft_app`'s public API
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

use camcraft_app::model;
use camcraft_app::operation::{self, OpParams, OpType};
use camcraft_app::schema::WCS_SLOTS;
use camcraft_app::tools::ToolKind;

use crate::actions;
use crate::state::{CommandKind, DisplayMode, Sim, State, UiTarget};

use super::icons::{Icon, IconSet};
use super::painted;
use super::theme::Palette;

/// The operation kinds in panel order - the app's kind registry labels
/// (M5.0.2), one list for the whole surface. Shared with pages.rs: the
/// Type combo there and the ribbon tips here quote the same table.
pub(crate) const TYPES: [&str; camcraft_app::operation::KIND_COUNT] =
    camcraft_app::operation::ALL_LABELS;

// ---------------------------------------------------------------- shortcuts

/// The shell's keyboard layer: history keys always, destructive and file
/// keys only while no text field owns focus (imgui's io.WantTextInput
/// guard - a dialog must never spawn mid-type). Every arm fires through
/// `crate::actions`, the same entry the ribbon uses.
pub fn shortcuts(ui: &mut Ui, state: &mut State) {
    let typing = ui.memory(|m| m.focused().is_some());
    let (modifiers, n, o, s, p, q, z, y, del, f) = ui.input(|i| {
        (
            i.modifiers,
            i.key_pressed(Key::N),
            i.key_pressed(Key::O),
            i.key_pressed(Key::S),
            i.key_pressed(Key::P),
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
        if p {
            // M1.7b: posting joins the file lane - the same dialog law
            // as Save (the gate's refusals arrive through the queue arm).
            actions::post_program(state);
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
    Post,
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
    Simulate,
    Deviate,
}

const GROUPS: [(&str, &[Cmd]); 5] = [
    ("File", &[Cmd::New, Cmd::Model, Cmd::Project, Cmd::Save, Cmd::Post]),
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
            Cmd::Create(6),
            Cmd::Create(7),
        ],
    ),
    ("Modify", &[Cmd::Undo, Cmd::Redo, Cmd::Run, Cmd::Delete]),
    ("Inspect", &[Cmd::Frame, Cmd::Paths, Cmd::Simulate, Cmd::Deviate]),
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
        Cmd::Post => "Post",
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
        Cmd::Simulate => "Simulate",
        Cmd::Deviate => "Deviate",
    }
}

fn text_of(c: Cmd) -> String {
    match c {
        // The ribbon caption is registry data (M5.0.2) - imgui's strings,
        // same order as the Create cells below. The list here is built
        // from the table, so an out-of-range index means a table bug:
        // stop in debug, name it and stay on screen in release.
        Cmd::Create(i) => match camcraft_app::operation::KindInfo::ALL.get(i) {
            Some(k) => k.caption.to_owned(),
            None => {
                debug_assert!(false, "Create({i}) past the kind table");
                eprintln!("ribbon: unknown op kind {i}");
                "op".to_owned()
            }
        },
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
        Cmd::Post => "Post NC program (validation gate), then review it\tCtrl+P".into(),
        Cmd::Stock => "Stock definition".into(),
        Cmd::Origin => "Work offset (WCS)".into(),
        Cmd::Tools => "Tool library".into(),
        Cmd::Create(i) => {
            // imgui's swap: the buttons sit disabled over an empty tool
            // list, and the tip says what to do about it (ribbon.rs:138).
            if camcraft_app::machine::kind_of(&state.world)
                == camcraft_app::machine::Kind::Lathe
            {
                "The document's machine is a lathe - turning operations \
                 land with M4.2; these milling cells do not drive it"
                    .into()
            } else if state.tool_names().is_empty() {
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
        Cmd::Simulate => "Simulate tool moves (transport bar)".into(),
        Cmd::Deviate => {
            "Judge the program's stock against the model (deviation map)".into()
        }
    }
}

pub(super) fn enabled(c: Cmd, state: &State) -> bool {
    match c {
        Cmd::Undo => state.history.can_undo(),
        Cmd::Redo => state.history.can_redo(),
        Cmd::Run => state.selected_operation().is_some(),
        Cmd::Delete => state.has_selection(),
        // Workspaces by machine kind (M3.2 stage E): the mill families
        // drive a mill and a mill-turn's live tooling, and nothing
        // else - a lathe document darkens every Create cell honestly
        // instead of offering mill paths its machine cannot run.
        Cmd::Create(_) => {
            camcraft_app::machine::kind_of(&state.world) != camcraft_app::machine::Kind::Lathe
                && !state.tool_names().is_empty()
        }
        Cmd::Simulate => state.has_toolpaths(),
        // The same gate as Simulate: a program with no generated move
        // has nothing to judge - the cell stays dark until there is.
        Cmd::Deviate => state.has_toolpaths(),
        _ => true,
    }
}

pub(super) fn toggled(c: Cmd, state: &State) -> bool {
    match c {
        Cmd::Paths => state.show_all_paths,
        Cmd::Simulate => state.sim.is_some(),
        Cmd::Deviate => state.program_sim.is_some(),
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
        Cmd::Post => Icon::Post,
        Cmd::Stock => Icon::Stock,
        Cmd::Origin => Icon::WorkOffset,
        Cmd::Tools => Icon::Tools,
        // Icons stay a UI table (asset names do not belong in the app),
        // but an exhaustive match on the kind, not on the index: a new
        // registry row with no icon here stops at the compiler, never
        // silently borrows the last glyph (M5.0.2 - the old `_ =>` arm
        // was one of the three silent-drift catch-alls).
        Cmd::Create(i) => match camcraft_app::operation::KindInfo::ALL.get(i) {
            Some(k) => op_icon(k.ty),
            None => {
                debug_assert!(false, "Create({i}) past the kind table");
                eprintln!("ribbon: unknown op kind {i}");
                Icon::Generate
            }
        },
        Cmd::Undo => Icon::Undo,
        Cmd::Redo => Icon::Redo,
        Cmd::Run => Icon::Generate,
        Cmd::Delete => Icon::Delete,
        Cmd::Frame => Icon::FrameAll,
        Cmd::Paths => Icon::Toolpaths,
        Cmd::Simulate => Icon::Simulate,
        // The stock glyph, reused (RECOMMENDED.md's no-new-asset law):
        // what the cell shows IS the judged stock.
        Cmd::Deviate => Icon::Stock,
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
        OpType::Pocket3D => Icon::Pocket,
        OpType::Finish3D => Icon::Finish3D,
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
        Cmd::Post => actions::post_program(state),
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
        Cmd::Simulate => {
            // View state, like Paths: the cell arms or disarms the
            // transport. The playhead inside is immediate-mode by
            // AGENTS.md's explicit exemption - no command to route.
            state.sim = if state.sim.is_some() { None } else { Some(Sim::new()) };
        }
        Cmd::Deviate => {
            // View state like Simulate, with one difference the actions
            // layer holds: arming RUNS the program sim (a world read,
            // never a world write), so the effect lives in actions.
            actions::toggle_deviate(state);
        }
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
    // The row is a flow, not the fixed imgui pitch: cells share the strip's
    // width, so adding an operation kind can never push the last group off
    // the edge of the window (M2.1a's seventh Create cell was what the
    // fixed 56 px box could no longer hold at 1280). CELL_W stands as the
    // cap, so a roomy window still draws ribbon.rs's exact geometry; below
    // FLOOR_W the row overflows on purpose rather than rendering illegible
    // cells. Painted content is centered in each rect, so narrowing is
    // pure geometry, no clipping law to re-learn.
    let n_cells: usize = GROUPS.iter().map(|(_, g)| g.len()).sum();
    let n_groups = GROUPS.len();
    let overhead = 2.0 * PAD_H
        + (n_cells - n_groups) as f32 * GAP
        + (n_groups - 1) as f32 * (GAP + 2.0);
    const FLOOR_W: f32 = 40.0;
    let cell_w = ((strip.width() - overhead) / n_cells as f32).clamp(FLOOR_W, CELL_W);
    // PAD_V (ribbon.rs): cells start 4 below the strip top.
    let cell_top = strip.top() + 4.0;
    let mut x = strip.left() + PAD_H;
    let mut salt = 0usize;
    for (gi, (title, group)) in GROUPS.iter().enumerate() {
        let start = x;
        for c in *group {
            cells.push(Cell {
                cmd: *c,
                rect: Rect::from_min_size(egui::Pos2::new(x, cell_top), Vec2::new(cell_w, CELL_H)),
                salt,
            });
            salt += 1;
            x += cell_w + GAP;
        }
        x -= GAP;
        captions.push(((start + x) * 0.5, *title));
        if gi + 1 < n_groups {
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
pub fn status(ui: &mut Ui, state: &mut State, pal: &Palette) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    // Owned so the button below can reborrow `state` mutably while the
    // label still shows the text (disjoint fields - cheap either way).
    let text = if state.status.is_empty() {
        "ready".to_owned()
    } else {
        state.status.clone()
    };
    // M3.1 C: the batch fix rides the bar whenever a revision has left
    // operations Stale - the tree marks them, the page says why, this is
    // the one click that brings the job current.
    let stale = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| {
            model::is_operation(&state.world, e)
                && operation::status_of(&state.world, e) == operation::OpStatus::Stale
        })
        .count();
    ui.horizontal(|ui| {
        ui.add(Label::new(RichText::new(text.as_str()).weak()).selectable(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("egui shell (M-g) - imgui retired")
                    .color(pal.text_faint)
                    .small(),
            );
            if stale > 0 {
                let button = ui.button(format!("Regenerate all stale ({stale})"));
                if button.clicked() {
                    state.pending.push_back(CommandKind::RegenerateAll);
                }
            }
        });
    });
    ui.allocate_rect(rect, Sense::hover());
}

// ------------------------------------------------------------------ browser

/// What a browser row's click means. Parallel to the painted Row list.
#[derive(Clone)]
enum Leaf {
    Section,
    /// The Setups page row (M3.2 A): one row over the document's whole
    /// setups list, like Tool rows - target-only, never arms Del.
    Setups,
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

/// What a Models row says about its model (M3.2 B): the fixture marker
/// rides the suffix column - the row's one status channel, browser.rs's
/// law - ahead of the hidden marker, and the kind/icon pair picks the
/// vise glyph for steel the program must dodge. Pure, so the tree's
/// fixture law is testable without a harness.
pub(crate) fn model_row_marks(is_fixture: bool, hidden: bool) -> (String, painted::Kind, Icon) {
    let mut suffix = String::new();
    if is_fixture {
        suffix.push_str(" (fixture)");
    }
    if hidden {
        suffix.push_str(" (hidden)");
    }
    (
        suffix,
        if is_fixture { painted::Kind::Fixture } else { painted::Kind::Model },
        if is_fixture { Icon::Fixture } else { Icon::Model },
    )
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
    // Setups (M3.2 A): the row appears once the document has any, and
    // the Stock and WCS rows below resolve through the ACTIVE one - the
    // tree shows the context you are standing in, never a global ghost.
    let active = camcraft_app::setup::active(&state.world);
    if !camcraft_app::setup::names(&state.world).is_empty() {
        push_row(
            &mut rows,
            &mut leaves,
            "Setups".to_owned(),
            active.clone().unwrap_or_default(),
            painted::Kind::Section,
            Some(Icon::WorkOffset),
            matches!(t, UiTarget::Setups),
            false,
            1,
            Leaf::Setups,
        );
    }
    let stock_name = camcraft_app::setup::stock_entity_name(&state.world, active.as_deref());
    push_row(
        &mut rows,
        &mut leaves,
        stock_name,
        String::new(),
        painted::Kind::Stock,
        Some(Icon::Stock),
        matches!(t, UiTarget::Stock),
        false,
        1,
        Leaf::Stock,
    );
    for (i, slot) in WCS_SLOTS.iter().enumerate() {
        let placed =
            camcraft_app::setup::wcs_entity(&state.world, active.as_deref(), i).is_some();
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
        let (suffix, kind, icon) =
            model_row_marks(model::is_fixture(&state.world, &entity), hidden);
        push_row(
            &mut rows,
            &mut leaves,
            name.clone(),
            suffix,
            kind,
            Some(icon),
            matches!(&t, UiTarget::Model(n) if *n == name) || selected,
            hidden,
            1,
            Leaf::Model(name),
        );
    }

    push_row(&mut rows, &mut leaves, "Tools".into(), String::new(), painted::Kind::Section, None, false, false, 0, Leaf::Section);
    let tools: Vec<(String, camcraft_app::tools::Tool)> = model::named_entities(&state.world)
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
            // Target-only, like the Tool rows: browsing setups never
            // arms Del, and one page lists them all.
            Leaf::Setups => actions::target(state, UiTarget::Setups),
            Leaf::Stock => {
                actions::target(state, UiTarget::Stock);
                let stock_name = camcraft_app::setup::stock_entity_name(
                    &state.world,
                    camcraft_app::setup::active(&state.world).as_deref(),
                );
                if model::find_by_name(&state.world, &stock_name).is_some() {
                    state.pending.push_back(CommandKind::Select(Some(stock_name)));
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

