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
use super::theme::Tokens;

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
/// toggled, label and tip. The table is the whole group layout - command
/// for command since M6.2 divided the Create row into the catalog's
/// family sections; the only hand-typed group left is the anchors below.
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
    /// The dropdown button of a folded section (M6.2): it carries the
    /// section's label, never a faceless chevron - the cell's whole
    /// caption is that label plus the combo's own arrow glyph.
    More(operation::Section),
    Undo,
    Redo,
    Run,
    Delete,
    Frame,
    Paths,
    Simulate,
    Deviate,
}

/// The four anchor groups: file work opens the row, setup follows, and
/// the run/judge groups close it. The middle is the registry's (below):
/// the old hand-typed eight-cell Create group was M1.4.3's debt, home
/// to kinds 0..=7 while the table grew to thirty.
const FILE: (&str, &[Cmd]) =
    ("File", &[Cmd::New, Cmd::Model, Cmd::Project, Cmd::Save, Cmd::Post]);
const SETUP: (&str, &[Cmd]) = ("Setup", &[Cmd::Stock, Cmd::Origin, Cmd::Tools]);
const MODIFY: (&str, &[Cmd]) = ("Modify", &[Cmd::Undo, Cmd::Redo, Cmd::Run, Cmd::Delete]);
const INSPECT: (&str, &[Cmd]) =
    ("Inspect", &[Cmd::Frame, Cmd::Paths, Cmd::Simulate, Cmd::Deviate]);

/// One ribbon group before layout: its caption and every cell it would
/// show in a row that never ran out of width. `section` marks the
/// registry-seeded Create groups - only those fold into a dropdown.
#[derive(Clone, Debug)]
pub(super) struct Group {
    pub title: &'static str,
    pub cells: Vec<Cmd>,
    pub section: Option<operation::Section>,
}

/// The whole ribbon as data (M6.2's home law): the anchors, the user's
/// pinned kinds, then every catalog section the document's machine can
/// run. Hiding, not darkening - the mill document's table carries no
/// Turning cells to grey out; `Family::admits` is the single pairing
/// this filter and the command door share. The registry's `section`
/// column is the only seed, and the row order is the product order, so
/// a section's cells list the kinds in table sequence.
#[must_use]
pub(super) fn ribbon_plan(
    machine: camcraft_app::machine::Kind,
    pinned: &[usize],
) -> Vec<Group> {
    let mut plan = vec![
        Group { title: FILE.0, cells: FILE.1.to_vec(), section: None },
        Group { title: SETUP.0, cells: SETUP.1.to_vec(), section: None },
    ];
    // Pinned kinds first among the Create groups, in pin order - and
    // only those the machine can run, so the hiding law has one more
    // surface saying the same thing (a pin to other work survives in
    // the list unread, waiting for the mill-turn document that shows
    // it again). The group never folds: the promise of a pin is that
    // it stays where the user put it, the one width rule pins are
    // exempt from, stated here rather than hidden in layout math.
    let pins: Vec<Cmd> = pinned
        .iter()
        .copied()
        .filter(|i| {
            operation::KindInfo::ALL
                .get(*i)
                .is_some_and(|k| k.family().admits(machine))
        })
        .map(Cmd::Create)
        .collect();
    if !pins.is_empty() {
        plan.push(Group { title: "Pinned", cells: pins, section: None });
    }
    plan.extend(operation::shown_for(machine).into_iter().map(|s| Group {
        title: s.label(),
        cells: s.kinds().into_iter().map(Cmd::Create).collect(),
        section: Some(s),
    }));
    plan.push(Group { title: MODIFY.0, cells: MODIFY.1.to_vec(), section: None });
    plan.push(Group { title: INSPECT.0, cells: INSPECT.1.to_vec(), section: None });
    plan
}

// The cell box is tokens, not constants (M6.1 A): `Tokens::cell_w`/
// `cell_h` - icon over caption, ribbon.rs's cell: glyph + 2 + one line,
// so the PNG and the painted fallback occupy the same box; the 11.5 px
// label needs 35, and shell.rs's ribbon height follows the token
// (`theme::ribbon_h`). GAP/PAD_H stay literals: the ribbon's own
// rhythm, not a theme knob.
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
        Cmd::More(s) => s.label(),
    }
}

