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
use hsmworks_app::operation::{FeatureSel, Height, OpParams, SrcRef};
use hsmworks_app::schema::WCS_SLOTS;
use hsmworks_app::units::{self, Unit};

use crate::interact;
use crate::state::{CommandKind, DragMode, GeoPick, PostView, State, UiTarget};

use super::icons::{Icon, IconSet};
use super::panes::{self, Cmd};
use super::painted;
use super::shell::{Tab, build_dock, headless, shell_layout};
use super::viewcube;
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

/// `frame` WITHOUT the drain: the real shell layout, but queued dialog
/// arms never execute (rfd modals are hand-test-only - same reason
/// `shortcuts_only` exists, for tests that need the whole ribbon on
/// screen). Callers assert queue membership and clear after.
fn frame_undrained(ui: &mut egui::Ui, sh: &mut ShellUi) {
    let ctx = ui.ctx().clone();
    if !sh.themed {
        theme::apply(&ctx);
        sh.themed = true;
    }
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

/// M1.7b's two faces: the ribbon cell (and Ctrl+P) queue the dialog arm -
/// membership only, the modal is hand-test - while the `*File` arm runs
/// the validation gate for real: a generated, fresh, silent document
/// posts one wrapped program, a stale one is refused BY NAME and the
/// previous program on disk is not touched (the gate writes nothing).
#[test]
fn post_cell_queues_and_the_file_arm_runs_the_gate() {
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let post = cells
        .iter()
        .find(|c| c.cmd == Cmd::Post)
        .expect("the File group carries a Post cell");
    {
        let mut harness = Harness::builder()
            .with_size(Vec2::new(1280.0, 720.0))
            .build_ui_state(frame_undrained, ShellUi::seeded());
        harness.run();
        let at = post.rect.center();
        set_pos(&mut harness, at);
        button_at(&mut harness, egui::PointerButton::Primary, at, true);
        button_at(&mut harness, egui::PointerButton::Primary, at, false);
        harness.run_steps(2);
        assert!(
            harness.state().state.pending.iter().any(|c| matches!(
                c,
                CommandKind::PostProgramDialog
            )),
            "the Post cell must fire through the actions law (pending: {:?})",
            harness.state().state.pending
        );
    }

    let mut harness = shell_harness();
    let path = std::env::temp_dir().join(format!("hsmworks-egui-post-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);

    // Phase 0 - the warned face of the gate, on the seed as shipped: its
    // contour takes the full block height, and plunging that rim beside
    // 30 mm of uncut wall is a ShankHit family the verifier already
    // cached. Fresh and generated is NOT enough - the post blocks by
    // name and writes nothing.
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    {
        let st = &harness.state().state;
        assert!(
            st.status.contains("post refused") && st.status.contains("post blocked")
                && st.status.contains("Contour1") && st.status.contains("shank"),
            "a warned path must block the gate (status: {})",
            st.status
        );
        assert!(!path.exists(), "a blocked post is never written");
    }

    // Phase 1 - the same contour re-cut as a 0.5 mm top skim is legal
    // geometry and a silent path: edit, regenerate, post, all through
    // the queue.
    {
        let mut skim = {
            let st = &harness.state().state;
            let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
            model::operation_of(&st.world, &e).expect("op").params.clone()
        };
        if let OpParams::Contour { bottom_z, .. } = &mut skim {
            *bottom_z = Height::fixed(29.5);
        }
        harness.state_mut().state.pending.push_back(CommandKind::EditOperationParams {
            name: "Contour1".into(),
            params: skim,
        });
        harness
            .state_mut().state.pending
            .push_back(CommandKind::GenerateOperation("Contour1".into()));
    }
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    assert!(st.status.starts_with("posted"), "skim post: {}", st.status);
    let text = std::fs::read_to_string(&path).expect("a posted program");
    assert!(
        text.starts_with('%')
            && text.contains("G90 G54")
            && text.contains("T01 M6")
            && text.trim_end().ends_with('%'),
        "one wrapped Fanuc program for the skim: {text}"
    );
    let before = text.clone();

    // The gate's refusal face, through the queue: an un-regenerated
    // parameter edit makes Contour1 stale, and a stale document must
    // not reach the machine.
    let mut edited_params = {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        model::operation_of(&st.world, &e).expect("op").params.clone()
    };
    // (A stepdown edit would be stale but silent here - the skim's
    // 0.5 mm span fits under any stepdown - so the witness is the
    // plunge feed, which rides every Z word's F.)
    if let OpParams::Contour { plunge_feed, .. } = &mut edited_params {
        *plunge_feed = 150.0;
    }
    harness.state_mut().state.pending.push_back(CommandKind::EditOperationParams {
        name: "Contour1".into(),
        params: edited_params,
    });
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    assert!(
        st.status.contains("post refused")
            && st.status.contains("Contour1")
            && st.status.contains("regenerate"),
        "a stale post must refuse by name (status: {})",
        st.status
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("still there"),
        before,
        "a refusal rewrites nothing"
    );

    // Regenerate through the same queue: fresh again, posts again.
    harness
        .state_mut().state.pending
        .push_back(CommandKind::GenerateOperation("Contour1".into()));
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    assert!(
        harness.state().state.status.starts_with("posted"),
        "regen status: {}",
        harness.state().state.status
    );
    assert_ne!(
        std::fs::read_to_string(&path).expect("rewritten"),
        before,
        "the re-posted program carries the new stepdown's passes"
    );
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

/// M3.1 C: the batch fix lives in the status strip, and appears exactly
/// when it means something - the moment a stale row exists to fix. The
/// click is one queue, one undo unit, and the button retires with the
/// staleness.
#[test]
fn status_bar_offers_regenerate_all_when_something_is_stale() {
    let mut harness = shell_harness();
    let edited = {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        let mut p = model::operation_of(&st.world, &e).expect("op").params;
        if let OpParams::Contour { plunge_feed, .. } = &mut p {
            *plunge_feed = 150.0;
        }
        p
    };
    harness
        .state_mut().state.pending
        .push_back(CommandKind::EditOperationParams { name: "Contour1".into(), params: edited });
    harness.run();
    {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("op");
        assert_eq!(hsmworks_app::operation::status_of(&st.world, &e), hsmworks_app::operation::OpStatus::Stale);
    }

    harness.get_by_label("Regenerate all stale (1)").click();
    harness.run();
    assert_eq!(harness.state().state.status, "regenerated 1 operation(s)");
    let st = &harness.state().state;
    let e = model::find_by_name(&st.world, "Contour1").expect("op");
    assert_eq!(hsmworks_app::operation::status_of(&st.world, &e), hsmworks_app::operation::OpStatus::Ok);
    // The affordance follows the state: no stale rows, no button.
    assert!(
        harness.query_by_label("Regenerate all stale (1)").is_none(),
        "the button outlived its staleness"
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
        (
            // A drill has no climb (the cycle owns the retract), no
            // Leads & Links (no wall to enter), and its Passes are the
            // cycle's own numbers - the tab law this row pins.
            "Drill",
            vec![
                ("Geometry", vec!["Click selects:", "Holes", "Detect holes"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "R plane (feed starts here)",
                        "Retract plane (between holes)",
                        "Hop below clearance",
                        "Depth",
                        "Override the hole floors",
                    ],
                ),
                ("Passes", vec!["Cycle", "Peck depth", "Dwell at bottom (s)"]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)"]),
            ],
        ),
        (
            // The 3-D pocket picks no geometry (the mesh silhouette is
            // the region - islands are computed, never selected): its
            // Geometry tab is the bare pick law, and its Passes carry
            // the entry ladder's own fields. "Retract Z", "Helix lead
            // radius" and "Max engagement (deg)" are guard-rendered
            // (retype's defaults have none of them on), so they are not
            // in this table.
            "3D Pocket",
            vec![
                ("Geometry", vec!["Click selects:"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "Retract plane",
                        "Hop below clearance",
                        "Top Z",
                        "Bottom Z",
                        "Stepdown",
                    ],
                ),
                ("Passes", vec![
                    "Stepover",
                    "Stock to leave",
                    "Strategy",
                    "Entry",
                    "Rest roughing",
                    "Feed optimization",
                ]),
                ("Feeds", vec!["Feed (mm/min)", "Plunge feed (mm/min)", "Climb milling"]),
            ],
        ),
        (
            // The finish picks no geometry (the whole model is its
            // region) and has no stepdown: tolerance authors the level
            // spacing itself, and - under the raster patterns - the line
            // spacing too (the cos-law, M2.2b). The Stepover row renders
            // because retype's default pattern is Parallel; its override
            // checkbox is the field's guard, not a field of its own.
            "3D Finish",
            vec![
                ("Geometry", vec!["Click selects:"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "Retract plane",
                        "Hop below clearance",
                        "Top Z",
                        "Bottom Z",
                    ],
                ),
                ("Passes", vec![
                    "Pattern",
                    "Stepover",
                    "Set line spacing",
                    "Floor the march's refinement",
                    "Minimum step",
                    "Ramp between lines",
                    "Tolerance",
                ]),
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

/// The pane half of the same law - the regression this pins: the egui port
/// kept `interact.rs`'s markers and viewport pick but lost the Origin
/// page's widgets (they lived in the imgui panes, retired un-ported at
/// M-g), so nothing could set `pick_anchor` (the crosses never rendered)
/// and the 3x3 grid was gone. The page must show the compass grid, place
/// the slot on the SAME point the markers draw from when a cell is
/// clicked (through SetWcs - the draft law), bracket the chosen cell, and
/// arm pick mode from the checkbox.
#[test]
fn origin_page_shows_the_anchor_grid_and_arms_pick_mode() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::WcsSlot(0);
    harness.run();
    // The draft default highlight is center (anchor_xy 4), so that cell
    // already reads bracketed - imgui's law, pinned here too.
    for name in ["NW", "N", "NE", "W", "C", "E", "SW", "S", "SE"] {
        let label = if name == "C" { "[C]" } else { name };
        assert!(
            harness.query_by_label(label).is_some(),
            "the compass cell {name} must be on the Origin page"
        );
    }
    assert!(!harness.state().state.wcs_ui.pick_anchor, "pick mode starts off");

    harness.get_by_label("SE").click();
    harness.run();
    let want = interact::anchor_points(&harness.state().state)[8].0;
    let e = model::find_by_name(&harness.state().state.world, WCS_SLOTS[0])
        .expect("the seed places G54");
    let q = model::transform_of(&harness.state().state.world, &e).unwrap_or_default().pos;
    let got = [q.x, q.y, q.z];
    assert!(
        got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6),
        "the grid click must place the slot on the anchor the markers draw ({q:?} vs {want:?})"
    );
    assert_eq!(harness.state().state.wcs_ui.anchor_xy, 8, "the click names the cell");
    // The chosen cell reads bracketed, imgui's highlight law.
    harness.run();
    assert!(
        harness.query_by_label("[SE]").is_some(),
        "the selected cell must show its brackets"
    );

    // The checkbox is the one writer of pick mode - without it the
    // viewport crosses are unreachable (this half of the regression).
    harness.get_by_label("Pick markers in viewport").click();
    harness.run();
    assert!(
        harness.state().state.wcs_ui.pick_anchor,
        "the checkbox must arm the marker overlay"
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
    expect(
        &mut harness,
        ctrl,
        Key::P,
        |c| matches!(c, CommandKind::PostProgramDialog),
        "Ctrl+P",
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
        (Cmd::Post, "Post NC program (validation gate), then review it\tCtrl+P"),
        (Cmd::Stock, "Stock definition"),
        (Cmd::Origin, "Work offset (WCS)"),
        (Cmd::Tools, "Tool library"),
        (Cmd::Create(0), "Add a Face mill operation"),
        (Cmd::Create(1), "Add a 2D Contour operation"),
        (Cmd::Create(2), "Add a 2D Pocket operation"),
        (Cmd::Create(3), "Add a Slot operation"),
        (Cmd::Create(4), "Add a Trace operation"),
        (Cmd::Create(5), "Add a Drill operation"),
        (Cmd::Create(6), "Add a 3D Pocket operation"),
        (Cmd::Create(7), "Add a 3D Finish operation"),
        (Cmd::Undo, "Undo\tCtrl+Z"),
        (Cmd::Redo, "Redo\tCtrl+Y"),
        (Cmd::Run, "Generate the selected operation"),
        (Cmd::Delete, "Delete the selection\tDel"),
        (Cmd::Frame, "Frame all\tF"),
        (Cmd::Paths, "Show every cached toolpath"),
        (Cmd::Simulate, "Simulate tool moves (transport bar)"),
        (Cmd::Deviate, "Judge the program's stock against the model (deviation map)"),
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

// --------------------------------------------- view cube and nav bar (M1.5)
//
// The furniture is painted, not widdgitised (the icon law's vector path), so
// these tests aim raw pointer events at rects the hud module recomputes from
// the laid-out pane - the same arithmetic the shell's own routing uses.

/// The furniture over the laid-out pane - `shell_layout`'s call, reproduced.
fn hud(sh: &ShellUi) -> viewcube::Hud {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    viewcube::Hud::new(egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)))
        .expect("the 1280-wide dock leaves the viewport room for the furniture")
}

fn face_center(h: &mut Harness<'static, ShellUi>, q: [f64; 3]) -> egui::Pos2 {
    let sh = h.state();
    viewcube::point_screen(&sh.state.cam, &hud(sh), q)
}

/// The cube's core claim: clicking a face's center turns the camera to that
/// standard view, immediately and with no command - and because the press is
/// never armed, the release picks nothing behind the cube.
#[test]
fn view_cube_face_click_orients_and_arms_nothing() {
    let mut harness = shell_harness();
    harness.run();
    let status0 = harness.state().state.status.clone();
    let (target0, dist0) = (
        harness.state().state.cam.target,
        harness.state().state.cam.dist,
    );
    let p = face_center(&mut harness, [0.0, 0.0, 1.0]);
    set_pos(&mut harness, p);
    button_at(&mut harness, egui::PointerButton::Primary, p, true);
    {
        let sh = harness.state();
        assert!(
            (sh.state.cam.pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "the TOP face must stand the camera over the part, pitch {}",
            sh.state.cam.pitch
        );
        assert_eq!((sh.state.cam.target, sh.state.cam.dist), (target0, dist0));
        assert!(sh.left_press.is_none(), "the furniture must never arm a press");
        assert!(sh.state.drag.is_none());
        assert!(sh.state.pending.is_empty(), "orientation queues nothing");
    }
    button_at(&mut harness, egui::PointerButton::Primary, p, false);
    harness.run_steps(2);
    let sh = harness.state();
    assert_eq!(sh.state.status, status0, "a cube click is not a pick");
    assert!(sh.left_press.is_none());
}

/// A press on the cube that is then dragged: oriented at the press, and the
/// motion must not promote to orbit - the gesture stays absorbed end to end.
#[test]
fn a_drag_started_on_the_cube_is_absorbed() {
    let mut harness = shell_harness();
    harness.run();
    let status0 = harness.state().state.status.clone();
    let p = face_center(&mut harness, [0.0, 0.0, 1.0]);
    set_pos(&mut harness, p);
    button_at(&mut harness, egui::PointerButton::Primary, p, true);
    let yaw0 = harness.state().state.cam.yaw;
    let d = p + egui::vec2(40.0, 0.0);
    set_pos(&mut harness, d);
    {
        let sh = harness.state();
        assert!(sh.state.drag.is_none(), "the cube must not promote a held press");
        assert_eq!(sh.state.cam.yaw, yaw0, "moving after a cube press must not orbit");
    }
    button_at(&mut harness, egui::PointerButton::Primary, d, false);
    harness.run_steps(2);
    let sh = harness.state();
    assert_eq!(sh.state.status, status0, "the release behind the cube must not pick");
    assert!(sh.state.pending.is_empty());
}

/// The nav column's five buttons: the exact house iso (after an orbit moved
/// it off), the two toggles flipping their fields twice, and Fit riding the
/// same queued `FrameAll` the F key pushes.
#[test]
fn the_nav_bar_drives_view_state() {
    let mut harness = shell_harness();
    harness.run();
    let p = {
        let h = hud(harness.state());
        h.nav_rect(viewcube::Nav::Frame).center()
    };

    // Fit rides the same queued `FrameAll` as the F key - proven through
    // the drain, like every other command test: asserting a command on
    // its press frame is egui-pass fragile (a later pass's drain can
    // execute it before we look), while the executed effect is the law.
    // Park the camera far off the framing, click, and it must be back.
    let framed = {
        let sh = harness.state();
        (sh.state.cam.target, sh.state.cam.dist)
    };
    harness.state_mut().state.cam.target = [400.0, 400.0, 400.0];
    harness.state_mut().state.cam.dist = 999.0;
    click_viewport(&mut harness, p);
    {
        let sh = harness.state();
        assert_eq!(
            (sh.state.cam.target, sh.state.cam.dist),
            framed,
            "Fit must run the same reframe the F key does"
        );
        assert!(
            !sh.state
                .pending
                .iter()
                .any(|c| matches!(c, CommandKind::FrameAll)),
            "the queued command outlived the drain"
        );
    }

    // Orbit away, then let Iso bring the house angle back exactly.
    let c = vp_center(harness.state());
    set_pos(&mut harness, c);
    button_at(&mut harness, egui::PointerButton::Primary, c, true);
    set_pos(&mut harness, c + egui::vec2(30.0, 0.0));
    button_at(&mut harness, egui::PointerButton::Primary, c + egui::vec2(30.0, 0.0), false);
    harness.run_steps(2);
    assert_ne!(harness.state().state.cam.yaw, 0.9, "the orbit must move yaw");
    let p = {
        let h = hud(harness.state());
        h.nav_rect(viewcube::Nav::Iso).center()
    };
    click_viewport(&mut harness, p);
    {
        let sh = harness.state();
        assert_eq!(sh.state.cam.yaw, 0.9, "Iso is the house yaw, exactly");
        assert_eq!(sh.state.cam.pitch, 0.5);
    }

    // Wireframe: the same field the Solids radios own, flipped twice.
    let p = {
        let h = hud(harness.state());
        h.nav_rect(viewcube::Nav::Wire).center()
    };
    click_viewport(&mut harness, p);
    assert_eq!(harness.state().state.display, crate::state::DisplayMode::Wireframe);
    click_viewport(&mut harness, p);
    assert_eq!(
        harness.state().state.display,
        crate::state::DisplayMode::ShadedEdges
    );

    // Paths and Axes flip their booleans, twice each, queueing nothing -
    // counting from their defaults (State::new: axes on, paths off).
    for (kind, want_paths, want_axes) in [
        (viewcube::Nav::Paths, true, true),
        (viewcube::Nav::Paths, false, true),
        (viewcube::Nav::Axes, false, false),
        (viewcube::Nav::Axes, false, true),
    ] {
        let p = {
            let h = hud(harness.state());
            h.nav_rect(kind).center()
        };
        click_viewport(&mut harness, p);
        let sh = harness.state();
        assert_eq!(sh.state.show_all_paths, want_paths);
        assert_eq!(sh.state.show_world_axes, want_axes);
        assert!(sh.state.pending.is_empty(), "{kind:?} queued a command");
    }
}

/// The furniture never interferes with the pane's own gestures: the center
/// click still picks (the seeded Block is under it), and the cube sits where
/// it cannot (top-right, away from the block's projection).
#[test]
fn the_cube_leaves_the_viewport_gestures_alone() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    let h = hud(harness.state());
    assert!(!h.contains(c), "the block under the pane center is not furniture");
    click_viewport(&mut harness, c);
    assert_eq!(
        harness.state().state.status,
        "selected Block",
        "the ordinary pick law must survive the cube"
    );
}

// ---------------------------------------------------------- M1.6: drilling

/// The 40x40x8 block bored through by a 16-facet bore - the app's own
/// recognition fixture, imported here so the UI law runs against the
/// same pixels a STEP file hands us.
fn bored_block() -> hsmworks_app::geometry::Mesh {
    use hsmworks_app::geometry::{Point3, Triangle};
    let tau = std::f64::consts::TAU;
    let bp = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        Point3::new(12.0 * a.cos(), 12.0 * a.sin(), z)
    };
    // Bore vertices pushed radially onto the 40x40 square: the diagonal
    // ones carry a corner, so each side's wall quads share its plane.
    let op = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * 20.0 / s, y * 20.0 / s, z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, facets: &mut Vec<Triangle>| {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    };
    let mut facets = Vec::new();
    for i in 0..16 {
        let j = i + 1;
        // Caps: annuli wound outward (+z top, -z bottom).
        quad(bp(j, 8.0), bp(i, 8.0), op(i, 8.0), op(j, 8.0), &mut facets);
        quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0), &mut facets);
        // Bore wall faces the axis; the block wall faces away.
        quad(bp(i, 8.0), bp(j, 8.0), bp(j, 0.0), bp(i, 0.0), &mut facets);
        quad(op(j, 8.0), op(i, 8.0), op(i, 0.0), op(j, 0.0), &mut facets);
    }
    hsmworks_app::geometry::Mesh { facets }
}

#[test]
fn ribbon_drill_cell_creates_a_drill_operation() {
    let mut harness = shell_harness();
    harness.run();
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let drill = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(5))
        .expect("the Create group carries a sixth, Drill cell");
    let at = drill.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Operation2")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .expect("the Drill cell created its operation through the actions law");
    // Empty holes at creation is legal (the page shows the list's
    // placeholder); the generator is what refuses, at Generate time.
    let OpParams::Drill { holes, cycle, .. } = &op.params else {
        panic!("the sixth cell must create a Drill operation, got {:?}", op.params.kind())
    };
    assert!(holes.is_empty(), "a fresh drill starts without a hole list");
    assert_eq!(*cycle, hsmworks_app::drilling::CycleChoice::Auto);
}

/// M3.2 stage E, the workspaces-by-kind law: the ribbon follows the
/// machine file's axis inventory. A lathe document darkens every mill
/// cell - honestly disabled with a tip that says why and when - and a
/// mill file lights them again. The UI never invents the kind; it reads
/// `machine::kind_of`, which reads the loaded `.mch`.
#[test]
fn a_lathe_machine_darkens_the_mill_cells() {
    let dir = std::env::temp_dir().join(format!("hsmw_kind_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let write = |name: &str, axes: &str| {
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!(
                r#"{{"general":{{"model":"TestBench","vendor":"Hsmworks Testbench"}},
                    "kinematics":{{"default":{{"units":{{"length":"mm"}},"parts":[
                      {axes}
                    ]}}}}}}"#
            ),
        )
        .expect("write machine file");
        path
    };
    let lathe = write(
        "lathe.mch",
        r#"{"type":"linear","name":"X","parts":[{"type":"linear","name":"Z"}]}"#,
    );
    let mill = write(
        "mill.mch",
        r#"{"type":"linear","name":"X","parts":[{"type":"linear","name":"Y","parts":[{"type":"linear","name":"Z"}]}]}"#,
    );

    let mut harness = shell_harness();
    harness.run();
    assert!(
        !harness.state().state.tool_names().is_empty(),
        "the demo document carries tools - the darkening is the machine, not the list"
    );

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadMachineFile(lathe));
    harness.run();
    let st = &harness.state().state;
    assert!(
        !panes::enabled(Cmd::Create(1), st),
        "a lathe machine must darken the mill cells"
    );
    assert!(
        !panes::enabled(Cmd::Create(0), st),
        "every Create cell, not just contour"
    );
    assert!(
        panes::tip_of(Cmd::Create(1), st).contains("lathe"),
        "the tip says what is dark and why: {:?}",
        panes::tip_of(Cmd::Create(1), st)
    );

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadMachineFile(mill));
    harness.run();
    let st = &harness.state().state;
    assert!(
        panes::enabled(Cmd::Create(1), st),
        "a Y axis in the file is the mill workspace again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// M3.3 stage C, the Setup pane's machine group headless: the built-in
/// library mounts by name, and the editor's Apply is a `LoadMachine`
/// at the mount's own path - so undo puts back the exact numbers the
/// library mounted, and a stranger name is refused by name.
#[test]
fn the_machine_group_mounts_the_library_and_the_editor_writes_through_history() {
    let mut harness = shell_harness();
    harness.run();
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Lathe".to_string()));
    harness.run();
    let (path, mounted) = hsmworks_app::machine::of(&harness.state().state.world);
    assert_eq!(path, "built-in: Lathe", "the mount records through the shared built-in path");
    let mut mounted = mounted.expect("the library lathe is mounted");
    assert_eq!(
        hsmworks_app::machine::kind(Some(&mounted)),
        hsmworks_app::machine::Kind::Lathe,
        "the declared kind drives the workspaces, not a guess"
    );
    // A declared lathe darkens the mill cells just like the inferred
    // one the test above judges - one law, two roads.
    assert!(!panes::enabled(Cmd::Create(1), &harness.state().state));

    let spindle_max = mounted.spindle_max;
    // The editor's Apply, fed the draft the dialog would have shown
    // with one number changed.
    mounted.spindle_max = Some(4321.0);
    harness.state_mut().state.pending.push_back(CommandKind::EditMachineApply {
        path,
        machine: Box::new(mounted),
    });
    harness.run();
    let (_, m) = hsmworks_app::machine::of(&harness.state().state.world);
    assert_eq!(
        m.expect("edited").spindle_max,
        Some(4321.0),
        "Apply reached the world through the history"
    );

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    let (_, m) = hsmworks_app::machine::of(&harness.state().state.world);
    assert_eq!(
        m.expect("mounted").spindle_max,
        spindle_max,
        "undo restores the library's exact numbers"
    );

    // A name the library does not hold is refused by name, and the
    // document stands exactly where it stood.
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Drill press".to_string()));
    harness.run();
    assert!(
        harness.state().state.status.contains("Drill press"),
        "the refusal names the stranger: {:?}",
        harness.state().state.status
    );
    let (_, m) = hsmworks_app::machine::of(&harness.state().state.world);
    assert_eq!(m.expect("still mounted").spindle_max, spindle_max);
}

#[test]
fn ribbon_pocket3d_cell_creates_a_3d_pocket_operation() {
    let mut harness = shell_harness();
    harness.run();
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let pocket3d = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(6))
        .expect("the Create group carries a seventh, 3D Pocket cell");
    let at = pocket3d.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Operation2")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .expect("the 3D Pocket cell created its operation through the actions law");
    // The defaults state the level law with: a plunge ladder from the
    // stock top, Auto strategy - no geometry pick field exists for this
    // kind because the region is the model's silhouette.
    let OpParams::Pocket3D { strategy, entry, .. } = &op.params else {
        panic!(
            "the seventh cell must create a 3D Pocket operation, got {:?}",
            op.params.kind()
        )
    };
    assert_eq!(*strategy, hsmworks_app::machining2d::PocketStrategy::Auto);
    assert_eq!(*entry, hsmworks_app::machining3d::Entry3D::Plunge);
}

