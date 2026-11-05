//! Area control (M11.5), the open boundary and the laws around it: an
//! open chain splits the stock and either side is faced from the air up
//! to it, never across it (rows laid along the chain); a containment
//! that would put the tool in the part is stopped and the depth-aware
//! verify of what is left is silent; a facing band across part material
//! is refused by name (and the default law is caught by the verify); the
//! controls ride one undoable edit, stamp only when set, round-trip and
//! are absent from the file at their default.
//!
//! Gate map (M11.5): covers app::operation (boundary, boundary_turn,
//! boundary_ctx, stamp, generate), app::commands (EditOperationParams),
//! app::project (area), app::verify.

mod area_support;

use area_support::*;

/// Face the block's top under an open boundary on one of its rim edges.
fn face_open(world: &World, edge: FeatureSel, side: ChainSide) -> Toolpath {
    let top = top_face(world, "part", 10.0);
    let a = AreaControls { open: Some(OpenBoundary { picks: pick("part", edge), side }), ..Default::default() };
    let (t, warns) = run(world, &op(sel("part", top), face(10.0), a)).expect("open-boundary face");
    assert_eq!(warns, 0, "{:?}", t.notes);
    t
}

/// The first plunge stands in air outside the stock (the tool's whole
/// body past an X or Y face of the box).
fn starts_in_air(t: &Toolpath, lo: Point3, hi: Point3) -> bool {
    let Some(p) = t.moves.iter().find(|m| m.kind == MoveKind::Plunge).map(|m| m.to) else {
        return false;
    };
    p.x + R <= lo.x + EPS || p.x - R >= hi.x - EPS || p.y + R <= lo.y + EPS || p.y - R >= hi.y - EPS
}

#[test]
fn an_open_chain_splits_the_stock_and_either_side_is_faced_from_the_air_up_to_it() {
    let world = block_world();
    let (lo, hi) = stock_box(&world);
    // The rim edge along y = +20: the block's side is Inside.
    let north = rim_edge(&world, |q| (q[1] - 20.0).abs() < 1e-9);
    let t = face_open(&world, north, ChainSide::Inside);
    let f = feeds(&t);
    assert!(starts_in_air(&t, lo, hi), "the path starts outside the stock");
    assert!(f.iter().all(|p| p.y + R <= 20.0 + EPS), "never across the chain");
    assert!((max_by(&f, |p| p.y) + R - 20.0).abs() < EPS, "works up to the chain");
    assert!((max_by(&f, |p| -p.y) + R + lo.y).abs() < EPS, "from the far stock edge");
    let first_row = f.first().map(|p| p.y).unwrap_or(f64::NAN);
    assert!(first_row < 0.0, "the band starts at the far edge and works in ({first_row})");
    // The other side: the strip between the chain and the stock edge.
    let t = face_open(&world, north, ChainSide::Outside);
    let f = feeds(&t);
    assert!(starts_in_air(&t, lo, hi));
    assert!(f.iter().all(|p| p.y - R >= 20.0 - EPS), "never across the chain");
    assert!((max_by(&f, |p| -p.y) + R + 20.0).abs() < EPS, "works down to the chain");
    assert!((max_by(&f, |p| p.y) + R - hi.y).abs() < EPS);
    let first_row = f.first().map(|p| p.y).unwrap_or(f64::NAN);
    assert!(first_row > 25.0 + EPS, "far edge first ({first_row})");
    // A chain along Y turns the rows to run with it.
    let east = rim_edge(&world, |q| (q[0] - 20.0).abs() < 1e-9);
    let t = face_open(&world, east, ChainSide::Outside);
    let f = feeds(&t);
    assert!(starts_in_air(&t, lo, hi));
    assert!(f.iter().all(|p| p.x - R >= 20.0 - EPS), "never across the chain");
    assert!((max_by(&f, |p| -p.x) + R + 20.0).abs() < EPS, "up to the chain");
}

#[test]
fn a_containment_that_puts_the_tool_in_the_part_is_caught_by_the_depth_aware_verify() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let world = world_with(Some(ImportModel::new(&path).expect("STEP")), -7.0, 27.0);
    let name = model::named_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| n != "Stock" && n != TOOL)
        .expect("part");
    // The bore's rim at the mouth: a closed chain of radius 12.5.
    let e = model::find_by_name(&world, &name).expect("model");
    let f = model::features_of(&world, &e).expect("features");
    let rim = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            c.closed && p.iter().all(|q| (q[2] - 20.0).abs() < 1e-6 && (q[0].hypot(q[1]) - 12.5).abs() < 0.01)
        })
        .map(|c| FeatureSel::Chain(c.id))
        .expect("the bore rim");
    let p = contour(20.0, 10.0, Side::Auto);
    let (_, clean) = run(&world, &op(sel(&name, rim), p.clone(), AreaControls::default())).expect("default");
    assert_eq!(clean, 0, "the bore wall contour is clean by default");
    let out = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    let (t, warns) = run(&world, &op(sel(&name, rim), p, out)).expect("generated, then judged");
    // The ring 10 mm into the wall: the part-aware walk stops it at the
    // mouth (named), and the depth-aware verify of what is left is
    // silent - the tool never enters the part.
    assert_eq!(warns, 0, "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("levels below skipped")), "{:?}", t.notes);
    assert!(feeds(&t).iter().all(|q| q.z >= 20.0 - 1e-9 || q.x.hypot(q.y) <= 12.5 - R + EPS));
}

