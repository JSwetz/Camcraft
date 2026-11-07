//! The cross drill stands off the RAW stock, not only the finished part
//! (issue #77): a cross drill run before the bar is turned to size -
//! the setup's stock wider than the part - measures its hop, approach
//! and clearance radii about the larger of the two, so no rapid between
//! stations passes through uncut stock (the Prime Directive: a rapid
//! through material is a collision). With the stock equal to the part
//! the chain is the part-only chain, bit for bit.
//!
//! Gate map (M4.4): covers app::operation (cross_spindle's stock
//! reading and its rapid-clear refusal, the cross arm of generate,
//! collect_job's stock facets).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// The finished bar's radius and the raw bar's: drilled before turning.
const PART_R: f64 = 10.0;
const STOCK_R: f64 = 20.0;

/// A closed bar of radius `r`, z in [-20, 20], revolved about world Z.
fn bar(r: f64) -> Mesh {
    let meridian = [(0.0, -20.0), (r, -20.0), (r, 20.0), (0.0, 20.0)];
    let segs = 64;
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
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

/// Three stations on the finished r10 face: +X, +Y, then -X (the
/// opposite pair takes the end-face detour).
fn holes() -> Vec<CrossHole> {
    let h = |x: f64, y: f64, z: f64, dir: Point3| CrossHole {
        origin: Point3::new(x, y, z),
        dir,
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -6.0,
        through: false,
    };
    vec![
        h(PART_R, 0.0, 10.0, Point3::new(-1.0, 0.0, 0.0)),
        h(0.0, PART_R, 0.0, Point3::new(0.0, -1.0, 0.0)),
        h(-PART_R, 0.0, -10.0, Point3::new(1.0, 0.0, 0.0)),
    ]
}

fn op() -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::Drill {
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
            cross: holes(),
        },
        stamp: None,
        setup: None,
        area: Default::default(),
    }
}

/// A world on `word`'s machine with the finished r10 bar and `stock` as
/// the setup's raw material (`None`: no stock declared).
fn world_with(word: &str, stock: Option<(StockShape, f64)>) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if let Some((shape, z0)) = stock {
        let at = Location { pos: Point3::new(0.0, 0.0, z0), rot: IDENTITY_Q };
        history.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    }
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes: rotaries(), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", bar(PART_R)));
    (world, history)
}

/// A live-tooling turret that states the A and B tilts the radial
/// poses ask for - so the machine-limits clause passes and the post
/// gate's verdict on the rapids is the collision laws' own.
fn rotaries() -> Vec<AxisLimits> {
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])]
}

/// The raw r20 bar over the part's own Z span.
fn raw_bar() -> Option<(StockShape, f64)> {
    Some((StockShape::Cylinder { diameter: 2.0 * STOCK_R, height: 40.0 }, -20.0))
}

/// Closest approach of a segment to the spindle (world Z) in XY.
fn approach(a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll <= 0.0 { 0.0 } else { (-(a.x * dx + a.y * dy) / ll).clamp(0.0, 1.0) };
    (a.x + t * dx).hypot(a.y + t * dy)
}

/// The smallest radius any rapid OUT of a kerf reaches: rapids that
/// follow a cut (the retract up the tool's own hole) are excused, as
/// are the swings above the stock's end face (z >= 20).
fn rapid_floor(t: &Toolpath) -> f64 {
    let mut in_kerf = false;
    let mut floor = f64::INFINITY;
    for m in &t.moves {
        if m.kind != MoveKind::Rapid {
            in_kerf = true;
            continue;
        }
        if in_kerf {
            in_kerf = false; // the retract to the feed-entry point
            continue;
        }
        if m.from.z.min(m.to.z) >= 20.0 {
            continue;
        }
        floor = floor.min(approach(m.from, m.to));
    }
    floor
}

