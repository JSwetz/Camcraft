//! M13.4 face origin snap: a picked face's area-weighted centroid and,
//! for a flat face, its plane height - the targets the UI's "Set origin"
//! and "Set origin (Z only)" send through `SetWcsOrigin`.
//!
//! Gate map (M13.4): covers app::pick::face, app::commands (SetWcsOrigin).

use camcraft_app::commands::{History, ImportModel, SetWcsOrigin};
use camcraft_app::geometry::{IDENTITY_Q, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::FeatureSel;
use camcraft_app::pick::{face_origin, facets_centroid};
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;

fn block_world() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))),
    );
    (world, history)
}

/// The block's face whose plane passes through `p` (normals are
/// sign-canonical, so the plane - not the normal's sign - tells top
/// from bottom).
fn face_along(world: &flecs_ecs::prelude::World, p: [f64; 3]) -> u64 {
    let e = model::find_by_name(world, "block").expect("block");
    let f = model::features_of(world, &e).expect("features");
    f.faces
        .iter()
        .find(|f| (f.normal[0] * p[0] + f.normal[1] * p[1] + f.normal[2] * p[2] - f.offset).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the box has that face")
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs() < 1e-9
}

#[test]
fn box_top_face_centroid_is_its_centre_at_top_z() {
    let (world, _) = block_world();
    let top = face_origin(&world, "block", FeatureSel::Face(face_along(&world, [10.0, 10.0, 10.0]))).expect("top");
    assert!(near(top.centroid, [10.0, 10.0, 10.0]), "centroid {:?}", top.centroid);
    assert!(top.planar);
    assert_eq!(top.plane_z, Some(10.0), "a flat face offers Z only");

    let side = face_origin(&world, "block", FeatureSel::Face(face_along(&world, [20.0, 10.0, 5.0]))).expect("side");
    assert!(near(side.centroid, [20.0, 10.0, 5.0]), "centroid {:?}", side.centroid);
    assert_eq!(side.plane_z, None, "a wall is not horizontal: no Z only");

    assert!(face_origin(&world, "block", FeatureSel::Face(0xdead_beef)).is_err(), "a stale face is refused");
    assert!(face_origin(&world, "block", FeatureSel::Vertex(1)).is_err(), "a point has no centroid");
    assert!(face_origin(&world, "nobody", FeatureSel::Face(1)).is_err(), "a gone entity is refused");
}

fn tri(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Triangle {
    Triangle {
        a: Point3::new(a[0], a[1], 0.0),
        b: Point3::new(b[0], b[1], 0.0),
        c: Point3::new(c[0], c[1], 0.0),
    }
}

#[test]
fn l_face_centroid_is_area_weighted() {
    // An L: the 2x1 bar (0..2, 0..1) as FOUR triangles, the 1x1 leg
    // (0..1, 1..2) as two. A per-vertex or per-facet average would lean
    // toward the bar's extra facets; area weighting does not.
    let tris = [
        tri([0.0, 0.0], [1.0, 0.0], [1.0, 1.0]),
        tri([0.0, 0.0], [1.0, 1.0], [0.0, 1.0]),
        tri([1.0, 0.0], [2.0, 0.0], [2.0, 1.0]),
        tri([1.0, 0.0], [2.0, 1.0], [1.0, 1.0]),
        tri([0.0, 1.0], [1.0, 1.0], [1.0, 2.0]),
        tri([0.0, 1.0], [1.0, 2.0], [0.0, 2.0]),
        // A zero-area sliver weighs nothing.
        tri([5.0, 5.0], [6.0, 6.0], [7.0, 7.0]),
    ];
    let c = facets_centroid(&tris, 0..tris.len()).expect("area");
    // (2 * (1, 0.5) + 1 * (0.5, 1.5)) / 3
    assert!(near(c, [2.5 / 3.0, 2.5 / 3.0, 0.0]), "centroid {c:?}");
    assert_eq!(facets_centroid(&tris, [6]), None, "no area, no centroid");
    assert_eq!(facets_centroid(&tris, [99]), None, "out-of-range facets are skipped");
}

#[test]
fn origin_at_face_round_trips_through_undo() {
    let (world, mut history) = block_world();
    let at = |w: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(w, "G54").expect("G54");
        let p = model::transform_of(w, &e).expect("placed").pos;
        [p.x, p.y, p.z]
    };
    let place = |p: [f64; 3]| Location { pos: Point3::new(p[0], p[1], p[2]), rot: IDENTITY_Q };
    history.execute(&world, SetWcsOrigin::new(&world, "G54", place([1.0, 2.0, 3.0])).expect("first"));

    let top = face_origin(&world, "block", FeatureSel::Face(face_along(&world, [10.0, 10.0, 10.0]))).expect("top");
    history.execute(&world, SetWcsOrigin::new(&world, "G54", place(top.centroid)).expect("centroid"));
    assert!(near(at(&world), [10.0, 10.0, 10.0]));
    assert!(history.undo(&world), "the snap is undoable");
    assert!(near(at(&world), [1.0, 2.0, 3.0]), "undo restores the previous origin");
    assert!(history.redo(&world));
    assert!(near(at(&world), [10.0, 10.0, 10.0]), "redo re-applies the centroid");

    // Z only: the current XY at the plane height.
    let z = top.plane_z.expect("flat");
    history.undo(&world);
    history.execute(&world, SetWcsOrigin::new(&world, "G54", place([1.0, 2.0, z])).expect("z only"));
    assert!(near(at(&world), [1.0, 2.0, 10.0]));
}