fn text_of(c: Cmd) -> String {
    match c {
        // The overflow button wears the section's name plus an
        // ellipsis - "Milling 3D…" reads as "the rest of this is in
        // here", which is exactly the law the dropdown keeps (M6.2:
        // a labeled door, never a faceless chevron).
        Cmd::More(s) => format!("{}…", s.label()),
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

/// The tip table, `tips_match_the_imgui_table`'s guard. The strings
/// were imgui's verbatim (ribbon.rs:140-222) until M6.2 restructured
/// the ribbon; the promise retired that day (RECOMMENDED.md) and the
/// table became the product's own - the test still pins it row for
/// row, because a tip is a promise and promises want a guard, not
/// because the wording is borrowed. Every advertised key is live:
/// `shortcuts` implements them all, and the pin gesture names itself.
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
            // The tool-less swap first (imgui's law, ribbon.rs:138):
            // an empty tool list is the deeper trouble - nothing
            // lights until a tool exists, whatever the machine is.
            // Then the family truth (M5.2 stage A): since M6.2 the
            // plan already hides what the machine cannot run, so the
            // darkening branch is defense in depth for any surface
            // that renders a plan the door would refuse - it says so
            // and names the fix, the same pair CreateOperation
            // enforces at the command. The pin hint rides every live
            // tip: the gesture a ribbon cell answers to beyond the
            // click, so the tip table is where it belongs.
            if state.tool_names().is_empty() {
                "No tool yet - add one (Setup > Tools)".into()
            } else {
                let machine = camcraft_app::machine::kind_of(&state.world);
                let dark = camcraft_app::operation::KindInfo::ALL
                    .get(i)
                    .is_some_and(|k| !k.family().admits(machine));
                if dark {
                    let (fword, mword) = match camcraft_app::operation::KindInfo::ALL[i].family() {
                        camcraft_app::operation::Family::Mill => ("milling", "mill"),
                        camcraft_app::operation::Family::Turn => ("turning", "lathe"),
                    };
                    format!(
                        "The document's machine is a {} - {fword} operations do not drive it; mount a {mword} or a mill-turn (Setup > Machine)",
                        machine.word()
                    )
                } else if i == 0 {
                    "Add a Face mill operation\tRight-click to pin/unpin".into()
                } else {
                    format!("Add a {} operation\tRight-click to pin/unpin", TYPES[i])
                }
            }
        }
        Cmd::More(s) => format!("The rest of the {} kinds", s.label()),
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
        // The family gate by pairing (M3.2 E, M5.2 A) is the command
        // door's law; since M6.2 the ribbon's plan already hides
        // what the machine cannot run, so this half stands as defense
        // in depth - the live gate is the tool list: a kind with no
        // tool cannot cut whatever its section says.
        Cmd::Create(i) => {
            let machine = camcraft_app::machine::kind_of(&state.world);
            let kind_admits = camcraft_app::operation::KindInfo::ALL
                .get(i)
                .is_none_or(|k| k.family().admits(machine));
            kind_admits && !state.tool_names().is_empty()
        }
        // A dropdown button is live whenever it is on the ribbon; the
        // kinds behind it carry their own gates.
        Cmd::More(_) => true,
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
        // The open dropdown's button stands toggled while it is open
        // (M6.2): the accent fill says "this door is the one you
        // opened".
        Cmd::More(s) => state.ribbon_more == Some(s),
        _ => false,
    }
}

