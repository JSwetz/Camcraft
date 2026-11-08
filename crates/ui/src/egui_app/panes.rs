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
use camcraft_app::operation::{self, OpType};
use camcraft_app::schema::WCS_SLOTS;
use camcraft_app::tools::ToolKind;

use crate::actions;
use crate::state::{CommandKind, DisplayMode, Sim, State, TreeMenuTarget, UiTarget};

use super::icons::{Icon, IconSet};
use super::layout;
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
    let (modifiers, n, o, s, p, q, z, y, del, f, c, v) = ui.input(|i| {
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
            i.key_pressed(Key::C),
            i.key_pressed(Key::V),
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
        if c && let Some(name) = state.selected_operation() {
            actions::copy_operation(state, &name);
        }
        if v {
            actions::paste_operation(state);
        }
        // The File tips' keys (ribbon.rs strings): N new, O a model,
        // Shift+O a project, S save, Shift+S save as.
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
            if modifiers.shift {
                actions::save_project_as(state);
            } else {
                actions::save_project(state);
            }
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
    SaveAs,
    Post,
    Stock,
    Origin,
    Tools,
    /// The machine door (the ribbon law): one cell that opens the
    /// three workspaces - Milling, Turning, Mill-turning - and wears
    /// the current one as its caption. Switching mounts the kind's
    /// built-in reference through `LoadBuiltinMachine`, so the change
    /// rides undo like every other setup move.
    Machine,
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
    /// The View door: one cell whose list toggles the dock's closable
    /// panels and resets the layout (layout.rs). View state, no command.
    View,
}

/// The four anchor groups: file work opens the row, setup follows, and
/// the run/judge groups close it. The middle is the registry's (below):
/// the old hand-typed eight-cell Create group was M1.4.3's debt, home
/// to kinds 0..=7 while the table grew - the registry counts the
/// kinds, this comment never must.
const FILE: (&str, &[Cmd]) =
    ("File", &[Cmd::New, Cmd::Model, Cmd::Project, Cmd::Save, Cmd::SaveAs, Cmd::Post]);
const SETUP: (&str, &[Cmd]) =
    ("Setup", &[Cmd::Stock, Cmd::Origin, Cmd::Tools, Cmd::Machine]);

/// The bar's whole op-shortcut budget (the ribbon law): at most this
/// many Create cells stand inline, and they are the user's pins. The
/// rest of the catalog lives behind the section doors at every width.
const MAX_PINS: usize = 4;
const MODIFY: (&str, &[Cmd]) = ("Modify", &[Cmd::Undo, Cmd::Redo, Cmd::Run, Cmd::Delete]);
const INSPECT: (&str, &[Cmd]) =
    ("Inspect", &[Cmd::Frame, Cmd::Paths, Cmd::Simulate, Cmd::Deviate]);
const VIEW: (&str, &[Cmd]) = ("View", &[Cmd::View]);

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
/// Turning cells to grey out; `KindInfo::admits` is the single word
/// this filter and the command door share (M13.5 B: per kind, so the
/// lathe's Drilling door fronts the one axial Drill cell). The
/// registry's `section` column is the only seed, and the row order is
/// the product order, so a section's cells list the kinds in table
/// sequence.
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
    // it again). These are the bar's only inline op cells, and their
    // number is its budget: the promise of a pin is that it stands
    // where the user put it, so the fold law starts Pinned inline,
    // unfolded, at most MAX_PINS deep.
    let pins: Vec<usize> = pinned
        .iter()
        .copied()
        .filter(|i| {
            operation::KindInfo::ALL
                .get(*i)
                .is_some_and(|k| k.admits(machine))
        })
        // The bar's op budget (the ribbon law): the first MAX_PINS
        // admitted pins stand inline. A document whose list outgrew
        // the cap - pinned on an earlier build, or one that ignored
        // it - still renders, it just shows the first four; the rest
        // wait behind their section doors, listed and reachable.
        .take(MAX_PINS)
        .collect();
    if !pins.is_empty() {
        plan.push(Group {
            title: "Pinned",
            cells: pins.iter().copied().map(Cmd::Create).collect(),
            section: None,
        });
    }
    plan.extend(operation::shown_for(machine).into_iter().filter_map(|s| {
        // One home per kind (the ribbon law's corollary): a pinned
        // kind stands on the bar and leaves its section's list - the
        // door holds "everything of this family not already in
        // reach", never a duplicate of what the user can see.
        let cells: Vec<Cmd> = s
            .kinds_for(machine)
            .into_iter()
            .filter(|i| !pins.contains(i))
            .map(Cmd::Create)
            .collect();
        (!cells.is_empty()).then_some(Group {
            title: s.label(),
            cells,
            section: Some(s),
        })
    }));
    plan.push(Group { title: MODIFY.0, cells: MODIFY.1.to_vec(), section: None });
    plan.push(Group { title: INSPECT.0, cells: INSPECT.1.to_vec(), section: None });
    plan.push(Group { title: VIEW.0, cells: VIEW.1.to_vec(), section: None });
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
        Cmd::Project => "Open",
        Cmd::Save => "Save",
        Cmd::SaveAs => "Save As",
        Cmd::Post => "Post",
        Cmd::Stock => "Stock",
        Cmd::Origin => "Origin",
        Cmd::Tools => "Tools",
        Cmd::Machine => "Machine",
        Cmd::Create(_) => "op", // formatted by the group row; ribbon shows TYPES
        Cmd::Undo => "Undo",
        Cmd::Redo => "Redo",
        Cmd::Run => "Run",
        Cmd::Delete => "Delete",
        Cmd::Frame => "Frame",
        Cmd::Paths => "Paths",
        Cmd::Simulate => "Simulate",
        Cmd::Deviate => "Deviate",
        Cmd::View => "Panels",
        Cmd::More(s) => s.label(),
    }
}

