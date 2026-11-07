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
        coolant: Default::default(),
    }
}

fn world_with(word: &str, part: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", part));
    // The stock IS the bar (stock from model): a raw stock wider than
    // the part moves every standoff out to it (issue #77,
    // drilling_cross_stock), and a box centred on the world origin
    // would read differently about each spindle - the law pinned here
    // is the spindle frame's alone.
    let stock = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::FromModel { source: "bar".into() },
        Location::default(),
    );
    history.execute(&world, stock.expect("stock from the bar"));
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
        let (plain, pw) = operation::generate(&world, &op(holes(0.0, 0.0)), "")
            .unwrap_or_else(|e| panic!("{word}: the on-origin bar drills: {e}"));

        let (world, mut history) = world_with(word, bar(AT.0, AT.1));
        place_g54(&world, &mut history, AT.0, AT.1);
        let (moved, mw) = operation::generate(&world, &op(holes(AT.0, AT.1)), "")
            .unwrap_or_else(|e| panic!("{word}: the bar on its work offset drills: {e}"));
        // Issue #80: the clear chain is not convicted - on a lathe the
        // axial-hole verdict once judged the radial path and blocked it.
        for w in [&pw, &mw] {
            assert!(!w.iter().any(|w| w.blocks_post()), "{word}: a clear cross drill is convicted: {w:?}");
        }

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

/// A cross hole on `prism()` moved +30 mm in X (faces at x = 20, the
/// one nearer the spindle line, and x = 42), mouth on the face at x.
fn block_hole(x: f64, dir_x: f64) -> Vec<CrossHole> {
    vec![CrossHole {
        origin: Point3::new(x, 0.0, 10.0),
        dir: Point3::new(dir_x, 0.0, 0.0),
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -6.0,
        through: false,
    }]
}

fn moved_prism(dx: f64) -> Mesh {
    let f = prism()
        .facets
        .iter()
        .map(|t| {
            let m = |p: Point3| Point3::new(p.x + dx, p.y, p.z);
            Triangle::new(m(t.a), m(t.b), m(t.c))
        })
        .collect();
    Mesh::from_facets(f)
}

#[test]
fn a_mill_turn_cross_hole_plunging_away_from_the_spindle_is_refused_by_name() {
    // The block sits 20..42 mm off an unmoved G54 in X. A hole on its
    // near face (x = 20) plunging +X, away from the spindle line: the
    // station law stands the tool at radius s on the mouth's side -
    // x ~ 48, beyond the FAR face - and the rapid in to the mouth runs
    // through the whole block at rapid feed.
    let (world, _h) = world_with("mill-turn", moved_prism(30.0));
    match operation::generate(&world, &op(block_hole(20.0, 1.0)), "") {
        Ok((t, _)) => {
            let through = t.moves.iter().any(|m| {
                m.kind == camcraft_app::cl::MoveKind::Rapid
                    && m.from.x.max(m.to.x) > 42.0
                    && m.from.x.min(m.to.x) < 20.0
                    && (m.from.z - 10.0).abs() < 1e-9
            });
            panic!("a hole plunging away from the spindle drilled {} moves (a rapid through the block: {through})", t.moves.len());
        }
        Err(e) => {
            let e = e.to_string();
            for s in ["away from the spindle", "(20, 0, 10)"] {
                assert!(e.contains(s), "the refusal names '{s}': {e}");
            }
        }
    }
    // The far face's hole plunges -X, toward the spindle: it drills.
    operation::generate(&world, &op(block_hole(42.0, -1.0)), "")
        .expect("a hole plunging toward the spindle drills on the same block");
}

#[test]
fn a_kernel_refusal_on_the_shifted_path_names_the_hole_in_world_coordinates() {
    // A Z plunge is the mill-side drill's word; the kernel refuses it
    // in the spindle frame (10, 0, 10) - the user's model has the
    // mouth at world (20, 4, 10).
    let (world, mut history) = world_with("mill-turn", bar(AT.0, AT.1));
    place_g54(&world, &mut history, AT.0, AT.1);
    let hole = vec![CrossHole {
        origin: Point3::new(AT.0 + 10.0, AT.1, 10.0),
        dir: Point3::new(0.0, 0.0, -1.0),
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -6.0,
        through: false,
    }];
    match operation::generate(&world, &op(hole), "") {
        Ok((t, _)) => panic!("a Z plunge drilled {} moves radially", t.moves.len()),
        Err(e) => {
            let e = e.to_string();
            assert!(e.contains("(20, 4, 10)"), "the refusal names the world mouth: {e}");
            assert!(!e.contains("(10, 0, 10)"), "no spindle-frame point leaks: {e}");
        }
    }
}

#[test]
fn a_work_offset_moved_after_a_mill_turn_cross_drill_stales_it_and_the_post_refuses() {
    use camcraft_app::commands::{CreateOperation, GenerateOperation};
    use camcraft_app::operation::OpStatus;
    let (world, mut history) = world_with("mill-turn", bar(0.0, 0.0));
    let create = CreateOperation::new(&world, "X1", "DR-0400", SrcRef::Entity("bar".into()), params(holes(0.0, 0.0)))
        .expect("create");
    history.execute(&world, create);
    history.execute(&world, GenerateOperation::new(&world, "X1").expect("the bar on G54 drills"));
    let e = camcraft_app::model::find_by_name(&world, "X1").expect("X1");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);

    // G54 moved 1 mm in Y under the generated op: every standoff was
    // measured about a line the spindle no longer turns on.
    place_g54(&world, &mut history, 0.0, 1.0);
    assert_eq!(operation::status_of(&world, &e), OpStatus::Stale, "the work offset is an input of the cross drill");
    let err = camcraft_app::posting::stream_records(&world).expect_err("a moved spindle never posts");
    for s in ["X1", "spindle", "G54"] {
        assert!(err.contains(s), "the post's refusal names '{s}': {err}");
    }

    // Moved back, the cached path is fresh again.
    place_g54(&world, &mut history, 0.0, 0.0);
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
}
