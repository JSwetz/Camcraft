//! A cross drill into a BOX of raw stock enters at the box's face, not
//! at its circumscribed (corner) radius (issue #85): each hole's mouth
//! is where its own line - the drill's body along it - first meets the
//! stock mesh, so a square bar drills no air above the face, its Auto
//! pecks and its reach depth (#84) are read from the face, and a valid
//! hole is not falsely refused. The station-change hops still swing
//! around the corners, and a round bar's chain is unchanged bit for bit.
//!
//! Gate map (M4.4): covers app::operation (cross_stock's per-hole entry
//! and rapid proof, cross_spindle, drill_reach's cross_reach).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures (a revolved r10 bar, box and cylinder
//! raw stock); no vendor material.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const PART_R: f64 = 10.0;
/// The feed-entry standoff the op's planes give (clearance 25 - top 20).
const R_D: f64 = 5.0;

/// A closed bar of radius `r`, z in [-20, 20], revolved about world Z.
fn bar(r: f64) -> Mesh {
    let segs = 64;
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for j in 0..segs {
        let (a1, a2) = (v(j, r, -20.0), v(j + 1, r, -20.0));
        let (b1, b2) = (v(j, r, 20.0), v(j + 1, r, 20.0));
        f.push(Triangle::new(a1, a2, b2));
        f.push(Triangle::new(a1, b2, b1));
        f.push(Triangle::new(Point3::new(0.0, 0.0, -20.0), a2, a1));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 20.0), b1, b2));
    }
    Mesh::from_facets(f)
}

fn hole(origin: Point3, dir: Point3, floor_d: f64) -> CrossHole {
    CrossHole { origin, dir, radius: 2.0, entry_d: 0.0, floor_d, through: false }
}

fn op(cross: Vec<CrossHole>) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: OpParams::Drill {
            clearance_z: Height::fixed(20.0 + R_D),
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
        },
        stamp: None,
        setup: None,
        area: Default::default(),
    }
}

fn rotaries() -> Vec<AxisLimits> {
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])]
}

fn square(side: f64) -> StockShape {
    StockShape::Box { dx: side, dy: side, dz: 40.0 }
}

/// The r10 bar on `word`'s machine with `stock` (z -20..20) and the
/// catalog DR-0400 re-ground by `grind`.
fn world_on(word: &str, stock: StockShape, grind: impl Fn(&mut Tool)) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes: rotaries(), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    tool.flute_len = 30.0;
    grind(&mut tool);
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", bar(PART_R)));
    (world, history)
}

fn first_plunge(t: &Toolpath) -> Option<(Point3, Point3)> {
    t.moves.iter().find(|m| m.kind == MoveKind::Plunge).map(|m| (m.from, m.to))
}

fn deepest(t: &Toolpath) -> f64 {
    t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.to.x).fold(f64::INFINITY, f64::min)
}

/// Before #85 a 44 mm square bar read as r31.11 (its corner radius):
/// the hole, 30 mm deep from the face (r22 to x -8), read 39.1 mm deep
/// and a 35 mm flute was refused. It reads 30 mm and posts.
#[test]
fn a_30_deep_hole_from_the_face_of_a_44_square_bar_posts_on_a_35_flute() {
    use camcraft_app::{model, operation::OpStatus, posting};
    let cross = vec![hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -18.0)];
    for word in ["lathe", "mill-turn"] {
        let (world, mut history) = world_on(word, square(44.0), |t| t.flute_len = 35.0);
        let o = op(cross.clone());
        let (t, _) = operation::generate(&world, &o, "")
            .unwrap_or_else(|e| panic!("{word}: the 30 mm hole from the face drills on a 35 mm flute: {e}"));
        let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("{word}: the chain plunges"));
        assert!((from.x - (22.0 + R_D)).abs() < 1e-6, "{word}: the feed entry stands {R_D} off the r22 face: {from:?}");
        assert!((deepest(&t) + 8.0).abs() < 1e-6, "{word}: the floor stays at x -8");
        let create = CreateOperation::new(&world, "CD1", "DR-0400", o.src.clone(), o.params.clone());
        history.execute(&world, create.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
        let op_e = model::find_by_name(&world, "CD1").expect("op");
        assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok, "{word}: generates fresh");
        if word == "mill-turn" {
            let path = std::env::temp_dir().join(format!("camcraft-85-{}.nc", std::process::id()));
            let posted = posting::post_setup(&world, &path);
            let _ = std::fs::remove_file(&path);
            assert!(posted.is_ok(), "{word}: the valid square-bar hole posts: {:?}", posted.err());
        }
    }
}

