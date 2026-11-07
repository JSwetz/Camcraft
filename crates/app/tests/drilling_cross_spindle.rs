//! The cross drill about the posted spindle (issue #67): the radial
//! kernel's standoffs - the bar's radius, each station's side, the hop
//! and clearance radii, the chord law between stations - are measured
//! about the spindle the post programs (world Z through the work
//! offset's origin), not about the world origin. A turned bar moved
//! off the world origin with G54 moved onto its axis drills exactly
//! the on-origin chain, shifted; the same bar under an unmoved G54 is
//! refused by name (the spindle law, on the lathe and the mill-turn);
//! and a plain lathe refuses by name a part the axis fit cannot read as
//! turned. The on-origin chain is pinned by the drilling_cross suite.
//!
//! Gate map (M4.4): covers app::operation (cross_spindle, the cross arm
//! of generate, collect_job's spindle_law door and its wcs_origin).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock, SetWcsOrigin};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// Where the moved bar's axis stands: off the world origin in X and Y.
const AT: (f64, f64) = (10.0, 4.0);

/// A closed r10 bar, z in [-20, 20], revolved about world Z and moved
/// by (`dx`, `dy`).
fn bar(dx: f64, dy: f64) -> Mesh {
    let meridian = [(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (0.0, 20.0)];
    let segs = 64;
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos() + dx, r * th.sin() + dy, z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// A right-trapezoid prism: its sides' perpendicular bisectors meet at
/// no one point, so no axis of revolution reads off it.
fn prism() -> Mesh {
    let q = [(-10.0, -8.0), (12.0, -8.0), (12.0, 4.0), (-10.0, 10.0)];
    let p = |i: usize, z: f64| Point3::new(q[i % 4].0, q[i % 4].1, z);
    let mut f = Vec::new();
    for i in 0..4 {
        let (a, b, c, d) = (p(i, -20.0), p(i + 1, -20.0), p(i + 1, 20.0), p(i, 20.0));
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    for z in [-20.0, 20.0] {
        f.push(Triangle::new(p(0, z), p(1, z), p(2, z)));
        f.push(Triangle::new(p(0, z), p(2, z), p(3, z)));
    }
    Mesh::from_facets(f)
}

/// Three stations about a bar whose axis stands at (`dx`, `dy`): +X,
/// +Y, then -X (the opposite pair takes the end-face detour).
fn holes(dx: f64, dy: f64) -> Vec<CrossHole> {
    let h = |x: f64, y: f64, z: f64, dir: Point3| CrossHole {
        origin: Point3::new(x + dx, y + dy, z),
        dir,
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -6.0,
        through: false,
    };
    vec![
        h(10.0, 0.0, 10.0, Point3::new(-1.0, 0.0, 0.0)),
        h(0.0, 10.0, 0.0, Point3::new(0.0, -1.0, 0.0)),
        h(-10.0, 0.0, -10.0, Point3::new(1.0, 0.0, 0.0)),
    ]
}

fn params(cross: Vec<CrossHole>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![],
        cross,
    }
}

fn op(cross: Vec<CrossHole>) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: params(cross),
        stamp: None,
        setup: None,
        area: Default::default(),
    }
}

fn world_with(word: &str, part: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Box { dx: 60.0, dy: 60.0, dz: 44.0 },
        Location::default(),
    );
    history.execute(&world, stock.expect("box stock"));
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", part));
    (world, history)
}

fn place_g54(world: &World, history: &mut History, x: f64, y: f64) {
    let at = Location { pos: Point3::new(x, y, 0.0), rot: IDENTITY_Q };
    history.execute(world, SetWcsOrigin::new(world, "G54", at).expect("set G54"));
}

/// The chain's moves read from G54: every point minus the offset.
fn about(t: &Toolpath, ox: f64, oy: f64) -> Vec<(Point3, Point3, Option<[f64; 3]>)> {
    let rel = |p: Point3| Point3::new(p.x - ox, p.y - oy, p.z);
    t.moves.iter().map(|m| (rel(m.from), rel(m.to), m.axis)).collect()
}

#[test]
fn a_cross_drill_off_the_world_origin_under_a_moved_g54_drills_the_on_origin_chain() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, bar(0.0, 0.0));
        let (plain, _) = operation::generate(&world, &op(holes(0.0, 0.0)), "")
            .unwrap_or_else(|e| panic!("{word}: the on-origin bar drills: {e}"));

        let (world, mut history) = world_with(word, bar(AT.0, AT.1));
        place_g54(&world, &mut history, AT.0, AT.1);
        let (moved, _) = operation::generate(&world, &op(holes(AT.0, AT.1)), "")
            .unwrap_or_else(|e| panic!("{word}: the bar on its work offset drills: {e}"));

        let (a, b) = (about(&plain, 0.0, 0.0), about(&moved, AT.0, AT.1));
        assert_eq!(a.len(), b.len(), "{word}: the same chain, move for move");
        for (i, (p, q)) in a.iter().zip(&b).enumerate() {
            let near = |u: Point3, v: Point3| (u - v).dot(u - v).sqrt() <= 1e-9;
            assert!(
                near(p.0, q.0) && near(p.1, q.1) && p.2 == q.2,
                "{word}: move {i} from G54 reads {q:?}, the on-origin chain {p:?}"
            );
        }
        // And every rapid off a hole's own kerf stands outside the bar
        // as measured about ITS axis - the standoff law, about the spindle.
        let r_about = |p: Point3| ((p.x - AT.0).powi(2) + (p.y - AT.1).powi(2)).sqrt();
        let first = moved.moves.first().map(|m| r_about(m.from)).unwrap_or(0.0);
        assert!((first - 15.0).abs() < 1e-9, "{word}: the clearance stands r10 + 5 off the axis, read {first}");
    }
}

#[test]
fn a_cross_drill_on_a_bar_off_an_unmoved_work_offset_is_refused_by_name() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, bar(AT.0, AT.1));
        match operation::generate(&world, &op(holes(AT.0, AT.1)), "") {
            Ok((t, _)) => panic!("{word}: a bar 10.770 mm off G54 drilled {} moves", t.moves.len()),
            Err(e) => {
                let e = e.to_string();
                for s in ["spindle", "10.770 mm", "G54"] {
                    assert!(e.contains(s), "{word}: the refusal names '{s}': {e}");
                }
            }
        }
    }
}

#[test]
fn a_lathe_cross_drill_on_a_part_that_reads_as_no_revolve_is_refused_by_name() {
    let part = prism();
    assert!(
        operation::spindle_reading(&part.facets, [0.0; 3]).is_none(),
        "the fixture's premise: the axis fit cannot read the prism"
    );
    let hole = vec![CrossHole {
        origin: Point3::new(12.0, 0.0, 10.0),
        dir: Point3::new(-1.0, 0.0, 0.0),
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -6.0,
        through: false,
    }];
    let (world, _h) = world_with("lathe", part);
    match operation::generate(&world, &op(hole.clone()), "") {
        Ok((t, _)) => panic!("an unreadable part drilled {} moves on a lathe", t.moves.len()),
        Err(e) => {
            let e = e.to_string();
            for s in ["cross drill", "cannot be read as turned", "mill-turn"] {
                assert!(e.contains(s), "the refusal names '{s}': {e}");
            }
        }
    }
    // The mill-turn holds a prismatic bar: it drills, about G54.
    let (world, _h) = world_with("mill-turn", prism());
    operation::generate(&world, &op(hole), "").expect("a mill-turn drills a prismatic bar");
}
