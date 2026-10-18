//! M-a: frame tests for the graduated shell - egui_kittest harnesses driving
//! the SAME `shell_layout` the window runs, not a test-local reconstitution.
//! (This is why they live in the lib, cfg(test): the point is to run the
//! shipping code; the spike's bin-local tests retire with the bin.)
//!
//! Ported from the spike's tests: `egui_frames_run` (8 frames, seeded
//! document survives the round trip), with M-b's browser pane,
//! `click_toggles_view_state` (a real pointer event on a real checkbox),
//! and - with M-c's property pages - `click_apply_edits_operation_through_
//! commands` (the draft/Apply law) plus the DragValue parity test: the drag
//! path and the typed path commit the same staged field the imgui
//! InputDouble did (divergence D5's proof). `history_key_queues_command`
//! proves the keyboard layer. M-d adds the file law:
//! `blank_startup_renders_and_frames_honestly` (the window opens empty -
//! the seed is gone from `Shell::new`) and `file_ops_round_trip_through_
//! commands` (Save/New/Open through the queue; the rfd modals are
//! hand-test-only). Since M-e the viewport itself is interactive: the
//! gesture tests drive `crate::interact::pick_at` and the camera arms
//! through egui's own pointer events - click-and-pick, the travel
//! threshold, MMB/RMB, wheel, the Face radio and anchor picking. The
//! snapshot is the churn gate for the per-panel migration steps. M1.4's
//! graphical tool editor adds the first floating surface: the flow test
//! drives the window (Edit opens, drag stages, Apply lands, Delete
//! ghost-closes) and `tool_editor_snapshot` pins its pixels beside the
//! shell's, which the closed-by-default window must leave untouched.
//!
//! The harness runs the managed-texture path: the viewport leaf shows a flat
//! placeholder instead of View3D's offscreen render, because the GPU
//! registration lives in Shell::draw/drain_textures and is proven by the
//! headless run - repeating it here would add wgpu nondeterminism to pixels
//! that carry no layout information. Everything else is shipping code: theme,
//! dock splits, panes, the command queue.
//!
//! Provenance: our own tests, specced from the egui_kittest and kittest
//! vendored sources; no HSMWorks material involved.

use egui::{ColorImage, Event, Key, Modifiers, TextureOptions, Vec2};
use egui_dock::DockState;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use hsmworks_app::model;
use hsmworks_app::operation::{FeatureSel, OpParams, SrcRef};
use hsmworks_app::schema::WCS_SLOTS;
use hsmworks_app::units::{self, Unit};

use crate::interact;
use crate::state::{CommandKind, DragMode, GeoPick, State, UiTarget};

use super::icons::IconSet;
use super::panes::{self, Cmd};
use super::shell::{Tab, build_dock, headless, shell_layout};
use super::{theme, world};

/// What the per-frame state of `Shell` is, minus the GPU half: the fields
/// `shell_layout` touches, so the harness exercises the window's real body.
struct ShellUi {
    state: State,
    dock: DockState<Tab>,
    viewport_points: [f32; 2],
    left_press: Option<[f32; 2]>,
    viewport_tex: Option<egui::TextureHandle>,
    /// The icon set, built lazily like `Shell`'s - the registration is
    /// CPU-side (`ctx.load_texture`), so the harness proves the PNG path,
    /// not just the painted fallback.
    icons: Option<IconSet>,
    themed: bool,
}

impl ShellUi {
    /// The birth `Shell::new` has since M-d: a plain empty document, no
    /// seed, no startup command - the window's real shape, rendered.
    fn blank() -> Self {
        Self {
            state: State::new(),
            dock: build_dock(),
            viewport_points: [1280.0, 649.0],
            left_press: None,
            viewport_tex: None,
            icons: None,
            themed: false,
        }
    }

    /// `blank` plus the demo document through the real command machinery
    /// (world.rs), then FrameAll off the queue - the harness default,
    /// because the property pages need content to hold up.
    fn seeded() -> Self {
        let mut sh = Self::blank();
        world::seed(&mut sh.state);
        sh.state.pending.push_back(CommandKind::FrameAll);
        sh.state.drain_actions();
        sh
    }
}

/// One frame under Shell::draw's law: first the commands the panels queued
/// last frame, then the whole shell. The theme and the viewport texture are
/// applied lazily because the harness owns its `Context` and hands it over
/// only through the `Ui` - same init order the window follows in resumed().
fn frame(ui: &mut egui::Ui, sh: &mut ShellUi) {
    let ctx = ui.ctx().clone();
    if !sh.themed {
        theme::apply(&ctx);
        sh.themed = true;
    }
    sh.state.drain_actions();
    let img = ColorImage::new([8, 8], vec![egui::Color32::from_rgb(24, 26, 31); 64]);
    let tex = sh
        .viewport_tex
        .get_or_insert_with(|| ctx.load_texture("egui-viewport", img, TextureOptions::NEAREST))
        .id();
    if sh.icons.is_none() {
        sh.icons = Some(IconSet::load(&ctx));
    }
    shell_layout(
        ui,
        &mut sh.state,
        &mut sh.dock,
        tex,
        sh.icons.as_ref(),
        &mut sh.viewport_points,
        &mut sh.left_press,
    );
}

/// The window at hand-test size, first frame settled (the builder's own
/// run_ok stabilises fonts and the theme).
fn shell_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::seeded())
}

/// A frame body with ONLY the keyboard layer: no shell, and crucially no
/// `drain_actions` - `State::run`'s dialog arms open real rfd modals, so
/// the file-key proof here is queue membership, never an executed command.
fn shortcuts_only(ui: &mut egui::Ui, st: &mut State) {
    panes::shortcuts(ui, st);
}

