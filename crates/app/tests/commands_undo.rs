//! M1.1 acceptance: everything is a command, undo/redo is exact, and
//! malformed input never enters the history.

use std::path::{Path, PathBuf};

use hsmworks_app::commands::{
    CommandError, DeleteEntity, History, ImportModel, SetStock, SetWcsOrigin,
};
use hsmworks_app::geometry::{cube, Mesh, Point3};
use hsmworks_app::model;
use hsmworks_app::schema::{Location, STOCK_NAME};
use hsmworks_app::stock::{CYLINDER_SEGMENTS, StockShape};
use hsmworks_app::world::new_world;

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

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
fn step_import_undo_redo_roundtrip() {
    let world = new_world();
    let mut history = History::default();

    let command = ImportModel::new(&testdata("box.step")).expect("box.step must parse");
    history.execute(&world, command);
    assert_eq!(facet_count(&world, "box"), Some(12));

    assert!(history.undo(&world));
    assert_eq!(facet_count(&world, "box"), None);

    assert!(history.redo(&world));
    assert_eq!(facet_count(&world, "box"), Some(12));
}

#[test]
fn iges_fails_at_construction_with_honest_error() {
    // Construction must refuse by type, not by a failed filesystem read.
    let err = ImportModel::new(Path::new("model.iges")).err();
    assert!(matches!(err, Some(CommandError::UnsupportedType(_))), "{err:?}");
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

/// Everything undo must bring back for stock: parameters, placement, facets.
fn stock_state(
    world: &flecs_ecs::prelude::World,
) -> Option<(StockShape, Location, Vec<hsmworks_app::geometry::Triangle>)> {
    let entity = model::find_by_name(world, STOCK_NAME)?;
    let shape = model::stock_def_of(world, &entity)?;
    Some((
        shape,
        model::transform_of(world, &entity).unwrap_or_default(),
        model::facets_of(world, &entity).unwrap_or_default(),
    ))
}

#[test]
fn stock_create_undo_redo_is_exact() {
    let world = new_world();
    let mut history = History::default();
    let cyl = StockShape::Cylinder {
        diameter: 25.4,
        height: 8.0,
    };

    history.execute(
        &world,
        SetStock::new(&world, cyl.clone(), Location::default()).expect("sane cylinder"),
    );
    assert_eq!(facet_count(&world, STOCK_NAME), Some(4 * CYLINDER_SEGMENTS as usize));
    let after = stock_state(&world).expect("stock exists");
    assert_eq!(after.0, cyl);

    assert!(history.undo(&world));
    assert_eq!(stock_state(&world), None, "undo removes created stock");

    assert!(history.redo(&world));
    // Deterministic meshing makes the redo byte-identical, not just similar.
    assert_eq!(stock_state(&world), Some(after));
}

#[test]
fn stock_replace_undo_restores_previous_params() {
    let world = new_world();
    let mut history = History::default();
    let box_a = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 20.0,
    };
    let box_b = StockShape::Box {
        dx: 40.0,
        dy: 40.0,
        dz: 30.0,
    };
    let at = |z: f64| Location {
        pos: Point3::new(0.0, 0.0, z),
        rot: hsmworks_app::geometry::IDENTITY_Q,
    };

    history.execute(&world, SetStock::new(&world, box_a.clone(), at(0.0)).unwrap());
    let before = stock_state(&world).expect("stock a");
    history.execute(&world, SetStock::new(&world, box_b.clone(), at(-2.0)).unwrap());
    assert_eq!(stock_state(&world).unwrap().0, box_b);

    assert!(history.undo(&world));
    assert_eq!(stock_state(&world), Some(before));
    assert!(history.redo(&world));
    assert_eq!(stock_state(&world).unwrap().0, box_b);
}

