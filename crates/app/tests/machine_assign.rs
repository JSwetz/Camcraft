//! Assigning a catalogue machine to the setup (issue #384): the Machines
//! pane's "Use for this setup" lands as `LoadMachine::from_catalogue` -
//! it mounts, undoes back to the previous machine and redoes; a machine
//! with validation refusals is refused at the command's front door by
//! name, and the pane's reason (`assign_block`) says the same.
//!
//! Fixture law (clean-room): the crafted files are our own values in the
//! `.mch` layout of `assets/machines/AGENTS.md`.
//! Provenance: our own tests, specced from the issue.
//!
//! Gate map (M4.4): covers app::commands::load_machine, app::machine::library, app::machine::validate.

use camcraft_app::commands::{CommandError, History, LoadMachine};
use camcraft_app::machine::library::{self, MachineSource};
use camcraft_app::machine::{self, Kind};
use camcraft_app::posting;
use camcraft_app::world::new_world;

/// A 3-axis mill whose X travel is `min .. max`.
fn mill(min: f64, max: f64) -> String {
    serde_json::json!({
      "general": { "model": "ASSIGN MILL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear", "travel": { "min": min, "max": max } },
        { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } }
    })
    .to_string()
}

fn temp_file(tag: &str, text: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft_machine_assign_{tag}_{}.mch", std::process::id()));
    std::fs::write(&p, text).expect("write the fixture file");
    p
}

#[test]
fn assigning_a_catalogue_machine_does_undoes_and_redoes() {
    let world = new_world();
    let mut history = History::default();
    // Start from the library lathe, so undo has a machine to restore.
    history.execute(&world, LoadMachine::builtin(&world, "Lathe").expect("library lathe"));
    let before = machine::of(&world);
    assert_eq!(before.0, posting::builtin_path("Lathe"));

    let file = temp_file("good", &mill(-150.0, 150.0));
    let source = MachineSource::File(file.clone());
    let entry = library::MachineEntry::from_file(&file);
    assert!(entry.is_valid(), "{:?}", entry.refusals);
    assert_eq!(entry.assign_block(&world), None, "a valid, unmounted machine can be assigned");
    history.execute(&world, LoadMachine::from_catalogue(&world, &source).expect("assigns"));
    let (path, m) = machine::of(&world);
    assert_eq!(path, file.display().to_string());
    assert_eq!(m.as_ref().map(|m| m.model.as_str()), Some("ASSIGN MILL"));
    assert_eq!(machine::kind_of(&world), Kind::Mill);
    assert_eq!(
        entry.assign_block(&world).as_deref(),
        Some("it is already this setup's machine"),
        "the mounted machine names why it can't be assigned again"
    );
    assert!(
        LoadMachine::from_catalogue(&world, &source).is_err(),
        "assigning the mounted machine again is a no-op, refused"
    );

    assert!(history.undo(&world));
    assert_eq!(machine::of(&world), before, "undo restores the previous machine exactly");
    assert!(history.redo(&world));
    assert_eq!(machine::of(&world).0, file.display().to_string(), "redo assigns it again");

    // A built-in entry assigns by its library path, like the Setup pane's picker.
    let mill_name = posting::MACHINES
        .iter()
        .map(|(n, _)| *n)
        .find(|n| *n != "Lathe")
        .expect("the library has more than the lathe");
    history.execute(
        &world,
        LoadMachine::from_catalogue(&world, &MachineSource::Builtin(mill_name)).expect("assigns"),
    );
    assert_eq!(machine::of(&world).0, posting::builtin_path(mill_name));
    let _ = std::fs::remove_file(&file);
}

#[test]
fn a_refused_machine_cannot_be_assigned_and_says_why() {
    let world = new_world();
    let before = machine::of(&world);
    let file = temp_file("bad", &mill(10.0, 5.0));
    let entry = library::MachineEntry::from_file(&file);
    let why = entry.assign_block(&world).expect("a refused file is blocked");
    assert!(why.contains("X travel: min 10 must be below max 5"), "{why}");
    match LoadMachine::from_catalogue(&world, &MachineSource::File(file.clone())) {
        Err(CommandError::Invalid(msg)) => {
            assert!(msg.contains("X travel: min 10 must be below max 5"), "{msg}");
        }
        other => panic!("expected a refusal by name, got {other:?}"),
    }
    assert_eq!(machine::of(&world), before, "nothing mounted");
    // A file gone from disk refuses too; a stale library name refuses.
    let _ = std::fs::remove_file(&file);
    assert!(LoadMachine::from_catalogue(&world, &MachineSource::File(file)).is_err());
    assert!(LoadMachine::from_catalogue(&world, &MachineSource::Builtin("No Such Mill")).is_err());
}
