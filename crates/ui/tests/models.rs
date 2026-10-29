//! Part placement through the UI queue (M5.2 stage G.1): the move
//! command, its undo units, the refuse wording, and the draft the
//! Models page shows after a move. No window - the queue stands in
//! for the placement editor's Apply.
//! Provenance: our own harness; behavior specced from the pages.
//!
//! Gate map (M4.4): covers ui::state, app::commands::move_model, app::model.

use camcraft_app::model;
use camcraft_app::schema::Location;
use camcraft_ui::state::{CommandKind, State};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl")
}

fn import_cube(state: &mut State) {
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    assert!(state.status.starts_with("imported cube"), "{:?}", state.status);
}

fn loc(state: &State, name: &str) -> Location {
    let e = model::find_by_name(&state.world, name).expect("the model");
    model::transform_of(&state.world, &e).expect("location")
}

#[test]
fn move_model_queue_roundtrip_and_draft() {
    let mut state = State::new();
    import_cube(&mut state);

    // Stand the cube on its edge 20 mm along +X, exactly what the
    // page's Apply sends.
    state.pending.push_back(CommandKind::MoveModel {
        name: "cube".into(),
        pos: [20.0, 0.0, 0.0],
        roll_deg: 90.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert_eq!(state.status, "moved cube");
    let got = loc(&state, "cube");
    assert_eq!([got.pos.x, got.pos.y, got.pos.z], [20.0, 0.0, 0.0]);
    let half = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        got.rot.iter().zip([half, 0.0, 0.0, half]).all(|(a, b)| (a - b).abs() < 1e-12),
        "roll 90 about X is stored as the quaternion: {:?}",
        got.rot
    );
    // The draft mirrors the command: reopening the page shows the pose.
    assert_eq!(state.model_ui.name, "cube");
    assert_eq!((state.model_ui.x, state.model_ui.roll_deg), (20.0, 90.0));

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let back = loc(&state, "cube");
    assert_eq!(back.pos, camcraft_app::geometry::Point3::ZERO, "undo reposes");
    assert_eq!(back.rot, [0.0, 0.0, 0.0, 1.0], "and derotates");

    state.pending.push_back(CommandKind::Redo);
    state.drain_actions();
    assert_eq!(loc(&state, "cube").pos.x, 20.0, "redo replays the move");
}

#[test]
fn move_model_refusals_carry_the_pages_voice() {
    let mut state = State::new();
    import_cube(&mut state);

    // The table is machine prose, not a part.
    state.pending.push_back(CommandKind::MoveModel {
        name: "Machine/Table".into(),
        pos: [1.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(
        state.status.contains("part not moved") && state.status.contains("machine table"),
        "{:?}",
        state.status
    );

    // A tagged fixture is placed by the Fixtures page, not here.
    state.pending.push_back(CommandKind::TagFixture {
        name: "cube".into(),
        setup: None,
    });
    state.drain_actions();
    state.pending.push_back(CommandKind::MoveModel {
        name: "cube".into(),
        pos: [1.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(state.status.contains("part not moved"), "{:?}", state.status);
    assert!(state.status.contains("fixture"), "the reason names itself: {:?}", state.status);
    // Refusal wrote nothing.
    assert_eq!(loc(&state, "cube").pos.x, 0.0);

    // NaN angles never reach the history (the SetWcs guard).
    state.pending.push_back(CommandKind::MoveModel {
        name: "cube".into(),
        pos: [0.0, 0.0, 0.0],
        roll_deg: f64::NAN,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(state.status.contains("rotation angles must be finite"), "{:?}", state.status);
}

#[test]
fn pure_rotation_is_undoable_like_any_move() {
    let mut state = State::new();
    import_cube(&mut state);
    state.pending.push_back(CommandKind::MoveModel {
        name: "cube".into(),
        pos: [0.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 180.0,
    });
    state.drain_actions();
    assert_eq!(state.status, "moved cube");
    let spun = loc(&state, "cube");
    assert_eq!(spun.pos, camcraft_app::geometry::Point3::ZERO, "no translation");
    assert!(
        spun.rot.iter().zip([0.0f64, 0.0, 1.0, 0.0]).all(|(a, b)| (a - b).abs() < 1e-12),
        "yaw 180 = (0,0,1,0): {:?}",
        spun.rot
    );
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(loc(&state, "cube").rot, [0.0, 0.0, 0.0, 1.0]);
}
