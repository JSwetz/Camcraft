//! M1.3 acceptance: tools and operations are entities like everything
//! else - commands own their fields, undo/redo is exact, generation runs
//! at command construction (validate early), and staleness is derived,
//! never stored.

use flecs_ecs::prelude::World;

use hsmworks_app::commands::{
    AddTool, ChangeTool, CommandError, CreateOperation, DeleteEntity, DeleteOperation,
    EditOperationLinks, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
    SetWcsOrigin,
};
use hsmworks_app::geometry::{cube, IDENTITY_Q, Point3};
use hsmworks_app::machining2d::Lead;
use hsmworks_app::model;
use hsmworks_app::operation::{self, OpParams, OpStatus, SrcRef};
use hsmworks_app::schema::{Location, Operation, Toolpath, STOCK_NAME};
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::{catalog, Tool};
use hsmworks_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn at(x: f64, y: f64, z: f64) -> Location {
    Location {
        pos: Point3::new(x, y, z),
        rot: IDENTITY_Q,
    }
}

/// Face parameters for the 60x60x20 box stock with a 4 mm cutter.
fn face_params() -> OpParams {
    OpParams::Face {
        stepover: 3.0,
        clearance_z: 25.0,
        cut_z: 19.5,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn contour_params() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: 8.0,
        top_z: 5.0,
        bottom_z: 0.0,
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
    }
}

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Location::default());
    history.execute(world, cmd.expect("box stock"));
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

/// Stock + a 4 mm end mill + a generated face operation: the fixture most
/// tests start from.
fn face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create face op"));
    let cmd = GenerateOperation::new(&world, "Face1").expect("generate face op");
    history.execute(&world, cmd);
    (world, history)
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

#[test]
fn tools_are_entities_and_undoable() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0800-4FL");

    let e = model::find_by_name(&world, "EM-0800-4FL").expect("tool entity");
    assert!(model::is_tool(&world, &e));
    assert_eq!(model::tool_of(&world, &e).map(|t| t.diameter), Some(8.0));
    assert_eq!(model::facets_of(&world, &e), None, "tools have no geometry");

    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, "EM-0800-4FL").is_none());
    assert!(history.redo(&world));
    let e = model::find_by_name(&world, "EM-0800-4FL").expect("restored");
    assert_eq!(model::tool_of(&world, &e), Some(catalog_tool("EM-0800-4FL")));
}

#[test]
fn add_tool_refusals_at_construction() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 10.0, 10.0, 10.0);
    let cmd = AddTool::new(&world, "T1", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("named tool"));

    // reserved setup names and the empty name
    assert!(matches!(
        AddTool::new(&world, STOCK_NAME, catalog_tool("EM-0400-2FL")),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        AddTool::new(&world, "G54", catalog_tool("EM-0400-2FL")),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        AddTool::new(&world, "", catalog_tool("EM-0400-2FL")),
        Err(CommandError::Invalid(_))
    ));
    // duplicate name
    assert!(matches!(
        AddTool::new(&world, "T1", catalog_tool("EM-0400-2FL")),
        Err(CommandError::NameConflict(_))
    ));
    // insane geometry
    assert!(matches!(
        AddTool::new(&world, "T2", Tool { diameter: 0.0, ..catalog_tool("EM-0400-2FL") }),
        Err(CommandError::Invalid(_))
    ));
    assert!(model::find_by_name(&world, "T2").is_none(), "refused names never spawn");
}

#[test]
fn change_tool_is_exact_and_marks_dependents_stale() {
    let (world, mut history) = face_world();
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    let mut fatter = catalog_tool("EM-0400-2FL");
    fatter.diameter = 6.0;
    history.execute(&world, ChangeTool::new(&world, "EM-0400-2FL", fatter.clone()).expect("edit"));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale, "the cache predates the edit");
    assert!(cache(&world, "Face1").is_some(), "stale keeps the old path on screen");

    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);

    // re-generating with the fatter tool clears the staleness
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    let e = model::find_by_name(&world, "EM-0400-2FL").expect("tool");
    assert_eq!(model::tool_of(&world, &e).map(|t| t.diameter), Some(6.0));
}

