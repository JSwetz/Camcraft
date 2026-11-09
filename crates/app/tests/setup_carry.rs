//! #317: the program sim carries the machined body into the next setup.
//! Face the top in Setup1, flip the part over X in Setup2 and face the
//! former bottom: Setup2 starts from what Setup1 cut, the final verdict
//! matches a single-frame reference, and a gouge cut in Setup1 is still
//! judged after the flip. A quarter turn re-indexes the body, a link
//! "Same as" carries it unchanged, and a turn the lattice cannot carry
//! exactly is refused by name.
//! Provenance: our own fixtures (a faced block) over plain re-clamp
//! practice; no vendor material consulted.
//!
//! Gate map (#317): covers app::stock_sim (simulate_document, carry,
//! incoming_body), app::setup_relative (transform_between),
//! app::setup_align, app::commands (AlignPartToSetup, SetSetupRelative,
//! CreateSetup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel,
    SetSetupRelative, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{CARRY_TURN_REFUSAL, DEV_TOL, ProgramSim, incoming_body, simulate_document};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// A part `2*hx` x `2*hy` x [0, 16] in a stock of the same footprint
/// from Z `z0` up `dz`, Setup1 materialized, and Face1 faced to `cut1`.
fn document(hx: f64, hy: f64, z0: f64, dz: f64, cut1: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-hx, -hy, 0.0), Point3::new(hx, hy, 16.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, z0), ..Default::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 2.0 * hx, dy: 2.0 * hy, dz }, at);
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    add_face(&world, &mut h, "Face1", cut1);
    (world, h)
}

fn add_face(world: &World, h: &mut History, name: &str, cut: f64) {
    let op = CreateOperation::new(world, name, TOOL, SrcRef::Stock, face(cut)).expect("create");
    h.execute(world, op);
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// Setup2 from Setup1's datum (it becomes the active setup).
fn second_setup(world: &World, h: &mut History) {
    h.execute(world, CreateSetup::new(world, None, Some("Setup1")).expect("Setup2"));
}

fn bits(s: &ProgramSim) -> Vec<u64> {
    s.dev.dev.iter().map(|v| v.to_bits()).collect()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn a_flipped_setup_starts_from_what_the_first_one_cut() {
    // Stock Z -2..18 over a part Z 0..16; Setup1 faces the top to 16.
    let (world, mut h) = document(30.0, 30.0, -2.0, 20.0, 16.0);
    second_setup(&world, &mut h);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    add_face(&world, &mut h, "Face2", 16.0);

    // Flipped about the part centre (Z 8): Setup1's untouched bottom
    // (-2) stands on top at 18, and its faced top (16) is the floor at 0
    // - the 2 mm Setup1 removed is absent from below.
    let incoming = incoming_body(&world, "Setup2").expect("Setup2's incoming stock");
    let (lo, hi) = incoming.bounds().expect("material");
    assert!(near(lo.z, 0.0, 1e-9) && near(hi.z, 18.0, 1e-9), "{lo:?} {hi:?}");
    assert!(near(lo.x, -30.0, 1e-9) && near(hi.y, 30.0, 1e-9), "{lo:?} {hi:?}");
    assert!(near(incoming.volume(), 60.0 * 60.0 * 18.0, 1e-6), "{}", incoming.volume());
    assert!(incoming.floor_judged(), "the faced side faced a tool");

    // Facing the former bottom leaves exactly the part: clean both sides.
    let sim = simulate_document(&world).expect("the program spans the flip");
    assert!(sim.dev.max_rest <= DEV_TOL && sim.dev.max_gouge <= DEV_TOL, "{:?}", (sim.dev.max_rest, sim.dev.max_gouge));

    // The single-frame reference: one setup holding a block Z 0..18 (the
    // carried body) faced to 16 reads the same verdict, column for column.
    let (refw, _rh) = document(30.0, 30.0, 0.0, 18.0, 16.0);
    let reference = simulate_document(&refw).expect("reference");
    assert_eq!((reference.dev.cells_x, reference.dev.cells_y), (sim.dev.cells_x, sim.dev.cells_y));
    for (k, (a, b)) in sim.dev.dev.iter().zip(&reference.dev.dev).enumerate() {
        assert!(a.is_nan() == b.is_nan() && (a.is_nan() || near(*a, *b, DEV_TOL)), "column {k}: {a} vs {b}");
    }
}

#[test]
fn a_gouge_cut_before_the_flip_is_still_judged_after_it() {
    // Setup1 faces 1 mm into the part's top; Setup2 is clean on its side.
    let (world, mut h) = document(30.0, 30.0, -2.0, 20.0, 15.0);
    second_setup(&world, &mut h);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    add_face(&world, &mut h, "Face2", 16.0);
    let sim = simulate_document(&world).expect("judged");
    assert!(near(sim.dev.max_gouge, 1.0, 1e-6), "the gouge survives the flip: {}", sim.dev.max_gouge);
    let at = sim.dev.max_gouge_at.expect("located");
    assert!(near(at.z, 1.0, 1e-6), "on the floor side, at Z 16 - 15: {at:?}");
}

#[test]
fn a_quarter_turn_carries_the_body_reindexed() {
    // A 60 x 40 part and stock; Setup2 turns it 90 degrees about Z.
    let (world, mut h) = document(30.0, 20.0, -2.0, 20.0, 16.0);
    second_setup(&world, &mut h);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::TurnZ90).expect("turn"));
    add_face(&world, &mut h, "Face2", 16.0);
    let incoming = incoming_body(&world, "Setup2").expect("incoming");
    let (lo, hi) = incoming.bounds().expect("material");
    assert!(near(hi.x - lo.x, 40.0, 1e-9) && near(hi.y - lo.y, 60.0, 1e-9), "lies 40 x 60: {lo:?} {hi:?}");
    assert!(near(hi.z, 16.0, 1e-9) && near(lo.z, -2.0, 1e-9), "Setup1's face stays on top: {lo:?} {hi:?}");
    assert!(near(incoming.volume(), 60.0 * 40.0 * 18.0, 1e-6), "{}", incoming.volume());
    assert!(!incoming.floor_judged());
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.max_rest <= DEV_TOL && sim.dev.max_gouge <= DEV_TOL, "{:?}", (sim.dev.max_rest, sim.dev.max_gouge));
}