/// A 40 x 40 x 5 plate with a 10 x 10 boss standing to Z 10, in a
/// 70 x 70 x 10 stock.
fn boss_world() -> World {
    let mut f = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 5.0)).facets.to_vec();
    f.extend(cube(Point3::new(-5.0, -5.0, 5.0), Point3::new(5.0, 5.0, 10.0)).facets.iter().cloned());
    world_with(Some(ImportModel::from_mesh("part", camcraft_app::geometry::Mesh::from_facets(f))), 0.0, 10.0)
}

#[test]
fn a_face_band_across_part_material_is_refused_by_name_and_caught_by_the_verify() {
    let world = boss_world();
    let plate = top_face(&world, "part", 5.0);
    // The default law faces the plate's whole band - through the boss;
    // the depth-aware verify says so (the post gate's backstop).
    let (_, warns) = run(&world, &op(sel("part", plate), face(5.0), AreaControls::default())).expect("default");
    assert!(warns > 0, "facing through the boss must be flagged");
    // Under Boundary settings the same band is refused before a move.
    let a = AreaControls { extension: 3.0, ..Default::default() };
    let err = run(&world, &op(sel("part", plate), face(5.0), a)).expect_err("refused");
    assert!(err.contains("part material standing above Cut Z"), "{err}");
}

#[test]
fn the_controls_ride_one_undoable_edit_and_stamp_only_when_set() {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "P", TOOL, SrcRef::Stock, pocket(8.0, 4.0)).expect("create"));
    let e = model::find_by_name(&world, "P").expect("op");
    let o0 = model::operation_of(&world, &e).expect("op");
    assert_eq!(
        operation::op_stamp(&world, &o0, "P"),
        operation::current_stamp(&world, TOOL, &o0.src, &o0.params, "P", None),
        "the identity stamps the bytes it always did"
    );
    let a = AreaControls { containment: Some(Containment::On), offset: 2.0, ..Default::default() };
    h.execute(&world, EditOperationParams::with_area(&world, "P", o0.params.clone(), a.clone()).expect("edit"));
    let o1 = model::operation_of(&world, &e).expect("op");
    assert_eq!(o1.area, a);
    assert_ne!(operation::op_stamp(&world, &o1, "P"), operation::op_stamp(&world, &o0, "P"));
    assert!(h.undo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, AreaControls::default());
    // Door refusals: a non-finite distance; controls on a kind with no area.
    let bad = AreaControls { offset: f64::NAN, ..Default::default() };
    assert!(EditOperationParams::with_area(&world, "P", o0.params.clone(), bad).is_err());
    let drill = operation::retype_params(&o0.params, OpType::Drill as usize);
    let set = AreaControls { offset: 1.0, ..Default::default() };
    assert!(!set.is_sane_for(&drill) && AreaControls::default().is_sane_for(&drill));
    assert!(set.is_sane_for(&o0.params));
}

#[test]
fn the_controls_round_trip_and_are_absent_from_the_file_at_their_default() {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "P", TOOL, SrcRef::Stock, pocket(8.0, 4.0)).expect("create"));
    let dir = std::env::temp_dir().join(format!("camcraft-area-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let file = dir.join("default.camproj");
    project::save(&world, &file).expect("save");
    let text = std::fs::read_to_string(&file).expect("read");
    assert!(!text.contains("\"area\""), "an untouched op writes no area key");
    let top = top_face(&world, "part", 10.0);
    let north = rim_edge(&world, |q| (q[1] - 20.0).abs() < 1e-9);
    let a = AreaControls {
        containment: Some(Containment::Outside),
        offset: -1.5,
        corner: CornerStyle::Sharp,
        extension: 2.0,
        extra: vec![
            ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(pick("part", top)) },
            ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Silhouette },
        ],
        open: Some(OpenBoundary { picks: pick("part", north), side: ChainSide::Outside }),
    };
    let e = model::find_by_name(&world, "P").expect("op");
    let params = model::operation_of(&world, &e).expect("op").params;
    h.execute(&world, EditOperationParams::with_area(&world, "P", params, a.clone()).expect("edit"));
    let file = dir.join("set.camproj");
    project::save(&world, &file).expect("save");
    let back = new_world();
    project::load(&back, &file).expect("load");
    let e = model::find_by_name(&back, "P").expect("op");
    assert_eq!(model::operation_of(&back, &e).expect("op").area, a);
    let again = dir.join("again.camproj");
    project::save(&back, &again).expect("save");
    assert_eq!(std::fs::read(&file).expect("a"), std::fs::read(&again).expect("b"), "save-load-save is stable");
    let _ = std::fs::remove_dir_all(&dir);
}
