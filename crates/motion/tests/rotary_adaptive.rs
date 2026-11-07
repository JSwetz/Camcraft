//! Adaptive on the rotary developed wrap (issue #9), specced from the
//! wrap/development literature in `multiaxis::wrap_exit` and the HSM
//! planner's own load and kink laws; no vendor material consulted.
//!
//! Issue #9: Adaptive clearing on the rotary developed wrap. Each
//! developed level (s the arc length at the wrap's radius, v the
//! station, height the radius) goes to the HSM planner exactly as a
//! planar 3-D level does; the planned path is wrapped back by the wrap
//! exit's own chord law (no developed arc stays an arc on the
//! cylinder). Read here: the load each planned level holds and its
//! in-cut breaks past 5 degrees, measured on the DEVELOPED path from
//! the clamped bar's skin; the planner's own overload notes; the same kink law
//! on the POSTED world program pulled back into the developed frame
//! (its chorded arcs); every world move posed radial; each level rings
//! cut (read from the program's shape, not its notes) named by radius; OffsetRings move for move what it always was (digest
//! read without the patch). The seam law itself (`hsm::Seam`) is pinned
//! by its unit test in `machining3d::hsm`.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary_offset,
//! wrap_exit), camcraft_motion::machining3d (hsm, entry), and
//! camcraft_motion::quality, on this suite's own revolve fixtures.
//! Lane shape (M4.4): the rings digest runs in seconds in debug; both
//! Adaptive plans (window ~3 min, whole bar ~10 min debug) are the heavy
//! lane's (release --ignored, ~2 min).

use camcraft_motion::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::load::{load_at_rim, DEFAULT_OPTIMAL_LOAD};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::{rotary_offset_developed, rotary_offset_in, AxisControl};
use camcraft_motion::quality::{measure, stock_for, QualitySpec};

const TAU: f64 = std::f64::consts::TAU;
const D: f64 = 6.0;
const R: f64 = D * 0.5;

/// A closed solid of revolution about Z at `n` turns: the meridian
/// (rho, z) swept.
fn swept(n: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let pt = |rho: f64, z: f64, j: usize| {
        let th = TAU * (j % n) as f64 / n as f64;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
    };
    let mut f = Vec::new();
    for w in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..n {
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, j + 1), pt(rb, zb, j), pt(rb, zb, j + 1));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

/// The app suites' turned bar: wall r6, shoulder, neck r5.
const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

fn bar() -> Vec<Triangle> {
    swept(16, &MERIDIAN)
}

/// A coaxial round bar of radius `rho` (the app suites clamp r6.2).
fn stock(rho: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    swept(32, &[(0.0, z0), (rho, z0), (rho, z1), (0.0, z1)])
}

fn params(strategy: PocketStrategy) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: D,
        stepover: 4.0,
        stock_to_leave: 0.5,
        clearance_z: 25.0,
        top_z: 14.0,
        bottom_z: -14.0,
        stepdown: 0.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

fn control() -> AxisControl {
    AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None }
}

/// The developed path's own box, padded by the tool: the measuring
/// stock's footprint.
fn box_of(path: &Toolpath) -> Region {
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for m in &path.moves {
        lo = Point2::new(lo.x.min(m.to.x), lo.y.min(m.to.y));
        hi = Point2::new(hi.x.max(m.to.x), hi.y.max(m.to.y));
    }
    let (a, b) = (Point2::new(lo.x - R, lo.y - R), Point2::new(hi.x + R, hi.y + R));
    let Ok(l) = Loop::new(vec![a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)]) else { panic!("box") };
    let Ok(rg) = Region::new(vec![l]) else { panic!("box region") };
    rg
}

/// Per developed level (its radius): planar feed moves, arcs, the worst
/// load (ae/D) read on the dexel stock, in-cut breaks past 5 degrees,
/// and whether a note names it ring-cut.
struct LevelRead {
    rho: f64,
    moves: usize,
    arcs: usize,
    load: f64,
    breaks: usize,
    named: bool,
}

/// The note every ring-cut developed level carries (seam, planner
/// fallback, hard planner refusal alike).
fn ring_tag(rho: f64) -> String {
    format!("Offset rings cut the developed level at radius {rho:.3} instead")
}

impl LevelRead {
    /// Cut by rings, read from the program's SHAPE: planar moves and not
    /// one proved arc (the HSM plan's arc output; rings post lines). A
    /// skin level (zero depth) counts only when named.
    fn ringed(&self, skin: f64) -> bool {
        self.moves > 0 && self.arcs == 0 && (self.rho < skin - 1e-6 || self.named)
    }
}

