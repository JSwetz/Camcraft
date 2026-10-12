//! UI shell smoke tests: real ImGui frames, no GPU, no window.
//! Provenance: our own harness; behavior specced from the shell itself.

#[test]
fn headless_frames_run() {
    hsmworks_ui::headless::run_frames(8)
        .map_err(|err| panic!("headless run failed: {err}"))
        .ok();
}

#[test]
fn headless_cli_exit_code_is_zero() {
    assert_eq!(hsmworks_ui::run(&[String::from("--headless")]), 0);
}

#[test]
fn state_starts_clean() {
    let state = hsmworks_ui::state::State::new();
    assert_eq!(state.status, "ready");
    assert!(!state.has_selection());
    assert!(!state.history.can_undo());
}

#[test]
fn selecting_a_missing_model_is_a_no_op_not_a_crash() {
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(CommandKind::Select(Some("ghost".into())));
    state.drain_actions();
    assert!(!state.has_selection());
}

#[test]
fn camera_frame_all_on_empty_world_is_polite() {
    let mut state = hsmworks_ui::state::State::new();
    state.pending.push_back(hsmworks_ui::state::CommandKind::FrameAll);
    state.drain_actions();
    assert_eq!(state.status, "nothing to frame");
}

#[test]
fn import_completes_on_the_worker_thread() {
    use hsmworks_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle(); // pumps the worker to its result
    assert!(state.status.starts_with("imported cube"), "{:?}", state.status);
    assert!(state.history.can_undo());

    state.pending.push_back(CommandKind::Undo);
    state.settle();
    assert!(!state.history.can_undo());
    assert!(state.history.can_redo());
}

#[test]
fn corrupt_import_errors_on_the_worker_never_panics() {
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    // Dispatch refuses by extension before reading; no IGES kernel exists.
    state
        .pending
        .push_back(CommandKind::ImportFile(std::path::PathBuf::from("x.iges")));
    state.settle();
    assert!(state.status.starts_with("import refused"), "{:?}", state.status);
    assert!(!state.history.can_undo());
}
