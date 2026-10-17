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
//! hand-test-only). The snapshot is the churn gate for the per-panel
//! migration steps.
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
use hsmworks_app::operation::OpParams;

use crate::state::{CommandKind, State};

use super::shell::{Tab, build_dock, headless, shell_layout};
use super::{theme, world};

/// What the per-frame state of `Shell` is, minus the GPU half: the fields
/// `shell_layout` touches, so the harness exercises the window's real body.
struct ShellUi {
    state: State,
    dock: DockState<Tab>,
    viewport_points: [f32; 2],
    viewport_tex: Option<egui::TextureHandle>,
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
            viewport_tex: None,
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
    shell_layout(ui, &mut sh.state, &mut sh.dock, tex, &mut sh.viewport_points);
}

/// The window at hand-test size, first frame settled (the builder's own
/// run_ok stabilises fonts and the theme).
fn shell_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::seeded())
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
/// leave" DragValue. Tree order is submission order, and the properties
/// page submits it before any other number in the shell - the only
/// role-SpinButton nodes anywhere are DragValues (the painted strips
/// carry no a11y marks), and params_fields puts it first.
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

/// Compile-time proof of the S0 gate, kept through graduation: exactly ONE
/// wgpu in the tree. If egui-wgpu resolved a different wgpu major than the
/// imgui renderer's re-export, the reference below would not typecheck and
/// the shell's registered-texture plan would be dead on arrival.
#[test]
fn wgpu_types_are_unified() {
    // Elided lifetimes bind output to the single input lifetime, so the
    // coercion is only accepted when both paths name the SAME type.
    fn coerce(d: &dear_imgui_wgpu::wgpu::Device) -> &egui_wgpu::wgpu::Device {
        d
    }
    // The coercion itself is the assertion; the pointer bind is the use.
    let _f: fn(&dear_imgui_wgpu::wgpu::Device) -> &egui_wgpu::wgpu::Device = coerce;
}
