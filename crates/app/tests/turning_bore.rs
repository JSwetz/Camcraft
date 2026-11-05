//! M5.2 stage F boring tests at the app level: the wiring - a bore
//! op's whole life through the document. What "right" means here: a
//! boring bar on a modeled hole, on a mounted lathe, generates Ok
//! with the envelope verdict silent; the cached path is the kernel's
//! signature seen from the world - every ride center inside the wall
//! by the bar's own nose, every rapid down the hole's axis (the bar
//! leaves through the hole's air, never over the blank); the wrong
//! tool on the kind is refused by name, a plain bar is refused as
//! having nothing to bore, and re-grinding the bar's nose convicts
//! through the TOOL digest (Stale both ways) - never a silent Ok.
//! The kernel's own laws are pinned one crate down (camcraft-motion's
//! turning_bore suite); this file is the document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the bore arm of
//! generate, the envelope verdict wiring, the global nose_radius row),
//! camcraft_app::commands (Create/GenerateOperation, ChangeTool,
//! EditOperationParams undo).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, EditOperationParams, GenerateOperation, History,
    ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Carried, Height, KindInfo, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const BLANK: f64 = 10.0;
const WALL: f64 = 6.0;
const FLOOR: f64 = -20.0;
const D: f64 = 0.4;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    camcraft_app::operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

fn rho(q: Point3) -> f64 {
    (q.x * q.x + q.y * q.y).sqrt()
}

/// The closed-solid revolve the turning suites share.
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

/// The bored bar: OD 10, an open hole of radius 6 from the top down
/// to a floor at -20, on a bar long past the hole so the axis fit
/// reads a revolve, not a dome.
fn bored() -> Mesh {
    revolve(&[
        (0.0, -50.0),
        (BLANK, -50.0),
        (BLANK, 50.0),
        (WALL, 50.0),
        (WALL, FLOOR),
        (0.0, FLOOR),
    ])
}

/// The plain bar the hole-existence door must refuse.
fn wall() -> Mesh {
    revolve(&[(0.0, -50.0), (BLANK, -50.0), (BLANK, 50.0), (0.0, 50.0)])
}

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

/// A document the boring kind lives in: the bored bar AND a smooth
/// wall in plain box stock, the lathe mounted, the catalog boring
/// bar (and the other lathe steel) on the rack.
fn bore_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            camcraft_app::stock::StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: 110.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    add_tool(&world, &mut history, "BR-1000");
    add_tool(&world, &mut history, "TN-1600-0R8");
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("bar", bored()));
    history.execute(&world, ImportModel::from_mesh("wall", wall()));
    (world, history)
}

fn bore_params() -> OpParams {
    OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(60.0),
        top_z: Height::fixed(40.0),
        bottom_z: Height::fixed(-10.0),
        feed: 120.0,
        plunge_feed: 40.0,
    }
}

fn create_on(world: &World, history: &mut History, name: &str, tool: &str, src: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    create_on(world, history, name, "BR-1000", "bar", params);
}

#[test]
fn a_bored_op_runs_clean_in_the_document() {
    // The document's whole life: create, generate, and the envelope
    // verdict comes back silent, so the op reads Ok with nothing
    // attached. The cached path from the world's side: every ride
    // center stands inside the modeled wall by the bar's own nose,
    // and the bar travels to and from the cut down the hole's AXIS -
    // the rapids are the hole's air, not the blank's outside.
    let (world, mut history) = bore_world();
    create(&world, &mut history, "BR1", bore_params());
    let c = cache(&world, "BR1").expect("cached");
    assert_eq!(status(&world, "BR1"), OpStatus::Ok, "the wall verdict is silent");
    assert!(c.warnings.is_empty(), "a matched bar reported: {:?}", c.warnings);
    let t = c.path;
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind != camcraft_app::cl::MoveKind::Rapid).collect();
    assert!(feeds.len() > 4, "the ride is chord-settled, not two endpoints");
    for m in &feeds {
        assert!(rho(m.to) <= WALL - D + 1e-9, "center stands inside the wall: {:?}", m.to);
    }
    for m in t.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Rapid) {
        assert!(rho(m.to) < 1e-6, "a boring rapid leaves the axis - through steel: {:?}", m.to);
    }
    assert!((feeds.last().unwrap().to.z - 40.0).abs() < 0.05, "the ride spends the window's top");
    assert!(t.is_chained(), "the chain holds");
}

#[test]
fn a_bore_kind_refuses_the_wrong_tool_by_name() {
    // The family door admits the KIND; the tool door is the
    // generator's, and a turning insert is not hole steel - the
    // refusal names the op's tool and what that tool is.
    let (world, mut history) = bore_world();
    common::create_past_the_tool_law(&world, &mut history, "BT1", "TN-1600-0R8", SrcRef::Entity("bar".into()), bore_params());
    let Err(err) = common::generate_past_the_tool_law(&world, "BT1") else {
        panic!("an outside insert cannot ride an inside wall");
    };
    let msg = err.to_string();
    assert!(msg.contains("needs a boring bar") && msg.contains("TN-1600-0R8"), "{msg}");
}

