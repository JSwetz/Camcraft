//! M1.1 acceptance: save/load round-trips are byte-identical; a wrong
//! version or corrupt file is a clean error.

use std::path::PathBuf;

use hsmworks_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWcsOrigin,
};
use hsmworks_app::geometry::feature;
use hsmworks_app::machining2d::{Lead, PocketStrategy, Side};
use hsmworks_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use hsmworks_app::schema::Location;
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::{Tool, catalog};
use hsmworks_app::geometry::{IDENTITY_Q, cube, cylinder, Point3};
use hsmworks_app::model;
use hsmworks_app::project::{self, ProjectError};
use hsmworks_app::world::new_world;

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hsmworks_proj_{tag}_{}.hsmproj", std::process::id()))
}

fn populated_world() -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("block_a", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("block_b", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
    );
    // give block_b an offset so the transform round-trip is observable
    if let Some(e) = model::find_by_name(&world, "block_b") {
        model::set_location(&world, &e, Point3::new(10.0, 0.5, -3.25));
    }
    world
}

#[test]
fn save_load_save_is_byte_identical() {
    let path = tmp_path("rt");
    let world = populated_world();
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");

    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "round-trip must be byte-identical");

    // loaded world has the same content
    let file = project::collect(&world2);
    assert_eq!(file.entities.len(), 2);
    let names: Vec<&str> = file.entities.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["block_a", "block_b"], "serializer order is by name");
}

#[test]
fn transforms_survive() {
    let path = tmp_path("tf");
    let world = populated_world();
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let e = model::find_by_name(&world2, "block_b").expect("loaded");
    let p = model::location_of(&world2, &e).expect("location");
    assert_eq!((p.x, p.y, p.z), (10.0, 0.5, -3.25));
    assert_eq!(model::facets_of(&world2, &e).map(|t| t.len()), Some(12));
}

#[test]
fn wrong_version_is_refused() {
    // Anything newer than this build's format stays refused; v1..v17
    // migrate (see v1_file_migrates / v2_file_migrates_to_v3 /
    // v3_file_migrates_to_v4 / v4_file_migrates_to_v5; heights.rs covers
    // the v5 numeric heights -> v6 Fixed migration,
    // side_roundtrip_v6_defaults_to_auto the v6 contour -> v7 side step,
    // units_round_trip_and_a_v7_file_means_millimeters v7 -> v8,
    // v8_file_steps_to_v9 v8 -> v9, v9_file_steps_to_v10 v9 -> v10,
    // v10_file_steps_to_v11_and_the_pocket3d_grammar_round_trips v10 ->
    // v11, v11_file_steps_to_v12 v11 -> v12, v12_file_steps_to_v13
    // v12 -> v13, v13_file_steps_to_v14 v13 -> v14,
    // v14_file_steps_to_v15_and_the_finish3d_grammar_round_trips
    // v14 -> v15, and
    // v15_file_steps_to_v16_and_the_parallel_grammar_round_trips
    // v15 -> v16, and
    // v16_file_steps_to_v17_and_the_ramp_grammar_round_trips v16 -> v17,
    // and v17_file_steps_to_v18_and_the_surface_finish_grammar_round_trips
    // v17 -> v18.
    let text = r#"{"version": 19, "entities": []}"#;
    let world = new_world();
    match project::open(&world, text) {
        Err(ProjectError::UnsupportedVersion(19)) => {}
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn v1_file_migrates_to_v2() {
    // A literal v1 project: no rotation field anywhere (v2 default + step-up).
    let text = r#"{
        "version": 1,
        "entities": [{
            "name": "legacy",
            "translation": [1.0, 2.0, 3.0],
            "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]
        }]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v1 must migrate");
    let e = model::find_by_name(&world, "legacy").expect("entity");
    let loc = model::transform_of(&world, &e).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (1.0, 2.0, 3.0));
    assert_eq!(loc.rot, IDENTITY_Q, "v1 translations mean identity");
    assert_eq!(model::facets_of(&world, &e).map(|t| t.len()), Some(1));
}

#[test]
fn rotation_survives_round_trip() {
    use hsmworks_app::geometry::quat_from_yaw;
    use std::f64::consts::FRAC_PI_2;

    let path = tmp_path("rot");
    let world = new_world();
    let e = model::spawn(&world, "tilt");
    model::set_facets(&world, &e, cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)).facets);
    model::set_transform(
        &world,
        &e,
        Location {
            pos: Point3::new(5.0, 0.0, 0.0),
            rot: quat_from_yaw(FRAC_PI_2),
        },
    );
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let e2 = model::find_by_name(&world2, "tilt").expect("loaded");
    let loc = model::transform_of(&world2, &e2).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (5.0, 0.0, 0.0));
    // Local (1,0,0) must land at world (5,1,0) after the yaw-90 rotation.
    let w = hsmworks_app::geometry::transform_point(Point3::new(1.0, 0.0, 0.0), loc.pos, loc.rot);
    assert!((w.x - 5.0).abs() < 1e-12 && (w.y - 1.0).abs() < 1e-12 && w.z.abs() < 1e-12);
}

#[test]
fn garbage_rotation_loads_as_identity() {
    // Prime Directive: a hand-corrupted quaternion is a logged fallback,
    // not a NaN geometry bomb.
    let text = r#"{"version": 2, "entities": [{"name": "g", "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 0.0]}]}"#;
    let world = new_world();
    project::open(&world, text).expect("opens");
    let e = model::find_by_name(&world, "g").expect("entity");
    assert_eq!(
        model::transform_of(&world, &e).expect("location").rot,
        IDENTITY_Q
    );
}

#[test]
fn corrupt_project_is_a_clean_error() {
    let world = new_world();
    assert!(matches!(
        project::open(&world, "this is not json"),
        Err(ProjectError::Corrupt(_))
    ));
    assert!(matches!(
        project::open(&world, r#"{"version": 1, "entities": [{"name": 5}]}"#),
        Err(ProjectError::Corrupt(_))
    ));
}

fn at(x: f64, y: f64, z: f64) -> Location {
    Location {
        pos: Point3::new(x, y, z),
        rot: IDENTITY_Q,
    }
}

#[test]
fn stock_and_wcs_round_trip() {
    let path = tmp_path("setup");
    let world = populated_world();
    let mut history = History::default();
    let boxy = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 30.0,
    };
    history.execute(&world, SetStock::new(&world, boxy.clone(), at(1.0, 2.0, -1.0)).unwrap());
    history
        .execute(&world, SetWcsOrigin::new(&world, "G54", at(12.5, -3.0, 0.5)).unwrap());
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "setup round-trip must be byte-identical");
    // Parametric stock persists parameters, never regenerated floats.
    let stock_dto = first
        .split("\"name\": \"Stock\"")
        .nth(1)
        .expect("stock entity in file");
    let stock_dto = &stock_dto[..stock_dto.find("\"name\"").unwrap_or(stock_dto.len())];
    assert!(stock_dto.contains("\"stock\""), "parameters persisted");
    assert!(!stock_dto.contains("facets"), "box stock stores no facets");

    let names: Vec<String> = project::collect(&world2)
        .entities
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["G54", "Stock", "block_a", "block_b"]);

    let stock = model::find_by_name(&world2, "Stock").expect("stock loaded");
    assert_eq!(model::stock_def_of(&world2, &stock), Some(boxy));
    assert_eq!(model::facets_of(&world2, &stock).map(|t| t.len()), Some(12));
    assert_eq!(
        model::transform_of(&world2, &stock).map(|l| [l.pos.x, l.pos.y, l.pos.z]),
        Some([1.0, 2.0, -1.0])
    );
    let g54 = model::find_by_name(&world2, "G54").expect("wcs loaded");
    assert!(model::is_wcs(&world2, &g54));
    assert_eq!(model::facets_of(&world2, &g54), None);
    assert_eq!(
        model::transform_of(&world2, &g54).map(|l| [l.pos.x, l.pos.y, l.pos.z]),
        Some([12.5, -3.0, 0.5])
    );
}

