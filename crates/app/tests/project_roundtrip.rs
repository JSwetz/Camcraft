//! M1.1 acceptance: save/load round-trips are byte-identical; a wrong
//! version or corrupt file is a clean error.

use std::path::PathBuf;

use hsmworks_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWcsOrigin,
};
use hsmworks_app::geometry::feature;
use hsmworks_app::machining2d::{Lead, PocketStrategy};
use hsmworks_app::operation::{self, FeatureSel, GeoSel, OpParams, OpStatus, SrcRef};
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
    // Anything newer than this build's format stays refused; v1/v2/v3/v4
    // migrate (see v1_file_migrates / v2_file_migrates_to_v3 /
    // v3_file_migrates_to_v4 / v4_file_migrates_to_v5).
    let text = r#"{"version": 6, "entities": []}"#;
    let world = new_world();
    match project::open(&world, text) {
        Err(ProjectError::UnsupportedVersion(6)) => {}
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
                clearance_z: 25.0,
                cut_z: 19.5,
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
                clearance_z: 20.0,
                retract_z: 15.0,
                top_z: 10.0,
                bottom_z: 8.0,
                stepdown: 2.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
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
                clearance_z: 25.0,
                cut_z: 4.5,
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
        clearance_z: 10.0,
        retract_z: 8.0,
        top_z: 4.5,
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