#[test]
fn stock_validates_at_construction() {
    let world = new_world();
    // NaN and absurd dimensions are refused; nothing reaches the world.
    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::Box {
                dx: f64::NAN,
                dy: 1.0,
                dz: 1.0
            },
            Location::default()
        ),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 5e6,
                dy: 1.0,
                dz: 1.0
            },
            Location::default()
        ),
        Err(CommandError::Invalid(_))
    ));
    assert_eq!(model::find_by_name(&world, STOCK_NAME), None);
    // A non-finite placement is refused too.
    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 1.0,
                dy: 1.0,
                dz: 1.0
            },
            Location {
                pos: Point3::new(0.0, f64::INFINITY, 0.0),
                rot: hsmworks_app::geometry::IDENTITY_Q,
            }
        ),
        Err(CommandError::Invalid(_))
    ));
}

#[test]
fn stock_from_model_copies_geometry_and_placement() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("src", cube_mesh()));
    let src = model::find_by_name(&world, "src").expect("imported");
    let src_tris = model::facets_of(&world, &src).expect("facets");

    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel { source: "src".into() },
            Location::default(),
        )
        .expect("copyable source"),
    );
    let (shape, _, facets) = stock_state(&world).expect("stock exists");
    assert_eq!(shape, StockShape::FromModel { source: "src".into() });
    assert_eq!(facets, src_tris, "stock geometry mirrors the source");
    // An identity request adopts the source's placement, keeping it put.
    assert_eq!(
        model::transform_of(&world, &model::find_by_name(&world, STOCK_NAME).unwrap()),
        model::transform_of(&world, &src)
    );

    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "ghost".into()
            },
            Location::default()
        ),
        Err(CommandError::NotFound(_))
    ));
    assert_eq!(facet_count(&world, STOCK_NAME), Some(12), "failed command changed nothing");
}

#[test]
fn wcs_lazy_create_undo_removes() {
    let world = new_world();
    let mut history = History::default();
    let at = Location {
        pos: Point3::new(10.0, -5.0, 2.5),
        rot: hsmworks_app::geometry::IDENTITY_Q,
    };

    history.execute(&world, SetWcsOrigin::new(&world, "G55", at).unwrap());
    let e = model::find_by_name(&world, "G55").expect("slot entity");
    assert!(model::is_wcs(&world, &e));
    assert_eq!(model::transform_of(&world, &e), Some(at));
    assert_eq!(model::facets_of(&world, &e), None, "offsets never have geometry");

    assert!(history.undo(&world));
    assert_eq!(model::find_by_name(&world, "G55"), None);
    assert!(history.redo(&world));
    assert!(model::is_wcs(&world, &model::find_by_name(&world, "G55").unwrap()));
}

#[test]
fn wcs_validates_slot_and_name() {
    let world = new_world();
    let mut history = History::default();
    assert!(matches!(
        SetWcsOrigin::new(&world, "G53", Location::default()),
        Err(CommandError::Invalid(_))
    ));
    // A model squatting on a slot name is a conflict, not an overwrite.
    history.execute(&world, ImportModel::from_mesh("G54", cube_mesh()));
    assert!(matches!(
        SetWcsOrigin::new(&world, "G54", Location::default()),
        Err(CommandError::NameConflict(_))
    ));
}

#[test]
fn delete_stock_undo_restores_kind() {
    let world = new_world();
    let mut history = History::default();
    let box_a = StockShape::Box {
        dx: 10.0,
        dy: 20.0,
        dz: 30.0,
    };
    history.execute(&world, SetStock::new(&world, box_a.clone(), Location::default()).unwrap());
    let before = stock_state(&world).expect("stock a");

    history.execute(&world, DeleteEntity::new(&world, STOCK_NAME).unwrap());
    assert_eq!(model::find_by_name(&world, STOCK_NAME), None);

    assert!(history.undo(&world));
    // The restored entity must be stock again, not a nameless mesh.
    assert_eq!(stock_state(&world), Some(before));
    let e = model::find_by_name(&world, STOCK_NAME).unwrap();
    assert!(model::is_stock(&world, &e));
}
