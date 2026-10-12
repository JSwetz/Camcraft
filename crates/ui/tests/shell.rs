//! UI shell smoke tests: real ImGui frames, no GPU, no window.
//! Provenance: our own harness; behavior specced from the shell itself.

#[test]
fn headless_frames_run() {
    hsmworks_ui::headless::run_frames(8)
        .map_err(|err| panic!("headless run failed: {err}"))
        .ok();
}

#[test]
fn headless_cli_exit_code_is_zero() {
    assert_eq!(hsmworks_ui::run(&[String::from("--headless")]), 0);
}

#[test]
fn state_starts_clean() {
    let state = hsmworks_ui::state::State::new();
    assert_eq!(state.status, "ready");
    assert!(!state.has_selection());
    assert!(!state.history.can_undo());
}

#[test]
fn selecting_a_missing_model_is_a_no_op_not_a_crash() {
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(CommandKind::Select(Some("ghost".into())));
    state.drain_actions();
    assert!(!state.has_selection());
}

#[test]
fn camera_frame_all_on_empty_world_is_polite() {
    let mut state = hsmworks_ui::state::State::new();
    state.pending.push_back(hsmworks_ui::state::CommandKind::FrameAll);
    state.drain_actions();
    assert_eq!(state.status, "nothing to frame");
}

#[test]
fn import_completes_on_the_worker_thread() {
    use hsmworks_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle(); // pumps the worker to its result
    assert!(state.status.starts_with("imported cube"), "{:?}", state.status);
    assert!(state.history.can_undo());

    state.pending.push_back(CommandKind::Undo);
    state.settle();
    assert!(!state.history.can_undo());
    assert!(state.history.can_redo());
}

#[test]
fn corrupt_import_errors_on_the_worker_never_panics() {
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    // Dispatch refuses by extension before reading; no IGES kernel exists.
    state
        .pending
        .push_back(CommandKind::ImportFile(std::path::PathBuf::from("x.iges")));
    state.settle();
    assert!(state.status.starts_with("import refused"), "{:?}", state.status);
    assert!(!state.history.can_undo());
}

fn box_stock(
    shape: hsmworks_app::stock::StockShape,
    pos: [f64; 3],
) -> hsmworks_ui::state::CommandKind {
    hsmworks_ui::state::CommandKind::SetStock {
        shape,
        pos,
        yaw_deg: 0.0,
    }
}

#[test]
fn set_stock_creates_the_entity_and_undo_removes_it() {
    use hsmworks_app::model;
    use hsmworks_app::schema::STOCK_NAME;
    use hsmworks_app::stock::StockShape;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 30.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.drain_actions();
    assert_eq!(state.status, "stock set");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("Stock entity");
    assert!(model::is_stock(&state.world, &stock));
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 30.0
        })
    );
    assert_eq!(model::facets_of(&state.world, &stock).map(|t| t.len()), Some(12));

    // Yaw enters through the same queue path and lands as a unit quat.
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Cylinder {
            diameter: 40.0,
            height: 20.0,
        },
        pos: [1.0, 2.0, -3.0],
        yaw_deg: 30.0,
    });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still there");
    let loc = model::transform_of(&state.world, &stock).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (1.0, 2.0, -3.0));
    let (s, c) = 15.0f64.to_radians().sin_cos();
    assert!(
        (loc.rot[2] - s).abs() < 1e-12 && (loc.rot[3] - c).abs() < 1e-12,
        "{:?}",
        loc.rot
    );

    // Undo the replace: the original box parameters come back exactly.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("box stock restored");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 30.0
        })
    );

    // Undo the creation: the entity is gone.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
    assert!(state.history.can_redo());
}