#[test]
fn ribbon_finish3d_cell_creates_a_3d_finish_operation() {
    let mut harness = shell_harness();
    harness.run();
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let finish3d = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(7))
        .expect("the Create group carries an eighth, 3D Finish cell");
    let at = finish3d.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Operation2")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .expect("the 3D Finish cell created its operation through the actions law");
    // The tolerance is the op's one shaping number (the stepdown law
    // derives from it and the tool), and no hop is on by default - the
    // pocket family's clearance law, quoted by the draft's own default.
    // The pattern defaults to the auto-split, never plain Waterline:
    // waterline alone leaves every flat shelf standing, and a silent
    // miss is the one answer this app never gives.
    let OpParams::Finish3D {
        pattern,
        stepover,
        tolerance,
        retract_z,
        ..
    } = &op.params
    else {
        panic!(
            "the eighth cell must create a 3D Finish operation, got {:?}",
            op.params.kind()
        )
    };
    assert_eq!(*tolerance, 0.05);
    assert_eq!(*retract_z, None);
    assert_eq!(
        *pattern,
        hsmworks_app::finishing3d::FinishPattern::Parallel,
        "the fresh finish op must ride flats and walls alike"
    );
    assert_eq!(*stepover, None, "spacing follows the cos-law until asked not to");
}

