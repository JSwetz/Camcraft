//! #330 review on PR #346, narrowed by #554: the plane is set by a face
//! only - an edge cannot define a plane, so the two-edge mode is gone and
//! every edge pick (alone, in pairs that used to lay a plane, or on two
//! bodies) is refused by name, pointing at the axis rows (#557). Still
//! pinned: a concave part's step face lays outward up from its facet
//! winding (never from the part's centre), a degenerate face is refused,
//! and a part tilted upside down lays its face with the antiparallel half
//! turn.
//! Provenance: our own fixtures over plain datum-face practice; no vendor
//! material consulted.
//!
//! Gate map (#330, #554): covers app::pick::plane (face side, edge refusals).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{CreateSetup, History, ImportModel, MoveModel};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, feature, quat_from_euler, quat_rotate_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{PlaneLay, lay_on_plane};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::world::new_world;

const AN_EDGE: &str = "an edge cannot set a plane; pick a flat face";

/// An L-bracket: the profile (x, z) (0,0) (40,0) (40,t) (t,t) (t,40)
/// (0,40), extruded 20 along Y. Its inner step is the face z = t,
/// x in t..40, outward +Z.
fn l_bracket(t: f64) -> Mesh {
    let prof = [(0.0, 0.0), (40.0, 0.0), (40.0, t), (t, t), (t, 40.0), (0.0, 40.0)];
    let p = |i: usize, y: f64| {
        let (x, z) = prof[i % prof.len()];
        Point3::new(x, y, z)
    };
    let mut f = Vec::new();
    // Caps: a fan from the reflex corner (t, t) - the L is star-shaped
    // about it. CCW in (x, z) is outward -Y at y = 0.
    for i in [4, 5, 0, 1] {
        f.push(Triangle::new(p(3, 0.0), p(i, 0.0), p(i + 1, 0.0)));
        f.push(Triangle::new(p(3, 20.0), p(i + 1, 20.0), p(i, 20.0)));
    }
    for i in 0..prof.len() {
        let (a, b, c, d) = (p(i + 1, 0.0), p(i, 0.0), p(i, 20.0), p(i + 1, 20.0));
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    Mesh::from_facets(f)
}

fn world_of(parts: Vec<(&str, Mesh)>, tilt: [f64; 3]) -> World {
    let world = new_world();
    let mut h = History::default();
    for (name, mesh) in parts {
        h.execute(&world, ImportModel::from_mesh(name, mesh));
        if tilt != [0.0; 3] {
            let [x, y, z] = tilt.map(f64::to_radians);
            let loc = Location { pos: Point3::new(5.0, -3.0, 2.0), rot: quat_from_euler(x, y, z) };
            h.execute(&world, MoveModel::new(&world, name, loc).expect("tilt"));
        }
    }
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    world
}

fn features(world: &World, name: &str) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(world, name).expect("entity");
    model::features_of(world, &e).expect("features")
}

/// An edge of `name` by its LOCAL ends.
fn edge(world: &World, name: &str, a: [f64; 3], b: [f64; 3]) -> GeoSel {
    let f = features(world, name);
    let id = feature::chains(&f)
        .into_iter()
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            (p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))).then_some(c.id)
        })
        .expect("the part has that edge");
    GeoSel { entity: name.into(), feature: FeatureSel::Chain(id) }
}

/// A local direction of `name` as the alignment `a` lays it.
fn laid_dir(world: &World, name: &str, a: Location, local: [f64; 3]) -> [f64; 3] {
    let e = model::find_by_name(world, name).expect("part");
    let loc = setup_align::compose(a, model::transform_of(world, &e).unwrap_or_default());
    let v = quat_rotate_point(loc.rot, Point3::new(local[0], local[1], local[2]));
    [v.x, v.y, v.z]
}