#[test]
fn from_model_stock_round_trips_through_facets() {
    let path = tmp_path("frommodel");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("src", cube(Point3::ZERO, Point3::new(2.0, 3.0, 1.0))),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel { source: "src".into() },
            Location::default(),
        )
        .unwrap(),
    );
    project::save(&world, &path).expect("save");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let stock = model::find_by_name(&world2, "Stock").expect("stock loaded");
    assert_eq!(
        model::stock_def_of(&world2, &stock),
        Some(StockShape::FromModel {
            source: "src".into()
        })
    );
    // The source is absent here, yet the stock still shows its geometry:
    // FromModel facets came through the file, not through a live copy.
    assert_eq!(model::facets_of(&world2, &stock).map(|t| t.len()), Some(12));
    assert!(
        project::collect(&world2).entities.len() >= 2,
        "Stock and src persist"
    );
}

#[test]
fn corrupt_stock_and_wcs_fields_are_refused() {
    let cases = [
        // negative dimension
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"box": {"dx": -5.0, "dy": 1.0, "dz": 1.0}}}]}"#,
        // table-defying size
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"box": {"dx": 5000000.0, "dy": 1.0, "dz": 1.0}}}]}"#,
        // stock must be named Stock
        r#"{"version": 2, "entities": [{"name": "Billet", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"cylinder": {"diameter": 10.0, "height": 1.0}}}]}"#,
        // parametric stock carrying a facet array
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"cylinder": {"diameter": 10.0, "height": 1.0}}, "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]}]}"#,
        // offsets are tags, not solids
        r#"{"version": 2, "entities": [{"name": "G54", "translation": [0.0, 0.0, 0.0], "kind": "wcs", "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]}]}"#,
        // not a machine slot
        r#"{"version": 2, "entities": [{"name": "G60", "translation": [0.0, 0.0, 0.0], "kind": "wcs"}]}"#,
        // from-model stock with nothing to show
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"from_model": {"source": "gone"}}}]}"#,
    ];
    for text in cases {
        let world = new_world();
        assert!(
            matches!(project::open(&world, text), Err(ProjectError::Corrupt(_))),
            "must refuse: {text}"
        );
        assert!(
            model::named_entities(&world).is_empty(),
            "refused file must leave the world untouched"
        );
    }
}

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

#[test]
fn tools_and_generated_operations_round_trip() {
    let path = tmp_path("cam");
    let world = new_world();
    let mut history = History::default();
    let boxy = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 20.0,
    };
    history.execute(
        &world,
        SetStock::new(&world, boxy, Location::default()).expect("box stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face {
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(19.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("create op"),
    );
    history
        .execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "CAM round-trip must be byte-identical");
    assert!(first.contains("\"tool\"") && first.contains("\"operation\""), "the v3 payloads are in the file: {first}");

    let names: Vec<String> = project::collect(&world2)
        .entities
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["EM-0400-2FL", "Face1", "Stock"]);

    // The tool is the catalog definition again, and it is not geometry.
    let tool = model::find_by_name(&world2, "EM-0400-2FL").expect("tool loaded");
    assert!(model::is_tool(&world2, &tool));
    assert_eq!(model::tool_of(&world2, &tool), Some(catalog_tool("EM-0400-2FL")));
    assert_eq!(model::facets_of(&world2, &tool), None);

    // The operation component - including the stamp the cache was taken
    // for - survives untouched, so it reads Ok, not Stale, after a load.
    let src1 = model::find_by_name(&world, "Face1").expect("source op");
    let src2 = model::find_by_name(&world2, "Face1").expect("loaded op");
    assert!(model::is_operation(&world2, &src2));
    assert_eq!(
        model::operation_of(&world2, &src2),
        model::operation_of(&world, &src1)
    );
    assert_eq!(
        model::operation_of(&world2, &src2).map(|o| o.stamp.is_some()),
        Some(true)
    );
    // The cached path replays exactly: same moves, same byte values.
    assert_eq!(
        model::toolpath_of(&world2, &src2).map(|t| t.path),
        model::toolpath_of(&world, &src1).map(|t| t.path)
    );
    assert!(model::toolpath_of(&world2, &src2).is_some_and(|t| !t.path.moves.is_empty()));

    // No operation leaked geometry or a stock definition either.
    let file = project::collect(&world2);
    let op_dto = file.entities.iter().find(|e| e.name == "Face1").expect("op dto");
    assert!(op_dto.operation.is_some() && op_dto.tool.is_none() && op_dto.facets.is_none());
}

#[test]
fn arc_move_round_trips() {
    // Cylinder stock, contour at a fitter-friendly tolerance: the wall
    // rim is tessellation, the fitter says circle, and the cache must
    // persist arc moves - not silently fall back to chords on reload.
    let path = tmp_path("arcs");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Cylinder {
                diameter: 30.0,
                height: 10.0,
            },
            Location::default(),
        )
        .expect("cylinder stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Skin1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Contour {
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(20.0),
                retract_z: Height::fixed(15.0),
                top_z: Height::fixed(10.0),
                bottom_z: Height::fixed(8.0),
                stepdown: 2.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
                side: Side::Auto,
                lead: Lead::None,
                tolerance: 0.1,
                tab_count: 0,
                tab_width: 0.0,
                tab_height: 0.0,
            },
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Skin1").expect("generate"));
    let src = model::find_by_name(&world, "Skin1").expect("op");
    let before = model::toolpath_of(&world, &src).expect("cache").path;
    assert!(before.has_arcs(), "a 32-gon wall at tol 0.1 contours to arcs");

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"arc\""), "the arc payload is in the file: {first}");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "arc moves round-trip byte-identically");

    let dst = model::find_by_name(&world2, "Skin1").expect("op loaded");
    let after = model::toolpath_of(&world2, &dst).expect("cache").path;
    assert_eq!(after, before, "arcs survive exactly - centers, sense, endpoints");
    assert_eq!(after.arc_count(), before.arc_count());
}