/// The M2.2b fields obey the draft law exactly as every other pair of
/// fields does: a staged change to pattern and stepover reaches the
/// world only through Apply, only as an EditOperationParams, and undo
/// brings the auto-split default back. (The combo and the checkbox are
/// custom-drawn and unreachable to the harness's click law - staging
/// the draft by hand is this pane's stand-in, the same route
/// `click_apply_edits_operation_through_commands` documents.)
#[test]
fn finish3d_pattern_and_stepover_apply_through_commands() {
    let mut harness = shell_harness();
    crate::actions::add_operation(&mut harness.state_mut().state, 7);
    harness.run_steps(2);
    let staged = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
        _ => panic!("the pane must draft a 3D Finish op"),
    };
    let (p0, s0) = staged(&harness);
    assert_eq!(p0, hsmworks_app::finishing3d::FinishPattern::Parallel);
    assert_eq!(s0, None);
    {
        let sh = harness.state_mut();
        sh.state.op_tab = 2; // Passes
        sh.state.sync_ops_ui();
        if let OpParams::Finish3D { pattern, stepover, .. } = &mut sh.state.op_ui.params {
            *pattern = hsmworks_app::finishing3d::FinishPattern::Waterline;
            *stepover = Some(1.5);
        }
    }
    harness.run();
    {
        let st = &harness.state().state;
        assert!(
            !st.pending
                .iter()
                .any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
            "staging must not queue - only Apply speaks"
        );
    }
    harness.get_by_label("Apply Parameters").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply Parameters").click();
    harness.run();
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e));
    let live = op.map(|o| match o.params {
        OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
        _ => panic!("kind drift"),
    });
    assert_eq!(
        live,
        Some((
            hsmworks_app::finishing3d::FinishPattern::Waterline,
            Some(1.5),
        )),
        "Apply must carry pattern and stepover to the world"
    );
    // Undo through the keyboard lane - the same pair the history-key
    // test injects; there is no Undo button in the pane to click.
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
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
            _ => panic!("kind drift"),
        });
    assert_eq!(
        back,
        Some((hsmworks_app::finishing3d::FinishPattern::Parallel, None)),
        "undo must restore the defaults the cell seeded"
    );
    // M2.2c: the Scallop word rides the same command lane. Stage it,
    // apply, and the world reads the crossed diagonals; undo returns
    // to the auto-split default.
    {
        let sh = harness.state_mut();
        sh.state.sync_ops_ui();
        if let OpParams::Finish3D { pattern, .. } = &mut sh.state.op_ui.params {
            *pattern = hsmworks_app::finishing3d::FinishPattern::Scallop;
        }
    }
    harness.run();
    harness.get_by_label("Apply Parameters").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply Parameters").click();
    harness.run();
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
            _ => panic!("kind drift"),
        });
    assert_eq!(
        op,
        Some((hsmworks_app::finishing3d::FinishPattern::Scallop, None)),
        "Apply must carry the scallop word to the world"
    );
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
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
            _ => panic!("kind drift"),
        });
    assert_eq!(
        back,
        Some((hsmworks_app::finishing3d::FinishPattern::Parallel, None)),
        "undo must invert the scallop apply like any other"
    );
    // M2.2e stage D: the ramp checkbox rides the same command lane, on
    // the same tab (the draft is back on Parallel, so the widget is
    // shown - Waterline would hide it). Stage by click, apply, undo.
    harness.state_mut().state.sync_ops_ui();
    harness.run();
    harness.get_by_label("Ramp between lines").scroll_to_me();
    harness.run();
    harness.get_by_label("Ramp between lines").click();
    harness.run();
    harness.get_by_label("Apply Parameters").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply Parameters").click();
    harness.run();
    let st = &harness.state().state;
    let ramped = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { ramp, .. } => ramp,
            _ => panic!("kind drift"),
        });
    assert_eq!(ramped, Some(true), "Apply must carry the ramp checkbox to the world");
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
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { ramp, .. } => ramp,
            _ => panic!("kind drift"),
        });
    assert_eq!(back, Some(false), "undo restores the seeded no-ramp default");
    // M2.2e stage E: the refinement floor rides the same lane, and
    // like the stepover override it is a checkbox that manufactures
    // its own number - the click alone stages Some(scalar), so the
    // Apply after a bare click carries the seeded floor, and undo
    // returns the march to its own 1/4096 right.
    harness.state_mut().state.sync_ops_ui();
    harness.run();
    harness.get_by_label("Floor the march's refinement").scroll_to_me();
    harness.run();
    harness.get_by_label("Floor the march's refinement").click();
    harness.run();
    let staged = match &harness.state().state.op_ui.params {
        OpParams::Finish3D { min_step, .. } => *min_step,
        _ => panic!("kind drift"),
    };
    assert!(staged.is_some(), "the floor checkbox staged no number");
    harness.get_by_label("Apply Parameters").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply Parameters").click();
    harness.run();
    let st = &harness.state().state;
    let floored = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { min_step, .. } => min_step.is_some(),
            _ => panic!("kind drift"),
        });
    assert_eq!(floored, Some(true), "Apply must carry the floor to the world");
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
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "Operation2")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { min_step, .. } => min_step,
            _ => panic!("kind drift"),
        });
    assert_eq!(back, Some(None), "undo restores the march's own floor");
}

