//! #316 in the surface: the setup page's "relative to previous setup"
//! control queues `SetSetupRelative` (never a world poke), the drain
//! lands it as one undo unit, Ctrl+Z / Ctrl+Y walk it, the first setup
//! shows no control, and a refused link pushes nothing. The work offset
//! is not touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::setup_relative;

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn two_setups() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 8.0)));
    state.history.execute(&state.world, part);
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: Some("Setup1".into()) });
    state
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_relative_pick::picker(ui, st, "Setup1");
    super::setup_relative_pick::picker(ui, st, "Setup2");
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

fn shown(state: &State) -> Option<String> {
    setup_relative::describe(&state.world, "Setup2")
}

#[test]
fn the_relative_control_links_the_setup_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(700.0, 400.0))
        .build_ui_state(frame, two_setups());
    h.run();
    assert!(h.state().pending.is_empty(), "drawing the control queues nothing");
    let combos = h.query_all_by(|n| n.role() == Role::ComboBox).count();
    assert_eq!(combos, 1, "the first setup has no previous setup to be relative to");
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("Not linked")).click();
    h.run();
    h.get_by_label("Flip over X from Setup1").click();
    h.run();
    assert_eq!(shown(h.state()).as_deref(), Some("Flip over X from Setup1"), "the drain lands the link");
    assert_eq!(h.state().status, "Setup2: Flip over X from Setup1");
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup2"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(shown(h.state()), None, "undo unlinks");
    key(&mut h, Key::Y);
    assert_eq!(shown(h.state()).as_deref(), Some("Flip over X from Setup1"), "redo links again");
}

#[test]
fn typed_turns_unlinking_and_refusals_go_through_the_drain() {
    let mut state = two_setups();
    let link = |angles| CommandKind::SetSetupRelative { setup: "Setup2".into(), to: Some("Setup1".into()), angles };
    run(&mut state, link([0.0, 0.0, 90.0]));
    assert_eq!(shown(&state).as_deref(), Some("Turn 90° about Z from Setup1"));
    run(&mut state, link([0.0, 0.0, 90.0]));
    assert!(state.status.contains("setup not re-placed"), "{}", state.status);
    run(&mut state, link([f64::NAN, 0.0, 0.0]));
    assert!(state.status.contains("finite"), "{}", state.status);
    run(&mut state, CommandKind::SetSetupRelative { setup: "Setup1".into(), to: Some("Setup2".into()), angles: [0.0; 3] });
    assert!(state.status.contains("loop"), "{}", state.status);
    run(&mut state, CommandKind::SetSetupRelative { setup: "Setup2".into(), to: None, angles: [0.0; 3] });
    assert_eq!(shown(&state), None);
    assert_eq!(state.status, "Setup2 is no longer placed relative to another setup");
    // One link and one unlink landed; the refusals pushed nothing.
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(shown(&state), None);
    assert_eq!(camcraft_app::setup_align::stored(&state.world, "Setup2"), None);
}
