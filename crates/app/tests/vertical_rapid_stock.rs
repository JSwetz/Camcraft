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
//! wiring, turning_rapids), camcraft_app::posting (the gate),
//! camcraft_sim::verify (rapids).
//! Lane shape: all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
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

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-vrapid-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}