#[test]
fn detect_holes_refuses_recognizes_and_undo_empties_again() {
    let mut harness = shell_harness();
    crate::actions::add_operation(&mut harness.state_mut().state, 5);
    harness.run_steps(2);
    // The pane now targets the new Drill op; its Geometry tab is the
    // Detect button's home, and the draft reads the created params.
    {
        let sh = harness.state_mut();
        sh.state.op_tab = 0;
        sh.state.sync_ops_ui();
    }
    harness.run();

    // 1. Refusal, honest and scoped: the op still stands on the stock,
    // and a stock's box walls are not holes.
    harness.get_by_label("Detect holes").click();
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        assert!(st.pending.is_empty(), "a refused detection must queue nothing");
        assert!(
            st.error_for(crate::state::ErrorScope::Operation)
                .is_some_and(|m| m.contains("no drilled holes")),
            "the refusal must name the fix, got: {}",
            st.status
        );
    }

    // 2. Aim the op at a plate that does have a bore, and click again.
    // The import goes straight through history (the mesh-import command
    // has no queue arm - the file dialog's worker executes it this way
    // too); the retarget rides the queue like any pane edit.
    {
        let sh = harness.state_mut();
        sh.state.pending.push_back(CommandKind::EditOperationLinks {
            name: "Operation2".into(),
            tool: None,
            src: Some(SrcRef::Entity("Plate".into())),
        });
        let (world, history) = (&sh.state.world, &mut sh.state.history);
        history.execute(
            world,
            hsmworks_app::commands::ImportModel::from_mesh("Plate", bored_block()),
        );
    }
    harness.run_steps(2);
    harness.get_by_label("Detect holes").click();
    harness.run_steps(2);
    let op_params = {
        let st = &harness.state().state;
        model::find_by_name(&st.world, "Operation2")
            .as_ref()
            .and_then(|e| model::operation_of(&st.world, e))
            .expect("Operation2 survives")
            .params
            .clone()
    };
    let OpParams::Drill { holes, .. } = &op_params else { unreachable!() };
    assert_eq!(holes.len(), 1, "the bore is the plate's only hole");
    assert!(holes[0].through && (holes[0].radius - 12.0).abs() < 1e-6);
    // The draft mirrors what the world now holds - rows show, no lie.
    assert_eq!(harness.state().state.op_ui.params, op_params);

    // 3. Undo retires the detection alone - holes were a command.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Operation2")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .unwrap();
    let OpParams::Drill { holes, .. } = &op.params else { unreachable!() };
    assert!(holes.is_empty(), "undo must empty the detected list again");
}