#[test]
fn a_plain_bar_refuses_to_be_bored_not_wrong() {
    // The killer law at the document level: the model's own min-wall
    // reads a plain bar's skin as its hole, and only the axis parity
    // knows better. Silent tangency is the failure this door closes;
    // the refusal is Empty, because there is nothing to bore.
    let (world, mut history) = bore_world();
    history.execute(
        &world,
        CreateOperation::new(&world, "BW1", "BR-1000", SrcRef::Entity("wall".into()), bore_params())
            .expect("create"),
    );
    let Err(err) = GenerateOperation::new(&world, "BW1") else {
        panic!("a plain bar has no hole to ride");
    };
    let msg = err.to_string();
    assert!(msg.contains("nothing to bore") && msg.contains("drilling"), "{msg}");
}

#[test]
fn a_regrind_of_the_nose_convicts_through_the_digest() {
    // The mounted-tool law: the ride radius rides the TOOL digest
    // (the global nose_radius row), so re-grinding the bar marks the
    // op Stale on its own - and the regenerated ride, deeper by the
    // new nose but still flush with the MODEL's wall, reads Ok.
    // Undoing the grind reads Stale again: the digest is a question
    // about NOW, never a remembered verdict.
    let (world, mut history) = bore_world();
    create(&world, &mut history, "BR1", bore_params());
    assert_eq!(status(&world, "BR1"), OpStatus::Ok);
    let mut fatter = catalog_tool("BR-1000");
    fatter.nose_radius = 0.8; // re-ground, same name; still sane steel
    history.execute(&world, ChangeTool::new(&world, "BR-1000", fatter).expect("a sane bar"));
    assert_eq!(status(&world, "BR1"), OpStatus::Stale, "a re-ground nose is a different cut");
    history.execute(&world, GenerateOperation::new(&world, "BR1").expect("regenerate"));
    assert_eq!(status(&world, "BR1"), OpStatus::Ok, "the wall is the model's, and it still holds");
    let c = cache(&world, "BR1").expect("cached");
    assert!(c.warnings.is_empty(), "the deeper ride reported: {:?}", c.warnings);
    let feeds: Vec<_> = c.path.moves.iter().filter(|m| m.kind != camcraft_app::cl::MoveKind::Rapid).collect();
    assert!(
        feeds.iter().all(|m| rho(m.to) <= WALL - 0.8 + 1e-9),
        "the regenerated ride still stands nose-out of the wall"
    );
    assert!(history.undo(&world), "undo the grind");
    assert_eq!(status(&world, "BR1"), OpStatus::Stale, "and the cached 0.8 ride reads stale now");
}

#[test]
fn the_bore_seed_stands() {
    // Registry end to end: pick the kind, accept the seed - and the
    // bored bar takes the whole ride on those numbers, verdict
    // silent.
    let (world, mut history) = bore_world();
    let c = Carried {
        feed: 120.0,
        plunge: 40.0,
        climb: true,
        clearance: Height::fixed(60.0),
        stepover: 0.0,
        stepdown: 0.0,
        top: Height::fixed(40.0),
        bottom: Height::fixed(-10.0),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let seed = (KindInfo::ALL[OpType::BoringTurning as usize].seed)(&c);
    let OpParams::BoringTurning { tolerance, feed, plunge_feed, .. } = &seed else {
        panic!("the seed must be bore params");
    };
    assert_eq!((*tolerance, *feed, *plunge_feed), (0.05, 120.0, 40.0), "the seed carries the setup's numbers");
    create_on(&world, &mut history, "ST", "BR-1000", "bar", seed);
    let c = cache(&world, "ST").expect("cached");
    assert_eq!(status(&world, "ST"), OpStatus::Ok, "the seed's numbers stand");
    assert!(c.warnings.is_empty(), "the seeded ride reported: {:?}", c.warnings);
}

#[test]
fn a_feed_edit_rides_the_history_both_ways() {
    // Everything-is-a-command: the parameter pane's edit is
    // EditOperationParams, the cache marks itself Stale against
    // the new number, and undo lands the old feed exactly.
    let (world, mut history) = bore_world();
    create(&world, &mut history, "BR1", bore_params());
    let mut p = bore_params();
    if let OpParams::BoringTurning { feed, .. } = &mut p {
        *feed = 60.0;
    }
    history.execute(&world, EditOperationParams::new(&world, "BR1", p).expect("a legal feed"));
    assert_eq!(status(&world, "BR1"), OpStatus::Stale, "the cached ride was the old feed's");
    assert!(history.undo(&world), "undo the edit");
    let e = model::find_by_name(&world, "BR1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::BoringTurning { feed, .. } = &o.params else { unreachable!() };
    assert_eq!(*feed, 120.0, "the bar's number is back");
    assert_eq!(status(&world, "BR1"), OpStatus::Ok, "and the cached path matches it again");
}

#[test]
fn an_upward_window_refuses_at_the_create_door() {
    // The trio's ordering law restated for the bore: top below
    // bottom is no window, and the door says it before any
    // geometry runs (AGENTS.md: validate early).
    let (world, _history) = bore_world();
    let params = OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(60.0),
        top_z: Height::fixed(-10.0),
        bottom_z: Height::fixed(40.0),
        feed: 120.0,
        plunge_feed: 40.0,
    };
    let Err(err) =
        CreateOperation::new(&world, "BB1", "BR-1000", SrcRef::Entity("bar".into()), params)
    else {
        panic!("a window that cuts upward is no window");
    };
    let msg = err.to_string();
    assert!(msg.contains("must cut downward"), "{msg}");
}
