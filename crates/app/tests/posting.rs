//! The in-app post (M1.7b): a live world, not a golden stream. The
//! laws here are the ones the file-level goldens cannot see - the
//! validation gate refusing (never writing, never silently posting a
//! stale or warned program), and the merged stream keeping every
//! operation's cycles inside its own section (the AT anchors are
//! stream-global, the dump builds them per-operation).
//!
//! Fixtures reuse the proven operation-level ones: the contour test's
//! model-box (operations.rs) and the drill test's matched-hole peck
//! (M1.6), so any warning here is a posting bug, not a generator one.

use flecs_ecs::prelude::World;

use hsmworks_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
    SetWcsOrigin,
};
use hsmworks_app::drilling::CycleChoice;
use hsmworks_app::geo2d::Point2;
use hsmworks_app::geometry::{cube, IDENTITY_Q, Point3};
use hsmworks_app::machining2d::{Lead, Side};
use hsmworks_app::model;
use hsmworks_app::schema::Location;
use hsmworks_app::operation::{Height, HeightReference, OpParams, SrcRef};
use hsmworks_app::posting;
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::catalog;
use hsmworks_app::world::new_world;

fn catalog_tool(name: &str) -> hsmworks_app::tools::Tool {
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

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Default::default());
    history.execute(world, cmd.expect("box stock"));
}

fn contour_params() -> OpParams {
    OpParams::Contour {
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
    }
}

fn drill_params(hole: hsmworks_app::drilling::Hole, top_z: f64, clearance_z: f64) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(clearance_z),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![hole],
    }
}

fn hole(x: f64, y: f64, radius: f64, entry_z: f64, floor_z: f64, through: bool) -> hsmworks_app::drilling::Hole {
    hsmworks_app::drilling::Hole {
        xy: Point2::new(x, y),
        radius,
        entry_z,
        floor_z,
        through,
    }
}

/// Contour1 (a model box, many moves, no cycles) + Drill1 (one matched
/// through hole, one peck cycle) - the two-op world the anchor law
/// needs. Both generate silent: any stream warning here is the post's.
fn contour_and_drill() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 100.0, 100.0, 10.0);
    add_tool(&world, &mut history, "EM-0600-4FL");
    add_tool(&world, &mut history, "DR-0400");
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour_params());
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate contour"));
    let cmd = CreateOperation::new(
        &world,
        "Drill1",
        "DR-0400",
        SrcRef::Stock,
        drill_params(hole(20.0, 20.0, 2.0, 10.0, 0.0, true), 10.0, 14.0),
    );
    history.execute(&world, cmd.expect("create drill"));
    history.execute(&world, GenerateOperation::new(&world, "Drill1").expect("generate drill"));
    for op in ["Contour1", "Drill1"] {
        let e = model::find_by_name(&world, op).expect("op");
        let cache = model::toolpath_of(&world, &e).expect("cache");
        assert!(cache.warnings.is_empty(), "{op} fixture must generate silent: {:?}", cache.warnings);
    }
    (world, history)
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("hsmw-posting-{}-{}.nc", tag, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn two_ops_post_to_one_program_cycles_stay_in_their_section() {
    let (world, _h) = contour_and_drill();
    let path = tmp_nc("two");
    let posted = posting::post_setup(&world, &path).expect("a fresh, silent world posts");
    assert_eq!(posted.ops, 2, "both sections are in the program");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);

    // The two section preambles split the program; the cycle block
    // must live in the drill's half. Without the merge-time anchor
    // offset, normalize would re-seat the cycle before the stream's
    // move #0 - inside the contour - and its inCycle flag would
    // swallow the contour's chain: the silent-wrong-answer this
    // pairing exists to catch.
    let parts: Vec<&str> = text.split("G90 G54 G40 G49 G80").collect();
    assert_eq!(parts.len(), 3, "one preamble per section: {text}");
    assert!(parts[1].contains("T02 M6"), "EM-0600-4FL is library rank 2");
    assert!(parts[1].contains("G01") || parts[1].contains("G02"), "the contour chain posted");
    assert!(
        !(parts[1].contains("G73") || parts[1].contains("G81")),
        "no cycle belongs in the contour's section: {}",
        parts[1]
    );
    assert!(parts[2].contains("T01 M6"), "DR-0400 is library rank 1");
    assert!(parts[2].contains("G98"), "the cycle retract mode posted once");
    assert!(parts[2].contains("G73 X20 Y20"), "the peck block, at its hole: {}", parts[2]);
    assert!(text.starts_with('%') && text.trim_end().ends_with('%'), "one % wrapped program");
    assert_eq!(text.matches("M30").count(), 1, "one footer");
}