/// ...same, with a text field owning focus (the live shell always has one
/// reachable - imgui's io.WantTextInput gate mirrored by `typing`).
fn shortcuts_with_field(ui: &mut egui::Ui, st: &mut State) {
    let mut scratch = String::new();
    ui.add(egui::TextEdit::singleline(&mut scratch).desired_width(80.0))
        .request_focus();
    panes::shortcuts(ui, st);
}

/// Port of the spike's `egui_frames_run`: real frames of the real shell.
/// A panic anywhere in the layout, the theme or the command drain fails the
/// test; the world check afterwards proves eight passes of the shell -
/// which rebuild their widget state every frame - left the document intact.
#[test]
fn egui_frames_run() {
    let mut harness = shell_harness();
    harness.run_steps(8);

    let st = &harness.state().state;
    let moves = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::toolpath_of(&st.world, &e))
        .map_or(0, |tp| tp.path.moves.len());
    assert!(
        moves > 0,
        "Contour1's cached toolpath vanished across frames (status: {})",
        st.status
    );
}

/// The keyboard layer M-a does ship: Ctrl+Z queues Undo through
/// `shortcuts`, exactly as a menu click would - the command reaches
/// `state.pending`, and the NEXT frame's drain executes it (the shell's
/// queue law, not a direct history poke).
#[test]
fn history_key_queues_command() {
    let mut harness = shell_harness();
    harness.run();
    // Freshly seeded: the redo lane is empty (executing clears it).
    assert!(
        !harness.state().state.history.can_redo(),
        "a plain run must not move the history"
    );
    // ModifiersChanged first: InputState tracks i.modifiers from that event
    // (the live window gets it from winit's ModifiersChanged), so the
    // injection must send the same pair or the Cmd check reads no modifier.
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    // Whether the pass that drains the queue lands in this run or the next
    // is egui's multi-pass business; the observable law is that Ctrl+Z
    // walked the history through the command machinery.
    harness.run_steps(2);
    assert!(
        harness.state().state.history.can_redo(),
        "Ctrl+Z did not reach the undo history"
    );
    assert!(
        !harness
            .state()
            .state
            .pending
            .iter()
            .any(|c| matches!(c, CommandKind::Undo)),
        "the queued command outlived the drain - queue and shell disagree"
    );
}

/// Port of the spike's `click_toggles_view_state` (M-b's browser pane):
/// the global view toggles are immediate UI state (AGENTS.md: not
/// undoable), reached here through a real pointer event, not a poke.
#[test]
fn click_toggles_view_state() {
    let mut harness = shell_harness();
    assert!(!harness.state().state.show_all_paths);
    harness.get_by_label("Show all toolpaths").click();
    harness.run();
    assert!(harness.state().state.show_all_paths);
    // The display mode follows the same law through the same door: a real
    // pointer event, view state (never a command, never the undo stack).
    assert_eq!(harness.state().state.display, crate::state::DisplayMode::ShadedEdges);
    harness.get_by_label("Wireframe").click();
    harness.run();
    assert_eq!(harness.state().state.display, crate::state::DisplayMode::Wireframe);
    assert!(harness.state().state.pending.is_empty(), "a view toggle queues nothing");
}

/// M-d's startup law, proven in pixels: the window opens on a plain
/// empty document - the seed left `Shell::new` for real file ops - and
/// the whole shell renders it without a hiccup. Then a bare FrameAll on
/// the blank world must refuse by name ("nothing to frame", the camera
/// untouched) instead of fitting a bogus box: Prime Directive on the one
/// command the empty startup actually receives.
#[test]
fn blank_startup_renders_and_frames_honestly() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    harness.run_steps(3);
    let st = &harness.state().state;
    assert_eq!(st.status, "ready", "startup must not run any command");
    assert!(
        model::named_entities(&st.world).is_empty(),
        "the window must start on an empty document"
    );
    harness
        .state_mut().state.pending
        .push_back(CommandKind::FrameAll);
    harness.run();
    assert_eq!(harness.state().state.status, "nothing to frame");
}

/// M-d's file law through the machinery the ribbon drives: the queue's
/// dialog-free path (the `*File` arms - the same `project::save` /
/// `project::load` the rfd dialogs invoke once a path is picked; the
/// modal itself is hand-test-only). Save, New, Open: New must retire
/// the undo timeline (state.rs), and the reopened document must carry
/// the saved operation's parameters back verbatim.
#[test]
fn file_ops_round_trip_through_commands() {
    let mut harness = shell_harness();
    let path = std::env::temp_dir().join(format!("hsmworks-egui-md-{}.hsmproj", std::process::id()));

    harness
        .state_mut().state.pending
        .push_back(CommandKind::SaveProjectFile(path.clone()));
    harness.run();
    assert!(
        harness.state().state.status.starts_with("saved"),
        "save did not report success (status: {})",
        harness.state().state.status
    );

    harness.state_mut().state.pending.push_back(CommandKind::NewDocument);
    harness.run();
    let st = &harness.state().state;
    assert!(
        model::find_by_name(&st.world, "Contour1").is_none(),
        "New left entities behind (status: {})",
        st.status
    );
    assert!(!st.history.can_undo(), "New must retire the timeline");
    assert_eq!(st.status, "new document");

    harness
        .state_mut().state.pending
        .push_back(CommandKind::OpenProjectFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    let climb = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { climb, .. } => climb,
            _ => false,
        });
    assert_eq!(
        climb,
        Some(true),
        "the reopened document lost Contour1's saved params (status: {})",
        st.status
    );
    assert!(st.status.starts_with("opened"), "open status: {}", st.status);
    let _ = std::fs::remove_file(&path);
}