// ------------------------------------------- M1.7c: move simulation

/// The playhead's law, pinned pure (kittest frames advance ~0 seconds,
/// so the arithmetic must be proven directly): time moves only while
/// playing, speed rides dt, the clamp lands exactly on the end and
/// stops there - no wrap - and pathological seconds freeze the
/// playhead rather than poison it.
#[test]
fn playhead_advance_law() {
    use crate::state::Sim;
    let mut s = Sim::new();
    assert!(s.playing, "arming a simulation starts it running");
    s.advance(2.0, 10.0);
    assert_eq!(s.t, 2.0);
    s.playing = false;
    s.advance(2.0, 10.0);
    assert_eq!(s.t, 2.0, "paused time stands still");
    s.playing = true;
    s.speed = 4.0;
    s.advance(2.0, 10.0);
    assert_eq!(s.t, 10.0, "4x of 2 s must land exactly on the end");
    assert!(!s.playing, "the playhead stops at the end - it never wraps");
    s.playing = true; // even a manual re-play must not push t past total
    s.advance(1.0, 10.0);
    assert_eq!(s.t, 10.0);
    let mut n = Sim::new();
    n.advance(f64::NAN, 10.0);
    assert_eq!(n.t, 0.0, "a NaN dt freezes the playhead");
    n.advance(1.0, f64::NAN);
    assert_eq!(n.t, 0.0, "a NaN total freezes it too");
    n.speed = f64::NAN;
    n.advance(1.0, 10.0);
    assert!(n.t.is_finite(), "a poisoned speed must never move t");
}

