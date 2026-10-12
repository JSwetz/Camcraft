//! M1.1 acceptance: save/load round-trips are byte-identical; a wrong
//! version or corrupt file is a clean error.

use std::path::PathBuf;

use hsmworks_app::commands::{History, ImportModel, SetStock, SetWcsOrigin};
use hsmworks_app::schema::Location;
use hsmworks_app::stock::StockShape;
use hsmworks_app::geometry::{IDENTITY_Q, cube, Point3};
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
    // Anything newer than this build's format stays refused; v1 migrates
    // (see v1_file_migrates).
    let text = r#"{"version": 3, "entities": []}"#;
    let world = new_world();
    match project::open(&world, text) {
        Err(ProjectError::UnsupportedVersion(3)) => {}
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
