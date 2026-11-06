//! The fixtures section's "Fixed" check row (M13.8 (2) owed), driven
//! through the real page code: the box queues `SetFixtureFixed` on the
//! pending lane - never a world poke - the drain lands it as one undo
//! unit, and Ctrl+Z walks it back through the same keyboard lane every
//! command rides.
//! Provenance: our own page law; no vendor material consulted.

use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::{ImportModel, TagFixture};
use camcraft_app::fixture;
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::model;

use crate::state::{CommandKind, State};

const ROW: &str = "Fixed (does not turn with the part)";

/// A block imported and marked as workholding, turning by default.
fn with_chuck() -> State {
    let mut state = State::new();
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("chuck", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 4.0, 4.0))),
    );
    state.history.execute(&state.world, TagFixture::new(&state.world, "chuck", None).expect("tag the steel"));
    state
}

fn is_fixed(state: &State) -> bool {
    model::find_by_name(&state.world, "chuck")
        .and_then(|e| fixture::def_of(&state.world, &e))
        .is_some_and(|d| d.fixed)
}

/// The keyboard layer, the drain, then the section: a frame under the
/// shell's own order (commands queued last frame run first).
fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::pages::fixtures_section(ui, st, None);
}

fn section(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(600.0, 400.0)).build_ui_state(frame, state)
}

#[test]
fn the_fixed_row_toggles_through_the_command_and_undo_walks_it_back() {
    let mut harness = section(with_chuck());
    harness.run();
    assert!(!is_fixed(harness.state()), "a fresh fixture turns with the part");
    assert!(harness.state().pending.is_empty(), "drawing the section queues nothing");

    // The caption toggles the box, as a checkbox's own text would. The
    // status line is the dispatch arm's voice: only `SetFixtureFixed`
    // through the queue writes it, so it proves the route was the
    // command, never a poke - and the undo below proves the history has it.
    harness.get_by_label(ROW).click();
    harness.run();
    assert!(is_fixed(harness.state()), "the drain lands the flag through the command");
    assert_eq!(harness.state().status, "chuck is fixed - it does not turn with the part");
    assert!(
        !harness.state().pending.iter().any(|c| matches!(c, CommandKind::SetFixtureFixed { .. })),
        "the queued command outlived the drain"
    );

    // Ctrl+Z, the same lane as every command.
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    assert!(!is_fixed(harness.state()), "undo restores the turning default");
    assert!(harness.state().history.can_redo(), "one undo unit on the history");
}