#[test]
fn v2_file_migrates_to_v3() {
    // A literal v2 project: no tool or operation fields anywhere. The
    // chained migration only carries the version forward.
    let text = r#"{
        "version": 2,
        "entities": [
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 40.0, "dy": 40.0, "dz": 10.0}}},
            {"name": "part", "translation": [1.0, 2.0, 3.0],
             "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v2 must migrate");
    let stock = model::find_by_name(&world, "Stock").expect("stock");
    assert_eq!(
        model::stock_def_of(&world, &stock),
        Some(StockShape::Box {
            dx: 40.0,
            dy: 40.0,
            dz: 10.0
        })
    );
    let part = model::find_by_name(&world, "part").expect("part");
    assert_eq!(model::facets_of(&world, &part).map(|t| t.len()), Some(1));

    // Saving stamps the file as current: the migration is a one-way door.
    let path = tmp_path("mig");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "migrated file must carry the current stamp: {saved}"
    );
}

#[test]
fn corrupt_tool_and_operation_fields_are_refused() {
    // Reusable payloads; `$tool` / `$face` are substituted below so the
    // JSON bodies stay single-brace and readable.
    let face = r#""params": {"face": {"stepover": 3.0, "clearance_z": 25.0, "cut_z": 19.5, "feed": 1200.0, "plunge_feed": 300.0, "climb": true}}"#;
    let tool = r#"{"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0, "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0, "holder_d": 44.0, "holder_len": 35.0, "material_group": "general", "rpm": 10000.0, "feed": 1200.0}"#;
    let bad_tool = tool.replace("\"diameter\": 4.0", "\"diameter\": -4.0");
    let bad_face = face.replace("\"stepover\": 3.0", "\"stepover\": -3.0");
    let cases = [
        // a tool entity without its definition
        r#"{"version": 3, "entities": [{"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool"}]}"#
            .to_string(),
        // negative diameter
        r#"{"version": 3, "entities": [{"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool", "tool": $tool}]}"#
            .replace("$tool", &bad_tool),
        // setup names are not tool names
        r#"{"version": 3, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "tool", "tool": $tool}]}"#
            .replace("$tool", tool),
        // a plain model wearing tool parameters
        r#"{"version": 3, "entities": [{"name": "m", "translation": [0.0, 0.0, 0.0], "tool": $tool}]}"#
            .replace("$tool", tool),
        // an operation without its definition
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation"}]}"#
            .to_string(),
        // an operation that cuts with nothing
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "", "src": "stock", $face}}]}"#
            .replace("$face", face),
        // an operation with an empty geometry-source name
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": {"entity": {"name": ""}}, $face}}]}"#
            .replace("$face", face),
        // operations have no geometry of their own
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]], "operation": {"tool": "T", "src": "stock", $face}}]}"#
            .replace("$face", face),
        // insane parameters
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": "stock", $face}}]}"#
            .replace("$face", &bad_face),
        // a cache that carries no path
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": "stock", $face, "cache": {"moves": [], "warnings": []}}}]}"#
            .replace("$face", face),
        // a cache whose moves do not chain
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": "stock", $face, "cache": {"moves": [{"kind": "rapid", "from": [-100.0, -100.0, 25.0], "to": [1.0, 0.0, 25.0]}, {"kind": "feed", "from": [0.0, 0.0, 25.0], "to": [0.0, 0.0, 19.5], "feed": 300.0}], "warnings": []}}}]}"#
            .replace("$face", face),
    ];
    for text in cases {
        let world = new_world();
        assert!(
            matches!(
                project::open(&world, &text),
                Err(ProjectError::Corrupt(_))
            ),
            "must refuse: {text}"
        );
        assert!(
            model::named_entities(&world).is_empty(),
            "refused file must leave the world untouched"
        );
    }
}

#[test]
fn v3_file_migrates_to_v4() {
    // A literal v3 file that already carries a feature pick: v3's src enum
    // could hold one, and v4 reads it verbatim - the migration step is the
    // stamp, not a reshape. The id 42 is unresolvable on purpose: a
    // tampered or stale pick is a generation error, never a load error.
    let text = r#"{
        "version": 3,
        "entities": [
            {"name": "part", "translation": [0.0, 0.0, 0.0],
             "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]},
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": {"selection": {"items": [
                 {"entity": "part", "feature": {"face": {"id": 42}}}
            ]}}, "params": {"face": {"stepover": 3.0, "clearance_z": 25.0,
                 "cut_z": 0.5, "feed": 1200.0, "plunge_feed": 300.0,
                 "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v3 must migrate");
    let op = model::find_by_name(&world, "Op1").expect("op");
    let o = model::operation_of(&world, &op).expect("operation component");
    assert_eq!(
        o.src,
        SrcRef::Selection(vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(42),
        }]),
        "the pick survives the step-up intact"
    );
    // Unresolvable id and no cache in the file: shape-legal, so it loads -
    // and reads Never (nothing was ever generated in this world), never a
    // load-time refusal, with nothing to preview.
    assert_eq!(operation::status_of(&world, &op), OpStatus::Never);
    assert!(operation::region_of(&world, &o).is_none());

    let path = tmp_path("mig4");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "resaved file must be stamped current: {saved}"
    );
}

#[test]
fn v4_file_migrates_to_v5() {
    // A literal v4 file: the v5 additions (patch picks, arc moves, face
    // staging, pocket hops) all default to "absent", so a v4 file carries
    // none of them and v5 reads it verbatim - the step-up is the stamp,
    // exactly as v3 -> v4 was. Both params kinds are here because the face
    // arm grew top_z/stepdown and the pocket arm grew retract_z.
    let text = r#"{
        "version": 4,
        "entities": [
            {"name": "part", "translation": [0.0, 0.0, 0.0],
             "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]},
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": {"selection": {"items": [
                 {"entity": "part", "feature": {"face": {"id": 42}}}
            ]}}, "params": {"face": {"stepover": 3.0, "clearance_z": 25.0,
                 "cut_z": 0.5, "feed": 1200.0, "plunge_feed": 300.0,
                 "climb": true}}}},
            {"name": "Op2", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 3.0, "stock_to_leave": 0.0, "clearance_z": 20.0,
                 "top_z": 8.0, "bottom_z": 2.0, "stepdown": 2.0,
                 "feed": 900.0, "plunge_feed": 250.0, "climb": true,
                 "strategy": "zigzag"}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v4 must migrate");
    let op = model::find_by_name(&world, "Op1").expect("op1");
    let o = model::operation_of(&world, &op).expect("operation component");
    assert_eq!(
        o.src,
        SrcRef::Selection(vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(42),
        }]),
        "the pick survives the step-up intact"
    );
    let op2 = model::find_by_name(&world, "Op2").expect("op2");
    let p = match model::operation_of(&world, &op2).expect("op2 params").params {
        OpParams::Pocket { strategy, .. } => strategy,
        ref other => panic!("pocket params expected, got {other:?}"),
    };
    assert_eq!(p, PocketStrategy::Zigzag, "a v4 pocket means zigzag");

    let path = tmp_path("mig5");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "resaved file must be stamped current: {saved}"
    );
    // The declared shapes stay unwritten while unused: a migrated file
    // must not grow keys a v4 build could not read past the version.
    for absent in ["\"retract_z\"", "\"patch\"", "\"arc\""] {
        assert!(!saved.contains(absent), "v5 default must not be serialized: {absent}");
    }
}