/// Port of the spike's `click_apply_edits_operation_through_commands`
/// (M-c: the properties pane is the real one now). The draft law end to
/// end: the checkbox edits the staged `op_ui` only, Apply queues
/// EditOperationParams, and the NEXT drain writes the world - the history
/// afterwards is the proof, not any intermediate queue state (egui may run
/// the state closure across several passes per `run`).
#[test]
fn click_apply_edits_operation_through_commands() {
    let mut harness = shell_harness();
    let staged_climb = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Contour { climb, .. } => climb,
        _ => false,
    };
    assert!(staged_climb(&harness));
    // M1.4 sectioning: the checkbox lives on the Feeds tab now, and a tab
    // cell is the way in - navigation first, then the click law itself.
    harness.get_by_label("Feeds").click();
    harness.run();
    harness.get_by_label("Climb milling").scroll_to_me();
    harness.run();
    harness.get_by_label("Climb milling").click();
    harness.run();
    assert!(!staged_climb(&harness), "checkbox did not reach the staged params");
    harness.get_by_label("Apply Parameters").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply Parameters").click();
    harness.run();
    let st = &harness.state().state;
    let live = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { climb, .. } => climb,
            _ => true,
        });
    assert_eq!(live, Some(false), "Apply did not reach the world");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
        "the queued command outlived the drain - queue and panels disagree"
    );
    assert!(
        !st.history.can_redo() && st.history.can_undo(),
        "Apply must sit on the undo timeline"
    );
}

/// The first SpinButton node's center: the Contour page's "Stock to
/// leave" DragValue. Tree order is submission order; the only
/// role-SpinButton nodes anywhere are DragValues (the painted strips
/// carry no a11y marks), and with the Passes tab open it is the pane's
/// only number (M1.4 sectioning) - the caller navigates before asking.
fn stock_to_leave_center(h: &Harness<'static, ShellUi>) -> egui::Pos2 {
    h.get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .expect("the properties pane owns DragValues")
        .rect()
        .center()
}

/// DragValue parity (divergence D5's proof, M-c): the drag path and the
/// typed path commit the SAME staged field the imgui InputDouble did.
/// Neither may touch the world by itself - numbers stage into `op_ui`
/// and ride an EditOperationParams only when Apply is clicked.
#[test]
fn drag_and_type_edit_the_same_staged_number() {
    let mut harness = shell_harness();
    // M1.4 sectioning: "Stock to leave" is on the Passes tab - navigate
    // there, and it becomes the only DragValue the pane renders.
    harness.get_by_label("Passes").click();
    harness.run();
    harness.run();
    let leave = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Contour {
            stock_to_leave,
            climb,
            ..
        } => {
            assert!(climb, "the drag/type paths must not disturb other staged fields");
            stock_to_leave
        }
        _ => panic!("the demo document must stage Contour params"),
    };
    let first = leave(&harness);
    let live0 = model::find_by_name(&harness.state().state.world, "Contour1")
        .and_then(|e| model::operation_of(&harness.state().state.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { stock_to_leave, .. } => stock_to_leave,
            _ => f64::NAN,
        })
        .expect("Contour1 exists with contour params");

    // The drag: press, one moved frame with the button down, release.
    // A same-frame click+move would become focus instead (clicked()
    // requests it before the drag branch), so the gesture is spread
    // across frames - the imgui InputDouble stepped on arrows and
    // wheel; the DragValue's counterpart is the horizontal drag.
    let c = stock_to_leave_center(&harness);
    harness.drag_at(c);
    harness.run();
    harness.hover_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    harness.drop_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    assert!(
        (leave(&harness) - (first + 3.0)).abs() < 0.05,
        "drag path: 30 px at speed 0.1 did not step {first} to {} (got {})",
        first + 3.0,
        leave(&harness)
    );

    // The typed entry: click focuses (the DragValue re-renders itself as
    // a TextEdit with select-all), typing replaces, and
    // update_while_editing commits the parse live.
    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .unwrap()
        .click();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .unwrap()
        .type_text("25");
    harness.run();
    assert!(
        (leave(&harness) - 25.0).abs() < 1e-9,
        "typed path: 25 did not commit (got {})",
        leave(&harness)
    );

    // And the world is untouched until Apply: no command was queued,
    // the live op still carries the seed value.
    let st = &harness.state().state;
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
        "a staged number edit must not queue a command by itself"
    );
    let live = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { stock_to_leave, .. } => stock_to_leave,
            _ => f64::NAN,
        })
        .expect("Contour1 exists with contour params");
    assert!(
        live == live0 && live != 25.0,
        "staged edits leaked into the world: live {live0} became {live}"
    );
}

