//! #107: the unsaved-changes flag. The document is dirty exactly when the
//! command history sits away from the position of the last save or load:
//! a command dirties it, a save cleans it, undoing back to the saved point
//! cleans it again and redoing past it dirties it again.
//!
//! Gate map: covers app::commands (History's saved point), app::project
//! (save/load as the clean points).
//!
//! Provenance: our own tests of standard document-editor semantics; no
//! HSMWorks material involved.

use camcraft_app::commands::{History, ImportModel, SetUnits};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::project;
use camcraft_app::units::Unit;
use camcraft_app::world::new_world;

fn temp_project(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-dirty-{tag}-{}.camproj", std::process::id()))
}

#[test]
fn dirty_tracks_the_saved_point_through_undo_and_redo() {
    let world = new_world();
    let mut history = History::default();
    assert!(!history.is_dirty(), "a fresh document is clean");

    history.execute(&world, ImportModel::from_mesh("a", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))));
    assert!(history.is_dirty(), "a command dirties the document");

    let path = temp_project("roundtrip");
    let saved = project::save(&world, &path);
    assert!(saved.is_ok(), "save: {saved:?}");
    history.mark_clean();
    assert!(!history.is_dirty(), "a save cleans the document");

    history.execute(&world, SetUnits::new(&world, Unit::Inches).expect("to inches"));
    assert!(history.is_dirty(), "a command after the save dirties it");
    assert!(history.undo(&world));
    assert!(!history.is_dirty(), "undo back to the saved point is clean");
    assert!(history.undo(&world));
    assert!(history.is_dirty(), "undo past the saved point is dirty");
    assert!(history.redo(&world));
    assert!(!history.is_dirty(), "redo back onto the saved point is clean");
    assert!(history.redo(&world));
    assert!(history.is_dirty(), "redo past the saved point is dirty again");

    // A load replaces the document: the timeline clears and the loaded
    // state is the new clean point.
    history.clear();
    let loaded = project::load(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(loaded.is_ok(), "load: {loaded:?}");
    assert!(!history.is_dirty(), "a loaded document is clean");
}

#[test]
fn a_new_branch_makes_the_saved_point_unreachable() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("a", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))));
    history.execute(&world, SetUnits::new(&world, Unit::Inches).expect("to inches"));
    history.mark_clean();
    assert!(history.undo(&world));
    // A fresh command discards the saved state from the redo stack: the
    // depth matches again, but the document is not the saved one.
    history.execute(&world, ImportModel::from_mesh("b", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))));
    assert!(history.is_dirty(), "same depth on a new branch is still dirty");
    assert!(history.undo(&world));
    assert!(history.is_dirty(), "the saved point is gone for good");
}