#[test]
fn v5_roundtrips_retract_and_auto() {
    // The step-5 fields, once actually set, round-trip exactly: the
    // pocket's hop plane and Auto choice, and the trace's hop plane.
    let path = tmp_path("hop5");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Pocket1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Pocket {
                stepover: 1.0,
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(25.0),
                retract_z: Some(Height::fixed(22.0)),
                top_z: Height::fixed(20.0),
                bottom_z: Height::fixed(18.0),
                stepdown: 2.0,
                feed: 900.0,
                plunge_feed: 250.0,
                climb: true,
                strategy: PocketStrategy::Auto,
            },
        )
        .expect("pocket"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Trace1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Trace {
                clearance_z: Height::fixed(25.0),
                retract_z: Some(Height::fixed(23.0)),
                top_z: Height::fixed(20.0),
                bottom_z: Height::fixed(18.0),
                stepdown: 2.0,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("trace"),
    );

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"retract_z\""), "the set hop plane is in the file");
    assert!(first.contains("\"auto\""), "the Auto choice is stored (resolution is not)");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "retract/Auto round-trip byte-identically");

    let op = model::find_by_name(&world2, "Pocket1").expect("pocket loaded");
    match model::operation_of(&world2, &op).expect("params").params {
        OpParams::Pocket {
            retract_z,
            strategy,
            ..
        } => {
            assert_eq!(retract_z, Some(Height::fixed(22.0)), "the hop plane survives exactly");
            assert_eq!(strategy, PocketStrategy::Auto, "the choice survives exactly");
        }
        ref other => panic!("pocket params expected, got {other:?}"),
    }
    let tr = model::find_by_name(&world2, "Trace1").expect("trace loaded");
    match model::operation_of(&world2, &tr).expect("params").params {
        OpParams::Trace { retract_z, .. } => assert_eq!(retract_z, Some(Height::fixed(23.0))),
        ref other => panic!("trace params expected, got {other:?}"),
    }
}

#[test]
fn patch_selection_round_trips() {
    // The welded wall of a faceted boss, picked once and saved by its
    // geometry hash: the file speaks `patch`, and a second world
    // recomputing features from the same facets resolves the same weld,
    // so the loaded op stays Ok rather than Stale.
    let path = tmp_path("patch");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("boss", cylinder(20.0, 8.0, 16)),
    );
    let e = model::find_by_name(&world, "boss").expect("boss");
    let f = model::features_of(&world, &e).expect("features");
    let pid = feature::patches(&f)
        .into_iter()
        .find(|p| p.faces.len() >= 8)
        .map(|p| p.id)
        .expect("the sixteen wall strips weld into one patch");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Boss1",
            "EM-0400-2FL",
            SrcRef::Selection(vec![GeoSel {
                entity: "boss".into(),
                feature: FeatureSel::Patch(pid),
            }]),
            OpParams::Face {
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(4.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Boss1").expect("generate"));
    let src_op = model::find_by_name(&world, "Boss1").expect("op");
    let o = model::operation_of(&world, &src_op).expect("operation");
    assert_eq!(operation::status_of(&world, &src_op), OpStatus::Ok, "generated first");

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"patch\""), "the wall pick is in the file: {first}");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "a welded pick round-trips byte-identically");

    let dst_op = model::find_by_name(&world2, "Boss1").expect("op loaded");
    let p = model::operation_of(&world2, &dst_op).expect("operation");
    assert_eq!(p.src, o.src, "the same weld, named by the same id");
    assert_eq!(p.stamp, o.stamp);
    assert_eq!(
        operation::status_of(&world2, &dst_op),
        OpStatus::Ok,
        "patch ids are content hashes, not file positions"
    );
}

#[test]
fn a_selection_round_trips_and_stays_ok() {
    let path = tmp_path("pick");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    // Top face plus one rim edge: the boundary loop and a centerline run
    // in one pick list (M1.5 step 8's mixed shape), contour parameters.
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let top = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
        .max_by(|a, b| a.offset.total_cmp(&b.offset))
        .map(|fa| fa.id)
        .expect("box top");
    let run = feature::chains(&f)
        .into_iter()
        .find(|c| {
            !c.closed
                && {
                    let p = feature::chain_points(&f, c);
                    p.len() >= 2 && (p[0][0] != p[1][0] || p[0][1] != p[1][1])
                }
        })
        .map(|c| c.id)
        .expect("a horizontal rim edge");
    let params = OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(4.5),
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
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Pick1",
            "EM-0400-2FL",
            SrcRef::Selection(vec![
                GeoSel {
                    entity: "part".into(),
                    feature: FeatureSel::Face(top),
                },
                GeoSel {
                    entity: "part".into(),
                    feature: FeatureSel::Chain(run),
                },
            ]),
            params,
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Pick1").expect("generate"));
    let src_op = model::find_by_name(&world, "Pick1").expect("op");
    let o = model::operation_of(&world, &src_op).expect("operation");
    assert_eq!(operation::status_of(&world, &src_op), OpStatus::Ok, "generated first");

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(
        first.contains("\"selection\"") && first.contains("\"face\"") && first.contains("\"chain\""),
        "the pick list is in the file verbatim: {first}"
    );

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "a picked project round-trips byte-identically");

    let dst_op = model::find_by_name(&world2, "Pick1").expect("op loaded");
    let p = model::operation_of(&world2, &dst_op).expect("operation");
    assert_eq!(p.src, o.src, "the pick list is the same list");
    // The cache replays against features recomputed from the same facets -
    // stable ids earn their name here: Ok, not Stale.
    assert_eq!(p.stamp, o.stamp);
    assert_eq!(operation::status_of(&world2, &dst_op), OpStatus::Ok, "a loaded pick is still honest");
    assert_eq!(
        operation::chains_of(&world2, &p).len(),
        operation::chains_of(&world, &o).len(),
        "preview sees the same runs before and after the file"
    );
}

#[test]
fn side_roundtrip_v6_defaults_to_auto() {
    // A literal v6 project: contour params predate the side key. The
    // serde default IS the migration - absent reads as `auto`, the ask
    // every pre-v7 file silently had - and saving stamps the v7 shape.
    let text = r#"{
        "version": 6,
        "entities": [
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 40.0, "dy": 40.0, "dz": 10.0}}},
            {"name": "T", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 6.0, "corner_r": 0.0,
                      "flute_len": 20.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 800.0}},
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": "none", "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v6 contour must migrate");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour { side, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*side, Side::Auto, "an absent side is Auto");

    let path = tmp_path("v7");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains("\"side\": \"auto\"")
            && saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v7 shape: {saved}"
    );
}

