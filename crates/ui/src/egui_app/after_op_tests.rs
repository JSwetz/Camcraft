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
    assert!(posting::post_options_of(&state.world).skip_stops);
    run(&mut state, CommandKind::Undo);
    assert!(!posting::post_options_of(&state.world).skip_stops, "undo turns the switch off");
}