#[test]
fn create_operation_validates_every_reference() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 20.0, 20.0, 10.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))));
    history.execute(&world, SetWcsOrigin::new(&world, "G54", at(0.0, 0.0, 0.0)).expect("wcs"));

    // unknown tool, and a name that is a model rather than a tool
    assert!(matches!(
        CreateOperation::new(&world, "O", "no-such", SrcRef::Stock, face_params()),
        Err(CommandError::NotFound(_))
    ));
    assert!(matches!(
        CreateOperation::new(&world, "O", "part", SrcRef::Stock, face_params()),
        Err(CommandError::Invalid(_))
    ));
    // sources must exist and not be offsets
    assert!(matches!(
        CreateOperation::new(&world, "O", "EM-0400-2FL", SrcRef::Entity("gone".into()), face_params()),
        Err(CommandError::NotFound(_))
    ));
    assert!(matches!(
        CreateOperation::new(&world, "O", "EM-0400-2FL", SrcRef::Entity("G54".into()), face_params()),
        Err(CommandError::Invalid(_))
    ));
    // parameters gate at construction (no struct-update on enum variants:
    // write the whole variant)
    assert!(matches!(
        CreateOperation::new(
            &world,
            "O",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face {
                stepover: f64::NAN,
                clearance_z: 25.0,
                cut_z: 19.5,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        ),
        Err(CommandError::Invalid(_))
    ));
    // the positive case: a model source with sane params is accepted
    assert!(CreateOperation::new(
        &world,
        "O",
        "EM-0400-2FL",
        SrcRef::Entity("part".into()),
        contour_params()
    )
    .is_ok());
}

#[test]
fn no_stock_refused_for_stock_source() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0400-2FL");
    assert!(matches!(
        CreateOperation::new(&world, "O", "EM-0400-2FL", SrcRef::Stock, face_params()),
        Err(CommandError::Invalid(_))
    ));
}

#[test]
fn generate_undo_redo_replays_the_identical_path() {
    let (world, mut history) = face_world();
    let first = cache(&world, "Face1").expect("generated");
    assert!(!first.path.is_empty(), "a 60x60 face pass is a real path");
    assert!(first.path.is_chained() && first.path.all_finite());
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Never, "undo restores the pre-generate state");
    assert!(cache(&world, "Face1").is_none(), "no component, not an empty one");

    assert!(history.redo(&world));
    assert_eq!(cache(&world, "Face1").map(|c| c.path), Some(first.path.clone()), "redo replays bytes, never re-runs");
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
}

#[test]
fn edit_params_clears_the_stamp_not_the_cache() {
    let (world, mut history) = face_world();
    let first = cache(&world, "Face1").expect("generated").path;

    let lower = OpParams::Face {
        stepover: 3.0,
        clearance_z: 25.0,
        cut_z: 19.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, EditOperationParams::new(&world, "Face1", lower).expect("edit"));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(cache(&world, "Face1").map(|c| c.path), Some(first.clone()), "cache survives the edit");

    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "the stamp comes back with the params");
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
}

#[test]
fn stock_resize_makes_generated_ops_stale() {
    // The law SetStock's facet count cannot enforce (12 tris either way),
    // the stamp can: stock params are in the digest.
    let (world, mut history) = face_world();
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);

    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    // undoing the regenerate returns the stock-resize staleness, cache and all
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert!(cache(&world, "Face1").is_some());
}

#[test]
fn stock_move_makes_generated_ops_stale() {
    let (world, mut history) = face_world();
    let e = model::find_by_name(&world, STOCK_NAME).expect("stock");
    // A command-less move is the model case; use SetStock to relocate
    // exactly (same parameters, new placement).
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at(5.0, 0.0, 0.0));
    history.execute(&world, cmd.expect("relocate"));
    assert_eq!(model::transform_of(&world, &e).map(|l| l.pos.x), Some(5.0));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale, "the stamp reads stock Location");
}

#[test]
fn retarget_links_clear_the_stamp() {
    let (world, mut history) = face_world();
    add_tool(&world, &mut history, "EM-0600-4FL");

    history.execute(
        &world,
        EditOperationLinks::new(&world, "Face1", Some("EM-0600-4FL"), None).expect("retarget"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(
        model::operation_of(&world, &model::find_by_name(&world, "Face1").expect("op")).map(|o| o.tool),
        Some("EM-0600-4FL".to_string())
    );
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert_eq!(
        model::operation_of(&world, &model::find_by_name(&world, "Face1").expect("op")).map(|o| o.tool),
        Some("EM-0400-2FL".to_string())
    );
}

#[test]
fn generation_failure_refuses_the_command_and_writes_nothing() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    let first_path = cache(&world, "Contour1").expect("cache").path;
    let stored_stamp = model::operation_of(
        &world,
        &model::find_by_name(&world, "Contour1").expect("op"),
    )
    .expect("op")
    .stamp;

    // Move the source out of the section plane (setup poke, not a command).
    let e = model::find_by_name(&world, "part").expect("part");
    model::set_location(&world, &e, Point3::new(0.0, 0.0, 200.0));

    let err = GenerateOperation::new(&world, "Contour1").err().expect("must fail");
    assert!(matches!(err, CommandError::Machining(_)), "{err:?}");
    // Nothing half-wrote: the old cache and its stamp stand.
    assert_eq!(cache(&world, "Contour1").map(|c| c.path), Some(first_path));
    let op = model::operation_of(&world, &model::find_by_name(&world, "Contour1").expect("op")).expect("op");
    assert_eq!(op.stamp, stored_stamp, "the stored stamp is untouched");
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale, "the moved source reads stale");
}

