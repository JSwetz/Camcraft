//! The built-in Lathe and Mill-turn strokes (#493): travel bands are
//! read in PROGRAM coordinates through the work offset (`app::machine`
//! module doc; the convention #457 / PR #477 states for the mills), so
//! a slide of stroke L along the spindle is stated `-L..L`. The shipped
//! `-615..0` / `-620..0` Z bands were machine-frame numbers read as
//! program ones: every clearance in front of the face (program Z > 0)
//! was "out of stroke" and no turning program posted. X stays one-sided:
//! a lathe X is measured from the spindle centreline, which IS the
//! program's X0, so `0..R` is already a program-frame band. Pinned
//! here: a face-and-turn job with its clearance in front of the face
//! passes on both built-ins, and a genuine over-travel still refuses by
//! name.
//!
//! Provenance: general lathe practice (Z0 on the finished face, the
//! part in -Z, X from the centreline); every number is our own fixture.
//!
//! Gate map (M4.4): covers app::machine, app::posting, app::commands.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, check_document};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

const TAU: f64 = std::f64::consts::TAU;
const TOOL: &str = "TN-1600-0R8";

/// A closed revolve of `meridian` (radius, z), 16 ways.
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// A flanged bar with its finished face on Z0 (G54 unplaced = the
/// world origin), the part in -Z: flange r10 from z -40 to -20, neck
/// r7 to the face. Stock is the bar's envelope plus 1 mm to face off
/// in front of Z0.
const BAR: [(f64, f64); 6] = [(0.0, -40.0), (10.0, -40.0), (10.0, -20.0), (7.0, -20.0), (7.0, 0.0), (0.0, 0.0)];

fn generate(world: &World, history: &mut History, name: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, TOOL, SrcRef::Entity("bar".into()), params);
    history.execute(world, cmd.expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name} generates");
}

/// Face and rough-turn the bar on the built-in `machine_name`, every
/// clearance at `clearance` in front of the face.
fn job_on(machine_name: &str, clearance: f64) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -40.0), ..Location::default() };
    let stock = SetStock::new(&world, StockShape::Cylinder { diameter: 20.0, height: 41.0 }, at);
    h.execute(&world, stock.expect("bar stock"));
    h.execute(&world, LoadMachine::builtin(&world, machine_name).expect("built-in machine"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR)));
    let facing = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(clearance),
        top_z: Height::fixed(1.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut h, "Face1", facing);
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(clearance),
        top_z: Height::fixed(1.0),
        bottom_z: Height::fixed(-20.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut h, "Rough1", rough);
    world
}

fn mounted(world: &World, name: &str) -> machine::Machine {
    let (path, m) = machine::of(world);
    assert_eq!(path, posting::builtin_path(name), "the built-in is the document machine");
    m.expect("the built-in is mounted")
}

fn passes_in_front_of_the_face(name: &str, stroke: f64) {
    let world = job_on(name, 5.0);
    let m = mounted(&world, name);
    let z = m.axis(2).and_then(|a| a.travel).expect("Z stroke");
    assert_eq!(z, (-stroke, stroke), "{name}: the Z slide, full stroke each way");
    let c = check_document(&world, &m).expect("a fresh program checks");
    assert!(c.is_ok(), "{name}: clearance Z 5 in front of the face: {:?}", c.problems);
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{name}: the gate is clean: {:?}", report.problems);
    let path = std::env::temp_dir().join(format!("camcraft_lathe_stroke_{stroke:.0}_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    let posted = posted.unwrap_or_else(|e| panic!("{name} must post a face-and-turn job: {e}"));
    assert!(!posted.program.is_empty());
}

fn overtravel_refuses_by_name(name: &str, stroke: f64) {
    // A clearance more than the whole stroke in front of a face the
    // program also cuts at Z0 fits no placement of the work offset.
    let clearance = stroke + 20.0;
    let world = job_on(name, clearance);
    let m = mounted(&world, name);
    let c = check_document(&world, &m).expect("checks");
    let want = format!("in Z, the stroke is {:.3}..{:.3}", -stroke, stroke);
    assert!(
        c.problems.iter().any(|p| p.starts_with("Rough1:") && p.contains(&format!("reaches {clearance:.3}")) && p.contains(&want)),
        "{name}: {:?}",
        c.problems
    );
    assert!(!c.problems.iter().any(|p| p.contains(" in X")), "{name}: X stays inside 0..R: {:?}", c.problems);
    let report = posting::verification_report(&world);
    assert!(report.problems.iter().any(|p| p.contains("the stroke is")), "{name}: {:?}", report.problems);
    let path = std::env::temp_dir().join(format!("camcraft_lathe_overtravel_{stroke:.0}_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_err(), "{name}: an over-travel never posts");
}

#[test]
fn a_turning_job_on_the_built_in_lathe_clears_the_face_and_posts() {
    passes_in_front_of_the_face("Lathe", 615.0);
}

#[test]
fn a_turning_job_on_the_built_in_millturn_clears_the_face_and_posts() {
    passes_in_front_of_the_face("Mill-turn", 620.0);
}

#[test]
fn a_clearance_past_the_lathe_carriage_still_refuses_by_name() {
    overtravel_refuses_by_name("Lathe", 615.0);
}

#[test]
fn a_clearance_past_the_millturn_quill_still_refuses_by_name() {
    overtravel_refuses_by_name("Mill-turn", 620.0);
}

#[test]
fn lathe_x_stays_a_centreline_band() {
    // X is measured from the spindle centreline, the program's own X0:
    // the one-sided band is right and is kept.
    for (name, r) in [("Lathe", 320.0), ("Mill-turn", 650.0)] {
        let world = job_on(name, 5.0);
        let x = mounted(&world, name).axis(0).and_then(|a| a.travel).expect("X stroke");
        assert_eq!(x, (0.0, r), "{name}");
    }
}