/// M1.4's sectioning law, as a drift guard in the tips-match style:
/// every named field of every kind renders on exactly ONE tab (visible
/// when its tab is open, absent from the tree otherwise), and a kind's
/// tab bar shows exactly the sections it has fields for - no dead tabs.
/// If a field moves sections, a label changes, or a guard drops a field,
/// this fails.
#[test]
fn every_field_lives_on_exactly_one_section() {
    let mut harness = shell_harness();
    let all = ["Geometry", "Heights", "Passes", "Leads & Links", "Feeds"];
    // Kind order == pages::kind_index == the Type combo's TYPES order;
    // `retype` is the law the combo itself runs, so the draft takes the
    // same shape the user would drive it into.
    let table = [
        (
            "Face",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Cut Z", "Depth staging"]),
                ("Passes", vec!["Stepover (Y spacing)"]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)", "Climb milling"]),
            ],
        ),
        (
            "Contour",
            vec![
                ("Geometry", vec!["Click selects:", "Side", "Tolerance"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "Retract Z (between passes)",
                        "Top Z",
                        "Bottom Z",
                        "Stepdown",
                    ],
                ),
                ("Passes", vec!["Stock to leave"]),
                ("Leads & Links", vec!["Lead in/out", "Tab count", "Tab width", "Tab height"]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)", "Climb milling"]),
            ],
        ),
        (
            "Pocket",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Retract plane", "Top Z", "Bottom Z", "Stepdown"]),
                ("Passes", vec!["Stepover", "Stock to leave", "Strategy"]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)", "Climb milling"]),
            ],
        ),
        (
            "Slot",
            vec![
                ("Geometry", vec!["Click selects:", "A X", "A Y", "B X", "B Y", "Slot width"]),
                ("Heights", vec!["Clearance Z", "Top Z", "Bottom Z", "Stepdown"]),
                ("Passes", vec!["Stock to leave", "Stepover"]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)", "Climb milling"]),
            ],
        ),
        (
            "Trace",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Retract plane", "Top Z", "Bottom Z", "Stepdown"]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)", "Climb milling"]),
            ],
        ),
    ];
    for (ki, (kind_name, sections)) in table.iter().enumerate() {
        let cur = harness.state().state.op_ui.params.clone();
        let retyped = panes::retype(&cur, ki);
        harness.state_mut().state.op_ui.params = retyped;
        let mut placement: Vec<(&str, usize)> = Vec::new();
        for (sname, labels) in sections {
            let si = all
                .iter()
                .position(|s| s == sname)
                .expect("the table names the SECTIONS");
            for l in labels.iter() {
                placement.push((*l, si));
            }
        }
        for (si, tab_name) in all.iter().enumerate() {
            harness.state_mut().state.op_tab = si;
            harness.run();
            let has_tab = sections.iter().any(|(n, _)| *n == *tab_name);
            assert_eq!(
                harness.query_by_label(tab_name).is_some(),
                has_tab,
                "{kind_name}: the tab bar must show '{tab_name}' iff the kind has fields for it",
            );
            // A kind cannot show a section it has no fields for - an
            // out-of-set op_tab falls back to Geometry (0), the law the
            // page's resolution line states.
            let effective = if has_tab { si } else { 0 };
            for (label, want) in &placement {
                assert_eq!(
                    harness.query_by_label(label).is_some(),
                    *want == effective,
                    "{kind_name}: '{label}' visible on tab {si}, it belongs to tab {want} - \
                     one placement per field, always",
                );
            }
        }
    }
}

/// A tab click is navigation, nothing more: the pane's state field moves,
/// no command is queued, and the section's fields replace the previous
/// ones (the view-state law from M1.4's toggle decision, same shape).
#[test]
fn tab_click_navigates_and_queues_nothing() {
    let mut harness = shell_harness();
    harness.run();
    assert!(harness.query_by_label("Side").is_some(), "Geometry opens by default");
    harness.get_by_label("Heights").click();
    harness.run();
    let st = &harness.state().state;
    assert_eq!(st.op_tab, 1, "the Heights cell did not open the Heights section");
    assert!(st.pending.is_empty(), "opening a section must not queue a command");
    assert!(harness.query_by_label("Clearance Z").is_some());
    assert!(harness.query_by_label("Side").is_none(), "the closed tab's fields leave the tree");
}

// ------------------------------------------------------- gesture plumbing
//
// The harness has no mouse loop, so pointer gestures are built from egui's
// own event types - the same RawInput stream the winit backend feeds the
// live window. One event per frame: press and release are separate frames,
// exactly like a human click, and `run_steps(2)` afterwards lets the shell's
// queue law execute what the gesture staged.

fn set_pos(h: &mut Harness<'static, ShellUi>, p: egui::Pos2) {
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run();
}

fn button_at(h: &mut Harness<'static, ShellUi>, b: egui::PointerButton, p: egui::Pos2, pressed: bool) {
    h.input_mut().events.push(Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

/// A plain (button-1) click at `p`, then the frames that drain its command.
fn click_viewport(h: &mut Harness<'static, ShellUi>, p: egui::Pos2) {
    set_pos(h, p);
    button_at(h, egui::PointerButton::Primary, p, true);
    button_at(h, egui::PointerButton::Primary, p, false);
    h.run_steps(2);
}

/// The pane's screen center, once a frame has laid it out.
fn vp_center(sh: &ShellUi) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    egui::pos2((x0 + x1) * 0.5, (y0 + y1) * 0.5)
}

/// M-e's core claim in the egui shell: a click inside the pane runs the
/// SHARED `interact::pick_at` law - status, hover, and a Select command
/// that moves the tree highlight off the operation the seed had selected.
#[test]
fn click_in_viewport_selects_model() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    click_viewport(&mut harness, c);
    let st = &harness.state().state;
    assert_eq!(st.status, "selected Block", "the center ray must hit the block");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::Select(_))),
        "the queued command outlived the drain"
    );
    let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
    assert!(model::is_selected(&st.world, &block), "click did not select the model");
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    assert!(
        !model::is_selected(&st.world, &op),
        "a model click must move the highlight off the operation"
    );
}

/// The hot-window gate, the negative way: a click far outside the pane's
/// rect must not touch the pick machinery at all - no status, no queue.
/// (The corner is ribbon paint; paint is not the viewport.)
#[test]
fn click_outside_the_viewport_is_not_a_pick() {
    let mut harness = shell_harness();
    harness.run();
    let status0 = harness.state().state.status.clone();
    click_viewport(&mut harness, egui::pos2(4.0, 4.0));
    let st = &harness.state().state;
    assert_eq!(st.status, status0, "a stray click ran a pick");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::Select(_))),
        "the stray click queued a command"
    );
}

