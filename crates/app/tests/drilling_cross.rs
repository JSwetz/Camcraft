//! M5.2 stage F2 cross-drilling tests at the app level: the
//! document's side of the radial law. What "right" means here: a
//! drill op wearing typed cross holes on a mill-turn document
//! generates Ok with every XY-only verify law silent (Mode::Radial
//! stands them down - the kernel's construction law judges this
//! chain); the cached path carries the pose tags tip-toward-shank
//! and its tips reach the promised depths; the station change
//! traverses outside the stock's silhouette; the three refusals
//! (mixed lists, a plain mill's turret, a bottom plane on radial
//! holes) name themselves; hole recognition over the drilled bar's
//! mesh feeds the same generator through `cross_holes_of`; and the
//! list rides the project file - a pure-Z document never grows the
//! `cross` key. The kernel's own laws are pinned one crate down
//! (camcraft-motion's drilling cross suite); this file is the
//! document.
//!
//! Gate map (M4.4): covers camcraft_app::operation (the cross arm of
//! generate, the region seam for posed plunges, cross_holes_of),
//! camcraft_app::commands (Create/GenerateOperation,
//! EditOperationParams undo), camcraft_app::machine (the turret
//! door), camcraft_app::project (the v40 `cross` grammar).
//! Lane shape (M4.4): all tests here run in seconds.

use std::path::PathBuf;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel,
    LoadMachine, SetStock,
};
use camcraft_app::drilling::CrossHole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_app::cl::MoveKind;
use camcraft_app::drilling::CycleChoice;

const TAU: f64 = std::f64::consts::TAU;

