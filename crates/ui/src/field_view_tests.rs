//! M7.2 B3b + C: the traced stock's field follows the cells that ask
//! for it - nothing walked while neither Deviate nor the transport is
//! up, a refusal kept and said once (never retried every frame), and
//! a mode change retiring whatever was walked. Provenance: our own
//! tests; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers ui::actions::ensure_field_view.

use crate::actions::ensure_field_view;
use crate::state::{Sim, State};

#[test]
fn nothing_is_walked_while_no_cell_asks() {
    let mut state = State::new();
    ensure_field_view(&mut state);
    assert!(state.field_view.is_none() && state.field_play.is_none() && state.field_mode.is_none());
}

#[test]
fn a_refused_playback_is_said_once_and_retired_with_the_transport() {
    let mut state = State::new();
    state.sim = Some(Sim::new()); // an empty document: no stock to cut
    ensure_field_view(&mut state);
    assert!(matches!(state.field_view, Some(Err(_))), "no stock is a named refusal");
    assert!(state.status.contains("traced playback refused"), "{}", state.status);
    let said = state.status.clone();
    ensure_field_view(&mut state);
    assert_eq!(state.status, said, "the same state is not re-walked or re-said");
    state.sim = None;
    ensure_field_view(&mut state);
    assert!(state.field_view.is_none() && state.field_mode.is_none(), "the transport down retires it");
}
