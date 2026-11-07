//! #96 at the app level: a mill op's vertical rapid down into declared
//! stock is a `RapidInStock` at generation and the post gate refuses
//! it by name; the same op over stock standing to its top is silent.
//! The turned family's axial rapids keep the reading they had before
//! (the column cannot see the meridian - `operation::turning_rapids`);
//! the turning_bore, turning_contour and lathe_post suites pin that.
//!
//! Provenance: our own fixtures (the posting suite's 10 x 10 x 5 box
//! under a contour), specced from the Prime Directive and the shop rule
//! "never traverse through uncut stock"; no vendor material.
//!
//! Gate map (#96): covers camcraft_app::operation (generate's verify
//! wiring, turning_rapids, job's prior plan), camcraft_app::rest
//! (PriorPlan), camcraft_app::posting (the gate),
//! camcraft_sim::verify (rapids, prior), camcraft_app::operation::stamp
//! (#109: the program ahead in the stamp).
//! Lane shape: all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

/// A contour of the 5 mm box: top 5, clearance 10, retract 8 - the
/// plunge-height rapid lands at Z 7.
fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The box under the contour in a block `stock_dz` tall (Z 0..dz).
fn world_with(stock_dz: f64) -> (World, Vec<Warning>, Vec<camcraft_app::cl::Move>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: stock_dz }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    (world, cache.warnings.clone(), cache.path.moves.clone())
}

#[test]
fn a_vertical_rapid_into_a_taller_block_is_named_and_blocks_the_post() {
    // The 10 mm block stands 5 mm over the contour's top: the rapid
    // straight down from clearance to the plunge height (Z 7) beside
    // the box crosses 3 mm of uncut stock.
    let (world, warns, moves) = world_with(10.0);
    let vertical_down: Vec<usize> = moves
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.kind == MoveKind::Rapid && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z < 10.0 - 1e-6 && m.to.z < m.from.z
        })
        .map(|(i, _)| i)
        .collect();
    assert!(!vertical_down.is_empty(), "the fixture must rapid down under the block's top: {moves:?}");
    // The first dive meets the raw block and is named; a later level's
    // re-entry down the first plunge's own kerf is acquitted by the
    // cuts before it (cut order), so it may stay silent.
    assert!(warns.contains(&Warning::RapidInStock { at: vertical_down[0] }), "the first dive went unnamed: {warns:?}");
    assert!(
        warns.iter().all(|w| matches!(w, Warning::RapidInStock { at } if vertical_down.contains(at))),
        "only the dives are convicted: {warns:?}"
    );
    let out = tmp_nc("dive");
    match posting::post_setup(&world, &out) {
        Ok(_) => panic!("a program with a rapid into the block posted"),
        Err(e) => assert!(
            e.contains("post blocked") && e.contains(&format!("rapid move {} ", vertical_down[0])),
            "refused, but not by the rapid law: {e}"
        ),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn the_same_contour_over_stock_to_its_top_is_silent() {
    let (world, warns, _) = world_with(5.0);
    assert!(warns.is_empty(), "{warns:?}");
    let out = tmp_nc("clean");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

/// A face of the whole stock block down to `cut` (from its top, one
/// pass).
fn face(cut: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(20.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The 10 mm block, a face of it to `face_cut`, then the contour -
/// both generated in program order (the names sort the face first,
/// `setup::operation_order`): the contour's verdict and the post's.
fn face_then_contour(face_cut: f64, tag: &str) -> (Vec<Warning>, Vec<Warning>, Result<(), String>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Op1Face", "EM-0600-4FL", SrcRef::Stock, face(face_cut));
    history.execute(&world, cmd.expect("create face"));
    let cmd = CreateOperation::new(&world, "Op2Contour", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("generate face"));
    history.execute(&world, GenerateOperation::new(&world, "Op2Contour").expect("generate contour"));
    let warns = |name: &str| {
        let e = model::find_by_name(&world, name).expect("op");
        model::toolpath_of(&world, &e).expect("cache").warnings
    };
    let out = tmp_nc(tag);
    let posted = posting::post_setup(&world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    (warns("Op1Face"), warns("Op2Contour"), posted)
}

#[test]
fn a_dive_into_stock_an_earlier_face_removed_posts() {
    // Review of #105: the face takes the 10 mm block down to Z 5, so
    // the contour's rapid 10 -> 7 beside the box falls through air the
    // program ahead of it already cut. The verdict reads the setup's
    // prior stock, not the raw block.
    let (face_w, contour_w, posted) = face_then_contour(5.0, "faced");
    assert!(face_w.is_empty(), "the face itself is clean: {face_w:?}");
    assert!(contour_w.is_empty(), "the faced block acquits the dive: {contour_w:?}");
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn a_dive_into_stock_no_earlier_op_removed_is_still_convicted() {
    // The face stops at Z 8: the dive to Z 7 still crosses 1 mm the
    // program never cut - named, and the post refuses it.
    let (_, contour_w, posted) = face_then_contour(8.0, "half-faced");
    assert!(
        contour_w.iter().any(|w| matches!(w, Warning::RapidInStock { .. })),
        "the dive under the face's floor went unnamed: {contour_w:?}"
    );
    match posted {
        Ok(()) => panic!("a dive under the face's floor posted"),
        Err(e) => assert!(e.contains("post blocked") && e.contains("rapid move"), "refused, but not by the rapid law: {e}"),
    }
}

#[test]
fn a_face_regenerated_shallower_marks_the_acquitted_contour_stale() {
    // #109: the contour's dive is acquitted by the face ahead of it, so
    // the face is an input of the contour's verdict. The face re-cut to
    // Z 8 leaves 1 mm the dive now crosses: the contour reads Stale and
    // the post refuses it, never posting the cached acquittal.
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Op1Face", "EM-0600-4FL", SrcRef::Stock, face(5.0));
    history.execute(&world, cmd.expect("create face"));
    let cmd = CreateOperation::new(&world, "Op2Contour", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("generate face"));
    history.execute(&world, GenerateOperation::new(&world, "Op2Contour").expect("generate contour"));
    let status = |name: &str| camcraft_app::operation::status_of(&world, &model::find_by_name(&world, name).expect("op"));
    assert_eq!(status("Op2Contour"), OpStatus::Ok, "the faced block acquits the dive");
    history.execute(&world, EditOperationParams::new(&world, "Op1Face", face(8.0)).expect("a legal cut"));
    history.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("regenerate face"));
    assert_eq!(status("Op1Face"), OpStatus::Ok, "the shallower face is fresh");
    assert_eq!(status("Op2Contour"), OpStatus::Stale, "the face ahead moved");
    let out = tmp_nc("refaced");
    match posting::post_setup(&world, &out) {
        Ok(_) => panic!("a contour acquitted by a face that no longer cuts there posted"),
        Err(e) => assert!(e.contains("Op2Contour") && e.contains("regenerate"), "refused, but not as stale: {e}"),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-vrapid-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}
