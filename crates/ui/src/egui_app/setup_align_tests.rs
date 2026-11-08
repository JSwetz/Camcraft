//! #326 in the surface: the Setups page's part-alignment control queues
//! `AlignPart` (never a world poke), the drain lands it as one undo unit,
//! Ctrl+Z / Ctrl+Y walk it, the typed angles reach the same command, and
//! a refused choice pushes nothing. The work offset is not touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::setup_align::{self, AlignPreset};

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_part() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 8.0)));
    state.history.execute(&state.world, part);
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    state
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_align_pick::picker(ui, st, "Setup1");
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

fn held(state: &State) -> String {
    super::align_text(setup_align::stored(&state.world, "Setup1"))
}

#[test]
fn the_alignment_control_turns_the_part_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(frame, with_part());
    h.run();
    assert_eq!(held(h.state()), "As modelled");
    assert!(h.state().pending.is_empty(), "drawing the control queues nothing");
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("As modelled")).click();
    h.run();
    h.get_by_label("Flip over X").click();
    h.run();
    assert_eq!(held(h.state()), "Flip over X", "the drain lands the flip");
    assert_eq!(h.state().status, "Setup1 part alignment: Flip over X");
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(held(h.state()), "As modelled", "undo restores as modelled");
    key(&mut h, Key::Y);
    assert_eq!(held(h.state()), "Flip over X", "redo restores the flip");
}

#[test]
fn typed_angles_reach_the_same_command_and_a_refusal_says_why() {
    let mut state = with_part();
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 90.0] });
    assert_eq!(held(&state), AlignPreset::TurnZ90.label());
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [30.0, 0.0, 0.0] });
    assert_eq!(held(&state), "turned X 30°, Y 0°, Z 0°");
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [30.0, 0.0, 0.0] });
    assert!(state.status.contains("part alignment not changed"), "{}", state.status);
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [f64::NAN, 0.0, 0.0] });
    assert!(state.status.contains("finite"), "{}", state.status);
    // Two turns landed, the refusals pushed nothing.
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(held(&state), "As modelled");
}