/// Every level rings cut is named by its radius, and every named level
/// is cut by rings (no proved arc).
fn assert_refusals_named(reads: &[LevelRead], skin: f64) {
    for r in reads {
        if r.ringed(skin) {
            assert!(r.named, "radius {:.3} is cut by rings without a note naming it", r.rho);
        }
        if r.named {
            assert_eq!(r.arcs, 0, "radius {:.3} is named ring-cut yet posts HSM arcs", r.rho);
        }
    }
}

fn read_levels(path: &Toolpath, top: f64, floor: f64) -> Vec<LevelRead> {
    let mut spec = QualitySpec::new(D);
    spec.cell = D / 60.0;
    spec.break_deg = 5.0;
    let Ok(mut dex) = stock_for(&box_of(path), top, floor, 1.0, &spec) else { panic!("stock") };
    let q = measure(path, &mut dex, &spec);
    // The reader's own bore law (`quality::measure`'s `bores`, as the
    // planar suite pocket3d_hsm reads it): a planar cut inside a helical
    // descent's circle is the bore's cleanup - entry engagement, not the
    // clearing's load.
    let bores: Vec<(Point2, f64)> = path
        .moves
        .iter()
        .filter(|m| (m.to.z - m.from.z).abs() > 1e-9)
        .filter_map(|m| m.arc.map(|a| (a.center, (m.from.x - a.center.x).hypot(m.from.y - a.center.y))))
        .collect();
    let in_bore = |m: &camcraft_motion::cl::Move| {
        bores.iter().any(|&(c, rr)| {
            (m.from.x - c.x).hypot(m.from.y - c.y) <= rr + 1e-6 && (m.to.x - c.x).hypot(m.to.y - c.y) <= rr + 1e-6
        })
    };
    let at = |m: &camcraft_motion::cl::Move| m.kind == MoveKind::Feed && m.from.z == m.to.z;
    let mut zs: Vec<f64> = path.moves.iter().filter(|m| at(m)).map(|m| m.to.z).collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup();
    // The CUT levels only: a plane some cutting descent (plunge, helix,
    // ramp, microlift return) ends on (to the micron: the ladder's level
    // and its descent's end differ in the last bit). A microlift's lifted
    // run is planar too, but nothing descends onto its height.
    zs.retain(|&z| path.moves.iter().any(|m| m.kind != MoveKind::Rapid && (m.to.z - z).abs() < 1e-6 && m.from.z > z + 1e-6));
    zs.iter()
        .map(|&z| {
            let idx: Vec<usize> =
                path.moves.iter().enumerate().filter(|(_, m)| at(m) && m.to.z == z).map(|(i, _)| i).collect();
            let deg = idx.iter().filter(|&&i| !in_bore(&path.moves[i])).filter_map(|&i| q.move_max.get(i)).copied().fold(0.0, f64::max);
            let tag = ring_tag(z);
            LevelRead {
                rho: z,
                moves: idx.len(),
                arcs: idx.iter().filter(|&&i| path.moves[i].arc.is_some()).count(),
                load: load_at_rim(deg, D, q.rim),
                breaks: q.break_list.iter().filter(|(p, t)| p.z == z && *t > 5.0).count(),
                named: path.notes.iter().any(|n| n.ends_with(&tag)),
            }
        })
        .collect()
}