/// The ribbon gate mirrors the post gate: a document with a generated
/// operation arms the cell, an empty one does not. The transport bar is
/// real shipping UI (Play/Pause by label, Stop disarms), and the whole
/// point is view state - arming queues no command and undo never sees
/// it.
#[test]
fn simulate_cell_arms_transport_and_stop_disarms() {
    let mut harness = shell_harness();
    harness.run();
    assert!(
        panes::enabled(Cmd::Simulate, &harness.state().state),
        "the seeded doc has a generated op - the cell must be live"
    );
    assert!(
        !panes::enabled(Cmd::Simulate, &State::new()),
        "nothing to simulate on an empty document"
    );
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let cell = cells
        .iter()
        .find(|c| c.cmd == Cmd::Simulate)
        .expect("the Inspect group carries a Simulate cell");
    let at = cell.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        assert!(st.sim.is_some(), "the cell must arm the transport");
        assert!(st.sim.as_ref().is_some_and(|s| s.playing), "armed means running");
        assert!(panes::toggled(Cmd::Simulate, st));
        // The command law: nothing was queued and nothing can be undone.
        assert!(st.pending.is_empty());
    }
    harness.get_by_label("Pause").click();
    harness.run_steps(2);
    assert!(
        harness.state().state.sim.as_ref().is_some_and(|s| !s.playing),
        "the transport's Pause must reach the playhead"
    );

    // Play at the end rewinds - no separate back-to-start control.
    {
        let sim = harness.state_mut().state.sim.as_mut().unwrap();
        sim.t = 1.0e9; // parked past the finish, as a finished run is
    }
    harness.run_steps(1); // let the slider/advance settle the parked value
    harness.get_by_label("Play").click();
    harness.run_steps(1);
    {
        let sim = harness.state().state.sim.as_ref().unwrap();
        assert!(sim.playing, "Play at the end restarts the run");
        assert!(sim.t < 1.0, "rewound playhead sits at the start, got {}", sim.t);
    }

    // Stop leaves simulation: the cell un-toggles, the bar is gone.
    harness.get_by_label("Stop").click();
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(st.sim.is_none());
    assert!(!panes::toggled(Cmd::Simulate, st));
}

/// The Deviate readout's sentence law, pinned pure (M2.3 stage B): the
/// clean program says exactly one thing; an unclean one names each
/// truth it found - count, volume, worst depth and the WHERE - and a
/// located verdict that somehow lost its location admits it with "?"
/// rather than printing a lie. The bar under the viewport is this
/// string and nothing else, so the string carries the whole verdict.
#[test]
fn deviate_readout_sentences_each_truth() {
    use hsmworks_app::geometry::Point3;
    let d = |rest_cells, gouge_cells, max_rest, max_gouge, tol| {
        hsmworks_app::stock_sim::Deviation {
            cells_x: 8,
            cells_y: 8,
            cell: 1.0,
            tol,
            dev: Vec::new(),
            rest_cells,
            gouge_cells,
            rest_volume: rest_cells as f64 * max_rest,
            gouge_volume: gouge_cells as f64 * max_gouge,
            max_rest,
            max_gouge,
            max_rest_at: (rest_cells > 0).then(|| Point3::new(1.25, -0.75, 18.0)),
            max_gouge_at: (gouge_cells > 0).then(|| Point3::new(-20.0, 5.5, 16.0)),
        }
    };
    let clean = crate::actions::deviate_readout(&d(0, 0, 0.0, 0.005, 0.01));
    assert_eq!(clean, "deviate: clean - the program meets the model everywhere");
    // A within-tolerance worst is still the clean sentence - the
    // display tolerance IS the verdict, per DEV_TOL's law.
    assert_eq!(
        crate::actions::deviate_readout(&d(3, 0, 0.004, 0.0, 0.01)),
        clean,
        "rest thinner than tolerance must not shout"
    );
    let rest = crate::actions::deviate_readout(&d(40, 0, 2.0, 0.0, 0.01));
    assert!(
        rest.starts_with("deviate: REST 40 cells (80.00")
            && rest.contains("worst 2.00 mm at (1.25, -0.75, z 18.00)"),
        "the rest sentence: {rest}"
    );
    assert!(!rest.contains("GOUGE"), "no gouge, no gouge clause: {rest}");
    let both = crate::actions::deviate_readout(&d(1, 7, 0.5, 1.25, 0.01));
    assert!(both.contains("REST") && both.contains("GOUGE"), "two truths, one line: {both}");
    assert!(both.contains("(-20.00, 5.50, z 16.00)"), "the gouge locates too: {both}");
    // A located aggregate whose point is gone prints "?" - an honest
    // gap, never fabricated coordinates.
    let mut lost = d(0, 4, 0.0, 0.9, 0.01);
    lost.max_gouge_at = None;
    assert!(
        crate::actions::deviate_readout(&lost).contains("GOUGE 4 cells") && both_needs_question(&lost),
        "the missing WHERE must show as '?'"
    );
    fn both_needs_question(d: &hsmworks_app::stock_sim::Deviation) -> bool {
        crate::actions::deviate_readout(d).ends_with("mm?)")
    }
}