#[test]
fn v8_file_steps_to_v9() {
    // A literal v8 project: `lead` has been a required key since v1, so
    // the v8 -> v9 step is the stamp itself - the file reads unchanged
    // and saving restamps it at this build's version.
    let text = r#"{
        "version": 8,
        "entities": [
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": {"arc": {"radius": 3.0}}, "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v8 file must migrate");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour { lead, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*lead, Lead::Arc { radius: 3.0 }, "an arc lead reads verbatim");

    let path = tmp_path("v9");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    // save() is to_string_pretty, so match format-agnostic fragments:
    // the tag key and the radius value, which nowhere else in this
    // document can both occur.
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION))
            && saved.contains("\"arc\"")
            && saved.contains("\"radius\": 3.0"),
        "the migration writes the v9 shape: {saved}"
    );
}

#[test]
fn v9_file_steps_to_v10() {
    // A literal v9 project: the drilling landing adds no required keys.
    // ToolDto's tip_angle/pitch serde-default to the inert 0.0 they meant
    // before, PathDto's cycles defaults to empty (no pre-v10 build ever
    // generated a cycle), and ParamsDto just gains a variant - so the
    // v9 -> v10 step is the stamp itself, exactly like v8 -> v9.
    let text = r#"{
        "version": 9,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": "none", "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "plunge", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 3.0]}
                 ], "warnings": []}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v9 file must migrate");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    assert!(
        matches!(o.params, OpParams::Contour { .. }),
        "the contour params read unchanged: {:?}",
        o.params
    );

    let path = tmp_path("v10");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v10 shape: {saved}"
    );
    // skip_serializing_if keeps a cycle-free cache byte-identical to its
    // v9 form; the key must not appear just because the build learned it.
    assert!(
        !saved.contains("\"cycles\""),
        "an empty cycle list is not written: {saved}"
    );
}

#[test]
fn drill_params_and_cycle_cache_round_trip_and_a_stray_cycle_is_refused() {
    // Hand-written v10 text, so the drill wire grammar itself is pinned:
    // the "drill" params tag, holes with through stored (not re-derived),
    // and cycles beside the moves of a drilling cache.
    let text = r#"{
        "version": 10,
        "entities": [
            {"name": "D1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"drill": {
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_stock", "offset": 0.0},
                 "cycle": "peck", "peck": 6.0, "dwell": 0.0,
                 "feed": 240.0, "plunge_feed": 120.0,
                 "holes": [
                     {"xy": [0.0, 0.0], "radius": 3.0, "entry_z": 10.0,
                      "floor_z": -5.0, "through": false},
                     {"xy": [20.0, 15.5], "radius": 3.0, "entry_z": 10.0,
                      "floor_z": 0.0, "through": true}
                 ]}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "feed", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 4.0], "feed": 240.0},
                     {"kind": "plunge", "from": [0.0, 0.0, 4.0], "to": [0.0, 0.0, -5.0]},
                     {"kind": "rapid", "from": [0.0, 0.0, -5.0], "to": [20.0, 15.5, 10.0]}
                 ], "warnings": [], "cycles": [
                     {"kind": "peck", "xy": [0.0, 0.0], "r_z": 4.0,
                      "bottom_z": -5.0, "entry_z": 10.0, "peck": 6.0,
                      "dwell": 0.0, "first_move": 1}
                 ]}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v10 drill grammar must read");
    let e = model::find_by_name(&world, "D1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Drill {
        cycle,
        peck,
        holes,
        ..
    } = &o.params
    else {
        panic!("drill params expected, got {:?}", o.params);
    };
    assert_eq!(*cycle, hsmworks_app::drilling::CycleChoice::Peck);
    assert_eq!(*peck, 6.0);
    assert_eq!(holes.len(), 2);
    assert_eq!((holes[1].xy.x, holes[1].xy.y), (20.0, 15.5));
    assert!(holes[1].through && !holes[0].through, "through reads verbatim");

    // Save, reload, save again: every float the file carries is a fixed
    // point of write-parse, so the drill shape must be byte-stable too.
    let path = tmp_path("drill");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the drill shape round-trips byte-identically");
    assert!(first.contains("\"drill\"") && first.contains("\"cycles\""));

    // Prime Directive: a cycle pointing past the end of its chain is a
    // corrupt file, refused before any world write - the warning index a
    // shank law would read out of it has nothing to point at.
    let stray = text.replace("\"first_move\": 1", "\"first_move\": 99");
    let world3 = new_world();
    match project::open(&world3, &stray) {
        Err(ProjectError::Corrupt(msg)) => assert!(msg.contains("past"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    assert!(model::named_entities(&world3).is_empty());
    // An unusable hole (a floor above the entry is not a hole) fails the
    // parameter sanity gate at open, not the generator.
    let bad_hole = text.replace("\"floor_z\": -5.0", "\"floor_z\": 12.0");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad_hole), Err(ProjectError::Corrupt(_))),
        "must refuse a hole whose floor is above its entry"
    );
}

#[test]
fn v10_file_steps_to_v11_and_the_pocket3d_grammar_round_trips() {
    // A literal v10 project (the newest shape before the 3-D pocket):
    // the v10 -> v11 step is the stamp itself - no required key is added,
    // since Pocket3D is a new variant and entry defaults to Plunge.
    let text = r#"{
        "version": 10,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 2.0, "stock_to_leave": 0.0, "clearance_z": 10.0,
                 "top_z": 5.0, "bottom_z": -2.0, "stepdown": 2.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "strategy": "zigzag"}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "plunge", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 5.0]}
                 ], "warnings": []}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v10 file must migrate");
    let e = model::find_by_name(&world, "P1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    assert!(
        matches!(o.params, OpParams::Pocket { .. }),
        "the pocket params read unchanged: {:?}",
        o.params
    );
    let path = tmp_path("v11");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the current stamp: {saved}"
    );

    // The pocket3d wire grammar, hand-written so the tags are pinned:
    // "pocket3d", snake_case entry, and no footprint key - the silhouette
    // is derived at generate, never carried.
    let text = r#"{
        "version": 11,
        "entities": [
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.4, "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "helix", "helix_radius": 2.0}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]}
                 ], "warnings": []}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v11 pocket3d grammar must read");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D {
        stepover,
        stock_to_leave,
        strategy,
        entry,
        helix_radius,
        retract_z,
        rest_rough,
        ..
    } = &o.params
    else {
        panic!("pocket3d params expected, got {:?}", o.params);
    };
    assert_eq!(*stepover, 2.5);
    assert_eq!(*stock_to_leave, 0.4);
    assert_eq!(*strategy, PocketStrategy::Waterline);
    assert_eq!(*entry, hsmworks_app::machining3d::Entry3D::Helix);
    assert_eq!(*helix_radius, 2.0);
    assert!(retract_z.is_none(), "absent retract stays absent");
    assert!(
        !*rest_rough,
        "absent rest_rough is off - a v11 file generated the plain ladder"
    );

    // Write-parse-write is a fixed point, and the entry tag is snake_case.
    let path = tmp_path("p3");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the pocket3d shape round-trips byte-identically");
    assert!(first.contains("\"pocket3d\""), "{first}");
    assert!(first.contains("\"entry\": \"helix\""), "{first}");
    assert!(!first.contains("footprint"), "the silhouette is never on the wire");

    // An unknown entry tag is corrupt text, refused, not defaulted.
    let bad = text.replace("\"helix\"", "\"spiral\"");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "must refuse an entry tag outside the vocabulary"
    );
}

