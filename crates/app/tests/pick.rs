//! Ray picking: nearest model along the ray, misses and degenerate rays
//! return None - never a panic.

use hsmworks_app::commands::{Command, ImportModel};
use hsmworks_app::geometry::{cube, Mesh, Point3};
use hsmworks_app::{model, pick, world::new_world};

fn cube_at(origin: Point3) -> Mesh {
    cube(origin, Point3::new(origin.x + 1.0, origin.y + 1.0, origin.z + 1.0))
}

#[test]
fn ray_hits_the_named_cube() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let found = pick::first_hit(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found.as_deref(), Some("a"));
}

#[test]
fn nearest_hit_wins() {
    let world = new_world();
    ImportModel::from_mesh("near", cube_at(Point3::ZERO)).apply(&world);
    ImportModel::from_mesh("far", cube_at(Point3::new(0.0, 0.0, -20.0))).apply(&world);
    let found = pick::first_hit(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found.as_deref(), Some("near"));
}

#[test]
fn miss_is_none() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let found = pick::first_hit(&world, [50.0, 50.0, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found, None);
}

#[test]
fn non_finite_ray_is_none_not_crash() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let nan = f64::NAN;
    assert_eq!(pick::first_hit(&world, [nan, 0.0, 0.0], [0.0, 0.0, -1.0]), None);
    assert_eq!(pick::first_hit(&world, [0.0, 0.0, 0.0], [f64::INFINITY, 0.0, 0.0]), None);
}

#[test]
fn empty_world_picks_nothing() {
    let world = new_world();
    assert_eq!(
        pick::first_hit(&world, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
        None
    );
    assert_eq!(model::named_entities(&world).len(), 0);
}