/// The machine mode's caption: the Title-case word the door wears,
/// read off the switcher's own table so cell and rows cannot disagree.
fn mode_caption(k: camcraft_app::machine::Kind) -> &'static str {
    actions::MACHINE_MODES
        .iter()
        .find(|(m, _, _)| *m == k)
        .map_or("Machine", |(_, c, _)| *c)
}

fn text_of(c: Cmd, state: &State) -> String {
    match c {
        // The overflow button wears the section's name plus an
        // ellipsis - "Milling 3D…" reads as "the rest of this is in
        // here", which is exactly the law the dropdown keeps (M6.2:
        // a labeled door, never a faceless chevron). The machine door
        // wears the same invitation: the ellipsis says "a list opens".
        Cmd::More(s) => format!("{}…", s.label()),
        Cmd::Machine => {
            format!("{}…", mode_caption(camcraft_app::machine::kind_of(&state.world)))
        }
        Cmd::View => format!("{}…", label_of(c)),
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
        Cmd::SaveAs => "Save the project under a new name\tCtrl+Shift+S".into(),
        Cmd::Post => "Post NC program (validation gate), then review it\tCtrl+P".into(),
        Cmd::Stock => "Stock definition".into(),
        Cmd::Origin => "Work offset (WCS)".into(),
        Cmd::Tools => "Tool library".into(),
        // The cell's caption already says which workspace is running;
        // the tip says what a click does, and how to undo it.
        Cmd::Machine => "Switch the workspace: Milling, Turning or Mill-turning (mounts the kind's built-in machine, undoable)".into(),
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
                    .is_some_and(|k| !k.admits(machine));
                if dark {
                    let (fword, mword) = match camcraft_app::operation::KindInfo::ALL[i].family() {
                        camcraft_app::operation::Family::Mill => ("milling", "mill"),
                        camcraft_app::operation::Family::Turn => ("turning", "lathe"),
                    };
                    format!(
                        "The document's machine is a {} - {fword} operations do not drive it; mount a {mword} or a mill-turn (Setup > Machine)",
                        machine.word()
                    )
                } else if let Some(reason) = tool_law_note(i, state) {
                    reason
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
        Cmd::Paths => if state.show_all_paths {
            "Hide every toolpath".into()
        } else {
            "Show every cached toolpath".into()
        },
        Cmd::Simulate => "Simulate tool moves (transport bar)".into(),
        Cmd::Deviate => {
            "Judge the program's stock against the model (deviation map)".into()
        }
        Cmd::View => "Show or hide the browser and properties panes, or reset the layout".into(),
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
                .is_none_or(|k| k.admits(machine));
            // The tool law (M10.1 B): darken, never hide - the cell
            // stays to explain itself, the fix is a library click away.
            kind_admits && !state.tool_names().is_empty() && tool_law_note(i, state).is_none()
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

/// The refusal a gated Create answer - a cell on the bar or a row
/// behind a door - speaks when it is clicked anyway: named, loud on
/// the status line, pointing at the fix. It reads the same two truths
/// `enabled` reads, in the same order of depth (no tool first, then
/// the machine's family), phrased as `actions::add_operation` phrases
/// its own refusal. Silence on a click reads as a dead button - the
/// law `State::apply_operation_params` already keeps for the pane -
/// so a gate that can be clicked must be able to answer. `None` when
/// the command is actually live, so no caller overwrites a status an
/// enabled command already owns.
fn gate_note(c: Cmd, state: &State) -> Option<String> {
    let Cmd::Create(i) = c else { return None };
    if enabled(c, state) {
        return None;
    }
    if state.tool_names().is_empty() {
        return Some(String::from(
            "cannot add an operation: no tool in the document - add one (Setup > Tools)",
        ));
    }
    let machine = camcraft_app::machine::kind_of(&state.world);
    let dark = camcraft_app::operation::KindInfo::ALL
        .get(i)
        .filter(|k| !k.admits(machine));
    match dark {
        Some(k) => {
            let (fword, mword) = match k.family() {
                camcraft_app::operation::Family::Mill => ("milling", "mill"),
                camcraft_app::operation::Family::Turn => ("turning", "lathe"),
            };
            Some(format!(
                "cannot add a {} operation: {fword} operations do not drive the document's {} - mount a {mword} or a mill-turn (Setup > Machine)",
                k.label,
                machine.word(),
            ))
        }
        None => tool_law_note(i, state).map(|r| {
            let label = camcraft_app::operation::KindInfo::ALL.get(i).map_or("", |k| k.label);
            format!("cannot add a {label} operation: {r}")
        }),
    }
}

/// Where a refused Create click leads (#191): the place that fixes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GateRoute {
    /// No tool, or none that fits the kind: the Tool library page.
    Tools,
    /// The mounted machine cannot run the kind's family: the machine
    /// door's workspace list.
    Machine,
}

/// The fix for a dark Create cell, when it has one ([`gate_note`]'s
/// cases, in its order).
pub(super) fn gate_route(c: Cmd, state: &State) -> Option<GateRoute> {
    let Cmd::Create(i) = c else { return None };
    if enabled(c, state) {
        return None;
    }
    if state.tool_names().is_empty() {
        return Some(GateRoute::Tools);
    }
    let machine = camcraft_app::machine::kind_of(&state.world);
    if camcraft_app::operation::KindInfo::ALL.get(i).is_some_and(|k| !k.admits(machine)) {
        return Some(GateRoute::Machine);
    }
    tool_law_note(i, state).map(|_| GateRoute::Tools)
}

/// A dark Create cell was clicked anyway (#191): say why on the status
/// line, then take the user to the fix rather than leave them at the
/// error - the Tool library, or the machine door opened on its list.
/// View state only; nothing is created or queued.
pub(super) fn follow_gate(c: Cmd, state: &mut State) {
    let Some(why) = gate_note(c, state) else { return };
    match gate_route(c, state) {
        Some(GateRoute::Tools) => {
            actions::target(state, UiTarget::Tool(String::new()));
            state.status = format!("{why} - opened the tool library");
        }
        Some(GateRoute::Machine) => {
            state.ribbon_more = None;
            state.ribbon_view_open = false;
            state.ribbon_mode_open = true;
            state.status = format!("{why} - pick a workspace");
        }
        None => state.status = why,
    }
}

/// The tool law's darkening (M10.1 B): the app's own refusal sentence
/// for creating kind `i` against the active setup's tools, or `None`
/// when a fitting tool is there. The predicate lives in app (the same
/// one `CreateOperation` enforces); the ribbon only renders it.
fn tool_law_note(i: usize, state: &State) -> Option<String> {
    camcraft_app::operation::KindInfo::ALL
        .get(i)
        .and_then(|k| camcraft_app::operation::tool_gate::create_refusal(&state.world, k))
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
        Cmd::Machine => state.ribbon_mode_open,
        Cmd::View => state.ribbon_view_open,
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
        Cmd::SaveAs => Some(Icon::Save),
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
        // The machine door carries no glyph either (the no-new-asset
        // law): its caption *is* the machine, and every word of it
        // names a workspace - an icon would repeat the cell's own text.
        Cmd::Machine => None,
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
        // The machine door's no-new-asset law: the caption names the
        // door, the ellipsis says a list opens.
        Cmd::View => None,
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
        // The roughing twins ARE the pocket ladder under two catalog
        // names (M2.4 A) - the Pocket3D precedent says: same word,
        // same pixels.
        OpType::Adaptive2D | OpType::Offset2D => Icon::Pocket,
        OpType::Slot => Icon::Slot,
        OpType::Trace => Icon::Trace,
        // The 2D run's new names keep their family's pixels (M2.4 B,
        // the twins' reuse law): the engrave twin IS Trace's field
        // shape, the chamfer ride is a contour peeling a derived
        // corner, and the circular ladder is the pocket family
        // around a centre.
        OpType::Engrave => Icon::Trace,
        OpType::Chamfer2D => Icon::Contour,
        // The deburr ride is the chamfer's word read on the model's
        // own edges - same promise, a loop of peeled corner, so the
        // contour's pixels again (M2.5 E B, the reuse law).
        OpType::Deburr => Icon::Contour,
        OpType::Circular => Icon::Pocket,
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
        // The standalone boring ride lives in the drilling section -
        // the drill's pixels, same word about a hole (M2.4 C, the
        // twins' reuse law). The thread mill lives in the 2D mill's
        // run and is a contour that closes on itself one pitch at a
        // time - the contour's pixels, a circle peeled to a thread
        // (the Chamfer2D precedent).
        OpType::Bore => Icon::Drill,
        OpType::Thread => Icon::Contour,
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
        Cmd::SaveAs => actions::save_project_as(state),
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
        Cmd::Paths => state.toggle_all_toolpaths(),
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
        // and clicking the open door again closes it. One dropdown at
        // a time: opening either closes the other.
        Cmd::More(s) => {
            state.ribbon_more = (state.ribbon_more != Some(s)).then_some(s);
            state.ribbon_mode_open = false;
            state.ribbon_view_open = false;
        }
        Cmd::Machine => {
            state.ribbon_mode_open = !state.ribbon_mode_open;
            if state.ribbon_mode_open {
                state.ribbon_more = None;
                state.ribbon_view_open = false;
            }
        }
        // Same door law; the dock itself is the shell's (layout.rs).
        Cmd::View => {
            state.ribbon_view_open = !state.ribbon_view_open;
            if state.ribbon_view_open {
                state.ribbon_more = None;
                state.ribbon_mode_open = false;
            }
        }
    }
}

/// The pin gesture (M6.2): toggle one kind's presence in the document's
/// pinned list. The pins are saved with the project, so the toggle is a
/// `SetRibbonPins` command on the history (issue #143): Ctrl+Z puts the
/// previous pins back and the document counts as changed. The status
/// line says what happened, because a silent toggle is a half-answer.
pub(super) fn toggle_pin(state: &mut State, i: usize) {
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
        // The bar's op budget: a fifth pin is refused by name, not
        // silently dropped (Prime Directive - the gesture asked for
        // something the law declines, so the law says so).
        None if pins.len() >= MAX_PINS => {
            state.status = format!(
                "the ribbon keeps {MAX_PINS} pins - unpin one first (right-click a pinned cell)"
            );
            return;
        }
        None => {
            pins.push(i);
            "pinned"
        }
    };
    match camcraft_app::commands::SetRibbonPins::new(&state.world, &pins) {
        Ok(command) => {
            state.history.execute(&state.world, command);
            state.status = format!("{verb} {} on the ribbon", k.caption);
        }
        // Unreachable from the toggle (it always changes the list), but a
        // refusal is named, never swallowed.
        Err(err) => state.status = format!("ribbon pin not changed: {err}"),
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

/// The machine door's dropdown rows, pure geometry (the section doors'
/// `Drop` law, worn by the switcher): the three workspaces in switcher
/// order, one theme row tall each, hung under the door button in the
/// strip's global frame. The tests click the switch through exactly
/// these rects - one source of truth for where each mode lives.
pub(super) fn machine_rows(
    button: Rect,
    row_h: f32,
) -> Vec<(camcraft_app::machine::Kind, Rect)> {
    actions::MACHINE_MODES
        .iter()
        .enumerate()
        .map(|(k, (kind, _, _))| {
            (
                *kind,
                Rect::from_min_size(
                    egui::Pos2::new(
                        button.left(),
                        button.bottom() + DROP_GAP + k as f32 * row_h,
                    ),
                    Vec2::new(DROP_W, row_h),
                ),
            )
        })
        .collect()
}
pub(super) const DROP_W: f32 = 200.0;
pub(super) const DROP_GAP: f32 = 2.0;

/// The ribbon's layout math, pure: a plan (ribbon_plan's groups) and the
/// strip give back cell rects, group captions (centre x, title), the
/// divider x's between groups, and one `Drop` per folded section.
/// `ribbon` draws exactly this, and the tests click through it - one
/// source of truth for where each command lives. FLOOR_W/GAP/PAD_H are
/// ribbon.rs's; the cell box is the theme's (Tokens::cell_w/cell_h).
///
/// The fold law (the ribbon law, retired M6.2's width law): a
/// section's kinds live behind their labeled door AT EVERY WIDTH -
/// the bar's inline op cells are exactly the pinned ones, at most
/// MAX_PINS of them. A crowd that only folds when squeezed is still a
/// crowd, and the user asked for this one directly. Cells share the
/// strip's width - a flow, not imgui's fixed pitch - and anchors
/// never fold; if the anchors and doors cannot fit, the row overflows
/// on purpose (clamped at FLOOR_W) rather than rendering a narrower
/// lie. Every section's door owns its full list; the layout draws
/// the door where the run of cells would have stood.
pub(super) fn ribbon_cells(strip: Rect, pal: &Tokens, plan: &[Group]) -> RibbonGeom {
    const FLOOR_W: f32 = 40.0;
    // Each group's inline run and folded tail: sections start fully
    // folded, anchors fully inline - the split IS the law, no loop
    // decides it.
    let groups: Vec<(&Group, Vec<Cmd>, Vec<Cmd>)> = plan
        .iter()
        .map(|g| match g.section {
            Some(_) => (g, Vec::new(), g.cells.clone()),
            None => (g, g.cells.clone(), Vec::new()),
        })
        .collect();
    let n_groups = groups.len();
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
        // Only section groups ever carry a hidden tail (the fold law
        // starts them folded and folds nothing else), so the `section`
        // test is a type guard, not a second opinion.
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
            &text_of(c, state),
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
        // A dark Create cell that gets clicked is not a dead pixel:
        // the refusal speaks (gate_note), the same answer the
        // dropdown rows give. Hover already taught it; the click
        // proves it.
        let gated = resp.clicked() && !fired && matches!(c, Cmd::Create(_));
        let pin = resp.secondary_clicked();
        resp.on_hover_text(tip_of(c, state));
        if fired {
            run_cmd(c, state);
        }
        if gated {
            follow_gate(c, state);
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
                let mut gated_row: Option<Cmd> = None;
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
                            // Read the click flags before
                            // `on_hover_text` moves the response -
                            // the cell law again, one line lower. The
                            // hint on a row is the bar's tip for the
                            // same command: what it creates, or why
                            // it cannot yet.
                            let clicked = resp.clicked();
                            let secondary = resp.secondary_clicked();
                            resp.on_hover_text(tip_of(*cmd, state));
                            if secondary
                                && let Cmd::Create(i) = *cmd
                            {
                                pinned_row = Some(i);
                            }
                            if clicked {
                                if en {
                                    hit = Some(*cmd);
                                } else {
                                    gated_row = Some(*cmd);
                                }
                            }
                        }
                    });
                if let Some(i) = pinned_row {
                    toggle_pin(state, i);
                }
                if let Some(c) = hit {
                    run_cmd(c, state);
                    state.ribbon_more = None;
                } else if let Some(c) = gated_row {
                    // The dark row answered: the refusal named on the
                    // status line, the door closed on the way out. A
                    // click into silence is what made the folded
                    // ribbon read as dead - this branch is why it
                    // never is.
                    state.ribbon_more = None;
                    follow_gate(c, state);
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
    // The machine door's dropdown (the ribbon law): the three
    // workspaces, the current one marked and inert, the others
    // mounting their kind's library reference - through the queue,
    // so the switch rides undo. Same hand-drawn popup law as the
    // section doors: foreground order, outside press closes it.
    if state.ribbon_mode_open {
        match cells.iter().find(|c| c.cmd == Cmd::Machine) {
            None => {
                // The anchor law says the door stands on the bar every
                // frame; finding none means that broke - named, not
                // silent, and the state drops itself.
                debug_assert!(false, "ribbon: machine door missing from the bar");
                eprintln!("ribbon: closing a mode list whose button is gone");
                state.ribbon_mode_open = false;
            }
            Some(cell) => {
                let button = cell.rect;
                let rows = machine_rows(button, pal.row_height);
                let machine = camcraft_app::machine::kind_of(&state.world);
                let mut pick: Option<camcraft_app::machine::Kind> = None;
                let mut chose = false;
                let panel = Rect::from_min_size(
                    button.left_bottom() + Vec2::new(0.0, DROP_GAP),
                    Vec2::new(DROP_W, rows.len() as f32 * pal.row_height),
                );
                egui::Area::new(base.with("machine-drop"))
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
                        for (k, (kind, r)) in rows.iter().enumerate() {
                            let resp =
                                ui.interact(*r, base.with(("mode-row", k)), Sense::click());
                            if resp.hovered() {
                                p.rect_filled(*r, 0.0, pal.field_hover);
                            }
                            let current = *kind == machine;
                            let label = if current {
                                format!("{}  (current)", mode_caption(*kind))
                            } else {
                                mode_caption(*kind).to_owned()
                            };
                            p.text(
                                r.left_center() + Vec2::new(8.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                label,
                                egui::FontId::proportional(pal.font_row),
                                if current { pal.text_faint } else { pal.text },
                            );
                            // A press on the current row is a choice
                            // too - it closes the list, it just
                            // mounts nothing.
                            if resp.clicked() {
                                chose = true;
                                if !current {
                                    pick = Some(*kind);
                                }
                            }
                        }
                    });
                if chose {
                    if let Some(k) = pick {
                        actions::set_machine_mode(state, k);
                    }
                    state.ribbon_mode_open = false;
                } else {
                    let outside = ui.input(|i| {
                        i.pointer.button_pressed(egui::PointerButton::Primary)
                            && i.pointer.interact_pos().is_none_or(|p| {
                                !button.expand(DROP_GAP).contains(p)
                                    && !rows.iter().any(|(_, r)| r.contains(p))
                            })
                    });
                    if outside {
                        state.ribbon_mode_open = false;
                    }
                }
            }
        }
    }
    // The View door's dropdown (layout.rs): the panel toggles and the
    // layout reset, same popup law as the doors above.
    if state.ribbon_view_open {
        match cells.iter().find(|c| c.cmd == Cmd::View) {
            None => {
                debug_assert!(false, "ribbon: view door missing from the bar");
                eprintln!("ribbon: closing a view list whose button is gone");
                state.ribbon_view_open = false;
            }
            Some(cell) => {
                let rows = view_rows_of(cell.rect, rect, pal);
                layout::view_drop(ui, state, pal, base.with("view-drop"), cell.rect, &rows);
            }
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}

/// The View door's rows under `button` in `strip` - the one geometry the
/// draw and the tests share.
pub(super) fn view_rows_of(
    button: Rect,
    strip: Rect,
    pal: &Tokens,
) -> Vec<(layout::ViewRow, Rect)> {
    layout::view_rows(button, strip, pal.row_height, DROP_W, DROP_GAP)
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
        ui.add(Label::new(RichText::new(text.as_str()).weak()).selectable(true))
            .on_hover_text(text.as_str())
            .context_menu(|ui| {
                if ui.button("Copy message").clicked() {
                    ui.ctx().copy_text(text.clone());
                    ui.close();
                }
            });
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserSection {
    Setup,
    Models,
    Tools,
    Operations,
}

impl BrowserSection {
    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone)]
pub(crate) enum Leaf {
    Section,
    Parent(BrowserSection),
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
        ToolKind::Taper => "chamfer mill",
    }
}

/// The whole tree, built exactly like browser.rs: four sections,
/// per-group selection/visibility rules, the suffix column mirrored as
/// `suffix` (kind, cache tag, warning count - the one status channel the
/// column model lays out). Sections show a faint placeholder row when
/// empty, in place of imgui's help paragraph (the tree paints rows, not
/// wrapped text).
pub(crate) fn browser_rows(state: &State) -> (Vec<painted::Row>, Vec<Leaf>) {
    let mut rows = Vec::new();
    let mut leaves = Vec::new();
    let t = state.pane_target();

    let setup_open = state.browser_expanded[BrowserSection::Setup.index()];
    push_row(&mut rows, &mut leaves, "Setup".into(), String::new(), painted::Kind::Parent(setup_open), None, false, false, 0, Leaf::Parent(BrowserSection::Setup));
    if setup_open {
        // As before, the manager appears only after the document has an
        // explicit setup. Stock and WCS follow the active setup.
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
        let stock_hidden = state.hidden_stocks.contains(&stock_name);
        push_row(
            &mut rows,
            &mut leaves,
            stock_name,
            String::new(),
            painted::Kind::Stock,
            Some(Icon::Stock),
            matches!(t, UiTarget::Stock),
            stock_hidden,
            1,
            Leaf::Stock,
        );
        for (i, slot) in WCS_SLOTS.iter().enumerate() {
            let placed =
                camcraft_app::setup::wcs_entity(&state.world, active.as_deref(), i).is_some();
            let offset_name = camcraft_app::setup::wcs_entity_name(&state.world, active.as_deref(), i)
                .unwrap_or_else(|| (*slot).to_owned());
            push_row(
                &mut rows,
                &mut leaves,
                (*slot).to_owned(),
                if placed { String::new() } else { "not placed".into() },
                painted::Kind::Wcs,
                Some(Icon::WorkOffset),
                matches!(t, UiTarget::WcsSlot(j) if j == i),
                state.hidden_offsets.contains(&offset_name),
                1,
                Leaf::Wcs(i),
            );
        }
    }

    let models_open = state.browser_expanded[BrowserSection::Models.index()];
    push_row(&mut rows, &mut leaves, "Models".into(), String::new(), painted::Kind::Parent(models_open), None, false, false, 0, Leaf::Parent(BrowserSection::Models));
    if models_open {
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
    }

    let tools_open = state.browser_expanded[BrowserSection::Tools.index()];
    push_row(&mut rows, &mut leaves, "Tools".into(), String::new(), painted::Kind::Parent(tools_open), None, false, false, 0, Leaf::Parent(BrowserSection::Tools));
    if tools_open {
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
    }

    let operations_open = state.browser_expanded[BrowserSection::Operations.index()];
    push_row(&mut rows, &mut leaves, "Operations".into(), String::new(), painted::Kind::Parent(operations_open), None, false, false, 0, Leaf::Parent(BrowserSection::Operations));
    if operations_open {
        let ordered = camcraft_app::setup::operation_order(&state.world);
        if ordered.is_empty() {
            push_row(&mut rows, &mut leaves, "(no operations yet)".into(), String::new(), painted::Kind::Section, None, false, true, 1, Leaf::Section);
        }
        for (name, entity) in ordered {
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
            let suffix = match operation::tool_gate::tool_problem_of(&state.world, &entity) {
                Some(why) => format!("{kind}  BLOCKED{warn_text}  {why}"),
                None => format!("{kind}  {tag}{warn_text}"),
            };
            let hidden = state.hidden_paths.contains(&name);
            push_row(
                &mut rows, &mut leaves,
                model::operation_label(&state.world, &name, &entity), suffix,
                painted::Kind::Op, Some(icon),
                matches!(&t, UiTarget::Operation(n) if *n == name),
                hidden, 1, Leaf::Op(name),
            );
        }
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
    let mut show_all_paths = state.show_all_paths;
    if ui.checkbox(&mut show_all_paths, "Show all toolpaths").changed() {
        if show_all_paths {
            state.show_all_toolpaths();
        } else {
            state.hide_all_toolpaths();
        }
    }
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
    if let Some(ix) = hit.drag_started
        && let Some(Leaf::Op(name)) = leaves.get(ix)
    {
        state.dragging_op = Some(name.clone());
    }
    // The visual insertion line and the command use the same target row.
    // Only operations in the same setup can be drop targets.
    if let Some(source) = state.dragging_op.clone() {
        let (released, pointer) = ui.input(|i| (i.pointer.any_released(), i.pointer.hover_pos()));
        let drop = pointer.filter(|p| rect.contains(*p)).and_then(|p| {
            let ix = ((p.y - rect.top()) / pal.row_height) as usize;
            let Leaf::Op(target) = leaves.get(ix)? else { return None };
            let setup_of = |name: &str| model::find_by_name(&state.world, name)
                .and_then(|e| model::operation_of(&state.world, &e))
                .and_then(|op| camcraft_app::setup::effective(&state.world, op.setup.as_deref()));
            (source != *target && setup_of(&source) == setup_of(target)).then_some((ix, target.clone(), p.y > rect.top() + (ix as f32 + 0.5) * pal.row_height))
        });
        if let Some((ix, target, after)) = drop {
            let y = rect.top() + (ix + usize::from(after)) as f32 * pal.row_height;
            ui.painter().hline(rect.x_range(), y, egui::Stroke::new(2.0, pal.accent));
            if released {
                state.pending.push_back(CommandKind::ReorderOperation { name: source, target, after });
            }
        }
        if released {
            state.dragging_op = None;
        }
    }
    if let Some(ix) = hit.eye {
        match leaves[ix].clone() {
            Leaf::Model(n) => State::toggle_visibility(&mut state.hidden_models, &n),
            Leaf::Stock => {
                let name = camcraft_app::setup::stock_entity_name(
                    &state.world,
                    camcraft_app::setup::active(&state.world).as_deref(),
                );
                State::toggle_visibility(&mut state.hidden_stocks, &name);
            }
            Leaf::Wcs(i) => {
                if let Some(name) = camcraft_app::setup::wcs_entity_name(
                    &state.world,
                    camcraft_app::setup::active(&state.world).as_deref(),
                    i,
                ) {
                    State::toggle_visibility(&mut state.hidden_offsets, &name);
                }
            }
            Leaf::Op(n) => State::toggle_visibility(&mut state.hidden_paths, &n),
            _ => {}
        }
    }
    if let Some(ix) = hit.row {
        match leaves[ix].clone() {
            Leaf::Section => {}
            Leaf::Parent(section) => {
                let expanded = &mut state.browser_expanded[section.index()];
                *expanded = !*expanded;
            }
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
    // Operation rows offer the operation verbs. The Operations parent
    // offers Paste even when the list has no operation rows yet.
    if let Some(ix) = hit.menu {
        let target = match leaves.get(ix) {
            Some(Leaf::Op(name)) => Some(TreeMenuTarget::Operation(name.clone())),
            Some(Leaf::Parent(BrowserSection::Operations)) => Some(TreeMenuTarget::Operations),
            _ => None,
        };
        state.tree_menu = target.filter(|target| state.tree_menu.as_ref() != Some(target));
        state.retype_confirm = None;
    }
    tree_menu(ui, state, pal, rect, &leaves);
    ui.allocate_rect(rect, Sense::hover());
}

/// The tree menu's verb rows, keyed past every kind ordinal.
const MENU_CLEAR: usize = usize::MAX;
const MENU_DELETE: usize = usize::MAX - 1;
const MENU_RENAME: usize = usize::MAX - 2;
const MENU_COPY: usize = usize::MAX - 3;
const MENU_PASTE: usize = usize::MAX - 4;

/// The operation's retype menu (M9.1): what replaced the page's Type
/// combo. Grouping is the retired combo's law restated once -
/// `Section::ORDER` headers, `KindInfo::admits` hiding a section none
/// of whose kinds the machine can run (M13.5 B: the lathe's Drilling
/// header fronts the axial Drill alone, every other row of it dark),
/// and the current kind's section staying listed even when the
/// machine says no, so a mid-edit machine swap never erases the
/// operation's own name. A kind row needs a second click before it queues
/// `ChangeOperationKind` - one undo unit, the numbers carried by the
/// one retype law below; a dark row is queued too, because the refusal
/// belongs at the command door (`family_gate`), where it names itself
/// on the status line - never a silent dead pixel. Hand-drawn like the
/// ribbon's dropdowns (the panel is geometry, not a Widget): an Area
/// at Foreground order, so a press on a row belongs to the row, never
/// to the tree behind it; any other primary press closes it.
fn tree_menu(ui: &mut Ui, state: &mut State, pal: &Tokens, rect: Rect, leaves: &[Leaf]) {
    let Some(target) = state.tree_menu.clone() else {
        return;
    };
    let row_ix = leaves.iter().position(|leaf| match (&target, leaf) {
        (TreeMenuTarget::Operation(name), Leaf::Op(row)) => name == row,
        (TreeMenuTarget::Operations, Leaf::Parent(BrowserSection::Operations)) => true,
        _ => false,
    });
    let Some(ix) = row_ix else {
        state.tree_menu = None;
        state.retype_confirm = None;
        return;
    };
    let name = match &target {
        TreeMenuTarget::Operation(name) => {
            if model::find_by_name(&state.world, name)
                .and_then(|entity| model::operation_of(&state.world, &entity))
                .is_none()
            {
                state.tree_menu = None;
                state.retype_confirm = None;
                return;
            }
            Some(name.as_str())
        }
        TreeMenuTarget::Operations => None,
    };
    let here = name.and_then(|name| model::find_by_name(&state.world, name))
        .and_then(|entity| model::operation_of(&state.world, &entity))
        .map(|op| op.params.kind().ordinal());
    let machine = camcraft_app::machine::kind_of(&state.world);
    // The rows: section headers and kinds, in registry order, read
    // from the same taxonomy as the ribbon's cells - never a list.
    let mut lines: Vec<(&'static str, bool, Option<usize>)> = Vec::new();
    // The row's verbs first: clearing the generated path (keeps the
    // operation) and deleting the operation are two different asks,
    // and the menu names both - each one undoable command.
    lines.push(("Operation", false, None));
    if name.is_some() {
        lines.push(("Rename operation", true, Some(MENU_RENAME)));
        lines.push(("Copy operation", true, Some(MENU_COPY)));
    }
    lines.push(("Paste operation", state.operation_clipboard.is_some(), Some(MENU_PASTE)));
    if name.is_some() {
        lines.push(("Clear toolpath", true, Some(MENU_CLEAR)));
        lines.push(("Delete operation (undoable)", true, Some(MENU_DELETE)));
    }
    let here_section = here.and_then(|index| operation::KindInfo::ALL.get(index).map(|k| k.section));
    for s in operation::Section::ORDER.into_iter().filter(|_| name.is_some()) {
        // The current kind's own section keeps every row (dark where
        // the machine says no); any other section lists only what the
        // door admits, and leaves the menu when that is nothing.
        let rows = if here_section == Some(s) {
            s.kinds()
        } else {
            s.kinds_for(machine)
        };
        if rows.is_empty() {
            continue;
        }
        lines.push((s.label(), false, None));
        for i in rows {
            if let Some(k) = operation::KindInfo::ALL.get(i) {
                lines.push((k.label, k.admits(machine), Some(i)));
            }
        }
    }
    let anchor = egui::Pos2::new(
        rect.left(),
        rect.top() + (ix + 1) as f32 * pal.row_height + DROP_GAP,
    );
    let panel = Rect::from_min_size(
        anchor,
        Vec2::new(DROP_W, lines.len() as f32 * pal.row_height),
    );
    let mut chosen: Option<usize> = None;
    egui::Area::new(ui.id().with(("tree-menu", ix)))
        .fixed_pos(anchor)
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
            for (k, (label, runnable, item)) in lines.iter().enumerate() {
                let r = Rect::from_min_size(
                    egui::Pos2::new(anchor.x, anchor.y + k as f32 * pal.row_height),
                    Vec2::new(DROP_W, pal.row_height),
                );
                let is_current = item.is_some_and(|i| Some(i) == here);
                if is_current {
                    p.rect_filled(r, 0.0, pal.selection_bg);
                }
                let resp = item.map(|_| {
                    ui.interact(r, ui.id().with(("tree-menu-row", k)), Sense::click())
                });
                if let Some(resp) = &resp
                    && resp.hovered()
                    && !is_current
                {
                    p.rect_filled(r, 0.0, pal.field_hover);
                }
                let armed = name.is_some_and(|name| state.retype_confirm.as_ref()
                    .is_some_and(|(pending_name, pending_kind)|
                        pending_name == name && *item == Some(*pending_kind)));
                let text = if is_current {
                    format!("{label} (current)")
                } else if armed {
                    format!("Confirm change to {label}")
                } else {
                    (*label).to_owned()
                };
                p.text(
                    r.left_center() + Vec2::new(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &text,
                    egui::FontId::proportional(pal.font_row),
                    match item {
                        None => pal.text_dim,
                        Some(_) if !runnable || is_current => pal.text_faint,
                        Some(_) => pal.text,
                    },
                );
                if let Some(resp) = resp
                    && resp.clicked()
                    && (*runnable || *item != Some(MENU_PASTE))
                {
                    chosen = *item;
                }
            }
        });
    if let Some(i) = chosen {
        if i == MENU_PASTE {
            actions::paste_operation(state);
        } else if let Some(name) = name {
            if i == MENU_RENAME {
                let label = model::find_by_name(&state.world, name)
                    .map(|e| model::operation_label(&state.world, name, &e))
                    .unwrap_or_else(|| name.to_owned());
                state.rename_draft = Some((name.to_owned(), label));
                actions::target(state, UiTarget::Operation(name.to_owned()));
            } else if i == MENU_COPY {
                actions::copy_operation(state, name);
            } else if i == MENU_CLEAR {
                state.pending.push_back(CommandKind::ClearToolpath(name.to_owned()));
            } else if i == MENU_DELETE {
                state.pending.push_back(CommandKind::DeleteOperation(name.to_owned()));
            } else if Some(i) != here {
                let choice = (name.to_owned(), i);
                if state.retype_confirm.as_ref() == Some(&choice) {
                    state.pending.push_back(CommandKind::ChangeOperationKind {
                        name: name.to_owned(), kind: i,
                    });
                } else {
                    state.retype_confirm = Some(choice);
                    return;
                }
            }
        }
        state.tree_menu = None;
        state.retype_confirm = None;
    } else if ui.input(|i| {
        i.pointer.button_pressed(egui::PointerButton::Primary)
            && i.pointer.hover_pos().is_none_or(|pos| !panel.contains(pos))
    }) {
        // The dropdowns' close law: a primary press that is nobody's
        // row is the menu's - it never stacks and never stalks a
        // document the user has moved on from.
        state.tree_menu = None;
        state.retype_confirm = None;
    }
}

// --------------------------------------------------------------- parameters