/// The whole turned bar in an r7 bar stock: three developed levels of
/// real material (radius 6.5, 6.0, 5.5) go to the HSM planner. About
/// ten minutes in debug, a minute in release: the heavy lane's.
#[test]
#[ignore = "heavy lane: release --ignored (three HSM plans over the developed bar, ~10 min debug)"]
fn adaptive_plans_the_developed_levels_under_the_load_and_kink_laws() {
    let (f, st) = (bar(), stock(7.0, -16.0, 16.0));
    let p = params(PocketStrategy::Adaptive);
    let dev = match rotary_offset_developed(&f, Some(&st), &p) {
        Ok(t) => t,
        Err(e) => panic!("the developed adaptive ladder: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    // The measuring stock is the clamped bar itself, developed: its top
    // the r7 skin. The ladder's first level stands ON that skin (zero
    // depth); the levels below it cut real stock, each read from the
    // skin above it - round 1's r6.2 stock left only the skin level to
    // the planner and its 0% reading proved nothing.
    let reads = read_levels(&dev, 7.0, 0.0);
    for r in &reads {
        println!(
            "radius {:.3}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, rings {}",
            r.rho, r.moves, r.arcs, r.load * 100.0, r.breaks, r.ringed(7.0)
        );
    }
    println!("notes: {:?}", dev.notes);
    // The planner's own peak load reading: it names every overload it
    // could not avoid (a spiral step out of the entry bore, the plan's
    // worst past its tolerance); a planned developed level names none.
    let over = planner_overloads(&dev.notes);
    assert!(over.is_empty(), "the planner's own load reading broke the law: {over:?}");
    let planned: Vec<&LevelRead> = reads.iter().filter(|r| !r.ringed(7.0) && r.moves > 0).collect();
    assert!(!planned.is_empty(), "the planner plans developed levels: {:?}", dev.notes);
    assert!(
        planned.iter().any(|r| r.rho < 7.0 - 1e-6 && r.arcs > 0),
        "a developed level under the skin is HSM-planned, its proved arcs posted: {:?}",
        dev.notes
    );
    for r in &planned {
        assert!(
            r.load <= DEFAULT_OPTIMAL_LOAD * 1.35,
            "radius {}: load {:.1}% D past {:.1}% + 35%",
            r.rho,
            r.load * 100.0,
            DEFAULT_OPTIMAL_LOAD * 100.0
        );
        assert_eq!(r.breaks, 0, "radius {}: in-cut breaks past 5 deg", r.rho);
    }
    // Every ring-cut level is said by name with its radius.
    assert_refusals_named(&reads, 7.0);
    // The world program: every move posed, every pose the radial of its
    // ball center (the axis stands on world Z here).
    let world = match rotary_offset_in(&f, Some(&st), &p, &control()) {
        Ok(t) => t,
        Err(e) => panic!("the wrapped adaptive rough: {e}"),
    };
    assert!(world.all_finite());
    assert!(world.moves.iter().all(|m| m.arc.is_none()), "no developed arc survives the wrap as an arc");
    let mut posed = 0usize;
    for m in &world.moves {
        let Some(a) = m.axis else { panic!("an unposed move at {:?}", m.to) };
        let c = Point3::new(m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]);
        let l = c.x.hypot(c.y);
        if l < 1e-6 {
            continue;
        }
        let dot = (a[0] * c.x + a[1] * c.y) / l;
        assert!(a[2].abs() < 1e-9 && dot > 1.0 - 1e-9, "a pose off the radial at {:?}", m.to);
        posed += 1;
    }
    assert!(posed > 50, "a whole bar of radial poses: {posed}");
    // The kink law on the POSTED program: the world moves pulled back
    // into the developed frame (each developed arc now its chords), read
    // by the same reader; every planned level holds it there too.
    let pulled = pull_back(&world, 7.0);
    let wreads = read_levels(&pulled, 7.0, 0.0);
    let wbreaks: usize = wreads.iter().filter(|w| planned.iter().any(|r| (r.rho - w.rho).abs() < 1e-5)).map(|w| w.breaks).sum();
    println!("world: {} moves (developed {}), in-cut breaks>5 on the planned levels {wbreaks}", world.moves.len(), dev.moves.len());
    for w in &wreads {
        // (No load read here: the chorded helix is no arc, so the bore
        // law cannot tell the entry's cleanup from the clearing.)
        println!("world radius {:.3}: {} planar moves, breaks>5 {}", w.rho, w.moves, w.breaks);
    }
    assert_eq!(wbreaks, 0, "the posted chords break the 5 deg in-cut law");
}

/// The world program (axis on world Z, through the origin) pulled back
/// into the developed frame: s the unwrapped azimuth at `rho_ref`, v the
/// station, height the tip's radius (to the micron, so a level's moves
/// share it). A radial pose's tip stands on the developed level itself.
fn pull_back(world: &Toolpath, rho_ref: f64) -> Toolpath {
    let micron = |v: f64| (v * 1e6).round() / 1e6;
    let mut th_prev: Option<f64> = None;
    let mut dev = |p: Point3| {
        let raw = p.y.atan2(p.x);
        let th = match th_prev {
            Some(t) => t + (raw - t + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI,
            None => raw,
        };
        th_prev = Some(th);
        Point3::new(th * rho_ref, p.z, micron(p.x.hypot(p.y)))
    };
    let Some(first) = world.moves.first() else { panic!("an empty world program") };
    let mut b = PathBuilder::new(dev(first.from));
    for m in &world.moves {
        b.push(m.kind, dev(m.to), m.feed);
    }
    b.finish()
}

/// A 12 long window of the neck in the r7 bar, a 1 mm stepdown. The pin
/// is lifted - every developed level under the skin reaches the HSM
/// planner, which plans it or refuses it naming its radius, Offset rings
/// cutting the refused ones - and nothing it plans names an overload.
/// Still ~3 min in debug (the skin level's two-lap plan): heavy lane.
#[test]
#[ignore = "heavy lane: release --ignored (the developed window's HSM plan, ~3 min debug)"]
fn the_developed_window_reaches_the_planner_and_names_each_refusal() {
    let (f, st) = (bar(), stock(7.0, -16.0, 16.0));
    let p = Pocket3DParams { top_z: 16.0, bottom_z: 4.0, stepdown: 1.0, ..params(PocketStrategy::Adaptive) };
    let dev = match rotary_offset_developed(&f, Some(&st), &p) {
        Ok(t) => t,
        Err(e) => panic!("the developed window: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    println!("window notes: {:?}", dev.notes);
    let reads = read_levels(&dev, 7.0, 0.0);
    for r in &reads {
        println!(
            "radius {:.3}: {} planar moves ({} arcs), load {:.1}% D, breaks>5 {}, rings {}",
            r.rho, r.moves, r.arcs, r.load * 100.0, r.breaks, r.ringed(7.0)
        );
    }
    let asked = |r: &LevelRead| r.arcs > 0 || r.ringed(7.0);
    assert!(
        reads.iter().any(|r| r.rho < 7.0 - 1e-6 && asked(r)),
        "a developed level reached the HSM planner (the old pin rang every one unnamed): {:?}",
        dev.notes
    );
    assert_refusals_named(&reads, 7.0);
    let over = planner_overloads(&dev.notes);
    assert!(over.is_empty(), "the planner's own load reading broke the law: {over:?}");
    for r in reads.iter().filter(|r| r.arcs > 0) {
        assert!(r.load <= DEFAULT_OPTIMAL_LOAD * 1.35, "radius {}: load {:.1}% D", r.rho, r.load * 100.0);
        assert_eq!(r.breaks, 0, "radius {}: in-cut breaks past 5 deg", r.rho);
    }
}

/// The planner's own overload notes: its worst load past the law, or a
/// spiral step out of the entry bore that could not hold it.
fn planner_overloads(notes: &[String]) -> Vec<&String> {
    notes.iter().filter(|n| n.contains("no spiral step holds the load") || n.contains("worst planned load")).collect()
}

/// FNV-1a over the moves at a 1e-6 lattice: the program, portable (the
/// trig under the wrap differs in its last bits across platforms' libm).
fn digest(t: &Toolpath) -> (usize, u64) {
    let q = |v: f64| (v * 1e6).round() as i64;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for m in &t.moves {
        let arc = m.arc.map(|a| (q(a.center.x), q(a.center.y), a.ccw, format!("{:?}", a.plane)));
        let s = format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}",
            m.kind,
            [q(m.from.x), q(m.from.y), q(m.from.z)],
            [q(m.to.x), q(m.to.y), q(m.to.z)],
            m.feed.map(q),
            arc,
            m.axis.map(|a| a.map(q))
        );
        for b in s.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (t.moves.len(), h)
}

#[test]
fn offset_rings_on_the_wrap_are_the_program_they_always_were() {
    let (f, st) = (bar(), stock(6.2, -16.0, 16.0));
    let t = match rotary_offset_in(&f, Some(&st), &params(PocketStrategy::OffsetRings), &control()) {
        Ok(t) => t,
        Err(e) => panic!("rings: {e}"),
    };
    let got = digest(&t);
    println!("rings digest {got:?}");
    assert_eq!(got, GOLDEN_RINGS, "the OffsetRings wrap program moved");
}

/// Read on a build of db95014 WITHOUT the issue #9 patch (this
/// fixture, Helix entry): the rings path is untouched, byte for byte.
/// (Its Plunge twin read (5194, 6097177489831100083) both there and on
/// round 1's patched build.) Re-pinned on the 1e-6 lattice in the review
/// round: the bitwise digest (5263, 2919766036457321567) still matched
/// on the same build, so the lattice is the only change.
const GOLDEN_RINGS: (usize, u64) = (5263, 11_592_310_142_122_113_043);
