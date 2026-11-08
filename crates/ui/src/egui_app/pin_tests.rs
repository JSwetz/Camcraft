//! Issue #143: the ribbon pin gesture goes through the history. Pinning and
//! unpinning a cell are `SetRibbonPins` commands, so Ctrl+Z / Ctrl+Y walk
//! them and the document counts as changed; the fifth pin is refused by
//! name and pushes nothing.
//! Provenance: our own tests over our own pin gesture.

use camcraft_app::ribbon::pinned_of;

use super::panes::toggle_pin;
use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

#[test]
fn pinning_a_ribbon_cell_is_undoable() {
    let mut state = State::new();
    assert!(!state.history.can_undo(), "a fresh document has no history");

    toggle_pin(&mut state, 2);
    assert_eq!(pinned_of(&state.world), vec![2]);
    assert!(state.status.starts_with("pinned "), "{}", state.status);
    assert!(
        state.history.can_undo(),
        "the pin is a change the history sees"
    );

    toggle_pin(&mut state, 2);
    assert!(pinned_of(&state.world).is_empty());
    assert!(state.status.starts_with("unpinned "), "{}", state.status);

    run(&mut state, CommandKind::Undo);
    assert_eq!(pinned_of(&state.world), vec![2], "undo brings the pin back");
    run(&mut state, CommandKind::Undo);
    assert!(pinned_of(&state.world).is_empty(), "undo removes the pin");
    assert!(!state.history.can_undo(), "nothing else was pushed");
    run(&mut state, CommandKind::Redo);
    assert_eq!(pinned_of(&state.world), vec![2], "redo pins it again");
}

#[test]
fn a_refused_fifth_pin_pushes_nothing() {
    let mut state = State::new();
    for i in 0..4 {
        toggle_pin(&mut state, i);
    }
    assert_eq!(pinned_of(&state.world), vec![0, 1, 2, 3]);
    toggle_pin(&mut state, 4);
    assert_eq!(
        pinned_of(&state.world),
        vec![0, 1, 2, 3],
        "the fifth pin is refused"
    );
    assert!(state.status.contains("unpin one first"), "{}", state.status);
    // Four pins, four undo steps; the refusal added none.
    for _ in 0..4 {
        run(&mut state, CommandKind::Undo);
    }
    assert!(pinned_of(&state.world).is_empty());
    assert!(
        !state.history.can_undo(),
        "the refused pin never entered the history"
    );
}
