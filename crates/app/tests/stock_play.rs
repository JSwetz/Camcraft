//! M7.2 C (app half): `stock_play::FieldPlay` - the display field cut
//! move by move with the transport. The end of playback is exactly the
//! Deviate picture (`field_view_with`), a backward seek restores what a
//! forward cut to the same move showed, nothing is cut before the first
//! move completes, and the transport's script maps onto the plan by
//! name. Provenance: our own fixtures over the M7.1 field engine;
//! nothing from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_play, app::stock_view (display_parts, display_target), app::sim seam.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::{progress, FieldPlay, Progress, MAX_CHECKPOINTS};
use camcraft_app::stock_sim::{Preview, PreviewCamera, Stock, DEV_TOL};
use camcraft_app::stock_view::field_view_with;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A 12x12 box: the smallest the Face program still makes several rows
/// of moves on, so a debug cut takes a second or two.
const HALF: f64 = 6.0;

/// ~1.5 mm cells, the `stock_view` suite's own cell law.
const COARSE: f64 = (2.0 * HALF) * (2.0 * HALF) * 20.0 / 3.6;

fn faced_doc() -> World {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let lo = Point3::new(-HALF, -HALF, 0.0);
    let hi = Point3::new(HALF, HALF, 18.0);
    history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    let params = OpParams::Face {
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    world
}

fn frame(s: &Stock) -> Preview {
    let cam = PreviewCamera::look_at(
        Point3::new(-25.0, -30.0, 45.0),
        Point3::new(0.0, 0.0, 10.0),
        Point3::new(0.0, 0.0, 1.0),
        0.5,
    )
    .expect("camera");
    s.preview(None, &cam, 24, 24, DEV_TOL).expect("frame")
}

/// Same hits, same depths (within f64 noise of the order-free max).
fn same(a: &Preview, b: &Preview) -> bool {
    a.depth.iter().zip(&b.depth).all(|(x, y)| {
        (x.is_nan() && y.is_nan()) || (x - y).abs() <= 1e-6
    })
}

#[test]
fn playback_unfolds_to_the_deviate_picture_and_seeks_back_exactly() {
    let world = faced_doc();
    let mut play = FieldPlay::with_budget(&world, false, COARSE).expect("playback builds");
    assert!(play.total() > 4, "the face program has rows to play: {}", play.total());
    assert!(!play.judged(), "unarmed playback carries no verdict");
    let uncut = frame(&play.view().stock);

    let half = play.total() / 2;
    assert!(play.seek(half).expect("forward"));
    let at_half = frame(&play.view().stock);
    assert!(!same(&uncut, &at_half), "half the program removed material the eye sees");

    assert!(play.seek(usize::MAX).expect("to the end"), "past the end clamps to the end");
    assert_eq!(play.at(), play.total());
    let full = field_view_with(&world, COARSE).expect("deviate view");
    assert!(same(&frame(&play.view().stock), &frame(&full.stock)), "the last frame IS the Deviate picture");

    assert!(play.seek(half).expect("backward"));
    assert!(same(&frame(&play.view().stock), &at_half), "a backward seek restores the forward cut");
    assert!(play.seek(0).expect("rewind"));
    assert!(same(&frame(&play.view().stock), &uncut), "move 0 is the uncut stock");
    assert!(!play.seek(0).expect("no-op"), "an unchanged seek says nothing changed");
    assert!(play.checkpoints() <= MAX_CHECKPOINTS + 1);
}

#[test]
fn the_transport_maps_onto_the_plan_by_name() {
    let world = faced_doc();
    let play = FieldPlay::with_budget(&world, false, COARSE).expect("playback builds");
    let e = model::find_by_name(&world, "Face1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("path").path;
    let tr = sim::Traverse::from_machine(&camcraft_app::posting::machine_value(&world));
    let sc = sim::script([("Face1", &tp)], &tr);

    let p0 = progress(&sc, 0.0).expect("a timeline");
    assert_eq!((p0.op.as_str(), p0.segs_done), ("Face1", 0), "nothing is cut before the first move completes");
    assert_eq!(play.index_at(&p0), Some(0));

    let end = progress(&sc, sc.total + 1.0).expect("a timeline");
    assert_eq!(play.index_at(&end), Some(play.total()), "the end cuts every move");

    let mid = progress(&sc, sc.total * 0.5).expect("a timeline");
    let k = play.index_at(&mid).expect("on the plan");
    assert!(k > 0 && k < tp.moves.len(), "mid-program is mid-plan: {k}");

    // A connector rapid ahead of the op is air, not a move.
    let lead = Progress { op: "Face1".into(), segs_done: 1, span: tp.moves.len() + 1 };
    assert_eq!(play.index_at(&lead), Some(0), "the connector cuts nothing");
    let ghost = Progress { op: "NoSuchOp".into(), segs_done: 3, span: 9 };
    assert_eq!(play.index_at(&ghost), None, "an op the plan lacks is a refusal, not a skip");
    assert!(progress(&sim::Script::default(), 1.0).is_none(), "an empty timeline has no position");
}
