//! The op menu's suppress and after-action rows and the post page's
//! "Skip pauses and homes" switch (#110): each queues its command, and
//! each rides the history - undo and redo included.
//! Provenance: our own page law; no vendor material consulted.

use camcraft_app::model;
use camcraft_app::operation::AfterOperation;
use camcraft_app::posting;

use super::dialog_tests::{face, opened};
use crate::state::{CommandKind, State};

fn op(state: &State) -> camcraft_app::schema::Operation {
    model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .expect("the op")
}

fn run(state: &mut State, kind: CommandKind) {
    state.pending.push_back(kind);
    state.drain_actions();
}

#[test]
fn after_suppress_and_skip_commands_commit_and_undo() {
    let mut state = opened(face());
    assert_eq!(op(&state).after, AfterOperation::None, "a new op carries on");
    run(&mut state, CommandKind::EditOperationAfter { name: "Op".into(), after: AfterOperation::ProgramStop });
    assert_eq!(op(&state).after, AfterOperation::ProgramStop);
    run(&mut state, CommandKind::Undo);
    assert_eq!(op(&state).after, AfterOperation::None, "Ctrl+Z restores it");
    run(&mut state, CommandKind::Redo);
    assert_eq!(op(&state).after, AfterOperation::ProgramStop, "Ctrl+Y reapplies it");

    run(&mut state, CommandKind::SuppressOperation { name: "Op".into(), suppressed: true });
    assert!(op(&state).suppressed);
    run(&mut state, CommandKind::Undo);
    assert!(!op(&state).suppressed, "undo unsuppresses");

    run(&mut state, CommandKind::SetSkipStops(true));
    assert!(posting::post_options(&state.world).skip_stops);
    run(&mut state, CommandKind::Undo);
    assert!(!posting::post_options(&state.world).skip_stops, "undo turns the switch off");
}

/// #111: the Output choice queues `SetPerOpOutput`, rides the history,
/// and a per-op post into a folder writes one file per operation.
#[test]
fn output_per_op_commits_undoes_and_posts_into_a_folder() {
    let mut state = opened(face());
    assert!(!posting::post_options(&state.world).per_op, "one program by default");
    run(&mut state, CommandKind::SetSkipStops(true));
    run(&mut state, CommandKind::SetPerOpOutput(true));
    let opts = posting::post_options(&state.world);
    assert!(opts.per_op && opts.skip_stops, "the two options are independent: {opts:?}");
    run(&mut state, CommandKind::Undo);
    assert!(!posting::post_options(&state.world).per_op, "Ctrl+Z restores one program");
    assert!(posting::post_options(&state.world).skip_stops, "and leaves the other switch");
    run(&mut state, CommandKind::Redo);
    assert!(posting::post_options(&state.world).per_op);
    let generate = camcraft_app::commands::GenerateOperation::new(&state.world, "Op").expect("generate");
    state.history.execute(&state.world, generate);
    let dir = std::env::temp_dir().join(format!("camcraft-egui-per-op-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp folder");
    run(&mut state, CommandKind::PostProgramFolder(dir.clone()));
    let names: Vec<String> = std::fs::read_dir(&dir)
        .map(|it| it.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(names, vec!["program_01_Op.nc".to_owned()], "status: {}", state.status);
    assert!(state.status.contains("one per operation"), "{}", state.status);
}

/// #596: the Tormach end-units row queues `SetPostOptions`, which rides
/// the history and leaves the other options as they were.
#[test]
fn end_units_choice_commits_and_undoes() {
    use camcraft_app::schema::{EndUnits, PostOptions};
    let mut state = opened(face());
    run(&mut state, CommandKind::SetSkipStops(true));
    let now = posting::post_options(&state.world);
    assert_eq!(now.end_units, EndUnits::Leave, "left modal by default");
    run(&mut state, CommandKind::SetPostOptions(PostOptions { end_units: EndUnits::Inch, ..now }));
    let opts = posting::post_options(&state.world);
    assert!(opts.end_units == EndUnits::Inch && opts.skip_stops, "{opts:?}");
    run(&mut state, CommandKind::Undo);
    assert_eq!(posting::post_options(&state.world).end_units, EndUnits::Leave, "Ctrl+Z restores it");
    assert!(posting::post_options(&state.world).skip_stops, "and leaves the other switch");
    run(&mut state, CommandKind::Redo);
    assert_eq!(posting::post_options(&state.world).end_units, EndUnits::Inch, "Ctrl+Y reapplies it");
}