#[test]
fn v11_file_steps_to_v12() {
    // A literal v11 project: the rest pass arrives as one optional key
    // (`rest_rough`), and a v11 file can contain it nowhere - the pocket
    // could not rest-rough. The serde default (off) is exactly what v11
    // generated, so the step-up is the stamp itself, like every bump
    // since v6.
    let text = r#"{
        "version": 11,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "zigzag", "entry": "ramp", "helix_radius": 1.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v11 file must migrate");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D { rest_rough, entry, .. } = &o.params else {
        panic!("the pocket3d params must read unchanged, got {:?}", o.params);
    };
    assert!(!*rest_rough, "a v11 file never rest-roughed; off is its meaning");
    assert_eq!(*entry, hsmworks_app::machining3d::Entry3D::Ramp);

    let path = tmp_path("v12");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v12 stamp: {saved}"
    );
    // The learned key is written in its false state: a v12 file names
    // the choice it made, so the shape stays a fixed point.
    assert!(saved.contains("\"rest_rough\": false"), "{saved}");
}

#[test]
fn v12_file_steps_to_v13() {
    // A literal v12 project: the engagement angle arrives as one
    // optional key (`max_engagement`), and a v12 file can contain it
    // nowhere - no v12 strategy read it. The serde default (45°, the
    // law's own) is unobservable there for exactly that reason, so the
    // step-up is the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 12,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "adaptive", "entry": "plunge", "helix_radius": 1.0}}}}
        ]
    }"#;
    let world = new_world();
    // The tag itself cannot appear in a real v12 file (the combo had no
    // Adaptive entry), but the wire accepts it as any other enum name -
    // what must land is the angle default.
    project::open(&world, text).expect("v12 file must migrate");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D { strategy, max_engagement, .. } = &o.params else {
        panic!("the pocket3d params must read unchanged, got {:?}", o.params);
    };
    assert_eq!(*strategy, PocketStrategy::Adaptive);
    assert!(
        (*max_engagement - 45.0).abs() < 1e-12,
        "absent means the law's 45°, got {max_engagement}"
    );

    let path = tmp_path("v13");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v13 stamp: {saved}"
    );
    // The learned key is written in its stated state: a fixed point, so
    // a second open cannot drift the number.
    assert!(saved.contains("\"strategy\": \"adaptive\""), "{saved}");
    assert!(saved.contains("\"max_engagement\": 45"), "{saved}");
}

#[test]
fn v13_file_steps_to_v14() {
    // A literal v13 project: the feed modulation arrives as one new
    // params key (`feed_opt`), absent from every v13 file - no v13
    // generator re-read a path against the sim. The serde default (off)
    // is exactly what v13 posted, every F word at the programmed feed,
    // so the step-up is the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 13,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "plunge", "helix_radius": 1.0,
                 "rest_rough": true, "max_engagement": 40.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v13 file must migrate");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D { feed_opt, rest_rough, max_engagement, .. } = &o.params else {
        panic!("the pocket3d params must read unchanged, got {:?}", o.params);
    };
    assert!(!*feed_opt, "a v13 file never modulated feeds; off is its meaning");
    assert!(*rest_rough, "the stated keys ride through the step-up untouched");
    assert!(
        (*max_engagement - 40.0).abs() < 1e-12,
        "a stated angle must survive the migration, got {max_engagement}"
    );

    let path = tmp_path("v14");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v14 stamp: {saved}"
    );
    // The learned key is written in its false state: a fixed point, so
    // a second open cannot drift it.
    assert!(saved.contains("\"feed_opt\": false"), "{saved}");
}

#[test]
fn v14_file_steps_to_v15_and_the_finish3d_grammar_round_trips() {
    // A literal v14 project: the waterline arrives as one new params
    // variant (`finish3d`), which no v14 file can contain - the tag did
    // not exist. Nothing existing changed shape; step-up is the stamp
    // itself, like every bump since v6.
    let text = r#"{
        "version": 14,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "plunge", "helix_radius": 1.0,
                 "rest_rough": false, "max_engagement": 45.0, "feed_opt": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v14 file must migrate");
    let path = tmp_path("v15a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v15 stamp: {saved}"
    );
    assert!(saved.contains("\"feed_opt\": true"), "the stated key rides through: {saved}");

    // The v15 wire grammar, hand-written so the tags are pinned:
    // "finish3d", no stepdown key (the scallop law derives it from
    // tolerance and tool - storing one could only disagree), and an
    // absent retract_z means hop-to-clearance.
    let text = r#"{
        "version": 15,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v15 finish3d grammar must read");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D {
        tolerance,
        retract_z,
        climb,
        feed,
        ..
    } = &o.params
    else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(*tolerance, 0.1);
    assert!(*climb);
    assert_eq!(*feed, 500.0);
    assert!(retract_z.is_none(), "absent retract means hop-to-clearance");

    // Write-parse-write is a fixed point, and the variant tag is clean.
    let path = tmp_path("v15b");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the finish3d shape round-trips byte-identically");
    assert!(first.contains("\"finish3d\""), "{first}");
    assert!(!first.contains("retract_z"), "None is absent, not null: {first}");

    // A tolerance of zero is nonsense the loader refuses, not a default.
    let bad = text.replace("\"tolerance\": 0.1", "\"tolerance\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v15 file with tolerance 0.0 must not load"
    );
}