#[test]
fn delete_operation_round_trips_with_its_cache() {
    let (world, mut history) = face_world();
    let op = model::operation_of(&world, &model::find_by_name(&world, "Face1").expect("op")).expect("definition");
    let first = cache(&world, "Face1").expect("cache").path;

    history.execute(&world, DeleteOperation::new(&world, "Face1").expect("delete"));
    assert!(model::find_by_name(&world, "Face1").is_none(), "entity and cache are gone together");

    assert!(history.undo(&world));
    let e = model::find_by_name(&world, "Face1").expect("restored");
    assert!(model::is_operation(&world, &e));
    assert_eq!(model::operation_of(&world, &e), Some(op), "definition restored exactly");
    assert_eq!(cache(&world, "Face1").map(|c| c.path), Some(first.clone()), "cache restored too - not a hollow entity");

    // regenerating now replays the identical path (deterministic generators)
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(cache(&world, "Face1").map(|c| c.path), Some(first));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
}

#[test]
fn delete_entity_restores_camparts() {
    // The generic delete must carry ToolDef/Operation/Toolpath through its
    // snapshot, or undo recreates a hollow entity and re-adding conflicts.
    let (world, mut history) = face_world();
    history.execute(&world, DeleteEntity::new(&world, "Face1").expect("generic delete"));
    assert!(model::find_by_name(&world, "Face1").is_none());
    assert!(history.undo(&world));
    let e = model::find_by_name(&world, "Face1").expect("restored");
    assert!(model::is_operation(&world, &e), "kind survives");
    assert!(model::toolpath_of(&world, &e).is_some(), "cache survives");
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    history.execute(&world, DeleteEntity::new(&world, "EM-0400-2FL").expect("delete tool"));
    assert!(history.undo(&world));
    let t = model::find_by_name(&world, "EM-0400-2FL").expect("tool restored");
    assert!(model::is_tool(&world, &t));
    assert_eq!(model::tool_of(&world, &t), Some(catalog_tool("EM-0400-2FL")));
}

#[test]
fn tools_and_operations_stay_out_of_the_models_list() {
    let (world, _history) = face_world();
    let names: Vec<String> = model::model_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(names.is_empty(), "setup furniture and CAM objects are not models: {names:?}");
    assert_eq!(model::named_entities(&world).len(), 3, "Stock, tool, operation");
}

#[test]
fn contour_on_a_model_box_generates() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    let c = cache(&world, "Contour1").expect("cache");
    assert!(!c.path.is_empty());
    assert!(c.path.is_chained());
    // world-space Aabb is in the digest: moving the model goes stale
    let e = model::find_by_name(&world, "part").expect("part");
    model::set_transform(&world, &e, at(3.0, 0.0, 0.0));
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);
}

#[test]
fn region_of_is_the_preview_of_what_generate_would_cut() {
    // The viewport's region outline: the generators' own region
    // construction, minus tool and path - so it answers "what gets cut"
    // for operations that have never been generated.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);

    let trace = Operation {
        tool: String::new(),
        src: SrcRef::Stock,
        params: OpParams::Trace {
            clearance_z: 25.0,
            top_z: 10.0,
            bottom_z: 5.0,
            stepdown: 5.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        },
        stamp: None,
    };
    assert_eq!(trace.params.region_z(), 10.0);
    let region = operation::region_of(&world, &trace).expect("a box sections cleanly at 10 mm");
    assert_eq!(region.loops().len(), 1);
    // The section keeps the mesh's per-face triangle splits, so the ring
    // is 8 collinear points tracing the same 60x60 rectangle.
    let pts = region.loops()[0].points();
    assert!(pts.len() >= 4);
    let extent = |get: fn(&hsmworks_app::geo2d::Point2) -> f64| -> f64 {
        let lo = pts.iter().map(get).fold(f64::MAX, f64::min);
        let hi = pts.iter().map(get).fold(f64::MIN, f64::max);
        hi - lo
    };
    assert!((extent(|p| p.x) - 60.0).abs() < 1e-9, "60 mm across X");
    assert!((extent(|p| p.y) - 60.0).abs() < 1e-9, "60 mm across Y");
    assert!(pts.iter().all(|p| p.x.abs() <= 30.0 + 1e-9 && p.y.abs() <= 30.0 + 1e-9));

    // Face: the silhouette footprint of the same box (no tool consulted).
    let face = Operation {
        tool: String::new(),
        src: SrcRef::Stock,
        params: OpParams::Face {
            stepover: 3.0,
            clearance_z: 25.0,
            cut_z: 19.5,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        },
        stamp: None,
    };
    assert_eq!(face.params.region_z(), 19.5);
    assert!(!operation::region_of(&world, &face).expect("footprint").is_empty());

    // A vanished source previews NOTHING, not a stale outline.
    let ghost = Operation {
        tool: String::new(),
        src: SrcRef::Entity("gone".into()),
        params: trace.params.clone(),
        stamp: None,
    };
    assert_eq!(operation::region_of(&world, &ghost), None);
}