/// The travel threshold is THE law separating click from orbit: past
/// CLICK_TRAVEL_PX the gesture is a drag - the camera turns, the release
/// picks nothing, and no command rides the queue.
#[test]
fn drag_promotes_past_the_travel_threshold() {
    let mut harness = shell_harness();
    harness.run();
    let c = vp_center(harness.state());
    let status0 = harness.state().state.status.clone();
    let (yaw0, pitch0) = (harness.state().state.cam.yaw, harness.state().state.cam.pitch);
    set_pos(&mut harness, c);
    button_at(&mut harness, egui::PointerButton::Primary, c, true);
    let d = c + egui::vec2(25.0, 0.0);
    set_pos(&mut harness, d);
    {
        let st = harness.state();
        assert_eq!(st.state.drag, Some(DragMode::Orbit), "25 px must promote to orbit");
        assert_ne!(st.state.cam.yaw, yaw0, "orbit must turn yaw");
        assert_eq!(st.state.cam.pitch, pitch0, "a pure horizontal drag keeps pitch");
    }
    button_at(&mut harness, egui::PointerButton::Primary, d, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert_eq!(st.drag, None, "the release must end the drag");
    assert_eq!(st.status, status0, "a drag must not pick");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::Select(_))),
        "the drag queued a Select"
    );
}

/// Middle orbits, right pans, and neither is a pick - the drag arms of the
/// shared law, driven through egui's own button events.
#[test]
fn mmb_orbits_and_rmb_pans() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    let status0 = harness.state().state.status.clone();
    set_pos(&mut harness, c);
    let (yaw0, t0) = (harness.state().state.cam.yaw, harness.state().state.cam.target);
    button_at(&mut harness, egui::PointerButton::Middle, c, true);
    let m1 = c + egui::vec2(20.0, 10.0);
    set_pos(&mut harness, m1);
    {
        let st = harness.state();
        assert_eq!(st.state.drag, Some(DragMode::Orbit));
        assert_ne!(st.state.cam.yaw, yaw0, "MMB drag must orbit");
        assert_eq!(st.state.cam.target, t0, "orbit must not move the target");
    }
    button_at(&mut harness, egui::PointerButton::Middle, m1, false);
    assert_eq!(harness.state().state.drag, None, "release must end the drag");

    button_at(&mut harness, egui::PointerButton::Secondary, c, true);
    // yaw1: the MMB orbit left its mark - the pan must not add another.
    let yaw1 = harness.state().state.cam.yaw;
    let m2 = c + egui::vec2(15.0, 0.0);
    set_pos(&mut harness, m2);
    {
        let st = harness.state();
        assert_eq!(st.state.drag, Some(DragMode::Pan));
        assert_ne!(st.state.cam.target, t0, "RMB drag must pan the target");
        assert_eq!(st.state.cam.yaw, yaw1, "pan must not rotate");
    }
    button_at(&mut harness, egui::PointerButton::Secondary, m2, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert_eq!(st.drag, None);
    assert_eq!(st.status, status0, "camera gestures must not report picks");
    assert!(st.pending.is_empty(), "camera gestures are not commands");
}

/// Wheel = dolly, one notch of the Line unit to one step of
/// `dolly_cursor`, positive toward the cursor - the same units the imgui
/// shell's raw `mouse_wheel` feeds. Pure camera state: nothing queued.
#[test]
fn wheel_dollies_toward_the_cursor() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    let status0 = harness.state().state.status.clone();
    set_pos(&mut harness, c);
    let d0 = harness.state().state.cam.dist;
    harness.input_mut().events.push(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(
        st.cam.dist < d0 * 0.95,
        "one notch in must shorten dist (got {} vs {})",
        st.cam.dist,
        d0
    );
    assert!(st.pending.is_empty());
    assert_eq!(st.status, status0);
}

/// The Geometry section's radio row drives `state.geo_pick` - the same
/// immediate-mode law as the imgui radio, no command - and a Face-mode
/// click on a vertical block face queues SetOperationGeometry for the open
/// operation. The seed's Contour1 already carries the TOP face; the click
/// must REPLACE it with the side face (a plain click is a set, not an
/// add), naming both the face and the operation in the status.
#[test]
fn face_mode_click_replaces_geometry_of_the_open_op() {
    let mut harness = shell_harness();
    harness.run();
    harness.get_by_label("Faces").click();
    harness.run();
    assert_eq!(harness.state().state.geo_pick, GeoPick::Face);

    // The four vertical faces of the seed's 60x60x30 block; the most
    // front-facing one whose projection sits clear inside the pane is the
    // deterministic click target (a convex body's front faces tile the
    // silhouette, so that pixel ray enters that face - never the top).
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp((w / hgt) as f64);
    let eye = harness.state().state.cam.eye_f32();
    let sides = [
        ([60.0, 30.0, 15.0], [1.0, 0.0, 0.0]),
        ([0.0, 30.0, 15.0], [-1.0, 0.0, 0.0]),
        ([30.0, 60.0, 15.0], [0.0, 1.0, 0.0]),
        ([30.0, 0.0, 15.0], [0.0, -1.0, 0.0]),
    ];
    let mut best: Option<(f32, egui::Pos2)> = None;
    for (c, n) in sides {
        let facing = ((f64::from(eye[0]) - c[0]) * n[0]
            + (f64::from(eye[1]) - c[1]) * n[1]
            + (f64::from(eye[2]) - c[2]) * n[2]) as f32;
        let Some(uv) = interact::project_uv(&mvp, c) else { continue };
        if !(uv[0] > 0.1 && uv[0] < 0.9 && uv[1] > 0.15 && uv[1] < 0.85 && facing > 0.0) {
            continue;
        }
        let p = egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt);
        if best.is_none_or(|(bf, _)| facing > bf) {
            best = Some((facing, p));
        }
    }
    let p = best.expect("the block shows a vertical face to the default camera").1;
    click_viewport(&mut harness, p);
    let st = &harness.state().state;
    // The pick's "added to" line rides one frame, then the drain's
    // execution status replaces it - seeing the executed status is the
    // proof the command went through the machinery, not a field poke.
    assert_eq!(st.status, "geometry selection updated for Contour1");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::SetOperationGeometry { .. })),
        "the queued command outlived the drain"
    );
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    let items = match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        other => panic!("Contour1's source became {other:?}"),
    };
    assert_eq!(items.len(), 1, "a plain click sets the list, not grows it");
    assert_eq!(items[0].entity, "Block");
    let FeatureSel::Face(fid) = items[0].feature else {
        panic!("a face click must queue a face, got {:?}", items[0].feature);
    };
    // And it must be the SIDE face the pixel was chosen for - the seed's
    // top face has normal z = 1; a vertical face's is ~0. This is what
    // distinguishes "replaced with the click" from "no-op on the old pick".
    let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
    let faces = model::features_of(&st.world, &block).expect("Block has features");
    let face = faces.faces.iter().find(|f| f.id == fid).expect("picked face exists");
    assert!(face.normal[2].abs() < 0.3, "the click landed on a horizontal face");
}

