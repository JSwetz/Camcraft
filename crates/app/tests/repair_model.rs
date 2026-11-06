//! RepairModel (M13.11): the repair is a command - it lands welded and
//! filled facets as one undo unit, undo restores the exact facets, and
//! every refusal is by name at construction (nothing to repair, bad
//! parameters, non-manifold input).
//!
//! Gate map (M4.4): covers app::commands::repair_model,
//! app::commands::update_model, app::model, camcraft_geometry::geometry::repair.

use camcraft_app::commands::{CommandError, History, ImportModel, RepairModel, SetStock};
use camcraft_app::geometry::repair::open_edge_count;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

fn facets(world: &flecs_ecs::prelude::World, name: &str) -> Option<std::sync::Arc<[Triangle]>> {
    model::find_by_name(world, name).and_then(|e| model::facets_of(world, &e))
}

fn holed_cube() -> Mesh {
    // `cube` lays the bottom (z-) quad first: dropping it opens a 4-edge hole.
    Mesh::from_facets(cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)).facets[2..].to_vec())
}

#[test]
fn repair_fills_the_hole_and_undo_restores_the_facet_count() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", holed_cube()));
    let before = facets(&world, "part").expect("imported");
    assert_eq!(before.len(), 10);
    let rev0 = model::find_by_name(&world, "part").map(|e| model::rev_of(&world, &e));

    let command = RepairModel::new(&world, "part", 0.001, 256).expect("repairs");
    assert_eq!((command.report().holes_filled, command.report().triangles_added), (1, 2));
    assert!(command.summary().contains("filled 1 hole"), "{}", command.summary());
    history.execute(&world, command);
    let after = facets(&world, "part").expect("still there");
    assert_eq!(after.len(), 12);
    assert_eq!(open_edge_count(&Mesh::from_shared(after)), 0);

    assert!(history.undo(&world));
    let restored = facets(&world, "part").expect("restored");
    assert_eq!(restored.len(), 10);
    assert_eq!(restored, before, "undo restores the exact previous facets");
    assert_eq!(model::find_by_name(&world, "part").map(|e| model::rev_of(&world, &e)), rev0);

    assert!(history.redo(&world));
    assert_eq!(facets(&world, "part").map(|f| f.len()), Some(12));
}

#[test]
fn a_closed_model_refuses_as_nothing_to_repair() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("box", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))));
    let err = RepairModel::new(&world, "box", 0.001, 256).expect_err("nothing to do");
    assert!(matches!(&err, CommandError::Invalid(m) if m.contains("nothing to repair")), "{err}");
    assert!(!history.can_redo() && history.can_undo(), "the refusal never touched the history");
}

#[test]
fn bad_parameters_and_bad_targets_refuse_by_name() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", holed_cube()));
    assert!(matches!(RepairModel::new(&world, "ghost", 0.001, 256), Err(CommandError::NotFound(_))));
    for tol in [-0.1, f64::NAN, f64::INFINITY] {
        let err = RepairModel::new(&world, "part", tol, 256).expect_err("bad tolerance");
        assert!(err.to_string().contains("weld tolerance"), "{err}");
    }
    let err = RepairModel::new(&world, "part", 0.001, 2).expect_err("bad budget");
    assert!(err.to_string().contains("at least 3 edges"), "{err}");
    // Setup furniture is never repaired as if it were a part.
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 10.0, dy: 10.0, dz: 10.0 }, Location::default())
            .expect("stock"),
    );
    let err = RepairModel::new(&world, STOCK_NAME, 0.001, 256).expect_err("furniture");
    assert!(err.to_string().contains("setup furniture"), "{err}");
}

#[test]
fn non_manifold_input_refuses_by_name_at_construction() {
    let world = new_world();
    let mut history = History::default();
    let mut tris = holed_cube().facets.to_vec();
    tris.push(Triangle::new(
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 1.0),
        Point3::new(0.5, 0.5, 2.0),
    ));
    history.execute(&world, ImportModel::from_mesh("fin", Mesh::from_facets(tris)));
    let err = RepairModel::new(&world, "fin", 0.001, 256).expect_err("non-manifold");
    assert!(err.to_string().contains("non-manifold edge"), "{err}");
    assert_eq!(facets(&world, "fin").map(|f| f.len()), Some(11), "untouched");
}
