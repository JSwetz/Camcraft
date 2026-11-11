//! #692 step 2: the one pick rule as the pages call it - a pick resolved
//! to the selection a page stores (vertex, chain / run / segment, face),
//! the screen-size radius held at a miss, the per-allocation graph cache a
//! per-frame hover leans on, the rule read backwards for picks made by id
//! (MCP), and only the active setup's stock as a pick body. Plus the
//! timing lane on the largest testdata model.
//!
//! Gate map (#692): covers app::pick::feature_sel, app::pick::feature_pick
//! (radius_per_depth, pick_bodies_of), app::pick::feature_graph
//! (feature_graph_shared).

use std::sync::Arc;
use std::time::Instant;

use camcraft_app::commands::{Command, ImportModel};
use camcraft_app::geometry::{Point3, Triangle, cube, cylinder, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{
    EdgeUnit, PickBody, PickKinds, PickQuery, PickRay, PickTarget, Projection, check_pickable, feature_graph_shared,
    graph_of, pick_bodies, pick_selection,
};
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;

fn aim(eye: [f64; 3], at: [f64; 3], kinds: PickKinds) -> PickQuery {
    let dir = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let ray = PickRay { origin: eye, dir, projection: Projection::Perspective };
    PickQuery { ray, radius: 1.0, radius_per_depth: 0.0, kinds, through: false }
}

fn block_world() -> flecs_ecs::prelude::World {
    let world = new_world();
    ImportModel::from_mesh("block", cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0))).apply(&world);
    world
}

#[test]
fn a_pick_resolves_to_the_selection_its_page_stores() {
    let world = block_world();
    let g = graph_of(&world, "block").expect("graph");
    let f = &*g.features;
    // A corner: the vertex id.
    let (pick, sel) =
        pick_selection(&world, &aim([60.0, -40.0, 40.0], [20.0, 0.0, 10.0], PickKinds::POINTS), EdgeUnit::Chain, |_| false)
            .expect("the corner");
    let Some(FeatureSel::Vertex(id)) = sel else { panic!("a point is a vertex: {sel:?}") };
    assert!(feature::resolve_vertex(f, id).is_some(), "it resolves as stored");
    assert!(matches!(pick.target, PickTarget::Point(_)));
    // The top rim's middle, y = 0: chain, run and segment.
    let q = aim([10.0, -40.0, 40.0], [10.0, 0.0, 10.0], PickKinds::EDGES);
    let edge_of = |unit| pick_selection(&world, &q, unit, |_| false).expect("the rim");
    let (pick, chain) = edge_of(EdgeUnit::Chain);
    let PickTarget::Edge { edge } = pick.target else { panic!("an edge: {pick:?}") };
    let Some(FeatureSel::Chain(c)) = chain else { panic!("{chain:?}") };
    assert!(feature::resolve_chain(f, c).is_some_and(|c| c.edges.contains(&edge)));
    let (_, run) = edge_of(EdgeUnit::Run);
    assert_eq!(run, feature::run_containing(f, edge).map(|r| FeatureSel::Chain(r.id)));
    let (_, seg) = edge_of(EdgeUnit::Segment);
    assert_eq!(seg, feature::singleton_chain_id(f, edge).map(FeatureSel::Chain));
    // The top face's middle: the planar face key.
    let (_, face) =
        pick_selection(&world, &aim([10.0, 10.0, 60.0], [10.0, 10.0, 10.0], PickKinds::FACES), EdgeUnit::Chain, |_| false)
            .expect("the top");
    assert!(matches!(face, Some(FeatureSel::Face(_))), "{face:?}");
}

#[test]
fn the_per_depth_radius_holds_at_a_miss() {
    // A ray passing 1 mm beside the block's top-front rim, missing the
    // block: the anchored radius has no surface to anchor to, the per-depth
    // one is one screen size at the rim's own depth.
    let tris: Arc<[Triangle]> = Arc::from(cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0)).facets.to_vec());
    let bodies = [PickBody::new("block", tris, Location::default())];
    let mut q = aim([10.0, -100.0, 11.0], [10.0, 0.0, 11.0], PickKinds::EDGES); // depth 100, 1 mm over the rim
    q.radius = 0.5;
    assert_eq!(pick_bodies(&bodies, &q), None, "0.5 mm misses a rim 1 mm off");
    q.radius_per_depth = 0.02; // 2 mm at depth 100
    assert!(pick_bodies(&bodies, &q).is_some_and(|p| matches!(p.target, PickTarget::Edge { .. })));
    q.radius_per_depth = 0.005; // 0.5 mm at depth 100
    assert_eq!(pick_bodies(&bodies, &q), None);
    q.radius_per_depth = f64::NAN; // refused, falls back to `radius`
    assert_eq!(pick_bodies(&bodies, &q), None);
}