/// Anchor pick mode through the viewport: a click ON a marker queues the
/// SAME `SetWcs` the pane's grid buttons queue, at that anchor's exact
/// position. `pick_anchor` is immediate UI state, like the pick-mode radio
/// above - not undoable, not queued.
#[test]
fn anchor_click_queues_setwcs() {
    let mut harness = shell_harness();
    let slot = WCS_SLOTS[0];
    let pos0 = {
        let e = model::find_by_name(&harness.state().state.world, slot).expect("seed sets G54");
        let p = model::transform_of(&harness.state().state.world, &e).unwrap_or_default().pos;
        [p.x, p.y, p.z]
    };
    // The first anchor that projects clearly inside the laid-out pane.
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp((w / hgt) as f64);
    let (want, p) = interact::anchor_points(&harness.state().state)
        .into_iter()
        .filter_map(|(pt, _)| {
            let uv = interact::project_uv(&mvp, pt)?;
            (uv[0] > 0.02 && uv[0] < 0.98 && uv[1] > 0.02 && uv[1] < 0.98)
                .then_some((pt, egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)))
        })
        .next()
        .expect("the anchor grid faces the default camera");
    assert!(
        (want[0] - pos0[0]).abs() + (want[1] - pos0[1]).abs() + (want[2] - pos0[2]).abs() > 1e-6,
        "the anchor coincides with the seed's origin - the assertion would prove nothing"
    );

    harness.state_mut().state.wcs_ui.pick_anchor = true;
    click_viewport(&mut harness, p);
    let st = &harness.state().state;
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::SetWcs { .. })),
        "the queued command outlived the drain"
    );
    let e = model::find_by_name(&st.world, slot).expect("G54 exists");
    let q = model::transform_of(&st.world, &e).unwrap_or_default().pos;
    let pos = [q.x, q.y, q.z];
    assert!(
        (pos[0] - want[0]).abs() < 1e-6
            && (pos[1] - want[1]).abs() < 1e-6
            && (pos[2] - want[2]).abs() < 1e-6,
        "the anchor click did not place {slot} at the marker (got {pos:?}, wanted {want:?})"
    );
}

/// The exit contract, lib side (the integration tests/egui_app.rs covers the
/// `--ui egui` dispatch; this one proves the headless body directly,
/// including the View3D readback and the fallback-adapter law).
#[test]
fn headless_run_succeeds() {
    headless::run().map_err(|err| panic!("egui headless run failed: {err}")).ok();
}

/// The churn gate for the migration: one rendered frame of the whole shell,
/// compared to `tests/snapshots/egui_app.png` (relative to this crate).
/// Regenerate deliberately with `UPDATE_SNAPSHOTS=1`; a failure writes
/// `.new.png`/`.diff.png` beside it for eyes. kittest.toml could loosen the
/// threshold per OS - not added; if cross-backend drift ever shows, that is
/// the knob, in this crate's root.
#[test]
fn egui_shell_snapshot() {
    let mut harness = shell_harness();
    harness.run_steps(3);
    harness.snapshot("egui_app");
}