/// `v` (unit) lies within 1e-9 rad (by the sine) of `axis`, same way.
fn on_axis(v: [f64; 3], axis: [f64; 3]) -> bool {
    let d = v[0] * axis[0] + v[1] * axis[1] + v[2] * axis[2];
    let c = [v[1] * axis[2] - v[2] * axis[1], v[2] * axis[0] - v[0] * axis[2], v[0] * axis[1] - v[1] * axis[0]];
    d > 0.0 && (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() <= 1e-9
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];
const PLANES: [(ArcPlane, [f64; 3]); 3] = [(ArcPlane::Xy, Z), (ArcPlane::Xz, Y), (ArcPlane::Yz, X)];

/// The step face (local z = t, x in t..40).
fn step_face(w: &World, t: f64) -> GeoSel {
    let id = features(w, "l")
        .faces
        .iter()
        .find(|f| f.normal[2].abs() > 0.99 && (f.offset.abs() - t).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the step face");
    GeoSel { entity: "l".into(), feature: FeatureSel::Face(id) }
}

fn lays_step_up(t: f64) {
    for tilt in [[0.0; 3], [20.0, 35.0, -50.0]] {
        let w = world_of(vec![("l", l_bracket(t))], tilt);
        let picks = [step_face(&w, t)];
        for (plane, axis) in PLANES {
            for flip in [false, true] {
                let a = lay_on_plane(&w, &picks, PlaneLay { plane, flip, place: false }).expect("laid");
                // The step's outward normal is local +Z (out of the material).
                let n = laid_dir(&w, "l", a, Z);
                let want = if flip { axis.map(|v| -v) } else { axis };
                assert!(on_axis(n, want), "t {t} tilt {tilt:?} {plane:?} flip {flip}: step normal {n:?}");
            }
        }
    }
}

#[test]
fn a_concave_step_face_lays_its_own_outside_up() {
    // The bounds centre (20, 10, 20) sits above the step (z = 5): an
    // away-from-the-centre rule would lay this part upside down.
    let w = world_of(vec![("l", l_bracket(5.0))], [0.0; 3]);
    let c = setup_align::part_centre(&w).expect("a centre");
    assert!(c.z > 5.0, "the fixture is concave over the step: {c:?}");
    lays_step_up(5.0);
}

#[test]
fn a_step_face_whose_plane_runs_through_the_centre_still_lays_its_outside_up() {
    let w = world_of(vec![("l", l_bracket(20.0))], [0.0; 3]);
    let c = setup_align::part_centre(&w).expect("a centre");
    assert!((c.z - 20.0).abs() < 1e-9, "the step's plane runs through the centre: {c:?}");
    lays_step_up(20.0);
}

/// #554: no edge pick sets a plane - one edge, the two edges of the step
/// that the #330 mode used to lay, a face with an edge, or edges on two
/// bodies: each refused by name, and the command writes nothing.
#[test]
fn every_edge_pick_is_refused_by_name() {
    let w = world_of(vec![("l", l_bracket(5.0))], [0.0; 3]);
    let step = [edge(&w, "l", [5.0, 0.0, 5.0], [40.0, 0.0, 5.0]), edge(&w, "l", [40.0, 0.0, 5.0], [40.0, 20.0, 5.0])];
    let face = step_face(&w, 5.0);
    for picks in [vec![step[0].clone()], step.to_vec(), vec![face, step[1].clone()]] {
        let lay = PlaneLay { plane: ArcPlane::Xy, flip: false, place: false };
        let e = lay_on_plane(&w, &picks, lay).expect_err("refused");
        assert!(e.contains(AN_EDGE) && e.contains("is an edge"), "{picks:?}: {e}");
        let cmd = camcraft_app::commands::AlignPartToSetup::lay_on_plane(&w, "Setup1", &picks, lay);
        assert!(cmd.is_err_and(|e| e.to_string().contains(AN_EDGE)), "the command refuses by the same name");
        assert_eq!(setup_align::stored(&w, "Setup1"), None, "a refusal writes nothing");
    }
    let a = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0));
    let b = cube(Point3::new(20.0, 0.0, 0.0), Point3::new(30.0, 10.0, 10.0));
    let w = world_of(vec![("a", a), ("b", b)], [0.0; 3]);
    let picks = [edge(&w, "a", [0.0, 0.0, 10.0], [10.0, 0.0, 10.0]), edge(&w, "b", [30.0, 0.0, 10.0], [30.0, 10.0, 10.0])];
    let e = lay_on_plane(&w, &picks, PlaneLay { plane: ArcPlane::Xy, flip: false, place: false }).expect_err("refused");
    assert!(e.contains(AN_EDGE), "{e}");
}

#[test]
fn a_degenerate_face_is_refused_by_name() {
    let mut f = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)).facets.to_vec();
    // A sliver with no area, off to the side: it spans no plane.
    f.push(Triangle::new(Point3::new(40.0, 0.0, 0.0), Point3::new(41.0, 1.0, 1.0), Point3::new(42.0, 2.0, 2.0)));
    let w = world_of(vec![("p", Mesh::from_facets(f))], [0.0; 3]);
    let id = features(&w, "p")
        .faces
        .iter()
        .find(|f| f.kind == feature::FaceKind::Degenerate)
        .map(|f| f.id)
        .expect("the sliver is a degenerate face");
    let pick = [GeoSel { entity: "p".into(), feature: FeatureSel::Face(id) }];
    let e = lay_on_plane(&w, &pick, PlaneLay { plane: ArcPlane::Xy, flip: false, place: false }).expect_err("refused");
    assert!(e.contains("is degenerate"), "{e}");
}

#[test]
fn a_part_tilted_upside_down_lays_its_top_back_up_with_a_half_turn() {
    // Upside down and turned 25 deg: the top's outward normal is -Z in the
    // document, antiparallel to G17's +Z (the half-turn branch).
    let w = world_of(vec![("p", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)))], [180.0, 0.0, 25.0]);
    let top = features(&w, "p")
        .faces
        .iter()
        .find(|f| f.normal[2].abs() > 0.99 && (f.offset.abs() - 10.0).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the top");
    let pick = [GeoSel { entity: "p".into(), feature: FeatureSel::Face(top) }];
    for flip in [false, true] {
        let a = lay_on_plane(&w, &pick, PlaneLay { plane: ArcPlane::Xy, flip, place: true }).expect("laid");
        let n = laid_dir(&w, "p", a, Z);
        assert!(on_axis(n, if flip { [0.0, 0.0, -1.0] } else { Z }), "flip {flip}: top normal {n:?}");
        let finite = [a.pos.x, a.pos.y, a.pos.z].iter().chain(&a.rot).all(|v| v.is_finite());
        assert!(finite, "flip {flip}: {a:?}");
    }
}