#[test]
fn v15_file_steps_to_v16_and_the_parallel_grammar_round_trips() {
    // A literal v15 project: the parallel finish arrives as two new
    // keys inside `finish3d` (`pattern`, `stepover`), absent from every
    // v15 file - the serde defaults (waterline, derived stepover) are
    // exactly what v15 ran. Nothing existing changed shape; step-up is
    // the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 15,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v15 file must migrate");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { pattern, stepover, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(*pattern, hsmworks_app::finishing3d::FinishPattern::Waterline);
    assert!(stepover.is_none(), "absent stepover means the cos-law derives it");
    let path = tmp_path("v16a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v16 stamp: {saved}"
    );
    // The default writes out explicitly now that it is a choice among
    // three, not the only possibility.
    assert!(saved.contains("\"pattern\": \"waterline\""), "{saved}");

    // The v16 wire grammar, hand-written so the spellings are pinned:
    // snake_case pattern words, an explicit stepover override.
    let text = r#"{
        "version": 16,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "pattern": "parallel_multi", "stepover": 0.3, "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v16 parallel grammar must read");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { pattern, stepover, ramp, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(
        *pattern,
        hsmworks_app::finishing3d::FinishPattern::ParallelMulti
    );
    assert_eq!(*stepover, Some(0.3));
    assert!(!*ramp, "a v16 file has no `ramp` word: the hop, exactly what v16 ran");

    // Write-parse-write is a fixed point, and the new keys ride.
    let path = tmp_path("v16b");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the parallel shape round-trips byte-identically");
    assert!(first.contains("\"pattern\": \"parallel_multi\""), "{first}");
    assert!(first.contains("\"stepover\": 0.3"), "{first}");

    // A zero stepover is nonsense the loader refuses, not a default.
    let bad = text.replace("\"stepover\": 0.3", "\"stepover\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v16 file with stepover 0.0 must not load"
    );

    // M2.2c: "scallop" is a new word in the same v16 grammar - no
    // version bump, because no v16 file ever carried it and every
    // v16 file still reads exactly as it did.
    let sc = text.replace("\"parallel_multi\"", "\"scallop\"");
    let world4 = new_world();
    project::open(&world4, &sc).expect("the scallop spelling must read");
    let e4 = model::find_by_name(&world4, "F1").expect("op");
    let o4 = model::operation_of(&world4, &e4).expect("operation");
    let OpParams::Finish3D { pattern, .. } = &o4.params else {
        panic!("finish3d params expected");
    };
    assert_eq!(*pattern, hsmworks_app::finishing3d::FinishPattern::Scallop);
    let path_sc = tmp_path("v16s");
    project::save(&world4, &path_sc).expect("save");
    let saved_sc = std::fs::read_to_string(&path_sc).expect("read");
    let _ = std::fs::remove_file(&path_sc);
    assert!(saved_sc.contains("\"pattern\": \"scallop\""), "{saved_sc}");

    // M2.2d and M2.2e, same law four times over: "radial", "spiral",
    // "contour" and "project" join the v16 grammar as words, not as
    // a version.
    for (word, want) in [
        ("radial", hsmworks_app::finishing3d::FinishPattern::Radial),
        ("spiral", hsmworks_app::finishing3d::FinishPattern::Spiral),
        ("contour", hsmworks_app::finishing3d::FinishPattern::Contour),
        ("project", hsmworks_app::finishing3d::FinishPattern::Project),
    ] {
        let t = text.replace("\"parallel_multi\"", &format!("\"{word}\""));
        let w = new_world();
        project::open(&w, &t).expect("the polar spellings must read");
        let e = model::find_by_name(&w, "F1").expect("op");
        let o = model::operation_of(&w, &e).expect("operation");
        let OpParams::Finish3D { pattern, .. } = &o.params else {
            panic!("finish3d params expected");
        };
        assert_eq!(*pattern, want);
        let path_p = tmp_path("v16p");
        project::save(&w, &path_p).expect("save");
        let saved_p = std::fs::read_to_string(&path_p).expect("read");
        let _ = std::fs::remove_file(&path_p);
        assert!(saved_p.contains(&format!("\"pattern\": \"{word}\"")), "{saved_p}");
    }
    // An unknown pattern word is refused, never defaulted.
    let bogus = text.replace("\"parallel_multi\"", "\"zigzag\"");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &bogus), Err(ProjectError::Corrupt(_))),
        "a v16 file with an unknown pattern must not load"
    );
}

#[test]
fn v16_file_steps_to_v17_and_the_ramp_grammar_round_trips() {
    // v16 -> v17: one more new key inside `finish3d` - `ramp` - absent
    // from every v16 file, whose serde default (false: the hop) is
    // exactly what v16 ran. Nothing existing changed shape; step-up is
    // the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 16,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "pattern": "parallel", "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v16 file must migrate");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { ramp, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert!(!*ramp, "absent ramp means the hop - what every pre-v17 file ran");
    let path = tmp_path("v17a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v17 stamp: {saved}"
    );
    // Written always, like `climb`: the choice is a fact about the file,
    // not a default to lean on.
    assert!(saved.contains("\"ramp\": false"), "{saved}");

    // The v17 wire grammar, hand-written so the spelling is pinned:
    // `"ramp": true` rides, and the write-parse-write fixed point keeps it.
    let rt = text
        .replace("\"version\": 16", "\"version\": 17")
        .replace("\"climb\": true", "\"climb\": true, \"ramp\": true");
    let world2 = new_world();
    project::open(&world2, &rt).expect("the v17 ramp spelling must read");
    let e2 = model::find_by_name(&world2, "F1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Finish3D { ramp, .. } = &o2.params else {
        panic!("finish3d params expected");
    };
    assert!(*ramp);
    let path = tmp_path("v17b");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the ramp shape round-trips byte-identically");
    assert!(first.contains("\"ramp\": true"), "{first}");

    // A ramp that is not a bool is corruption, not a default.
    let bad = rt.replace("\"ramp\": true", "\"ramp\": \"yes\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad), Err(ProjectError::Corrupt(_))),
        "a non-bool ramp must not load"
    );
}

#[test]
fn v17_file_steps_to_v18_and_the_surface_finish_grammar_round_trips() {
    // v17 -> v18: two more keys inside `finish3d` - `min_step` (the
    // march's refinement floor) and `ramp_angle` (the chain's slope
    // cap) - absent from every v17 file, whose serde defaults are
    // exactly what v17 ran: no floor (the march's own 1/4096 right)
    // and the 45-degree diagonal v17's chain law already certified.
    // Nothing existing changed shape; step-up is the stamp itself,
    // like every bump since v6.
    let text = r#"{
        "version": 17,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "pattern": "parallel", "tolerance": 0.1, "ramp": true,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v17 file must migrate");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { min_step, ramp_angle, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(*min_step, None, "absent floor means the march's own right - what every pre-v18 file ran");
    assert_eq!(*ramp_angle, 45.0, "absent angle means the v17 diagonal");
    let path = tmp_path("v18a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v18 stamp: {saved}"
    );
    // The floor is Optional and stays absent while it is None (the
    // skip-if-none law, like stepover); the angle is always - like
    // the hop plane, a default is a fact about the machine, not a
    // key to economize on.
    assert!(!saved.contains("\"min_step\""), "None must not mint a key: {saved}");
    assert!(saved.contains("\"ramp_angle\": 45.0"), "{saved}");

    // The v18 wire grammar, hand-written so the spelling is pinned:
    // floor and angle ride, and the write-parse-write fixed point
    // keeps them.
    let rt = text
        .replace("\"version\": 17", "\"version\": 18")
        .replace("\"climb\": true", "\"climb\": true, \"min_step\": 0.05, \"ramp_angle\": 30.0");
    let world2 = new_world();
    project::open(&world2, &rt).expect("the v18 floor-and-angle spelling must read");
    let e2 = model::find_by_name(&world2, "F1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Finish3D { min_step, ramp_angle, .. } = &o2.params else {
        panic!("finish3d params expected");
    };
    assert_eq!(*min_step, Some(0.05));
    assert_eq!(*ramp_angle, 30.0);
    let path = tmp_path("v18b");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the surface-finish shape round-trips byte-identically");
    assert!(first.contains("\"min_step\": 0.05"), "{first}");

    // Corruptions are corruption, not defaults: a floor that is not
    // a number, and an angle that is not one either.
    let bad = rt.replace("\"min_step\": 0.05", "\"min_step\": \"fine\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad), Err(ProjectError::Corrupt(_))),
        "a non-number min_step must not load"
    );
    let bad = rt.replace("\"ramp_angle\": 30.0", "\"ramp_angle\": \"steep\"");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &bad), Err(ProjectError::Corrupt(_))),
        "a non-number ramp_angle must not load"
    );
}