#[test]
fn the_shared_graph_cache_answers_the_same_allocation_without_rebuilding() {
    let tris: Arc<[Triangle]> = Arc::from(cylinder(20.0, 10.0, 16).facets.to_vec());
    let a = feature_graph_shared(&tris);
    let b = feature_graph_shared(&tris);
    assert!(Arc::ptr_eq(&a, &b), "the same allocation is one graph");
    let copy: Arc<[Triangle]> = Arc::from(tris.to_vec());
    let c = feature_graph_shared(&copy);
    assert_eq!(c.edges, a.edges, "equal content, equal graph");
    assert!(feature_graph_shared(&Arc::from(Vec::<Triangle>::new())).edges.is_empty());
}

#[test]
fn a_pick_by_id_must_be_one_a_click_could_make() {
    let world = new_world();
    ImportModel::from_mesh("boss", cylinder(20.0, 10.0, 16)).apply(&world);
    let e = model::find_by_name(&world, "boss").expect("entity");
    let f = model::features_of(&world, &e).expect("features");
    let g = graph_of(&world, "boss").expect("graph");
    let pick = |feature| GeoSel { entity: "boss".into(), feature };
    // A rim vertex is an end of a feature edge but no feature point.
    let rim = feature::corner_verts(&f)
        .into_iter()
        .filter_map(|v| feature::vertex_id(&f, v))
        .find(|id| !g.points.iter().any(|p| p.id == *id))
        .expect("a vertex along the rim");
    let msg = check_pickable(&world, &pick(FeatureSel::Vertex(rim))).expect_err("a rim vertex is no point");
    assert!(msg.contains("not a feature point"), "{msg}");
    // A wall seam chain is refused; the rim chain is taken.
    let chains = feature::chains(&f);
    let seam = chains.iter().find(|c| !camcraft_app::pick::is_feature_chain(&g, c)).expect("a seam chain");
    let msg = check_pickable(&world, &pick(FeatureSel::Chain(seam.id))).expect_err("a seam is no edge");
    assert!(msg.contains("not a model edge"), "{msg}");
    let rim_chain = camcraft_app::pick::feature_chains(&g).into_iter().next().expect("a rim chain");
    assert!(rim_chain.closed, "the rim is a closed loop");
    assert_eq!(check_pickable(&world, &pick(FeatureSel::Chain(rim_chain.id))), Ok(()));
    // Faces always qualify; a stale id is left to the command.
    assert_eq!(check_pickable(&world, &pick(FeatureSel::Face(1))), Ok(()));
    assert_eq!(check_pickable(&world, &pick(FeatureSel::Vertex(1))), Ok(()));
    assert_eq!(check_pickable(&world, &GeoSel { entity: "ghost".into(), feature: FeatureSel::Vertex(rim) }), Ok(()));
}

/// 400 hover picks over a 20 x 20 grid across `mesh`'s bounds, per kind
/// set, at a 12 px radius on a 720 px pane; printed, never asserted.
fn time_hover(label: &str, mesh: camcraft_app::geometry::Mesh) {
    let n = mesh.facets.len();
    let world = new_world();
    ImportModel::from_mesh("part", mesh).apply(&world);
    let e = model::find_by_name(&world, "part").expect("entity");
    let tris = model::facets_of(&world, &e).expect("facets");
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for t in tris.iter() {
        for p in [t.a, t.b, t.c] {
            for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
    }
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let span = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    let eye = [c[0] + span, c[1] - span, c[2] + span];
    let per_depth = 12.0 * 2.0 * std::f64::consts::FRAC_PI_6.tan() / 720.0; // 12 px on a 720 px pane
    let t0 = Instant::now();
    let g = graph_of(&world, "part").expect("graph");
    let cold = t0.elapsed();
    println!("{label}: {n} facets, {} feature edges, {} points; cold graph {cold:?}", g.edges.len(), g.points.len());
    let edges_faces = PickKinds { faces: true, ..PickKinds::EDGES };
    for (kind, kinds) in
        [("faces", PickKinds::FACES), ("edges+faces", edges_faces), ("points", PickKinds::POINTS), ("all", PickKinds::ALL)]
    {
        let (mut total, mut worst, mut hits) = (std::time::Duration::ZERO, std::time::Duration::ZERO, 0);
        for i in 0..20 {
            for j in 0..20 {
                let at = [lo[0] + (hi[0] - lo[0]) * f64::from(i) / 19.0, lo[1] + (hi[1] - lo[1]) * f64::from(j) / 19.0, c[2]];
                let mut q = aim(eye, at, kinds);
                q.radius_per_depth = per_depth;
                let t = Instant::now();
                hits += usize::from(pick_selection(&world, &q, EdgeUnit::Chain, |_| false).is_some());
                let dt = t.elapsed();
                total += dt;
                worst = worst.max(dt);
            }
        }
        println!("  {kind:>12}: 400 hover picks, mean {:?}, worst {worst:?}, {hits} hits", total / 400);
    }
}

#[test]
#[ignore = "timing lane (#692 step 2): cargo test --release -p camcraft-app --test feature_pick_wire -- --ignored --nocapture"]
fn hover_pick_timing_on_the_largest_testdata_model() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/sample_ap214.step");
    time_hover("sample_ap214.step", camcraft_app::geometry::load_model(&path).expect("sample_ap214 imports"));
    // The testdata models are small; a dense smooth wall is the stress.
    time_hover("cylinder x25000", cylinder(80.0, 40.0, 25_000));
}
