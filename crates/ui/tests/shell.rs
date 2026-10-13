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
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 0,
        });
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
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 0,
        });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still stock");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Box { dx, dy, dz }),
        "double fit must be idempotent"
    );
}

#[test]
fn cylinder_fit_keeps_the_cylinder_choice() {
    // Feedback 4: fit ignored the selected shape and rebuilt the stock as a
    // box, after which the pane snapped the radio back to Box - the cylinder
    // looked broken in both Apply-then-Fit and plain Fit flows.
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

    state.stock_ui.shape = 1; // the user picks "Cylinder" in the pane
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 1,
        });
    state.drain_actions();
    assert_eq!(state.status, "stock fitted to models");
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock");
    let Some(StockShape::Cylinder { diameter, height }) = model::stock_def_of(&state.world, &stock)
    else {
        panic!("cylinder fit must produce cylinder stock");
    };
    let bounds = model::solid_world_aabb(&state.world).expect("model extent");
    let diag = ((bounds.max.x - bounds.min.x) + 4.0).hypot((bounds.max.y - bounds.min.y) + 4.0);
    let eps = 1e-9;
    assert!(diameter >= diag - eps, "{diameter} must reach the padded diagonal");
    assert!(
        height >= (bounds.max.z - bounds.min.z) + 4.0 - eps,
        "{height}"
    );

    // And the draft must agree with the world after the resync, so the
    // radio never silently reverts.
    state.sync_setup_ui();
    assert_eq!(state.stock_ui.shape, 1, "radio stays on Cylinder");
    assert!((state.stock_ui.diameter - diameter).abs() < eps);

    // Double cylinder fit stays idempotent (stock excluded from the bounds).
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 1,
        });
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("still stock");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Cylinder { diameter, height }),
        "double cylinder fit must be idempotent"
    );

    // Undo unwinds one fit at a time, restoring the exact parameters: the
    // first fit created the stock, so the second undo removes it.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let stock = model::find_by_name(&state.world, STOCK_NAME).expect("stock after one undo");
    assert_eq!(
        model::stock_def_of(&state.world, &stock),
        Some(StockShape::Cylinder { diameter, height }),
        "undo of an idempotent re-fit leaves the same cylinder"
    );
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(
        model::find_by_name(&state.world, STOCK_NAME).is_none(),
        "undoing the creating fit removes the stock"
    );
}

#[test]
fn from_model_stock_refuses_fit_with_a_clear_status() {
    use hsmworks_app::model;
    use hsmworks_app::schema::STOCK_NAME;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 2,
        });
    state.drain_actions();
    assert!(
        state.status.contains("follows its source"),
        "{:?}",
        state.status
    );
    assert!(!state.history.can_undo());
    assert!(model::find_by_name(&state.world, STOCK_NAME).is_none());
}

#[test]
fn stock_fit_without_models_is_a_polite_status() {
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state
        .pending
        .push_back(CommandKind::StockFit {
            margin: 2.0,
            shape: 0,
        });
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
fn anchor_grid_places_origin_on_the_stock_box() {
    // A grid click in the pane boils down to these two helpers feeding the
    // existing SetWcs command; pin their contract end to end.
    use hsmworks_app::model;
    use hsmworks_app::stock::StockShape;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.pending.push_back(box_stock(
        StockShape::Box {
            dx: 60.0,
            dy: 40.0,
            dz: 20.0,
        },
        [10.0, -5.0, 0.0],
    ));
    state.drain_actions();
    let b = model::stock_world_aabb(&state.world).expect("stock box");
    assert_eq!((b.min.x, b.min.y, b.min.z), (-20.0, -25.0, 0.0));
    assert_eq!((b.max.x, b.max.y, b.max.z), (40.0, 15.0, 20.0));

    // "NE / Top": max X, max Y, max Z of the stock box.
    let p = model::anchor_point(&b, [2, 2, 2]);
    state.pending.push_back(CommandKind::SetWcs {
        slot: "G54".into(),
        pos: p,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let g54 = model::find_by_name(&state.world, "G54").expect("origin");
    let loc = model::transform_of(&state.world, &g54).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (40.0, 15.0, 20.0));

    // Undo removes the origin; undoing once more removes the stock, and
    // then there is no anchor box at all.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, "G54").is_none());
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::stock_world_aabb(&state.world).is_none());
}

#[test]
fn origin_undo_leaves_no_stale_readout() {
    use hsmworks_app::model;
    use hsmworks_ui::state::{CommandKind, State};

    let mut state = State::new();
    state.wcs_ui.slot = 1; // G55
    state.pending.push_back(CommandKind::SetWcs {
        slot: "G55".into(),
        pos: [12.0, 3.0, -1.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    state.sync_setup_ui();
    assert_eq!(state.wcs_ui.x, 12.0);
    assert_eq!(state.wcs_ui.y, 3.0);
    assert_eq!(state.wcs_ui.z, -1.0);

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    state.sync_setup_ui();
    assert!(model::find_by_name(&state.world, "G55").is_none());
    assert_eq!(
        (state.wcs_ui.x, state.wcs_ui.y, state.wcs_ui.z),
        (0.0, 0.0, 0.0),
        "the readout follows reality after an undo"
    );
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