#[test]
fn invalid_stock_parameters_are_reported_not_applied() {
    use hsmworks_app::model;
    use hsmworks_app::schema::STOCK_NAME;
    use hsmworks_app::stock::StockShape;
    use hsmworks_ui::state::State;

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Box {
            dx: f64::NAN,
            dy: 10.0,
            dz: 10.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.drain_actions();
    assert!(state.status.starts_with("stock not set"), "{:?}", state.status);
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
    assert!(!state.history.can_undo(), "a refused command never enters history");
}

#[test]
fn stock_fit_wraps_models_and_double_fit_does_not_grow() {
    use hsmworks_app::model;
    use hsmworks_app::schema::STOCK_NAME;
    use hsmworks_app::stock::StockShape;
    use hsmworks_ui::state::{CommandKind, State};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    state.settle();
    assert!(state.status.starts_with("imported"), "{:?}", state.status);

    let bounds = model::solid_world_aabb(&state.world).expect("model extent");
    state
        .pending
        .push_back(CommandKind::StockFit { margin: 2.0 });
    state.drain_actions();
    assert_eq!(state.status, "stock fitted to models");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock");
    let Some(StockShape::Box { dx, dy, dz }) = model::stock_def_of(&state.world, &stock) else {
        panic!("fit must produce box stock");
    };
    let eps = 1e-9;
    assert!(dx >= (bounds.max.x - bounds.min.x) + 4.0 - eps, "{dx}");
    assert!(dy >= (bounds.max.y - bounds.min.y) + 4.0 - eps, "{dy}");
    assert!(dz >= (bounds.max.z - bounds.min.z) + 4.0 - eps, "{dz}");

    // The stock is not a model: fitting twice must not inflate it.
    state
        .pending
        .push_back(CommandKind::StockFit { margin: 2.0 });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still stock");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box { dx, dy, dz }),
        "double fit must be idempotent"
    );
}

#[test]
fn stock_fit_without_models_is_a_polite_status() {
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state
        .pending
        .push_back(CommandKind::StockFit { margin: 2.0 });
    state.drain_actions();
    assert_eq!(state.status, "nothing to fit: no models in the document");
    assert!(!state.history.can_undo());
}

#[test]
fn set_wcs_creates_offset_undo_removes_and_bad_slot_refuses() {
    use hsmworks_app::model;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(CommandKind::SetWcs {
        slot: "G55".into(),
        pos: [10.0, 5.0, 0.0],
        yaw_deg: 90.0,
    });
    state.drain_actions();
    assert_eq!(state.status, "G55 origin set");
    let g55 = model::find_by_name(&state.world, "G55").expect("offset entity");
    assert!(model::is_wcs(&state.world, &g55));
    assert_eq!(model::facets_of(&state.world, &g55), None, "offsets are not solids");
    let loc = model::transform_of(&state.world, &g55).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (10.0, 5.0, 0.0));

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, "G55").is_none());

    // Not a machine slot on this control: refused, and nothing was deleted.
    state.pending.push_back(CommandKind::SetWcs {
        slot: "G53".into(),
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(state.status.starts_with("offset not set"), "{:?}", state.status);
}

#[test]
fn delete_by_name_round_trips_stock() {
    use hsmworks_app::model;
    use hsmworks_app::schema::STOCK_NAME;
    use hsmworks_app::stock::StockShape;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Cylinder {
            diameter: 30.0,
            height: 10.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.drain_actions();
    state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
    state.drain_actions();
    assert_eq!(state.status, "deleted Stock");
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("restored");
    assert!(model::is_stock(&state.world, &stock), "kind survives delete+undo");
}

#[test]
fn setup_furniture_stays_out_of_the_model_list() {
    use hsmworks_app::model;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        hsmworks_app::stock::StockShape::Box {
            dx: 10.0,
            dy: 10.0,
            dz: 10.0,
        },
        [0.0, 0.0, 0.0],
    ));
    state.pending.push_back(CommandKind::SetWcs {
        slot: "G54".into(),
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    // The exact filter tree.rs renders through.
    let listed: Vec<String> = model::model_entities(&state.world)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(listed.is_empty(), "{listed:?} must not list Stock or G54");
    assert_eq!(model::named_entities(&state.world).len(), 2, "both still in the world");
}