#[test]
fn a_setup_linked_same_as_the_first_carries_the_body_unchanged() {
    let (world, mut h) = document(30.0, 30.0, -2.0, 20.0, 17.0);
    second_setup(&world, &mut h);
    let link = SetSetupRelative::link(&world, "Setup2", "Setup1", [0.0, 0.0, 0.0]).expect("link");
    h.execute(&world, link);
    add_face(&world, &mut h, "Face2", 16.0);
    let carried = simulate_document(&world).expect("identity carry");

    // The same two faces in one setup: the same verdict, bit for bit.
    let (refw, mut rh) = document(30.0, 30.0, -2.0, 20.0, 17.0);
    add_face(&refw, &mut rh, "Face2", 16.0);
    let reference = simulate_document(&refw).expect("reference");
    assert_eq!(bits(&carried), bits(&reference));
    assert_eq!(carried.dev.max_rest.to_bits(), reference.dev.max_rest.to_bits());
}

#[test]
fn a_turn_the_lattice_cannot_carry_is_refused_by_name() {
    let (world, mut h) = document(30.0, 30.0, -2.0, 20.0, 16.0);
    second_setup(&world, &mut h);
    let skew = AlignPartToSetup::angles(&world, "Setup2", [0.0, 0.0, 30.0]).expect("30 degrees about Z");
    h.execute(&world, skew);
    let op = CreateOperation::new(&world, "Face2", TOOL, SrcRef::Entity("part".into()), face(16.0)).expect("create");
    h.execute(&world, op);
    // #318: Face2 reads the program ahead, whose stock cannot be carried
    // in - its generation is refused by name, never cut on a guess.
    let err = GenerateOperation::new(&world, "Face2").err().expect("generation refused").to_string();
    assert!(err.contains("from setup 'Setup1' into setup 'Setup2'") && err.contains(CARRY_TURN_REFUSAL), "{err}");
    // The program cannot be judged either: Face2 never generated, so the
    // walk refuses before it reaches the carry.
    assert!(incoming_body(&world, "Setup2").is_err());
}