/// Command to bundled glyph: the same table the retired imgui icons.rs
/// store keyed (and ribbon.rs's `Item.icon` chose per cell), so the ribbon,
/// the browser's `spec.icon` slots and the store name one set of pixels.
fn icon_of(c: Cmd) -> Option<Icon> {
    match c {
        Cmd::New => Some(Icon::New),
        Cmd::Model => Some(Icon::OpenModel),
        Cmd::Project => Some(Icon::OpenProject),
        Cmd::Save => Some(Icon::Save),
        Cmd::Post => Some(Icon::Post),
        Cmd::Stock => Some(Icon::Stock),
        Cmd::Origin => Some(Icon::WorkOffset),
        Cmd::Tools => Some(Icon::Tools),
        // Icons stay a UI table (asset names do not belong in the app),
        // but an exhaustive match on the kind, not on the index: a new
        // registry row with no icon here stops at the compiler, never
        // silently borrows the last glyph (M5.0.2 - the old `_ =>` arm
        // was one of the three silent-drift catch-alls).
        Cmd::Create(i) => match camcraft_app::operation::KindInfo::ALL.get(i) {
            Some(k) => Some(op_icon(k.ty)),
            None => {
                debug_assert!(false, "Create({i}) past the kind table");
                eprintln!("ribbon: unknown op kind {i}");
                Some(Icon::Generate)
            }
        },
        // The overflow button carries no glyph (M6.2): the ellipsis in
        // its label is the whole affordance, so no asset is minted for
        // it and no font subset is trusted with a chevron.
        Cmd::More(_) => None,
        Cmd::Undo => Some(Icon::Undo),
        Cmd::Redo => Some(Icon::Redo),
        Cmd::Run => Some(Icon::Generate),
        Cmd::Delete => Some(Icon::Delete),
        Cmd::Frame => Some(Icon::FrameAll),
        Cmd::Paths => Some(Icon::Toolpaths),
        Cmd::Simulate => Some(Icon::Simulate),
        // The stock glyph, reused (RECOMMENDED.md's no-new-asset law):
        // what the cell shows IS the judged stock.
        Cmd::Deviate => Some(Icon::Stock),
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
        OpType::Contour5D => Icon::Contour5D,
        OpType::Parallel5D => Icon::Parallel5D,
        OpType::Swarf => Icon::Swarf,
        OpType::AdvancedSwarf => Icon::AdvancedSwarf,
        OpType::RotaryContour => Icon::RotaryContour,
        OpType::RotaryParallel => Icon::RotaryParallel,
        OpType::RotaryOffset => Icon::RotaryOffset,
        OpType::MultiAxisRough => Icon::MultiAxisRough,
        OpType::Flow5D => Icon::Flow5D,
        OpType::MultiAxisContour => Icon::MultiAxisContour,
        // The face mill's glyph, reused (the no-new-asset law, and the
        // Pocket3D precedent above): what the cell makes is a face -
        // spun rather than swept, same word, same pixels.
        OpType::Facing => Icon::Face,
        // The profile kinds keep the same glyph for the same reason:
        // what they leave is the turned surface - the bar's own
        // skin, roughed to a wall or finished to tolerance (M5.2 B).
        OpType::ProfileRough | OpType::ProfileFinish => Icon::Face,
        // Same reuse for the blade kinds: the kerf and the severed
        // column are both the turned surface giving way to the
        // tool's width (M5.2 C) - and so is the threaded wall, the
        // same surface recut one helix at a time (M5.2 D). The
        // contour rides leave the same turned skin, finished to a
        // promised face (M5.2 E).
        OpType::GrooveRough | OpType::GrooveFinish | OpType::Part | OpType::Threading => Icon::Face,
        OpType::Chamfering | OpType::TaperTurning | OpType::MultiPassTurning => Icon::Face,
        // So is the formed surface the gauge stamps (M5.2 E2), and
        // the wall the bar rides - the turned surface held to size
        // from the inside (M5.2 F), same glyph by the no-new-asset
        // law the whole family keeps.
        OpType::FormTurning | OpType::BoringTurning => Icon::Face,
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
        // Immediate-mode view state (the Paths law): which door stands
        // open is a frame fact, not a document fact - nothing to undo,
        // and clicking the open door again closes it.
        Cmd::More(s) => state.ribbon_more = (state.ribbon_more != Some(s)).then_some(s),
    }
}