/// The Deviate ribbon cell is Simulate's sibling under the same laws:
/// gate = a generated program exists; a click runs the sim over the
/// live world and stores the verdict as pure view state - no command,
/// nothing undoable - the second click disarms, and the seeded doc's
/// verdict is present (its stock leaves skin, so don't assert a clean
/// sentence here - the shape is the app tests' job).
#[test]
fn deviate_cell_stores_the_verdict_as_view_state() {
    let mut harness = shell_harness();
    harness.run();
    assert!(
        panes::enabled(Cmd::Deviate, &harness.state().state),
        "the seeded doc has a generated op - the cell must be live"
    );
    assert!(
        !panes::enabled(Cmd::Deviate, &State::new()),
        "nothing to judge on an empty document"
    );
    let (cells, _, _) = panes::ribbon_cells(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        Vec2::new(1280.0, super::shell::RIBBON_H),
    ));
    let cell = cells
        .iter()
        .find(|c| c.cmd == Cmd::Deviate)
        .expect("the Inspect group carries a Deviate cell");
    let at = cell.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        let ps = st.program_sim.as_ref().expect("the cell must arm the verdict");
        assert_eq!(st.status, crate::actions::deviate_readout(&ps.dev));
        assert!(!st.status.contains("refused"), "the seed judges: {}", st.status);
        assert!(panes::toggled(Cmd::Deviate, st));
        // The command law: judging the program writes no world state.
        assert!(st.pending.is_empty());
    }
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(st.program_sim.is_none(), "the second click disarms");
    assert_eq!(st.status, "deviate off");
    assert!(!panes::toggled(Cmd::Deviate, st));
}

/// The readout string is a pure law too - the bar's whole job is this
/// text, and the seeded contour pins op name, verb progression and the
/// units lens without opening a window.
#[test]
fn sim_readout_names_the_op_and_the_verb() {
    let sh = ShellUi::seeded();
    let sc = super::shell::sim_script(&sh.state);
    assert!(!sc.is_empty() && sc.total > 0.0, "the seed must produce a timed path");
    assert_eq!(sc.ops.len(), 1);
    let s = super::shell::sim_readout(&sc, 0.0, Unit::Millimeters);
    assert!(s.starts_with("Contour1 · STANDBY · "), "before the first move: {s}");
    let r = super::shell::sim_readout(&sc, sc.cum[0] * 0.5, Unit::Millimeters);
    assert!(r.starts_with("Contour1 · RAPID · "), "the contour opens on a rapid: {r}");
    let e = super::shell::sim_readout(&sc, sc.total, Unit::Millimeters);
    assert!(e.starts_with("Contour1 · END · "), "the finish says so: {e}");
    // The first cutting move names its feed in the active unit system.
    let fi = sc
        .segs
        .iter()
        .position(|g| g.secs > 0.0 && g.feed.is_some())
        .expect("a contour has feed moves");
    let start = if fi == 0 { 0.0 } else { sc.cum[fi - 1] };
    let mid = start + (sc.cum[fi] - start) * 0.5;
    let f = super::shell::sim_readout(&sc, mid, Unit::Millimeters);
    assert!(f.starts_with("Contour1 · F ") && f.contains("mm/min"), "feed verb: {f}");
    let i = super::shell::sim_readout(&sc, mid, Unit::Inches);
    assert!(i.contains("in/min"), "the verb rides the units lens: {i}");
    // An empty script has no name to show and must not pretend.
    let none = super::shell::sim_readout(
        &hsmworks_app::sim::Script::default(),
        0.0,
        Unit::Millimeters,
    );
    // The empty script is spent by sim.rs law (no segments, done at once),
    // so the verb is END - the bar itself never shows it (the total==0 arm
    // prints "nothing to run"); this row pins the name, not the verb.
    assert!(none.starts_with("(none) · END · "), "degenerate: {none}");
}

// ------------------------------------------- M1.7d: G-code viewer

#[test]
fn gcode_segment_highlights_the_move_the_scroll_line_posts() {
    // The resolver behind the amber highlight, pure: the segment is the
    // path's own polyline arriving at the cursor line - previous stored
    // position to current; None lines rest the cursor where it was; the
    // first position collapses to its endpoint ring; a closed viewer
    // (hidden is display, M1.7c's law) highlights nothing.
    let mk = |line_pos: Vec<Option<[f64; 3]>>, line: usize, open: bool| {
        let mut st = State::new();
        st.post_view = Some(PostView {
            path: std::path::PathBuf::from("p.nc"),
            program: String::new(),
            line_pos,
            open,
            line,
            blocks: Vec::new(),
            trace: false,
        });
        super::shell::gcode_segment(&st)
    };
    let chain = || {
        vec![
            Some([0.0, 0.0, 10.0]),
            None,
            Some([10.0, 0.0, 10.0]),
            Some([10.0, 0.0, 0.0]),
        ]
    };
    assert_eq!(mk(chain(), 0, true), Some(([0.0, 0.0, 10.0], [0.0, 0.0, 10.0])));
    assert_eq!(
        mk(chain(), 1, true),
        Some(([0.0, 0.0, 10.0], [0.0, 0.0, 10.0])),
        "a comment line rests the cursor on the last move"
    );
    assert_eq!(
        mk(chain(), 2, true),
        Some(([0.0, 0.0, 10.0], [10.0, 0.0, 10.0])),
        "the segment arriving at that line's position"
    );
    assert_eq!(
        mk(chain(), 99, true),
        Some(([10.0, 0.0, 10.0], [10.0, 0.0, 0.0])),
        "a past-the-end cursor clamps to the last move"
    );
    assert_eq!(mk(chain(), 2, false), None);
    assert_eq!(mk(vec![], 0, true), None, "header-only program: nothing to highlight");
    assert_eq!(mk(vec![None, None], 1, true), None);
    // No viewer at all (fresh document) resolves to nothing either.
    assert_eq!(super::shell::gcode_segment(&State::new()), None);
}