#[test]
fn an_unborn_toolpath_refuses_to_post() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    // Never generated: the gate refuses by name, and writes nothing.
    let path = tmp_nc("never");
    let err = posting::post_setup(&world, &path).expect_err("an empty path must not post");
    assert!(err.contains("Face1") && err.contains("generate it first"), "{err}");
    assert!(!path.exists(), "a refusal leaves no file");
}

fn face_params() -> OpParams {
    OpParams::Face {
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

#[test]
fn a_stale_operation_blocks_the_post_and_leaves_the_file_untouched() {
    let (world, mut history) = contour_and_drill();
    let path = tmp_nc("stale");
    posting::post_setup(&world, &path).expect("fresh posts");
    let before = std::fs::read_to_string(&path).expect("written");

    // The drill op reads the stock, so moving the part goes stale
    // through the world-Aabb digest term.
    let e = model::find_by_name(&world, "part").expect("part");
    model::set_transform(&world, &e, Location { pos: Point3::new(3.0, 0.0, 0.0), rot: IDENTITY_Q });
    let err = posting::post_setup(&world, &path).expect_err("stale blocks");
    assert!(err.contains("Contour1") && err.contains("regenerate"), "{err}");
    assert_eq!(std::fs::read_to_string(&path).expect("still there"), before, "a refusal rewrites nothing");

    // Regenerating through the command clears it and the post runs again.
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("regenerate"));
    posting::post_setup(&world, &path).expect("fresh again posts");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_warned_path_blocks_even_though_it_generated() {
    // The M1.6 deep-hole fixture: generation succeeds WITH a ShankHit
    // warning (the user must be able to read the numbers). The post
    // gate is where that warning becomes a refusal - the path exists,
    // is fresh, and is still not safe to machine.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 100.0, 100.0, 130.0);
    add_tool(&world, &mut history, "DR-0400");
    let cmd = CreateOperation::new(
        &world,
        "Deep1",
        "DR-0400",
        SrcRef::Stock,
        drill_params(hole(0.0, 0.0, 2.0, 130.0, 0.0, true), 130.0, 140.0),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Deep1").expect("generate"));
    let e = model::find_by_name(&world, "Deep1").expect("op");
    assert!(
        !model::toolpath_of(&world, &e).expect("cache").warnings.is_empty(),
        "the fixture must carry its warning"
    );

    let path = tmp_nc("warned");
    let err = posting::post_setup(&world, &path).expect_err("a warning blocks the gate");
    assert!(err.contains("post blocked") && err.contains("shank"), "{err}");
    assert!(!path.exists(), "blocked output is never written");
}

#[test]
fn a_gouging_program_is_refused_by_the_verification_gate() {
    // Stage D's own law: the program-level sim is no longer just an
    // informed verdict - it is the gate. The fixture is Deviate's
    // stage-B sentence made into a world: a face 0.5 mm below the top
    // of a model that fills the stock. Generation is SILENT (the face
    // verifies against the stock, not the model - that net is this
    // gate's job), so the refusal below can only come from the sim.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 20.0)),
        ),
    );
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    assert!(
        model::toolpath_of(&world, &e).expect("cache").warnings.is_empty(),
        "the fixture must generate silent - the sim is the only net here"
    );

    let path = tmp_nc("gouge");
    let err = posting::post_setup(&world, &path).expect_err("a gouged program must not post");
    assert!(
        err.contains("post blocked") && err.contains("gouge:") && err.contains("below the model"),
        "{err}"
    );
    assert!(!path.exists(), "the gate writes nothing on refusal");

    // The gate's evidence is the Deviate cell's evidence: the same
    // walk, so the count in the sentence is the sim's own.
    let sim = hsmworks_app::stock_sim::simulate_document(&world).expect("sim runs");
    let lines = posting::verification_gate(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains(&format!("{} cell(s)", sim.dev.gouge_cells)),
        "{}",
        lines[0]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn machine_limits_block_the_post_and_within_limits_does_not() {
    // The mounted machine joins the gate with the same walk stage C
    // informs with. No model in this world: the sim judges nothing, so
    // any refusal here can only be the machine's sentence.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));

    let tight = hsmworks_app::machine::Machine {
        model: "TestMill 1".into(),
        vendor: "Hsmworks Testbench".into(),
        spindle_min: Some(50.0),
        spindle_max: Some(10000.0), // EM-0400-2FL runs 12000
        ..Default::default()
    };
    history
        .execute(&world, LoadMachine::new(&world, "tight.mch".into(), Some(tight.clone())).expect("sane"));
    let path = tmp_nc("limits");
    let err = posting::post_setup(&world, &path).expect_err("the ceiling blocks");
    assert!(
        err.contains("machine TestMill 1: 1 limit problem(s)") && err.contains("above the machine's 10000"),
        "{err}"
    );
    assert!(!path.exists(), "a limit refusal writes nothing");

    // The same program under a machine that can give it: the gate
    // opens without touching the program - it never re-plans.
    let roomy = hsmworks_app::machine::Machine { spindle_max: Some(12000.0), ..tight };
    history
        .execute(&world, LoadMachine::new(&world, "roomy.mch".into(), Some(roomy)).expect("sane"));
    assert!(posting::verification_gate(&world).is_empty());
    posting::post_setup(&world, &path).expect("within limits posts");
    assert!(path.exists(), "and writes");
    let _ = std::fs::remove_file(&path);

    // No machine mounted, nothing claimed (the unstated-limit law's
    // gate half): clearing the machine cannot make this program illegal.
    history.execute(&world, LoadMachine::new(&world, String::new(), None).expect("clear"));
    posting::post_setup(&world, &path).expect("an unmounted checker blocks nothing");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_program_the_sim_cannot_judge_still_posts() {
    // The refusal-to-judge is not a judgment: a model-sourced contour
    // with fixed heights and NO stock gives the program sim nothing to
    // chew, so the gate holds no verdict and must not borrow one. The
    // per-op WARN gate is the net under this world, and it is silent.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    let e = model::find_by_name(&world, "Contour1").expect("op");
    assert!(
        model::toolpath_of(&world, &e).expect("cache").warnings.is_empty(),
        "generation must be silent - the sim's silence here is the tested thing"
    );
    assert!(hsmworks_app::stock_sim::simulate_document(&world).is_err(), "fixture: no stock, no judge");
    assert!(posting::verification_gate(&world).is_empty(), "no verdict, no veto");
    posting::post_setup(&world, &tmp_nc("nostock")).expect("unjudged is not unsafe");
}

#[test]
fn an_empty_document_refuses_before_anything_else() {
    let world = new_world();
    let err = posting::post_setup(&world, &tmp_nc("empty")).expect_err("nothing to post");
    assert_eq!(err, "no operations to post");
}

#[test]
fn a_moved_wcs_origin_moves_the_posted_numbers() {
    // The context law: the posted program measures from the slot's
    // origin as the machine would - the OriginX/Y/Z PARAMs must carry
    // it, so a relocated WCS shows up as geometry that moved in XY,
    // not as a silent G55 vs G54 mismatch.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));

    // Both the default (no slot placed) and the shifted program.
    let path = tmp_nc("wcs");
    posting::post_setup(&world, &path).expect("faces post");
    let plain = std::fs::read_to_string(&path).expect("written");

    history.execute(
        &world,
        SetWcsOrigin::new(&world, "G54", Location { pos: Point3::new(10.0, 5.0, 1.0), rot: IDENTITY_Q })
            .expect("set G54"),
    );
    // The face's heights are Fixed: the origin is pure post context
    // (the OriginX/Y/Z PARAMs), not a digest input - the toolpath is
    // right where it was, the machine underneath it moved. The posted
    // numbers must follow the origin, the stamp must not.
    posting::post_setup(&world, &path).expect("still fresh: WCS moved the machine, not the inputs");
    let moved = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(moved.contains("G90 G54"), "still the G54 word");
    assert_ne!(moved, plain, "the origin rode the PARAMs into the program");
}

#[test]
fn a_non_g54_slot_is_what_the_script_writes() {
    // The WorkOffset PARAM is grammar-driven: an op whose heights
    // reference slot G55 posts its motions under G55.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    // A heights-referenced slot must be placed before it can be measured
    // from (the same law the panels enforce by refusing to resolve).
    history.execute(
        &world,
        SetWcsOrigin::new(&world, "G55", Location::default()).expect("place G55"),
    );
    let params = OpParams::Face {
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::WorkOffset(1), 25.0),
        cut_z: Height::of(HeightReference::WorkOffset(1), 19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let path = tmp_nc("g55");
    posting::post_setup(&world, &path).expect("posts");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("G90 G55"), "the script took the slot from the PARAM: {text}");
}

#[test]
fn posted_carries_line_positions_matching_the_move_chain() {
    // M1.7d viewer contract, the app half: every position a program
    // line resolves to is the endpoint of some real move (the map
    // invents nothing), at least every cycle-free contour move gets a
    // positioned line (coverage), and the first positioned line is the
    // stream's first move (order). Floats compare on the lattice law's
    // tolerance, not bitwise: the round trip is mm -> um -> mm.
    let (world, _h) = contour_and_drill();
    let path = tmp_nc("linepos");
    let posted = posting::post_setup(&world, &path).expect("a fresh, silent world posts");
    let _ = std::fs::remove_file(&path);

    assert_eq!(posted.line_pos.len(), posted.lines, "one slot per program line");
    assert_eq!(posted.lines, posted.program.lines().count(), "the retained text is the program");

    let mut endpoints: Vec<[f64; 3]> = Vec::new();
    let mut contour_moves = 0usize;
    for op in ["Contour1", "Drill1"] {
        let e = model::find_by_name(&world, op).expect("op");
        let tp = model::toolpath_of(&world, &e).expect("cached path");
        if op == "Contour1" {
            contour_moves = tp.path.moves.len();
        }
        endpoints.extend(tp.path.moves.iter().map(|m| [m.to.x, m.to.y, m.to.z]));
    }
    let same = |a: &[f64; 3], b: &[f64; 3]| {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    };
    let resolved: Vec<[f64; 3]> = posted.line_pos.iter().copied().flatten().collect();
    assert!(
        resolved.len() >= contour_moves,
        "the contour's {} moves must each reach the viewer, got {} positioned lines",
        contour_moves,
        resolved.len()
    );
    assert!(
        same(&resolved[0], &endpoints[0]),
        "first positioned line {:?} is not the first move {:?}",
        resolved[0],
        endpoints[0]
    );
    for p in &resolved {
        assert!(
            endpoints.iter().any(|e| same(e, p)),
            "position {p:?} is posted by no move - the viewer would highlight a lie"
        );
    }
}
