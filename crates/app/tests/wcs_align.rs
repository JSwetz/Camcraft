//! #187: the 3-2-1 work-offset alignment from picks - a flat face sets Z
//! (outward), a straight edge sets X (in that plane), a vertex or bore
//! sets the origin (on the face) - on a block placed square and on one
//! turned 30 degrees about Z, plus every named refusal. The result rides
//! the existing `SetWcsOrigin` and round-trips the pane's angles.
//! Provenance: our own fixtures over the plane-line-point datum law.
//!
//! Gate map (#187): covers app::pick::align, app::commands (SetWcsOrigin).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{History, ImportModel, MoveModel, SetWcsOrigin};
use camcraft_app::geometry::{Point3, cube, feature, quat_from_euler, quat_rotate_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::align_321;
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// A 20 x 20 x 10 block at the origin, optionally turned `yaw_deg` about Z.
fn block(yaw_deg: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    if yaw_deg != 0.0 {
        let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: quat_from_euler(0.0, 0.0, yaw_deg.to_radians()) };
        h.execute(&world, MoveModel::new(&world, "block", loc).expect("turn the block"));
    }
    world
}

fn features(world: &World) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(world, "block").expect("block");
    model::features_of(world, &e).expect("features")
}

/// Picks are by the block's LOCAL geometry (ids do not move with it).
fn face(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = f
        .faces
        .iter()
        .find(|f| (f.normal[0] * p[0] + f.normal[1] * p[1] + f.normal[2] * p[2] - f.offset).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the block has that face");
    GeoSel { entity: "block".into(), feature: FeatureSel::Face(id) }
}

fn edge(world: &World, a: [f64; 3], b: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))
        })
        .map(|c| c.id)
        .expect("the block has that edge");
    GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }
}

fn corner(world: &World, at: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts[vi as usize] == at)
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the block has that corner");
    GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(id) }
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn a_square_block_aligns_to_the_world_frame() {
    let w = block(0.0);
    let picks = [face(&w, [10.0, 10.0, 10.0]), edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]), corner(&w, [0.0, 0.0, 10.0])];
    let a = align_321(&w, &picks).expect("face, edge, corner");
    assert!(near(a.pos, [0.0, 0.0, 10.0]), "origin on the corner: {:?}", a.pos);
    assert!(near(a.z, [0.0, 0.0, 1.0]), "Z out of the top face: {:?}", a.z);
    assert!(near(a.x, [1.0, 0.0, 0.0]), "X along the edge, away from the origin: {:?}", a.x);
    assert!(near(a.y, [0.0, 1.0, 0.0]), "right-handed: {:?}", a.y);
    assert!(a.roll_deg.abs() < 1e-9 && a.pitch_deg.abs() < 1e-9 && a.yaw_deg.abs() < 1e-9, "{a:?}");
}

#[test]
fn the_origin_at_the_far_end_turns_x_round() {
    let w = block(0.0);
    let picks = [face(&w, [10.0, 10.0, 10.0]), edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]), corner(&w, [20.0, 0.0, 10.0])];
    let a = align_321(&w, &picks).expect("aligned");
    assert!(near(a.pos, [20.0, 0.0, 10.0]), "{:?}", a.pos);
    assert!(near(a.x, [-1.0, 0.0, 0.0]), "X runs from the origin along the edge: {:?}", a.x);
    assert!(near(a.y, [0.0, -1.0, 0.0]), "{:?}", a.y);
    assert!((a.yaw_deg.abs() - 180.0).abs() < 1e-9, "{a:?}");
}

#[test]
fn a_turned_block_aligns_to_its_own_edge_and_the_angles_round_trip() {
    let w = block(30.0);
    let picks = [face(&w, [10.0, 10.0, 10.0]), edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]), corner(&w, [0.0, 0.0, 10.0])];
    let a = align_321(&w, &picks).expect("aligned");
    let (s, c) = 30f64.to_radians().sin_cos();
    assert!(near(a.x, [c, s, 0.0]), "X follows the turned edge: {:?}", a.x);
    assert!(near(a.z, [0.0, 0.0, 1.0]), "{:?}", a.z);
    assert!((a.yaw_deg - 30.0).abs() < 1e-9, "{a:?}");
    // The angles the pane sends rebuild the same frame.
    let q = quat_from_euler(a.roll_deg.to_radians(), a.pitch_deg.to_radians(), a.yaw_deg.to_radians());
    let x = quat_rotate_point(q, Point3::new(1.0, 0.0, 0.0));
    assert!(near([x.x, x.y, x.z], a.x), "the encoded X: {x:?}");
    // And the existing command takes them.
    let mut h = History::default();
    let loc = Location { pos: Point3::new(a.pos[0], a.pos[1], a.pos[2]), rot: q };
    h.execute(&w, SetWcsOrigin::new(&w, "G54", loc).expect("SetWcsOrigin takes the frame"));
    let g54 = model::find_by_name(&w, "G54").and_then(|e| model::transform_of(&w, &e)).expect("G54 placed");
    assert!(near([g54.pos.x, g54.pos.y, g54.pos.z], a.pos));
}

#[test]
fn a_side_face_tilts_z_and_without_an_origin_the_edge_start_is_used() {
    let w = block(0.0);
    // The front wall (y = 0): Z points out of it, -Y; X along its top edge.
    let picks = [face(&w, [10.0, 0.0, 5.0]), edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0])];
    let a = align_321(&w, &picks).expect("aligned");
    assert!(near(a.z, [0.0, -1.0, 0.0]), "outward from the wall: {:?}", a.z);
    assert!(a.x[0].abs() > 1.0 - 1e-9 && a.x[1].abs() < 1e-9 && a.x[2].abs() < 1e-9, "{:?}", a.x);
    assert!(a.pos[1].abs() < 1e-9 && (a.pos[2] - 10.0).abs() < 1e-9, "on the wall, at the edge's end: {:?}", a.pos);
}

#[test]
fn every_missing_or_doubled_datum_is_refused_by_name() {
    let w = block(0.0);
    let top = face(&w, [10.0, 10.0, 10.0]);
    let side = face(&w, [10.0, 0.0, 5.0]);
    let front = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let upright = edge(&w, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
    let c0 = corner(&w, [0.0, 0.0, 10.0]);
    let c1 = corner(&w, [20.0, 0.0, 10.0]);
    let says = |picks: &[GeoSel], want: &str| {
        let e = align_321(&w, picks).expect_err("must refuse");
        assert!(e.contains(want), "{want:?} not in {e:?}");
    };
    says(&[], "pick a flat face");
    says(&[front.clone()], "pick a flat face");
    says(&[top.clone()], "pick a straight edge");
    says(&[top.clone(), side, front.clone()], "only one flat face");
    says(&[top.clone(), front.clone(), front.clone()], "only one edge");
    says(&[top.clone(), front.clone(), c0, c1], "only one origin");
    says(&[top.clone(), upright], "square to the face");
    let ghost = GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(0xdead_beef) };
    says(&[top, front, ghost], "no longer in the model");
}
