//! M10.1 D (the command half): the tool law's gates at command
//! construction. A kind with no fitting tool in the active setup's
//! tools is refused by kind and setup name before any entity exists;
//! a named tool from the wrong family is refused by name; the mirror
//! edge (a groove blade into a milling op) holds; a tool retarget, an
//! op retype and a tool retype meet the same wall; kind-keeping tool
//! edits and source-only link edits never do.
//!
//! Provenance: machine-tool handbook tooling practice read against our
//! own command model (validate early, AGENTS.md); no vendor material
//! consulted.
//!
//! Gate map (M4.4): covers app::operation::tool_gate, app::commands
//! (CreateOperation, EditOperationLinks, ChangeOperationKind, ChangeTool).
//! Lane shape (M4.4): all tests here run in milliseconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeOperationKind, ChangeTool, CreateOperation, CreateSetup, EditOperationLinks,
    History, LoadMachine, SetStock,
};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{tool_gate, Height, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool, ToolKind};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

/// Box stock on the document's machine (`None` = the unstated mill),
/// with the named catalog tools on the rack.
fn world_with(machine: Option<&str>, tools: &[&str]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 };
    history.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    if let Some(word) = machine {
        let m = Machine {
            model: format!("TestBench {word}"),
            kind: Some(word.to_string()),
            ..Default::default()
        };
        let cmd = LoadMachine::new(&world, format!("{word}.mch"), Some(m));
        history.execute(&world, cmd.expect("machine"));
    }
    for t in tools {
        history.execute(&world, AddTool::new(&world, t, catalog_tool(t)).expect("tool"));
    }
    (world, history)
}

fn face() -> OpParams {
    OpParams::Face {
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn groove() -> OpParams {
    OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        feed: 150.0,
        plunge_feed: 60.0,
    }
}

fn refusal(r: Result<CreateOperation, camcraft_app::commands::CommandError>) -> String {
    match r {
        Ok(_) => panic!("the tool law must refuse this creation"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn an_end_mill_cannot_create_a_groove() {
    // The hand-test case: a lathe whose only tool is an end mill.
    let (world, _h) = world_with(Some("lathe"), &["EM-0400-2FL"]);
    let msg = refusal(CreateOperation::new(&world, "G1", "EM-0400-2FL", SrcRef::Stock, groove()));
    assert!(msg.contains("No grooving blade in the setup's tools"), "{msg}");
    assert!(model::find_by_name(&world, "G1").is_none(), "refused before any entity exists");
}

#[test]
fn an_end_mill_beside_a_blade_is_refused_by_name() {
    let (world, mut h) = world_with(Some("lathe"), &["EM-0400-2FL", "GT-0200"]);
    let msg = refusal(CreateOperation::new(&world, "G1", "EM-0400-2FL", SrcRef::Stock, groove()));
    assert!(msg.contains("'EM-0400-2FL' is a flat end mill"), "{msg}");
    assert!(msg.contains("grooving blade"), "{msg}");
    let ok = CreateOperation::new(&world, "G1", "GT-0200", SrcRef::Stock, groove());
    h.execute(&world, ok.expect("the blade fits the groove"));
}

#[test]
fn a_groove_blade_cannot_create_a_milling_op() {
    // The mirror edge, and the setup's own name once one is declared.
    let (world, mut h) = world_with(Some("mill-turn"), &["GT-0200"]);
    let msg = refusal(CreateOperation::new(&world, "F1", "GT-0200", SrcRef::Stock, face()));
    assert!(msg.contains("No flat end mill or bull-nose in the setup's tools"), "{msg}");
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let msg = refusal(CreateOperation::new(&world, "F1", "GT-0200", SrcRef::Stock, face()));
    assert!(msg.contains("No flat end mill or bull-nose in Setup1's tools"), "{msg}");
    assert!(model::find_by_name(&world, "F1").is_none());
}

#[test]
fn the_query_agrees_with_the_door() {
    let (world, _h) = world_with(None, &["DR-0400"]);
    let pocket = OpType::Pocket.info();
    assert!(tool_gate::fitting_tools(&world, pocket).is_empty());
    let why = tool_gate::create_refusal(&world, pocket).expect("no mill on the rack");
    assert!(why.starts_with("No flat end mill or bull-nose in the setup's tools"), "{why}");
    let drill = OpType::Drill.info();
    assert_eq!(tool_gate::fitting_tools(&world, drill), vec![String::from("DR-0400")]);
    assert!(tool_gate::create_refusal(&world, drill).is_none());
    assert_eq!(
        tool_gate::kinds_phrase(drill.tool_kinds),
        "drill, tap or boring bar"
    );
}

#[test]
fn a_tool_retarget_meets_the_same_wall() {
    let (world, mut h) = world_with(None, &["EM-0400-2FL", "DR-0400"]);
    let ok = CreateOperation::new(&world, "F1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, ok.expect("end mill faces"));
    let Err(err) = EditOperationLinks::new(&world, "F1", Some("DR-0400"), None) else {
        panic!("a drill cannot face");
    };
    let msg = err.to_string();
    assert!(msg.contains("'DR-0400' is a drill") && msg.contains("Face"), "{msg}");
    // Naming the tool the op already holds is no reassignment.
    EditOperationLinks::new(&world, "F1", Some("EM-0400-2FL"), Some(SrcRef::Stock))
        .expect("a same-tool link edit is not gated");
}

#[test]
fn an_op_retype_meets_the_same_wall() {
    let (world, mut h) = world_with(None, &["EM-0400-2FL"]);
    let ok = CreateOperation::new(&world, "F1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, ok.expect("end mill faces"));
    let Err(err) = ChangeOperationKind::new(&world, "F1", OpType::Finish3D as usize) else {
        panic!("a flat end mill cannot ride a ball's tangent locus");
    };
    assert!(err.to_string().contains("ball-nose"), "{err}");
    ChangeOperationKind::new(&world, "F1", OpType::Pocket as usize)
        .expect("a pocket takes the same end mill");
}

#[test]
fn change_tool_refuses_a_retype_that_strands_an_op() {
    let (world, mut h) = world_with(None, &["EM-0400-2FL"]);
    let ok = CreateOperation::new(&world, "F1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, ok.expect("end mill faces"));
    let blade = catalog_tool("GT-0200");
    assert_eq!(blade.kind, ToolKind::Groove);
    let Err(err) = ChangeTool::new(&world, "EM-0400-2FL", blade.clone()) else {
        panic!("a retype that strands F1 must be refused");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("cannot become a grooving blade") && msg.contains("F1"),
        "{msg}"
    );
    // A kind-keeping edit (a re-ground diameter) is never gated.
    let mut wider = catalog_tool("EM-0400-2FL");
    wider.diameter += 1.0;
    h.execute(&world, ChangeTool::new(&world, "EM-0400-2FL", wider).expect("same kind"));
    // An unreferenced tool may become anything.
    h.execute(&world, AddTool::new(&world, "Spare", catalog_tool("EM-0600-4FL")).expect("add"));
    ChangeTool::new(&world, "Spare", blade).expect("nothing rides Spare");
}