/// M1.4: the graphical tool editor, end to end through the real drain. The
/// dock page is the LIBRARY (no field widgets, so every label queried here
/// resolves in exactly one surface); `Edit` opens the window on the world's
/// numbers; a drag stages into `tool_ui` WITHOUT touching the world (the
/// draft law); `Apply` lands it through `ChangeTool`; and the dock's
/// `Delete` proves the ghost law - the named tool is gone, so the window
/// is gone the next frame.
#[test]
fn tool_editor_window_edits_live_and_stages_through_commands() {
    let mut harness = shell_harness();
    // Browser rows are painted strips; retargeting is the established
    // navigation idiom (state_mut like the gesture tests).
    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.run();
    let world_d = |h: &Harness<'static, ShellUi>| {
        model::find_by_name(&h.state().state.world, "EM-0600-4FL")
            .and_then(|e| model::tool_of(&h.state().state.world, &e))
            .map(|t| t.diameter)
    };
    assert!(
        harness.query_by_label("Diameter").is_none(),
        "the dock library must not own the editor's labels"
    );
    harness.get_by_label("Edit").click();
    harness.run();
    harness.run();
    assert_eq!(harness.state().state.tool_editor.as_deref(), Some("EM-0600-4FL"));
    assert!(harness.state().state.pending.is_empty(), "opening must queue nothing");
    let d0 = world_d(&harness).expect("the seed owns EM-0600-4FL");
    assert!(
        (harness.state().state.tool_ui.diameter - d0).abs() < 1e-12,
        "the window must open on the world's diameter"
    );
    // The window's first SpinButton is Diameter (the library renders no
    // DragValues, so "first" is deterministic): 30 px at speed 0.1.
    let c = harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .expect("the editor window owns DragValues")
        .rect()
        .center();
    harness.drag_at(c);
    harness.run();
    harness.hover_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    harness.drop_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    assert!(
        (harness.state().state.tool_ui.diameter - (d0 + 3.0)).abs() < 0.05,
        "the drag must stage the draft"
    );
    assert!(
        (world_d(&harness).expect("still there") - d0).abs() < 1e-12,
        "a drag must not touch the world"
    );
    harness.get_by_label("Apply").click();
    harness.run();
    assert!(
        (world_d(&harness).expect("still there") - (d0 + 3.0)).abs() < 0.05,
        "Apply must land the staged number (status: {})",
        harness.state().state.status
    );
    // Ghost law: deleting the tool from the library closes the window.
    harness.get_by_label("Delete").click();
    harness.run();
    assert!(
        harness.state().state.tool_editor.is_none(),
        "the window must not outlive its tool"
    );
    assert!(
        harness.query_by_label("Shank diameter").is_none(),
        "the window's widgets must go with it"
    );
    assert!(world_d(&harness).is_none(), "Delete must empty the library too");
}

/// The editor's pixels are their own churn gate, so the shell snapshot can
/// keep its "window closed by default" law: fields left, banded to-scale
/// profile right, floating over the dock. Regenerate with UPDATE_SNAPSHOTS=1.
#[test]
fn tool_editor_snapshot() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run_steps(3);
    harness.snapshot("egui_tool_editor");
}

// The S0 one-wgpu proof retired with the imgui renderer at M-g: with
// `dear_imgui_wgpu` gone there is a single wgpu in the tree by
// construction, and nothing left to coerce.

// ------------------------------------------------------- M-f: the actions law

/// The keyboard law M-f closes: every file key the ribbon tips advertise
/// must queue exactly its command, and must stay silent while a text field
/// owns focus. Driven through the keyboard layer alone (`shortcuts_only`),
/// so the proof is queue membership - never a drain that would raise rfd.
#[test]
fn file_keys_queue_their_commands() {
    let ctrl = Modifiers { command: true, ..Default::default() };
    let ctrl_shift = Modifiers { command: true, shift: true, ..Default::default() };
    let mut harness = Harness::builder()
        .with_size(Vec2::new(400.0, 200.0))
        .build_ui_state(shortcuts_only, State::new());

    // Ctrl+<key> arms exactly the command its tip promises. The shift
    // variant must split the two Open arms, and it must not double-fire.
    let expect = |h: &mut Harness<'static, State>,
                      mods: Modifiers,
                      key: Key,
                      want: fn(&CommandKind) -> bool,
                      name: &str| {
        h.state_mut().pending.clear();
        // ModifiersChanged first: i.modifiers only tracks that event (the
        // law proven in `history_key_queues_command`).
        h.input_mut().events.push(Event::ModifiersChanged(mods));
        h.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: mods,
        });
        h.run();
        let p = &h.state().pending;
        assert_eq!(p.len(), 1, "{name} armed {} commands, expected exactly one", p.len());
        assert!(want(&p[0]), "{name} armed {p:?}");
    };
    expect(&mut harness, ctrl, Key::N, |c| matches!(c, CommandKind::NewDocument), "Ctrl+N");
    expect(
        &mut harness,
        ctrl,
        Key::O,
        |c| matches!(c, CommandKind::OpenModelDialog),
        "Ctrl+O",
    );
    expect(
        &mut harness,
        ctrl_shift,
        Key::O,
        |c| matches!(c, CommandKind::OpenProjectDialog),
        "Ctrl+Shift+O",
    );
    expect(
        &mut harness,
        ctrl,
        Key::S,
        |c| matches!(c, CommandKind::SaveProjectDialog),
        "Ctrl+S",
    );

    // The focus gate: the same Ctrl+O over a focused text field arms
    // nothing (a dialog must never spawn mid-type - imgui's io.WantTextInput).
    let mut typing = Harness::builder()
        .with_size(Vec2::new(400.0, 200.0))
        .build_ui_state(shortcuts_with_field, State::new());
    typing.run();
    typing.state_mut().pending.clear();
    typing.input_mut().events.push(Event::ModifiersChanged(ctrl));
    typing.input_mut().events.push(Event::Key {
        key: Key::O,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: ctrl,
    });
    typing.run();
    assert!(
        typing.state().pending.is_empty(),
        "Ctrl+O armed a dialog over a focused text field: {:?}",
        typing.state().pending
    );
}

/// The painted ribbon cell is a click on the actions law: `ribbon_cells`
/// (the single layout source) locates the Pocket cell, a real pointer
/// press/release there must run `actions::add_operation`, and the shell's
/// next-frame drain must land the operation in the world. Proves the ribbon
/// fires the shared layer, not a forked poke - the seam M-a left open.
#[test]
fn ribbon_click_creates_through_the_actions_law() {
    let mut harness = shell_harness();
    harness.run();
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let pocket = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(2))
        .expect("the Create group carries a Pocket cell");
    let at = pocket.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(
        model::find_by_name(&st.world, "Operation2").is_some(),
        "the Pocket click did not create through the actions layer (status: {})",
        st.status
    );
    assert!(
        matches!(&st.ui_target, UiTarget::Operation(n) if n == "Operation2"),
        "quick-add must leave its target on the new operation"
    );
}