#[test]
fn tangential_lead_round_trips() {
    // The new tag in both directions, and by hand-written text so the
    // wire spelling itself is pinned, not just Rust's serde mirror.
    let text = r#"{
        "version": 9,
        "entities": [
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": {"tangential": {"radius": 4.5}}, "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("a v9 file with the tangential tag must read");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour { lead, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*lead, Lead::Tangential { radius: 4.5 });

    let path = tmp_path("lead-t");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains("\"tangential\"") && saved.contains("\"radius\": 4.5"),
        "the tag writes back: {saved}"
    );
}

#[test]
fn side_choice_round_trips_and_a_hand_edited_inside_refuses_at_generate() {
    // The stored value is the user's ask, never a resolution: Inside
    // survives save/load byte-for-byte...
    let path = tmp_path("side");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: 10.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("tool"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "C1",
            "EM-0600-4FL",
            SrcRef::Entity("part".into()),
            OpParams::Contour {
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(10.0),
                retract_z: Height::fixed(8.0),
                top_z: Height::fixed(4.0),
                bottom_z: Height::fixed(2.0),
                stepdown: 8.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
                side: Side::Auto,
                lead: Lead::None,
                tolerance: 0.01,
                tab_count: 0,
                tab_width: 0.0,
                tab_height: 0.0,
            },
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");

    // ...which a hand-edited file proves is loadable verbatim: commands
    // refuse Inside on a whole-section source (operations.rs's
    // `inside_on_entity_source_is_refused`), so the only way an Inside
    // + Entity op reaches a world is by editing the JSON. The load
    // accepts it; generate refuses it by name - the backstop.
    let edited = text.replace("\"side\": \"auto\"", "\"side\": \"inside\"");
    assert_ne!(edited, text, "the saved file carries the side key once");
    let world2 = new_world();
    project::open(&world2, &edited).expect("a hand-edited file loads as any other");
    let e = model::find_by_name(&world2, "C1").expect("op");
    let o = model::operation_of(&world2, &e).expect("operation");
    let OpParams::Contour { side, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*side, Side::Inside, "the ask survives the file");
    let err = GenerateOperation::new(&world2, "C1")
        .err()
        .expect("generate must backstop the hand-edited Inside");
    assert!(err.to_string().contains("needs picked rings"), "{err}");

    // And the honest path: an Inside on a Selection source round-trips
    // the choice and generates. The pick list is built against the
    // loaded world (feature ids are per-world), on the same path the
    // hand-edit took - minus the edit.
    let items = {
        let e = model::find_by_name(&world2, "part").expect("part");
        let f = model::features_of(&world2, &e).expect("features");
        let top = f
            .faces
            .iter()
            .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
            .max_by(|a, b| a.offset.total_cmp(&b.offset))
            .map(|fa| fa.id)
            .expect("box top");
        vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(top),
        }]
    };
    history.execute(
        &world2,
        CreateOperation::new(
            &world2,
            "C2",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            OpParams::Contour {
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(10.0),
                retract_z: Height::fixed(8.0),
                top_z: Height::fixed(4.0),
                bottom_z: Height::fixed(2.0),
                stepdown: 8.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
                side: Side::Inside,
                lead: Lead::None,
                tolerance: 0.01,
                tab_count: 0,
                tab_width: 0.0,
                tab_height: 0.0,
            },
        )
        .expect("create C2"),
    );
    history.execute(
        &world2,
        GenerateOperation::new(&world2, "C2").expect("generate C2"),
    );
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(
        first.contains("\"side\": \"inside\""),
        "the choice is in the file"
    );
    let world3 = new_world();
    project::load(&world3, &path).expect("load");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the side choice round-trips byte-identically");
    let e = model::find_by_name(&world3, "C2").expect("op loaded");
    let o = model::operation_of(&world3, &e).expect("operation");
    let OpParams::Contour { side, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*side, Side::Inside, "Inside survives the file unchanged");
}

#[test]
fn units_round_trip_and_a_v7_file_means_millimeters() {
    use hsmworks_app::units::{self, Unit};

    // An inch document: the file says so, a save-load-save is stable,
    // and the millimeter numbers inside are untouched by the lens.
    let world = populated_world();
    units::set(&world, Unit::Inches);
    let path = tmp_path("units_in");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"units\": \"in\""), "inch must be written: {first}");
    let world2 = new_world();
    project::open(&world2, &first).expect("reopen");
    assert_eq!(units::of(&world2), Unit::Inches);
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "save-load-save must be stable");

    // A millimeter document writes no units key at all - the whole v7
    // corpus is mm, so absence must keep meaning mm.
    let world = populated_world();
    let path = tmp_path("units_mm");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(!text.contains("\"units\""), "mm is the default, not a key: {text}");

    // A literal v7 file: no units key anywhere, the step-up is the stamp.
    let text = r#"{
        "version": 7,
        "entities": [{
            "name": "legacy",
            "translation": [0.0, 0.0, 0.0],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]
        }]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v7 must migrate");
    assert_eq!(units::of(&world), Unit::Millimeters, "a v7 file is millimeters");

    // Opening always lands the value: an inch file into an inch-leaked
    // world must come back mm.
    let world = new_world();
    units::set(&world, Unit::Inches);
    project::open(&world, text).expect("reopen into a dirty world");
    assert_eq!(units::of(&world), Unit::Millimeters, "no previous-document leak");

    // A nonsense unit is a corrupt file, refused before any world write.
    let text = r#"{"version": 8, "units": "furlongs", "entities": []}"#;
    let world = new_world();
    match project::open(&world, text) {
        Err(ProjectError::Corrupt(msg)) => assert!(msg.contains("furlongs"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    assert_eq!(units::of(&world), Unit::Millimeters, "refusal wrote nothing");
}