/// A 60 x 60 box: the mouth sits at the face (r30 plus the feed-entry
/// standoff), exactly where a d60 round bar's mouth does on that line -
/// not at the corner radius r42.43.
#[test]
fn a_60_box_mouth_sits_at_the_face_not_the_corner_radius() {
    let cross = vec![hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_on(word, square(60.0), |_| {});
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the box cross drill generates: {e}"));
        let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("{word}: the chain plunges"));
        assert!((from.x - (30.0 + R_D)).abs() < 1e-6, "{word}: the mouth stands at the r30 face: {from:?}");
        let (world, _h) = world_on(word, StockShape::Cylinder { diameter: 60.0, height: 40.0 }, |_| {});
        let (round, _) = operation::generate(&world, &op(cross.clone()), "").expect("the round bar drills");
        assert_eq!(first_plunge(&round).map(|p| p.0), Some(from), "{word}: the box face and the d60 bar enter alike");
    }
}

/// An off-centre hole on a box face: 7 mm off its station's centre
/// line with a 6 mm drill - the drill's edge band (y 4..10) lies on the
/// flat x = 30 face, so the mouth is the face, not the corner circle's
/// r42.24 at the edge.
#[test]
fn an_off_centre_hole_on_a_box_face_enters_at_the_face() {
    let cross = vec![hole(Point3::new((PART_R * PART_R - 49.0).sqrt(), 7.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_on(word, square(60.0), |t| {
            t.diameter = 6.0;
            t.shank_d = 6.0;
        });
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the off-centre box hole drills: {e}"));
        let (from, _) = first_plunge(&t).unwrap_or_else(|| panic!("{word}: the chain plunges"));
        assert!((from.x - (30.0 + R_D)).abs() < 1e-6, "{word}: the off-centre mouth stands at the face: {from:?}");
        assert!((from.y - 7.0).abs() < 1e-9, "{word}: on its own line");
    }
}

/// Closest approach of a segment to the spindle in XY.
fn approach(a: Point3, b: Point3) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll <= 0.0 { 0.0 } else { (-(a.x * dx + a.y * dy) / ll).clamp(0.0, 1.0) };
    (a.x + t * dx).hypot(a.y + t * dy)
}

/// A quarter-turn station change on a 60 box: the mouths move in to the
/// faces, but every rapid that is not along its own hole's line (the
/// hop chord that swings round the bar) keeps the drill's rim outside
/// the box's corner radius r42.43 - the corners are still there.
#[test]
fn the_station_change_hop_still_clears_the_box_corners() {
    let cross = vec![
        hole(Point3::new(PART_R, 0.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0),
        hole(Point3::new(0.0, PART_R, 0.0), Point3::new(0.0, -1.0, 0.0), -3.0),
    ];
    let corner = 1800.0_f64.sqrt();
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = world_on(word, square(60.0), |_| {});
        let (t, _) = operation::generate(&world, &op(cross.clone()), "")
            .unwrap_or_else(|e| panic!("{word}: the box station change drills: {e}"));
        let mut hops = 0;
        for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
            let along = (m.from.x - m.to.x).abs() < 1e-9 || (m.from.y - m.to.y).abs() < 1e-9;
            if along {
                continue;
            }
            hops += 1;
            let got = approach(m.from, m.to);
            assert!(got >= corner + 2.0 - 1e-9, "{word}: the hop passes r{got:.3}, the rim inside the corners");
        }
        assert!(hops >= 1, "{word}: the chain swings between stations");
    }
}

/// FNV-1a over the chain's bits: kind, both ends, the pose.
fn fingerprint(t: &Toolpath) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u64| {
        for b in v.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for m in &t.moves {
        eat(m.kind as u64);
        for p in [m.from, m.to] {
            for c in [p.x, p.y, p.z] {
                eat(c.to_bits());
            }
        }
        for c in m.axis.unwrap_or([0.0; 3]) {
            eat(c.to_bits());
        }
    }
    h
}

/// A round raw bar keeps the chain it had before #85, bit for bit: the
/// three-station job and an off-centre hole on a d40 bar, fingerprints
/// taken from the pre-#85 build.
#[test]
fn a_cylinder_stock_chain_is_unchanged_bit_for_bit() {
    let three = vec![
        hole(Point3::new(PART_R, 0.0, 10.0), Point3::new(-1.0, 0.0, 0.0), -6.0),
        hole(Point3::new(0.0, PART_R, 0.0), Point3::new(0.0, -1.0, 0.0), -6.0),
        hole(Point3::new(-PART_R, 0.0, -10.0), Point3::new(1.0, 0.0, 0.0), -6.0),
    ];
    let off = vec![hole(Point3::new((PART_R * PART_R - 81.0).sqrt(), 9.0, 0.0), Point3::new(-1.0, 0.0, 0.0), -3.0)];
    let got: Vec<u64> = [three, off]
        .into_iter()
        .map(|cross| {
            let (world, _h) = world_on("mill-turn", StockShape::Cylinder { diameter: 40.0, height: 40.0 }, |_| {});
            let (t, _) = operation::generate(&world, &op(cross), "").expect("the round bar drills");
            fingerprint(&t)
        })
        .collect();
    assert_eq!(got, vec![2202346499980779381, 1828336726059972913], "the round-bar chains moved");
}