#[test]
fn a_successful_post_opens_the_viewer_and_new_document_retires_it() {
    // The real law end to end: the gate's own story (edit, regenerate,
    // skim post) is M1.7b's test at the head of this file - reused here
    // untouched, because what this pins is the viewer half: text and
    // positions from the same engine run, open on success, gone with
    // the document.
    let mut harness = shell_harness();
    let path = std::env::temp_dir().join(format!("hsmworks-egui-view-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut skim = {
        let st = &harness.state().state;
        let e = hsmworks_app::model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        hsmworks_app::model::operation_of(&st.world, &e)
            .expect("op")
            .params
            .clone()
    };
    if let hsmworks_app::operation::OpParams::Contour { bottom_z, .. } = &mut skim {
        *bottom_z = hsmworks_app::operation::Height::fixed(29.5);
    }
    harness.state_mut().state.pending.push_back(CommandKind::EditOperationParams {
        name: "Contour1".into(),
        params: skim,
    });
    harness
        .state_mut().state.pending
        .push_back(CommandKind::GenerateOperation("Contour1".into()));
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    {
        let st = &harness.state().state;
        assert!(st.status.starts_with("posted"), "skim post: {}", st.status);
        let v = st.post_view.as_ref().expect("a successful post carries the viewer");
        assert!(v.open, "the post opens the window");
        assert_eq!(
            v.program.lines().count(),
            v.line_pos.len(),
            "one position slot per program line"
        );
        assert!(
            v.line_pos.iter().any(|p| p.is_some()),
            "a contour program has moves to highlight"
        );
        // The cursor starts at the head when the post sets it; frames
        // after the window opens publish a center row. What must hold
        // either way: it is a line the program has, never past the end.
        assert!(v.line < v.program.lines().count(), "cursor in range: {}", v.line);
    }
    let _ = std::fs::remove_file(&path);
    harness.state_mut().state.pending.push_back(CommandKind::NewDocument);
    harness.run();
    assert!(
        harness.state().state.post_view.is_none(),
        "a frozen program is evidence of the old document, never the new one"
    );
}

#[test]
fn the_viewer_window_publishes_the_scroll_cursor() {
    // Paint-side law, no dialog: the window pass writes the middle row
    // back as the sync cursor, and while the window is open that cursor
    // resolves to a highlighted segment. 120 lines at ~400 px cannot
    // all show, so the published row is a real center row - never the
    // head, never past the tail.
    let mut harness = shell_harness();
    harness.state_mut().state.post_view = Some(PostView {
        path: std::path::PathBuf::from("p.nc"),
        program: (0..120)
            .map(|i| format!("G01 X{i}.0 Y0. F200."))
            .collect::<Vec<_>>()
            .join("\n"),
        line_pos: (0..120).map(|i| Some([i as f64, 0.0, 0.0])).collect(),
        open: true,
        line: 0,
        blocks: Vec::new(),
        trace: false,
    });
    harness.run();
    {
        let st = &harness.state().state;
        let v = st.post_view.as_ref().expect("the frame keeps the viewer");
        assert!(v.line > 0 && v.line < 120, "center row published: {}", v.line);
        assert!(super::shell::gcode_segment(st).is_some(), "and it highlights");
    }
    harness.state_mut().state.post_view.as_mut().expect("still there").open = false;
    assert!(
        super::shell::gcode_segment(&harness.state().state).is_none(),
        "closing the window hides the highlight"
    );
}

/// M3.2 stage B, the tree's fixture law: a tagged model reads as work-
/// holding steel - vise glyph, "(fixture)" in the status column - while
/// keeping the row's hideability (a parked vise still gets an eye). The
/// mapping is pure so both branches are checked without a world.
#[test]
fn a_fixture_model_reads_as_a_vise_in_the_tree() {
    let (suffix, kind, icon) = panes::model_row_marks(true, false);
    assert_eq!(suffix, " (fixture)");
    assert_eq!(kind, painted::Kind::Fixture);
    assert_eq!(icon, Icon::Fixture);
    // A plain model row is untouched: the change is marker-driven,
    // never name-driven.
    let (suffix, kind, icon) = panes::model_row_marks(false, false);
    assert_eq!(suffix, "");
    assert_eq!(kind, painted::Kind::Model);
    assert_eq!(icon, Icon::Model);
    // Both markers, in column order: fixture first, then display state.
    let (suffix, _, _) = panes::model_row_marks(true, true);
    assert_eq!(suffix, " (fixture) (hidden)");
    // The eye law: hideable rows own the eye column; fixtures are
    // hideable models.
    assert!(painted::has_eye(painted::Kind::Fixture));
    assert!(painted::has_eye(painted::Kind::Model));
    assert!(!painted::has_eye(painted::Kind::Section));
}

#[test]
fn selecting_a_post_script_is_a_command_with_the_pane_labels() {
    // M3.3 A's UI half, headless: the file arm lands a user script in
    // the world singleton and says so on the status line; the
    // built-in arm resets; undo puts the previous choice back; and a
    // script that does not exist is a visible refusal, never a quiet
    // keep-the-old choice.
    use hsmworks_app::posting;
    use hsmworks_app::schema::PostRef;

    let dir = std::env::temp_dir().join(format!("hsmw-ui-post-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let cps = dir.join("userpost.cps");
    std::fs::write(&cps, "function onOpen() { writeln(\"(UIPOST-3)\"); }").expect("write cps");

    let mut harness = shell_harness();
    harness.run();
    assert_eq!(posting::post_of(&harness.state().state.world), PostRef::Builtin);

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::SelectPostFile(cps.clone()));
    harness.run();
    {
        let st = &harness.state().state;
        assert!(st.status.contains("post:"), "{}", st.status);
        assert!(matches!(posting::post_of(&st.world), PostRef::File(_)));
    }

    // Undo the selection - the built-in is back, by the history law.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert_eq!(posting::post_of(&harness.state().state.world), PostRef::Builtin);

    // The built-in arm on an already-built-in document refuses visibly
    // (the "already selected" law), and a missing file names itself.
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::SelectPostBuiltin);
    harness.run();
    assert!(
        harness.state().state.status.contains("post not selected"),
        "a no-op choice is a stated refusal: {}",
        harness.state().state.status
    );
    harness.state_mut().state.pending.push_back(CommandKind::SelectPostFile(
        dir.join("gone.cps"),
    ));
    harness.run();
    let st = &harness.state().state;
    assert!(st.status.contains("post not selected") && st.status.contains("gone.cps"), "{:?}", st.status);

    let _ = std::fs::remove_dir_all(&dir);
}