/// The pin gesture (M6.2): toggle one kind's presence in the document's
/// pinned list, through the app's `ribbon` seam - the only home of the
/// singleton write. Deliberately NOT a command: ROADMAP's law for pins
/// is "layout, not cuts" - undo replays the toolpath edits, and a pin
/// neither cuts stock nor changes a program. The status line says what
/// happened, because a silent toggle is a half-answer.
fn toggle_pin(state: &mut State, i: usize) {
    let Some(k) = operation::KindInfo::ALL.get(i) else {
        debug_assert!(false, "toggle_pin({i}) past the kind table");
        eprintln!("ribbon: unknown op kind {i}");
        return;
    };
    let mut pins = camcraft_app::ribbon::pinned_of(&state.world);
    let verb = match pins.iter().position(|p| *p == i) {
        Some(at) => {
            pins.remove(at);
            "unpinned"
        }
        None => {
            pins.push(i);
            "pinned"
        }
    };
    camcraft_app::ribbon::pinned_set(&state.world, &pins);
    state.status = format!("{verb} {} on the ribbon", k.caption);
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

/// One folded section's dropdown, pure geometry (M6.2): the More button
/// that owns it and its hidden kinds' row rects, all in GLOBAL
/// coordinates - so the layout math is testable (and the reach of every
/// kind provable) without a frame, and the draw only relocates it into
/// its Area. Rows are in registry order, the same order the section
/// drew before it folded.
pub(super) struct Drop {
    pub section: operation::Section,
    pub button: Rect,
    pub rows: Vec<(Cmd, Rect)>,
}

/// The dropdown's own rhythm (ribbon constants law): wide enough for
/// the longest registry label, rows one theme row tall.
/// What one layout pass answers: cells to draw, group captions (mid x,
/// title), divider x's, and the doors with their dropdown rows.
pub(super) type RibbonGeom = (Vec<Cell>, Vec<(f32, &'static str)>, Vec<f32>, Vec<Drop>);
const DROP_W: f32 = 200.0;
const DROP_GAP: f32 = 2.0;

/// The ribbon's layout math, pure: a plan (ribbon_plan's groups) and the
/// strip give back cell rects, group captions (centre x, title), the
/// divider x's between groups, and one `Drop` per folded section.
/// `ribbon` draws exactly this, and the tests click through it - one
/// source of truth for where each command lives. FLOOR_W/GAP/PAD_H are
/// ribbon.rs's; the cell box is the theme's (Tokens::cell_w/cell_h).
///
/// The width law (M6.2): cells share the strip's width - a flow, not
/// imgui's fixed pitch - and when the fair share drops under FLOOR_W
/// the section with the largest inline remainder folds its last cell
/// into a dropdown (ties go to the earlier section), repeating until
/// every remaining cell can render legibly. The fold is width-driven
/// and nothing else: a wide window never folds, and the first fold of
/// a group is width-neutral (the cell leaves, the labeled More button
/// arrives), which bounds the loop. Anchors and Pinned never fold; if
/// even those cannot fit, the row overflows on purpose rather than
/// rendering illegible cells, as before.
pub(super) fn ribbon_cells(strip: Rect, pal: &Tokens, plan: &[Group]) -> RibbonGeom {
    const FLOOR_W: f32 = 40.0;
    // Each group's inline remainder and folded tail; plans start fully
    // inline, the loop below moves cells across the seam.
    let mut groups: Vec<(&Group, Vec<Cmd>, Vec<Cmd>)> = plan
        .iter()
        .map(|g| (g, g.cells.clone(), Vec::new()))
        .collect();
    let n_groups = groups.len();
    loop {
        let n: usize = groups
            .iter()
            .map(|(_, shown, hidden)| shown.len() + usize::from(!hidden.is_empty()))
            .sum();
        if n == 0 || n_groups == 0 {
            break;
        }
        let overhead = 2.0 * PAD_H
            + (n - n_groups) as f32 * GAP
            + (n_groups - 1) as f32 * (GAP + 2.0);
        if (strip.width() - overhead) / n as f32 >= FLOOR_W {
            break;
        }
        // Fold the largest inline remainder among the section groups
        // (the fair share says where the crowd is); ties go to the
        // earlier section, so the fold order is a function of the plan
        // and the width, never of iteration history.
        let victim = groups
            .iter()
            .enumerate()
            .filter(|(_, (g, shown, _))| g.section.is_some() && !shown.is_empty())
            .max_by_key(|(gi, (_, shown, _))| (shown.len(), std::cmp::Reverse(*gi)))
            .map(|(gi, _)| gi);
        let Some(gi) = victim else { break };
        // The victim filter kept `shown` non-empty, so the pop lands;
        // an empty pop would mean the filter broke - name it, stop
        // folding, never lose a cell silently (Prime Directive).
        match groups[gi].1.pop() {
            Some(cmd) => groups[gi].2.insert(0, cmd),
            None => {
                debug_assert!(false, "ribbon fold law: empty remainder chosen");
                eprintln!("ribbon: internal layout error, folding stopped");
                break;
            }
        }
    }
    let n_cells: usize = groups
        .iter()
        .map(|(_, shown, hidden)| shown.len() + usize::from(!hidden.is_empty()))
        .sum();
    let overhead = 2.0 * PAD_H
        + (n_cells - n_groups) as f32 * GAP
        + (n_groups - 1) as f32 * (GAP + 2.0);
    let cell_w = ((strip.width() - overhead) / n_cells.max(1) as f32).clamp(FLOOR_W, pal.cell_w);
    // PAD_V (ribbon.rs): cells start 4 below the strip top.
    let cell_top = strip.top() + 4.0;
    let mut cells = Vec::new();
    let mut captions = Vec::new();
    let mut dividers = Vec::new();
    let mut drops = Vec::new();
    let mut x = strip.left() + PAD_H;
    let mut salt = 0usize;
    for (gi, (group, shown, hidden)) in groups.iter().enumerate() {
        let start = x;
        let mut draw = |cmd: Cmd, x: &mut f32, cells: &mut Vec<Cell>| {
            cells.push(Cell {
                cmd,
                rect: Rect::from_min_size(
                    egui::Pos2::new(*x, cell_top),
                    Vec2::new(cell_w, pal.cell_h),
                ),
                salt: {
                    salt += 1;
                    salt - 1
                },
            });
            *x += cell_w + GAP;
        };
        for c in shown {
            draw(*c, &mut x, &mut cells);
        }
        // Only section groups ever accumulate a hidden tail (the fold
        // loop touches nothing else), so the `section` test is a type
        // guard, not a second opinion.
        if let (Some(section), true) = (group.section, !hidden.is_empty()) {
            // The section's door stands at the end of its inline run,
            // and owns the dropdown of everything behind it.
            let button = Rect::from_min_size(
                egui::Pos2::new(x, cell_top),
                Vec2::new(cell_w, pal.cell_h),
            );
            let row_h = pal.row_height;
            let rows = hidden
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    (
                        *c,
                        Rect::from_min_size(
                            egui::Pos2::new(
                                button.left(),
                                button.bottom() + DROP_GAP + k as f32 * row_h,
                            ),
                            Vec2::new(DROP_W, row_h),
                        ),
                    )
                })
                .collect();
            drops.push(Drop {
                section,
                button,
                rows,
            });
            cells.push(Cell {
                cmd: Cmd::More(section),
                rect: button,
                salt: {
                    salt += 1;
                    salt - 1
                },
            });
            x += cell_w + GAP;
        }
        x -= GAP;
        captions.push(((start + x) * 0.5, group.title));
        if gi + 1 < n_groups {
            x += GAP + 2.0;
            dividers.push(x - (GAP + 2.0) / 2.0 - 1.0);
        }
    }
    (cells, captions, dividers, drops)
}

/// The ribbon strip: painted cells over a fixed-height top panel. Group
/// captions and hairline dividers match imgui's layout math, the cells
/// themselves are painted::cell - all from `ribbon_cells`'s geometry.
/// Since M-g each cell carries its bundled glyph (the retired imgui
/// ribbon's Item.icon table), or the painted fallback headless.
pub fn ribbon(
    ui: &mut Ui,
    state: &mut State,
    pal: &Tokens,
    icons: Option<&IconSet>,
) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    let base = ui.id().with("ribbon");
    // The plan is read fresh every frame from the two facts it is a
    // function of - the document's machine and its pins (M6.2). No
    // cached shape: a machine mount or an unpin cannot leave a stale
    // group standing.
    let machine = camcraft_app::machine::kind_of(&state.world);
    let plan = ribbon_plan(machine, &camcraft_app::ribbon::pinned_of(&state.world));
    let (cells, captions, dividers, drops) = ribbon_cells(rect, pal, &plan);
    // Self-heal first: a folded-away door (window widened, pin moved a
    // cell inline, document swapped) must not leave `ribbon_more`
    // naming a button that no longer exists.
    if let Some(s) = state.ribbon_more
        && !cells.iter().any(|cell| cell.cmd == Cmd::More(s))
    {
        state.ribbon_more = None;
    }
    let before = state.ribbon_more;
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
            icon_of(c).and_then(|i| icons.and_then(|s| s.texture(i))),
        );
        // The pin gesture lives at the cell, not in a command: the
        // right-click that answers it names itself in every live tip
        // (tip_of), and its law - layout rides the document, undo
        // replays cuts - is ROADMAP M6.2's, recorded in RECOMMENDED.md.
        // Read before `on_hover_text` moves the response into the
        // tooltip wiring.
        let fired = resp.clicked() && enabled(c, state);
        let pin = resp.secondary_clicked();
        resp.on_hover_text(tip_of(c, state));
        if fired {
            run_cmd(c, state);
        }
        if let (Cmd::Create(i), true) = (c, pin) {
            toggle_pin(state, i);
        }
    }
    for (mid, title) in captions {
        // The caption row sits under its cells' row: PAD_V (4) + the
        // cell box + 5 - 44 at the defaults, one derivation now that
        // cell_h is a token.
        painted::caption(ui.painter(), mid, rect.top() + 4.0 + pal.cell_h + 5.0, title, pal);
    }
    for dx in dividers {
        painted::divider(ui.painter(), dx, rect, pal);
    }
    // The open door's dropdown: one Area at foreground order (egui's
    // popup layer), anchored under its button, its rows the section's
    // hidden kinds in registry
    // order. Geometry arrives global (ribbon_cells); the Area-local
    // rect is the global one minus the panel origin - one pair of
    // numbers, no second layout.
    if let Some(s) = state.ribbon_more {
        // The self-heal above proved a door is on the ribbon; finding
        // none means that proof broke, so the state drops itself -
        // named, printed, never a silent unwrap.
        match drops.iter().find(|d| d.section == s) {
            None => {
                debug_assert!(false, "ribbon: open door has no dropdown");
                eprintln!("ribbon: closing a dropdown whose button is gone");
                state.ribbon_more = None;
            }
            Some(drop) => {
                // Every rect here is global: ribbon_cells computed the
                // button and row rects in the strip's global frame, and
                // egui paints and hit-tests this ui in global coords
                // too. The Area adds only the layer - Foreground order,
                // so a press on a row belongs to the row, never to the
                // cell it happens to cover.
                let panel = Rect::from_min_size(
                    drop.button.left_bottom() + Vec2::new(0.0, DROP_GAP),
                    Vec2::new(DROP_W, drop.rows.len() as f32 * pal.row_height),
                );
                let mut hit: Option<Cmd> = None;
                let mut pinned_row: Option<usize> = None;
                let rows = &drop.rows;
                egui::Area::new(base.with(("drop", drop.section.label())))
                    .fixed_pos(panel.min)
                    .order(egui::Order::Foreground)
                    .movable(false)
                    .show(ui.ctx(), |ui| {
                        let p = ui.painter();
                        p.rect_filled(panel, pal.radius_menu, pal.ink);
                        p.rect_stroke(
                            panel,
                            pal.radius_menu,
                            egui::Stroke::new(1.0, pal.border),
                            egui::StrokeKind::Outside,
                        );
                        for (k, (cmd, r)) in rows.iter().enumerate() {
                            let resp = ui.interact(*r, base.with(("drop-row", k)), Sense::click());
                            if resp.hovered() {
                                p.rect_filled(*r, 0.0, pal.field_hover);
                            }
                            let en = enabled(*cmd, state);
                            let label = match *cmd {
                                Cmd::Create(i) => TYPES.get(i).copied().unwrap_or("op"),
                                other => label_of(other),
                            };
                            p.text(
                                r.left_center() + Vec2::new(8.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                label,
                                egui::FontId::proportional(pal.font_row),
                                if en { pal.text } else { pal.text_faint },
                            );
                            if resp.secondary_clicked()
                                && let Cmd::Create(i) = *cmd
                            {
                                pinned_row = Some(i);
                            }
                            if resp.clicked() && en {
                                hit = Some(*cmd);
                            }
                        }
                    });
                if let Some(i) = pinned_row {
                    toggle_pin(state, i);
                }
                if let Some(c) = hit {
                    run_cmd(c, state);
                    state.ribbon_more = None;
                } else if state.ribbon_more == before {
                    // Primary press anywhere outside the door and its
                    // rows closes it (egui's popup law, hand-drawn
                    // because the panel is geometry, not a Widget). A
                    // press that already toggled a door this frame
                    // (`state.ribbon_more != before`) is the new
                    // door's own business.
                    let outside = ui.input(|i| {
                        i.pointer.button_pressed(egui::PointerButton::Primary)
                            && i.pointer.interact_pos().is_none_or(|p| {
                                !drop.button.expand(DROP_GAP).contains(p)
                                    && !drop.rows.iter().any(|(_, r)| r.contains(p))
                            })
                    });
                    if outside {
                        state.ribbon_more = None;
                    }
                }
            }
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
pub fn status(ui: &mut Ui, state: &mut State, pal: &Tokens) {
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
        ToolKind::Turning => "turning insert",
        ToolKind::Groove => "grooving blade",
        ToolKind::Thread => "threading insert",
        ToolKind::Form => "form gauge",
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
    pal: &Tokens,
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
        Vec2::new(ui.available_width(), rows.len() as f32 * pal.row_height),
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