/// Before #77 this chain started and approached at r15 and chorded the
/// station change 15 mm from the spindle - 5 mm inside the r20 raw bar -
/// and a mill-turn with its A/B rotaries stated POSTED it (the rapid-in-
/// stock law stands down in `Mode::Radial`). Now every rapid stands off
/// the raw stock: start and approach at r25, the chord at r25.
#[test]
fn a_cross_drill_on_raw_stock_wider_than_the_part_hops_outside_the_stock() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, raw_bar());
        let (t, _) = operation::generate(&world, &op(), "")
            .unwrap_or_else(|e| panic!("{word}: the raw-stock cross drill generates: {e}"));
        let floor = rapid_floor(&t);
        let first = t.moves.first().map(|m| m.from.x.hypot(m.from.y)).unwrap_or(0.0);
        assert!(
            floor >= STOCK_R - 1e-9,
            "{word}: a rapid reaches r {floor:.3} inside the r{STOCK_R} raw stock"
        );
        assert!(first >= STOCK_R + 5.0 - 1e-9, "{word}: the clearance stands off the stock: {first:.3}");
    }
}

#[test]
fn a_cross_drill_with_the_stock_equal_to_the_part_drills_the_part_only_chain() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, None);
        let (plain, _) = operation::generate(&world, &op(), "")
            .unwrap_or_else(|e| panic!("{word}: the part-only cross drill generates: {e}"));
        // The stock modeled as the part itself (stock from model).
        let (world, mut history) = world_with(word, None);
        let same = SetStock::new(&world, StockShape::FromModel { source: "bar".into() }, Location::default());
        history.execute(&world, same.expect("stock from the part"));
        let (t, _) = operation::generate(&world, &op(), "")
            .unwrap_or_else(|e| panic!("{word}: the stock==part cross drill generates: {e}"));
        assert_eq!(plain.moves.len(), t.moves.len(), "{word}: the same chain");
        for (i, (p, q)) in plain.moves.iter().zip(&t.moves).enumerate() {
            assert!(
                p.from == q.from && p.to == q.to && p.axis == q.axis && p.kind == q.kind,
                "{word}: move {i} reads {q:?}, the part-only chain {p:?}"
            );
        }
        assert!(rapid_floor(&t) >= PART_R - 1e-9, "{word}: the part-only chain hops outside the part");
    }
}

#[test]
fn a_station_change_whose_chord_would_clip_the_bar_is_refused_by_name() {
    // Two mouths on the r10 face off their stations' centre lines: the
    // kernel's quarter-turn chord law assumes mouths ON the station
    // axes, so this chord passes 10.757 mm from the spindle - the r2
    // drill's rim 1.24 mm inside the bar. The rapid proof refuses it
    // by name rather than post it.
    let h = |x: f64, y: f64, dir: Point3| CrossHole {
        origin: Point3::new(x, y, 0.0),
        dir,
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -3.0,
        through: false,
    };
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_with(word, None);
        let mut o = op();
        if let OpParams::Drill { cross, .. } = &mut o.params {
            *cross = vec![h(8.0, -6.0, Point3::new(-1.0, 0.0, 0.0)), h(-6.0, 8.0, Point3::new(0.0, -1.0, 0.0))];
        }
        match operation::generate(&world, &o, "") {
            Ok((t, _)) => panic!("{word}: a chord through the bar drilled {} moves", t.moves.len()),
            Err(e) => {
                let e = e.to_string();
                for s in ["cross-drill rapid", "through material", "10.757"] {
                    assert!(e.contains(s), "{word}: the refusal names '{s}': {e}");
                }
            }
        }
    }
}

#[test]
fn re_sizing_the_raw_stock_stales_the_cross_drill_and_the_post_refuses_it() {
    use camcraft_app::commands::{CreateOperation, GenerateOperation};
    use camcraft_app::model;
    use camcraft_app::operation::OpStatus;
    use camcraft_app::posting;
    let (world, mut history) = world_with("mill-turn", raw_bar());
    let o = op();
    let create = CreateOperation::new(&world, "CD1", "DR-0400", o.src.clone(), o.params.clone());
    history.execute(&world, create.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
    let status = || operation::status_of(&world, &model::find_by_name(&world, "CD1").expect("op"));
    assert_eq!(status(), OpStatus::Ok, "the raw-stock cross drill generates fresh");
    let path = std::env::temp_dir().join(format!("camcraft-77-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_ok(), "the clear chain posts: {:?}", posted.err());
    // A wider raw bar after generation: the cached hops stand inside it.
    let wider = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
    let set = SetStock::new(&world, StockShape::Cylinder { diameter: 60.0, height: 40.0 }, wider);
    history.execute(&world, set.expect("wider stock"));
    assert_eq!(status(), OpStatus::Stale, "the stock the standoffs read moved");
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_err(), "a cross drill whose stock moved never posts");
}
