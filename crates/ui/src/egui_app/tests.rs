//! M-a: frame tests for the graduated shell - egui_kittest harnesses driving
//! the SAME `shell_layout` the window runs, not a test-local reconstitution.
//! (This is why they live in the lib, cfg(test): the point is to run the
//! shipping code; the spike's bin-local tests retire with the bin.)
//!
//! Ported from the spike's tests: `egui_frames_run` (8 frames, seeded
//! document survives the round trip). The spike's click-through tests
//! (`click_toggles_view_state`, `click_apply_edits_operation_through_commands`)
//! drive REAL pane widgets, so they graduate with their panes - M-b (browser
//! toggles) and M-c (the Contour page) port them back. `history_key_queues_
//! command` proves the keyboard layer that M-a does ship. The snapshot is the
//! churn gate for the per-panel migration steps that follow.
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
use egui_kittest::Harness;

use hsmworks_app::model;

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
    /// The same birth `Shell::new` has: the demo document through the real
    /// command machinery (world.rs), then FrameAll off the queue.
    fn seeded() -> Self {
        let mut state = State::new();
        world::seed(&mut state);
        state.pending.push_back(CommandKind::FrameAll);
        state.drain_actions();
        Self {
            state,
            dock: build_dock(),
            viewport_points: [1280.0, 649.0],
            viewport_tex: None,
            themed: false,
        }
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
