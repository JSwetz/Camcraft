//! S3: frame tests for the spike shell - egui_kittest harnesses driving
//! the SAME `shell_layout` the window runs, not a test-local reconstitution
//! (which is why they live in the bin: a bin's modules are invisible to
//! tests/, and borrowing shell.rs's copy would test nothing about egui).
//!
//! Ports of the two frame-executing tests in crates/ui/tests/shell.rs:
//! `headless_frames_run` -> `egui_frames_run` (8 frames, seeded document
//! survives the round trip), `headless_cli_exit_code_is_zero` ->
//! `egui_headless_cli_exit_zero` (main's --headless arm exits with exactly
//! this Result). Added here: pointer-event interaction through the real
//! widgets, and `egui_shell_snapshot` - a rendered-image regression, the
//! churn gate for the per-panel migration steps that follow.
//!
//! The harness runs the managed-texture path: the viewport leaf shows a
//! flat placeholder instead of View3D's offscreen render, because the GPU
//! registration lives in Shell::draw/drain_textures and is proven by the
//! headless run - repeating it here would add wgpu nondeterminism to
//! pixels that carry no layout information. Everything else is shipping
//! code: theme, painted ribbon/status strips, dock splits, property pages,
//! the command queue.
//!
//! Provenance: our own spike tests, specced from the egui_kittest and
//! kittest vendored sources; no HSMWorks material involved.

use egui::{ColorImage, TextureOptions, Vec2};
use egui_dock::DockState;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use hsmworks_app::model;
use hsmworks_app::operation::OpParams;
use hsmworks_ui::state::{CommandKind, State};

use super::{build_dock, headless, shell_layout, theme, world, Tab};

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
        .get_or_insert_with(|| ctx.load_texture("spike-viewport", img, TextureOptions::NEAREST))
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

/// Port of `shell.rs::headless_frames_run`: real frames of the real shell.
/// A panic anywhere in the layout, the theme or the command drain fails the
/// test; the world check afterwards proves eight passes of the panels -
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

/// The browser's global view toggles are immediate UI state (AGENTS.md:
/// not undoable), reached here through a real pointer event, not a poke.
#[test]
fn click_toggles_view_state() {
    let mut harness = shell_harness();
    assert!(!harness.state().state.show_all_paths);
    harness.get_by_label("Show all toolpaths").click();
    harness.run();
    assert!(harness.state().state.show_all_paths);
}

/// The command path by pointer event: the Contour page's checkbox edits the
/// staged copy, Apply Parameters turns the difference into an
/// `EditOperationParams` on `state.pending` - the next frame's
/// `drain_actions` executes it, and the operation in the world says so.
/// That staged/live agreement is also what re-inerts the Apply button, so
/// the final frame proves the whole round trip.
#[test]
fn click_apply_edits_operation_through_commands() {
    let mut harness = shell_harness();
    // The seed's contour climbs (world.rs), staged == live on frame one.
    let staged_climb = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Contour { climb, .. } => climb,
        _ => false,
    };
    assert!(staged_climb(&harness));

    // The contour page is taller than the 720p pane, so both targets sit
    // under the scroll fold: bring each into view first - the same gesture
    // a user makes, which is the point of running the real page.
    harness.get_by_label("Climb milling").scroll_to_me();
    harness.run();
    harness.get_by_label("Climb milling").click();
    harness.run();
    assert!(
        !staged_climb(&harness),
        "checkbox did not reach the staged params"
    );

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
        !st.pending.iter().any(|c| matches!(
            c,
            CommandKind::EditOperationParams { .. }
        )),
        "the queued command outlived the drain - queue and panels disagree"
    );
    assert!(
        !st.history.can_redo() && st.history.can_undo(),
        "Apply must sit on the undo timeline"
    );
}

/// Port of `shell.rs::headless_cli_exit_code_is_zero`, spike side: main's
/// `--headless` arm turns exactly this Result into the exit code, so the
/// assertion is the same contract the gate runs - including the View3D
/// readback and the fallback-adapter law.
#[test]
fn egui_headless_cli_exit_zero() {
    headless::run()
        .map_err(|err| panic!("egui headless run failed: {err}"))
        .ok();
}

/// The churn gate for the migration: one rendered frame of the whole shell,
/// compared to `tests/snapshots/egui_shell.png` (relative to this crate).
/// Regenerate deliberately with `UPDATE_SNAPSHOTS=1`; a failure writes
/// `.new.png`/`.diff.png` beside it for eyes. kittest.toml could loosen the
/// threshold per OS - not added; if cross-backend drift ever shows, that is
/// the knob, in this crate's root.
#[test]
fn egui_shell_snapshot() {
    let mut harness = shell_harness();
    harness.run_steps(3);
    harness.snapshot("egui_shell");
}