fn catalog_tool(name: &str) -> Tool {
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

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    camcraft_app::operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

// ---- the fixture: the drilled bar, closed and punched ----

/// The point at along-coordinate `s` and transverse (`pu`, `pv`) on
/// axes `u`, `v` - the same backward-`coord` builder the motion
/// cross suite welds its walls with.
fn sp(along: usize, u: usize, v: usize, s: f64, pu: f64, pv: f64) -> Point3 {
    let mut c = [0.0; 3];
    c[along] = s;
    c[u] = pu;
    c[v] = pv;
    Point3::new(c[0], c[1], c[2])
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3) -> Vec<Triangle> {
    vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

fn circle(cu: f64, cv: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|k| {
            let a = TAU * (k as f64) / (n as f64);
            (cu + r * a.cos(), cv + r * a.sin())
        })
        .collect()
}

fn circle_cw(cu: f64, cv: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    let mut c = circle(cu, cv, r, n);
    c.reverse();
    c
}

fn wall_ring(along: usize, u: usize, v: usize, poly: &[(f64, f64)], s0: f64, s1: f64) -> Vec<Triangle> {
    let (s0, s1) = if matches!((u, v, along), (0, 1, 2) | (1, 2, 0) | (2, 0, 1)) {
        (s0, s1)
    } else {
        (s1, s0)
    };
    let mut f = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let a = sp(along, u, v, s0, poly[i].0, poly[i].1);
        let b = sp(along, u, v, s0, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
        let c = sp(along, u, v, s1, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
        let d = sp(along, u, v, s1, poly[i].0, poly[i].1);
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    f
}

#[allow(clippy::too_many_arguments)] // the frame-permutation builders: one slot per axis, every call site fills all eight
fn disc(along: usize, u: usize, v: usize, cu: f64, cv: f64, r: f64, n: usize, s: f64) -> Vec<Triangle> {
    let p = circle(cu, cv, r, n);
    let ctr = sp(along, u, v, s, cu, cv);
    (0..n)
        .map(|i| {
            Triangle::new(
                ctr,
                sp(along, u, v, s, p[i].0, p[i].1),
                sp(along, u, v, s, p[(i + 1) % n].0, p[(i + 1) % n].1),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)] // same law as `disc`, plus the rounded-rect hole's own radii
fn annulus(
    along: usize,
    u: usize,
    v: usize,
    cu: f64,
    cv: f64,
    hu: f64,
    hv: f64,
    r: f64,
    n: usize,
    s: f64,
) -> Vec<Triangle> {
    let q = |k: usize| -> (f64, f64) {
        let a = TAU * (k as f64) / (n as f64);
        let (co, si) = (a.cos(), a.sin());
        let t = (co.abs() / hu).max(si.abs() / hv);
        (cu + co / t, cv + si / t)
    };
    let c = |k: usize| -> (f64, f64) {
        let a = TAU * (k as f64) / (n as f64);
        (cu + r * a.cos(), cv + r * a.sin())
    };
    let mut f = Vec::new();
    for k in 0..n {
        let qa = sp(along, u, v, s, q(k).0, q(k).1);
        let qb = sp(along, u, v, s, q(k + 1).0, q(k + 1).1);
        let ca = sp(along, u, v, s, c(k).0, c(k).1);
        let cb = sp(along, u, v, s, c(k + 1).0, c(k + 1).1);
        f.push(Triangle::new(qa, ca, cb));
        f.push(Triangle::new(qa, cb, qb));
    }
    f
}

/// The F2 bar: x in [-15, 15], y in [-5, 5], z in [-15, 15], with a
/// BLIND X hole at z = 10 (floor x = 5) and a THROUGH X hole at
/// z = -10. The motion cross suite's port, closed: every face is
/// one surface - the two z-bands the holes occupy are punched
/// annuli, everywhere else solid quads - because the document
/// silhouettes it by slicing, and a coplanar double surface is a
/// non-manifold contact to the slicer even when the ray laws ride
/// it. Same walls, floors, and holes; honest caps.
fn radial_bar() -> Vec<Triangle> {
    let p = Point3::new;
    let mut t = Vec::new();
    // The four prismatic faces, band by band on the x-caps.
    t.extend(quad(p(-15.0, 5.0, -15.0), p(-15.0, 5.0, 15.0), p(15.0, 5.0, 15.0), p(15.0, 5.0, -15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(15.0, -5.0, -15.0), p(15.0, -5.0, 15.0), p(-15.0, -5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, 15.0), p(15.0, -5.0, 15.0), p(15.0, 5.0, 15.0), p(-15.0, 5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(-15.0, 5.0, -15.0), p(15.0, 5.0, -15.0), p(15.0, -5.0, -15.0)));
    // -X face: punched once in the through hole's band, solid
    // across everything else (the blind hole never reaches it).
    t.extend(quad(p(-15.0, -5.0, -5.0), p(-15.0, -5.0, 15.0), p(-15.0, 5.0, 15.0), p(-15.0, 5.0, -5.0)));
    t.extend(annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, -15.0));
    // +X face: punched where each hole meets it, solid between.
    t.extend(annulus(0, 1, 2, 0.0, 10.0, 5.0, 5.0, 2.0, 24, 15.0));
    t.extend(annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, 15.0));
    t.extend(quad(p(15.0, -5.0, -5.0), p(15.0, 5.0, -5.0), p(15.0, 5.0, 5.0), p(15.0, -5.0, 5.0)));
    // The bores: blind wall + floor, through wall.
    t.extend(wall_ring(0, 1, 2, &circle_cw(0.0, 10.0, 2.0, 24), 5.0, 15.0));
    t.extend(disc(0, 1, 2, 0.0, 10.0, 2.0, 24, 5.0));
    t.extend(wall_ring(0, 1, 2, &circle_cw(0.0, -10.0, 2.0, 24), -15.0, 15.0));
    t
}

fn cross(origin: Point3, dir: Point3, radius: f64, floor_d: f64, through: bool) -> CrossHole {
    CrossHole { origin, dir, radius, entry_d: 0.0, floor_d, through }
}

fn nx() -> Point3 {
    Point3::new(-1.0, 0.0, 0.0)
}

fn ny() -> Point3 {
    Point3::new(0.0, -1.0, 0.0)
}

/// The two typed holes the bar actually carries.
fn typed() -> Vec<CrossHole> {
    vec![
        cross(Point3::new(15.0, 0.0, 10.0), nx(), 2.0, -10.0, false),
        cross(Point3::new(15.0, 0.0, -10.0), nx(), 2.0, -30.0, true),
    ]
}

fn cross_params(cross: Vec<CrossHole>, bottom_z: Option<Height>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(20.0),
        retract_z: None,
        top_z: Height::fixed(15.0),
        bottom_z,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![],
        cross,
    }
}

fn cross_world(word: &str) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 44.0, dy: 44.0, dz: 44.0 },
            Location::default(),
        )
        .expect("box stock"),
    );
    let cmd = LoadMachine::new(&world, "bench.mch".into(), Some(machine(word)));
    history.execute(&world, cmd.expect("machine"));
    add_tool(&world, &mut history, "DR-0400");
    history.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(radial_bar())));
    (world, history)
}

fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, "DR-0400", SrcRef::Entity("bar".into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// Max hypot over the bar's vertices - the silhouette radius the
/// kernel's traverse law stands off from.
fn stock_r() -> f64 {
    radial_bar()
        .iter()
        .flat_map(|f| [f.a, f.b, f.c])
        .map(|p| (p.x * p.x + p.y * p.y).sqrt())
        .fold(0.0_f64, f64::max)
}

// ---- the document's life ----

#[test]
fn a_cross_drill_op_runs_clean_in_the_document() {
    // Typed blind + through holes on a mill-turn: Ok, silent, and
    // the cached path is the kernel's signature from the world's
    // side - every pose tag states tip-toward-shank (+X for an
    // inward -X plunge), the chain is closed, no Z cycles (the
    // chain IS the radial truth), and the tips reach the promised
    // depths: blind floor x = 5, through bottom x = -16 (floor
    // -15 minus the exit allowance).
    let (world, mut history) = cross_world("mill-turn");
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    let c = cache(&world, "CD1").expect("cached");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "Radial laws stand down: {:?}", c.warnings);
    assert!(c.warnings.is_empty(), "a clean radial chain reported: {:?}", c.warnings);
    let t = c.path;
    assert!(t.is_chained() && t.all_finite());
    assert!(t.cycles.is_empty(), "radial work carries no Z cycle list");
    assert!(
        t.moves.iter().all(|m| m.axis == Some([1.0, 0.0, 0.0])),
        "every pose is the -X plunge's tip->shank: +X"
    );
    let deepest_through = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.to.z < 0.0)
        .map(|m| m.to.x)
        .fold(f64::MAX, f64::min);
    assert!((deepest_through - -16.0).abs() < 1e-9, "through tip at x = {deepest_through}");
    let blind_floor = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.to.z > 0.0)
        .map(|m| m.to.x)
        .fold(f64::MAX, f64::min);
    assert!((blind_floor - 5.0).abs() < 1e-9, "blind floor at x = {blind_floor}");
}

#[test]
fn the_station_change_traverses_outside_the_silhouette() {
    // Two holes on different stations (X and Y): every rapid either
    // rides a hole's own axis (the kerf's air) or stays outside the
    // silhouette radius - the chord law the kernel proved, read
    // back off the document's cached path.
    let (world, mut history) = cross_world("mill-turn");
    let holes = vec![
        cross(Point3::new(15.0, 0.0, 10.0), nx(), 2.0, -8.0, false),
        cross(Point3::new(0.0, 15.0, 10.0), ny(), 2.0, -8.0, false),
    ];
    create(&world, &mut history, "CD1", cross_params(holes, None));
    let c = cache(&world, "CD1").expect("cached");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "{:?}", c.warnings);
    let r = stock_r();
    let t = c.path;
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid {
            continue;
        }
        let axial = (m.from.y.abs() < 1e-9 && m.to.y.abs() < 1e-9)
            || (m.from.x.abs() < 1e-9 && m.to.x.abs() < 1e-9);
        if axial {
            continue; // the hole's own kerf
        }
        for f in [0.0, 0.5, 1.0] {
            let p = Point3::new(
                m.from.x + (m.to.x - m.from.x) * f,
                m.from.y + (m.to.y - m.from.y) * f,
                m.from.z + (m.to.z - m.from.z) * f,
            );
            let rho = (p.x * p.x + p.y * p.y).sqrt();
            assert!(rho >= r - 1e-9, "rapid {i} clips the blank at ({}, {})", p.x, p.y);
        }
    }
}

// ---- the refusals, each by name ----

fn op(params: OpParams) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params,
        stamp: None,
        setup: None,
    }
}

#[test]
fn a_mixed_list_refuses_by_name() {
    let (world, _h) = cross_world("mill-turn");
    let mut p = cross_params(typed(), None);
    if let OpParams::Drill { holes, .. } = &mut p {
        holes.push(camcraft_app::drilling::Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 2.0,
            entry_z: 15.0,
            floor_z: 0.0,
            through: false,
        });
    }
    let e = operation::generate(&world, &op(p), "").expect_err("a mixed list cannot run");
    assert!(format!("{e}").contains("not both"), "{e}");
}

