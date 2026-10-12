//! M1.1 acceptance: everything is a command, undo/redo is exact, and
//! malformed input never enters the history.

use hsmworks_app::commands::{CommandError, DeleteEntity, History, ImportModel};
use hsmworks_app::geometry::{cube, Mesh, Point3};
use hsmworks_app::model;
use hsmworks_app::world::new_world;

fn cube_mesh() -> Mesh {
    cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))
}

fn facet_count(world: &flecs_ecs::prelude::World, name: &str) -> Option<usize> {
    model::find_by_name(world, name).and_then(|e| model::facets_of(world, &e).map(|t| t.len()))
}

#[test]
fn import_undo_redo_roundtrip() {
    let world = new_world();
    let mut history = History::default();

    history.execute(&world, ImportModel::from_mesh("cube1", cube_mesh()));
    assert_eq!(facet_count(&world, "cube1"), Some(12));

    assert!(history.undo(&world));
    assert_eq!(facet_count(&world, "cube1"), None);

    assert!(history.redo(&world));
    assert_eq!(facet_count(&world, "cube1"), Some(12));

    assert!(history.can_undo()); // the command is back on the done stack
    assert!(!history.can_redo());
}

#[test]
fn delete_restores_state_exactly() {
    let world = new_world();
    let mut history = History::default();
    let original = cube_mesh().facets;

    history.execute(&world, ImportModel::from_mesh("cube2", cube_mesh()));
    let command = DeleteEntity::new(&world, "cube2").expect("entity exists");
    history.execute(&world, command);
    assert_eq!(facet_count(&world, "cube2"), None);

    history.undo(&world);
    let restored = model::find_by_name(&world, "cube2").and_then(|e| model::facets_of(&world, &e));
    assert_eq!(restored, Some(original));
}

#[test]
fn fresh_command_clears_redo_stack() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("a", cube_mesh()));
    history.undo(&world);
    assert!(history.can_redo());
    history.execute(&world, ImportModel::from_mesh("b", cube_mesh()));
    assert!(!history.can_redo());
}

#[test]
fn empty_history_is_polite() {
    let world = new_world();
    let mut history = History::default();
    assert!(!history.undo(&world));
    assert!(!history.redo(&world));
    assert!(history.is_empty());
}

#[test]
fn delete_missing_entity_fails_at_construction() {
    let world = new_world();
    let result = DeleteEntity::new(&world, "ghost");
    assert!(matches!(result, Err(CommandError::NotFound(_))));
}

#[test]
fn corrupt_import_fails_at_construction() {
    let path = std::env::temp_dir().join(format!("hsmworks_corrupt_{}.stl", std::process::id()));
    if std::fs::write(&path, b"garbage not an stl at all \x00\xff").is_err() {
        return; // no temp dir on this machine; the corpus tests cover parsing
    }
    let result = ImportModel::new(&path);
    let _ = std::fs::remove_file(&path);
    match result {
        Err(CommandError::Import(_)) => {}
        Err(other) => panic!("expected Import error, got {other:?}"),
        Ok(_) => panic!("garbage must not construct a command"),
    }
}