/// The tip table is imgui's string law (ribbon.rs verbatim). Row-for-row,
/// this is the guard against the drift M-f found on 7 of 18 cells - and the
/// Create row's no-tool swap, the case the empty startup actually shows.
#[test]
fn tips_match_the_imgui_table() {
    let with_tool = ShellUi::seeded().state;
    let empty = State::new();
    let rows: &[(Cmd, &str)] = &[
        (Cmd::New, "New document\tCtrl+N"),
        (Cmd::Model, "Open a mesh model\tCtrl+O"),
        (Cmd::Project, "Open a project\tCtrl+Shift+O"),
        (Cmd::Save, "Save the project\tCtrl+S"),
        (Cmd::Stock, "Stock definition"),
        (Cmd::Origin, "Work offset (WCS)"),
        (Cmd::Tools, "Tool library"),
        (Cmd::Create(0), "Add a Face mill operation"),
        (Cmd::Create(1), "Add a 2D Contour operation"),
        (Cmd::Create(2), "Add a 2D Pocket operation"),
        (Cmd::Create(3), "Add a Slot operation"),
        (Cmd::Create(4), "Add a Trace operation"),
        (Cmd::Undo, "Undo\tCtrl+Z"),
        (Cmd::Redo, "Redo\tCtrl+Y"),
        (Cmd::Run, "Generate the selected operation"),
        (Cmd::Delete, "Delete the selection\tDel"),
        (Cmd::Frame, "Frame all\tF"),
        (Cmd::Paths, "Show every cached toolpath"),
    ];
    for (c, want) in rows {
        assert_eq!(&panes::tip_of(*c, &with_tool), want, "tip drifted for {c:?}");
    }
    // Every Create button shares imgui's swap while the tool list is empty.
    for i in 0..panes::TYPES.len() {
        assert_eq!(
            panes::tip_of(Cmd::Create(i), &empty),
            "No tool yet - add one (Setup > Tools)",
            "the no-tool tip must name the fix (Create({i}))"
        );
    }
}

// ------------------------------------------------------------- M1.1: units

/// The Units radio on the setup page is the whole lens control: it rides
/// `SetUnits` through the queue (a command, not a field poke), undo puts
/// the millimeter default back, and a document never changes because of
/// it - the world's numbers stay exactly as they were.
#[test]
fn units_radio_flips_the_lens_and_undo_puts_it_back() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    harness.run();
    assert_eq!(harness.state().state.units(), Unit::Millimeters);

    harness.get_by_label("Inches").click();
    harness.run();
    assert_eq!(
        units::of(&harness.state().state.world),
        Unit::Inches,
        "the click must reach the world through SetUnits"
    );

    // A second click on the already-shown choice queues nothing: a no-op
    // does not belong in the history (SetUnits validates at construction).
    let before = harness.state().state.history.can_redo();
    harness.get_by_label("Inches").click();
    harness.run();
    assert_eq!(harness.state().state.history.can_redo(), before, "no-op must be inert");

    // Ctrl+Z back to millimeters - same keyboard lane as every command.
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    assert_eq!(
        units::of(&harness.state().state.world),
        Unit::Millimeters,
        "undo restores the lens"
    );
}

/// The inch lens in action, through the real editor window: the DragValue
/// moves per dragged pixel the SAME millimeter distance it moves in mm
/// mode (speed ÷ 25.4 - a lens, not a gearbox), a typed `0.5` stores
/// exactly 12.7 on the micron lattice, and Apply lands snapped numbers
/// through `ChangeTool` like always.
#[test]
fn inch_mode_drag_keeps_px_parity_and_types_onto_the_lattice() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    harness.get_by_label("Inches").click();
    harness.run();
    assert_eq!(harness.state().state.units(), Unit::Inches);

    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run();
    harness.run();
    let world_d = |h: &Harness<'static, ShellUi>| {
        model::find_by_name(&h.state().state.world, "EM-0600-4FL")
            .and_then(|e| model::tool_of(&h.state().state.world, &e))
            .map(|t| t.diameter)
    };
    let d0 = world_d(&harness).expect("the seed owns EM-0600-4FL");

    let c = harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .expect("the editor window owns DragValues")
        .rect()
        .center();
    harness.drag_at(c);
    harness.run();
    harness.hover_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    harness.drop_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    // The same 0.05 tolerance the mm drag test carries, and the same
    // reason: egui's smart aim rounds a drag toward the nicest number of
    // the SHOWN unit inside a one-physical-pixel radius, which by the
    // speed-÷25.4 law is the identical ±0.1 mm in both modes (here it
    // pulled 9.000 to 0.355 in = 9.017 mm, 0.017 off).
    assert!(
        (harness.state().state.tool_ui.diameter - (d0 + 3.0)).abs() < 0.05,
        "inch px-parity: 30 px must move 3.0 mm from {d0}, got {}",
        harness.state().state.tool_ui.diameter
    );
    assert!(
        (world_d(&harness).expect("still there") - d0).abs() < 1e-12,
        "a drag must not touch the world"
    );

    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .unwrap()
        .click();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .unwrap()
        .type_text("0.5");
    harness.run();
    assert_eq!(
        harness.state().state.tool_ui.diameter,
        12.7,
        "a typed half inch must store exactly 12.7 mm"
    );
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(
        world_d(&harness).expect("still there"),
        12.7,
        "Apply must land the snapped millimeters (status: {})",
        harness.state().state.status
    );
}
