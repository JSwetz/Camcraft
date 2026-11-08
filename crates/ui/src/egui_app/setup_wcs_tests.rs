//! #196 in the surface: the Setups page's work-offset picker queues
//! `SetSetupWorkOffset` (never a world poke), the drain lands it as one
//! undo unit, Ctrl+Z / Ctrl+Y walk it, and a refused pick pushes nothing.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_setup() -> State {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    state
}

fn pick(state: &State) -> Option<usize> {
    camcraft_app::setup::work_offset(&state.world, "Setup1")
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_wcs_pick::picker(ui, st, "Setup1");
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

#[test]
fn the_picker_sets_the_setups_work_offset_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 400.0))
        .build_ui_state(frame, with_setup());
    h.run();
    assert_eq!(pick(h.state()), None, "a fresh setup picks nothing (Auto)");
    assert!(h.state().pending.is_empty(), "drawing the picker queues nothing");
    let auto = super::setup_wcs_pick::pick_text(None);
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(auto)).click();
    h.run();
    h.get_by_label("G55").click();
    h.run();
    assert_eq!(pick(h.state()), Some(1), "the drain lands G55");
    assert_eq!(h.state().status, "setup Setup1 posts at G55");
    key(&mut h, Key::Z);
    assert_eq!(pick(h.state()), None, "undo restores Auto");
    key(&mut h, Key::Y);
    assert_eq!(pick(h.state()), Some(1), "redo restores G55");
}

#[test]
fn a_refused_pick_changes_nothing_and_says_why() {
    let mut state = with_setup();
    run(&mut state, CommandKind::SetSetupWorkOffset { setup: "Setup1".into(), slot: Some(9) });
    assert_eq!(pick(&state), None);
    assert!(state.status.contains("work offset not picked"), "{}", state.status);
    // The refusal pushed nothing: the one undo unit is the CreateSetup.
    run(&mut state, CommandKind::Undo);
    assert!(camcraft_app::setup::names(&state.world).is_empty());
    assert!(!state.history.can_undo());
}
