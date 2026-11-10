//! #571: turning a part on the Model page (plain `MoveModel`) keeps a
//! square box or cylinder stock square to the setup, re-fitted round the
//! part with the allowance it had on each side - the same rule a locate
//! keeps (#561). A move that does not turn the part carries the stock
//! rigidly as before; a stock copied from the model or already off the
//! axes still turns with the part; undo restores the block exactly.
//! Provenance: our own fixtures over plain vise practice (the blank is
//! clamped square); no vendor material consulted.
//!
//! Gate map (#571): covers app::commands::move_model, app::stock_follow.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{History, ImportModel, MoveModel, SetStock};
use camcraft_app::geometry::{Point3, cube, quat_from_euler};
use camcraft_app::model;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 20 x 10 x 8 part at the origin, and `shape` placed at `at`.
fn document(lo: Point3, hi: Point3, shape: StockShape, at: Location) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    h.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    (world, h)
}

fn bounds(world: &World, name: &str) -> ([f64; 3], [f64; 3]) {
    let e = model::find_by_name(world, name).expect("entity");
    let b = model::entity_world_aabb(world, &e).expect("bounds");
    ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])
}

/// The stock's shape, placement and facets, for exact undo checks.
fn stock_state(world: &World) -> (Option<StockShape>, Option<Location>, Vec<[f64; 9]>) {
    let e = model::find_by_name(world, STOCK_NAME).expect("stock");
    let tris = model::facets_of(world, &e)
        .map(|t| t.iter().map(|t| [t.a.x, t.a.y, t.a.z, t.b.x, t.b.y, t.b.z, t.c.x, t.c.y, t.c.z]).collect())
        .unwrap_or_default();
    (model::stock_def_of(world, &e), model::transform_of(world, &e), tris)
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= 1e-6)
}

const IDENTITY: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

/// Box round the 20 x 10 x 8 part: -X 1, +X 2, -Y 3, +Y 4, under 0, top 5.
fn boxed() -> (World, History) {
    let shape = StockShape::Box { dx: 23.0, dy: 17.0, dz: 13.0 };
    document(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0), shape, Location { pos: p(10.5, 5.5, 0.0), ..Location::default() })
}

fn turned_30() -> Location {
    Location { pos: p(50.0, 40.0, 0.0), rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()) }
}

#[test]
fn a_part_turned_30_degrees_keeps_its_box_stock_square_with_each_allowance() {
    let (world, mut h) = boxed();
    let before = stock_state(&world);
    h.execute(&world, MoveModel::new(&world, "part", turned_30()).expect("turned"));
    let (_, at, _) = stock_state(&world);
    assert_eq!(at.expect("placed").rot, IDENTITY, "the block stays square to the machine");
    let (plo, phi) = bounds(&world, "part");
    let (slo, shi) = bounds(&world, STOCK_NAME);
    let want_lo = [plo[0] - 1.0, plo[1] - 3.0, plo[2]];
    let want_hi = [phi[0] + 2.0, phi[1] + 4.0, phi[2] + 5.0];
    assert!(close(slo, want_lo) && close(shi, want_hi), "stock {slo:?}..{shi:?} wants {want_lo:?}..{want_hi:?}");
    // Undo restores shape, place and block bytes; redo replays them.
    let after = stock_state(&world);
    assert!(h.undo(&world));
    assert_eq!(stock_state(&world), before);
    assert!(h.redo(&world));
    assert_eq!(stock_state(&world), after);
}

#[test]
fn a_part_turned_30_degrees_keeps_its_cylinder_stock_upright_with_its_allowance() {
    // A 10 x 10 x 10 part centred on a 20 dia x 12 bar: 2 on top.
    let shape = StockShape::Cylinder { diameter: 20.0, height: 12.0 };
    let (world, mut h) = document(p(-5.0, -5.0, 0.0), p(5.0, 5.0, 10.0), shape.clone(), Location::default());
    let before = stock_state(&world);
    let to = Location { pos: p(30.0, 0.0, 0.0), rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()) };
    h.execute(&world, MoveModel::new(&world, "part", to).expect("turned"));
    let (s, at, _) = stock_state(&world);
    let at = at.expect("placed");
    assert_eq!(at.rot, IDENTITY, "the bar's axis stays on the machine Z");
    assert!(close([at.pos.x, at.pos.y, at.pos.z], [30.0, 0.0, 0.0]), "{:?}", at.pos);
    assert_eq!(s, Some(shape), "same reach from the axis, same allowance: same bar");
    assert!(h.undo(&world));
    assert_eq!(stock_state(&world), before);
}

#[test]
fn a_translation_only_move_carries_the_stock_rigidly_as_before() {
    let (world, mut h) = boxed();
    let (shape, _, tris) = stock_state(&world);
    h.execute(&world, MoveModel::new(&world, "part", Location { pos: p(5.0, 3.0, 1.0), ..Location::default() }).expect("slid"));
    let (s, at, t) = stock_state(&world);
    assert_eq!(s, shape);
    assert_eq!(t, tris, "the block is not rebuilt");
    assert_eq!(at, Some(Location { pos: p(15.5, 8.5, 1.0), ..Location::default() }));
    // A turned part slid without a further turn: still a rigid carry.
    h.execute(&world, MoveModel::new(&world, "part", turned_30()).expect("turned"));
    let (s, at, t) = stock_state(&world);
    let at = at.expect("placed");
    let slide = Location { pos: p(60.0, 45.0, 2.0), rot: turned_30().rot };
    h.execute(&world, MoveModel::new(&world, "part", slide).expect("slid"));
    let (s2, at2, t2) = stock_state(&world);
    assert_eq!((s2, t2), (s, t));
    let at2 = at2.expect("placed");
    assert_eq!(at2.rot, at.rot, "no turn: the block keeps its orientation");
    assert!(close([at2.pos.x, at2.pos.y, at2.pos.z], [at.pos.x + 10.0, at.pos.y + 5.0, at.pos.z + 2.0]), "{:?}", at2.pos);
}

#[test]
fn a_turned_or_copied_stock_still_turns_rigidly_with_the_part() {
    // A box already off the machine axes (15 deg) is not ours to square.
    let shape = StockShape::Box { dx: 30.0, dy: 30.0, dz: 10.0 };
    let off = Location { pos: p(10.0, 5.0, 0.0), rot: quat_from_euler(0.0, 0.0, 15f64.to_radians()) };
    let (world, mut h) = document(p(0.0, 0.0, 0.0), p(20.0, 10.0, 8.0), shape.clone(), off);
    let to = Location { pos: p(0.0, 0.0, 0.0), rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()) };
    h.execute(&world, MoveModel::new(&world, "part", to).expect("turned"));
    let (s, at, _) = stock_state(&world);
    let at = at.expect("placed");
    let want = quat_from_euler(0.0, 0.0, 45f64.to_radians());
    assert!((0..4).all(|k| (at.rot[k] - want[k]).abs() < 1e-12), "turned with the part: {:?}", at.rot);
    assert_eq!(s, Some(shape));
    // A stock copied from the model rides the model.
    let (world, mut h) = document(
        p(0.0, 0.0, 0.0),
        p(20.0, 10.0, 8.0),
        StockShape::FromModel { source: "part".into() },
        Location::default(),
    );
    h.execute(&world, MoveModel::new(&world, "part", turned_30()).expect("turned"));
    let (_, at, _) = stock_state(&world);
    let part = model::transform_of(&world, &model::find_by_name(&world, "part").expect("part"));
    assert_eq!(at, part, "the copy sits on the part");
}
