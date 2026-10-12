//! M1.1 acceptance: save/load round-trips are byte-identical; a wrong
//! version or corrupt file is a clean error.

use std::path::PathBuf;

use hsmworks_app::commands::{History, ImportModel};
use hsmworks_app::geometry::{cube, Point3};
use hsmworks_app::model;
use hsmworks_app::project::{self, ProjectError};
use hsmworks_app::world::new_world;

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hsmworks_proj_{tag}_{}.hsmproj", std::process::id()))
}

fn populated_world() -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("block_a", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("block_b", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
    );
    // give block_b an offset so the transform round-trip is observable
    if let Some(e) = model::find_by_name(&world, "block_b") {
        model::set_location(&world, &e, Point3::new(10.0, 0.5, -3.25));
    }
    world
}

#[test]
fn save_load_save_is_byte_identical() {
    let path = tmp_path("rt");
    let world = populated_world();
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");

    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "round-trip must be byte-identical");

    // loaded world has the same content
    let file = project::collect(&world2);
    assert_eq!(file.entities.len(), 2);
    let names: Vec<&str> = file.entities.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["block_a", "block_b"], "serializer order is by name");
}

#[test]
fn transforms_survive() {
    let path = tmp_path("tf");
    let world = populated_world();
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let e = model::find_by_name(&world2, "block_b").expect("loaded");
    let p = model::location_of(&world2, &e).expect("location");
    assert_eq!((p.x, p.y, p.z), (10.0, 0.5, -3.25));
    assert_eq!(model::facets_of(&world2, &e).map(|t| t.len()), Some(12));
}

#[test]
fn wrong_version_is_refused() {
    let text = r#"{"version": 2, "entities": []}"#;
    let world = new_world();
    match project::open(&world, text) {
        Err(ProjectError::UnsupportedVersion(2)) => {}
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn corrupt_project_is_a_clean_error() {
    let world = new_world();
    assert!(matches!(
        project::open(&world, "this is not json"),
        Err(ProjectError::Corrupt(_))
    ));
    assert!(matches!(
        project::open(&world, r#"{"version": 1, "entities": [{"name": 5}]}"#),
        Err(ProjectError::Corrupt(_))
    ));
}
