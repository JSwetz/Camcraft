//! The op page's Coolant row (#112): shown under Tool for a committed
//! operation, showing the op's own choice, and its edit riding
//! `EditOperationParams` through the history - undo and redo included.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use camcraft_app::model;
use camcraft_app::operation::CoolantMode;

use super::dialog_tests::{face, opened, page};
use crate::state::{CommandKind, State};

fn coolant(state: &State) -> CoolantMode {
    model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| o.coolant)
        .expect("the op")
}

#[test]
fn the_coolant_row_shows_the_ops_choice_and_its_edit_undoes() {
    let mut harness = page(opened(face()));
    harness.run();
    assert!(harness.query_by_label("Coolant").is_some(), "the header grid carries the row");
    let row = harness.get_by_label("Coolant").rect();
    let shows_flood = harness.query_all_by(|n| n.role() == Role::ComboBox).any(|n| {
        let c = n.rect().center().y;
        c >= row.top() - 2.0 && c <= row.bottom() + 2.0 && n.value().as_deref() == Some("Flood")
    });
    assert!(shows_flood, "a new operation floods, as every post did before the row existed");
    assert!(harness.state().pending.is_empty(), "drawing the row queues nothing");

    let state = harness.state_mut();
    state.pending.push_back(CommandKind::EditOperationCoolant { name: "Op".into(), coolant: CoolantMode::Mist });
    state.drain_actions();
    assert_eq!(coolant(state), CoolantMode::Mist, "the edit commits through its command");
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(coolant(state), CoolantMode::Flood, "Ctrl+Z restores the previous coolant");
    state.pending.push_back(CommandKind::Redo);
    state.drain_actions();
    assert_eq!(coolant(state), CoolantMode::Mist, "Ctrl+Y reapplies it");
}