#[test]
fn a_plain_mill_has_no_turret() {
    let (world, _h) = cross_world("mill");
    let e = operation::generate(&world, &op(cross_params(typed(), None)), "")
        .expect_err("cross holes ask for live tooling");
    assert!(format!("{e}").contains("live-tooling"), "{e}");
}

#[test]
fn a_bottom_plane_cannot_drive_radial_holes() {
    let (world, _h) = cross_world("mill-turn");
    let e = operation::generate(&world, &op(cross_params(typed(), Some(Height::fixed(0.0)))), "")
        .expect_err("the depth rides each hole, not a plane");
    assert!(format!("{e}").contains("cannot drive radial holes"), "{e}");
}

// ---- recognition, round trip, staleness ----

#[test]
fn recognition_finds_the_bar_two_holes_and_they_generate() {
    // The document's own detector, over the imported mesh: exactly
    // the two modeled bores - a blind floor at x = 5 and a through
    // that spans the bar - and the chain they drive comes back Ok.
    let (world, mut history) = cross_world("mill-turn");
    let found =
        operation::cross_holes_of(&world, None, &SrcRef::Entity("bar".into())).expect("recognized");
    assert_eq!(found.len(), 2, "{found:?}");
    let blind = found.iter().find(|h| h.origin.z > 0.0).expect("the blind one");
    assert!((blind.floor_d - -10.0).abs() < 0.5 && !blind.through, "{blind:?}");
    let thru = found.iter().find(|h| h.origin.z < 0.0).expect("the through one");
    assert!(thru.through, "{thru:?}");
    create(&world, &mut history, "CD1", cross_params(found, None));
    let c = cache(&world, "CD1").expect("cached");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "{:?}", c.warnings);
}

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft_cross_{tag}_{}.camproj", std::process::id()))
}

#[test]
fn the_cross_list_rides_the_project_file() {
    // Save/load round trip: the list comes back exact (directions
    // NOT latticed - a unit vector survives to the bit) and the op
    // regenerates Ok in the loaded document. A pure-Z drill file
    // never grows the `cross` key at all - the v40 default.
    let (world, mut history) = cross_world("mill-turn");
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    let path = tmp_path("cross");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("readable");
    assert!(text.contains("\"cross\""), "the radial list is in the file");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let e = model::find_by_name(&world2, "CD1").expect("op entity");
    let op = model::operation_of(&world2, &e).expect("the op is in the file");
    let OpParams::Drill { cross, .. } = &op.params else { panic!("drill params") };
    assert_eq!(cross, &typed(), "the list is exact through the file");
    history.execute(&world2, GenerateOperation::new(&world2, "CD1").expect("regenerate"));
    assert_eq!(status(&world2, "CD1"), OpStatus::Ok, "and it runs again");
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_z_only_drill_file_never_writes_the_cross_key() {
    // The v39 -> v40 step-up promises: absent means empty means
    // exactly the old document.
    let (world, mut history) = cross_world("mill");
    let p = OpParams::Drill {
        clearance_z: Height::fixed(20.0),
        retract_z: None,
        top_z: Height::fixed(15.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![camcraft_app::drilling::Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 2.0,
            entry_z: 15.0,
            floor_z: 5.0,
            through: false,
        }],
        cross: vec![],
    };
    create(&world, &mut history, "D1", p);
    let path = tmp_path("zonly");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("readable");
    assert!(!text.contains("\"cross\""), "a pure-Z file stays a pure-Z file");
    std::fs::remove_file(&path).ok();
}

#[test]
fn editing_the_cross_list_ages_the_cache_and_undo_restores() {
    // Everything-is-a-command: deepening the blind floor marks the
    // cached chain Stale (the digest moved), and undoing the edit
    // lands the old list exactly - Ok once regenerated.
    let (world, mut history) = cross_world("mill-turn");
    create(&world, &mut history, "CD1", cross_params(typed(), None));
    assert_eq!(status(&world, "CD1"), OpStatus::Ok);
    let mut deeper = typed();
    deeper[0].floor_d = -12.0;
    history.execute(
        &world,
        EditOperationParams::new(&world, "CD1", cross_params(deeper, None)).expect("edit"),
    );
    assert_eq!(status(&world, "CD1"), OpStatus::Stale, "the digest moved with the floor");
    assert!(history.undo(&world), "undo the edit");
    assert_eq!(status(&world, "CD1"), OpStatus::Ok, "the cached chain is the old list's again");
}
