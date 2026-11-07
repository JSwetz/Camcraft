//! Issue #103: a turned path's axial rapids judged in the meridian
//! plane against the revolved stock. A profile rough whose approach
//! along the spindle axis stands inside a bar wider than the part is
//! named at generation and refused at the post; the same approach
//! beside a bar the part's own size is clean.
//!
//! Gate map (M4.4): covers app::operation (turning_rapids, generate's
//! rapid law wiring, job's prior plan for the turned family),
//! app::posting (the verification gate on a lathe document).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

fn lathe() -> Machine {
    Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        ..Default::default()
    }
}

/// The stage-B flanged bar (r6, neck r3.5 over Z 2) in a round bar of
/// `diameter` from Z -16 to 16, roughed by R1.
fn rough_in_bar(diameter: f64) -> (World, Vec<Warning>, Vec<camcraft_app::cl::Move>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, StockShape::Cylinder { diameter, height: 32.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe())).expect("lathe"));
    let name = "TN-1600-0R8";
    let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    let bar = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)], 16);
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let cmd = CreateOperation::new(&world, "R1", name, SrcRef::Entity("bar".into()), rough);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "R1").expect("generate"));
    let e = model::find_by_name(&world, "R1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    (world, cache.warnings.clone(), cache.path.moves.clone())
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-axial-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn an_axial_approach_inside_a_wider_bar_is_named_and_blocks_the_post() {
    // The rough approaches at r 6.85 from Z 25 down to Z 2: a 20 mm bar
    // stands there from Z 16 down - 14 mm of uncut stock.
    let (world, warns, moves) = rough_in_bar(20.0);
    let m0 = moves.first().expect("a path");
    assert!(m0.kind == MoveKind::Rapid && m0.from.x == m0.to.x && m0.to.z < 16.0, "the fixture's approach: {m0:?}");
    assert!(warns.contains(&Warning::RapidInStock { at: 0 }), "the approach into the bar went unnamed: {warns:?}");
    let out = tmp_nc("wide");
    match posting::post_setup(&world, &out) {
        Ok(_) => panic!("a program rapiding into the bar posted"),
        Err(e) => assert!(e.contains("post blocked") && e.contains("rapid move 0 "), "refused, but not by the rapid law: {e}"),
    }
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn the_same_approach_beside_a_bar_the_parts_size_is_clean() {
    let (world, warns, _) = rough_in_bar(12.0);
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    let out = tmp_nc("snug");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}
