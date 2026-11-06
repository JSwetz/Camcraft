//! The rotary verdict (M13): the three rotary kinds over the setup a
//! user builds for them - the turned bar, a coaxial round bar stock a
//! hair over the part's OD, a 6 mm ball - verify SILENT and post on
//! the tilting head; the same programs over steel they really drive
//! through (an oversized bar, a clearance plane inside the stock, a
//! rough asked to hop through skin it never peels) still convict and
//! block. And the pose law the verdicts stand on: the ball the stream
//! puts at the tip-plus-r-along-the-axis is the ball the ladder
//! planned, tangent to the skin - not one radius off it.
//! Provenance: our own fixtures (the operations suite's turned bar);
//! the law is the Prime Directive's "correct, or visibly refused".
//!
//! Gate map (M4.4): covers app::operation, app::posting, app::verify, app::multiaxis.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::{quat_from_axis_angle, Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const R: f64 = 3.0; // the BN-0600's ball
/// The turned bar's meridian (rho, z): wall r6, shoulder, neck r5.
const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

fn revolve(n: usize) -> Vec<Triangle> {
    let pt = |rho: f64, z: f64, j: usize| {
        let th = std::f64::consts::TAU * j as f64 / n as f64;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..n {
            let k = (j + 1) % n;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
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

/// The bar laid down: axis world x, crown world z.
fn laid() -> Vec<Triangle> {
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    revolve(16).iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Contour,
    Parallel,
    Offset,
}

fn params(kind: Kind, clearance: f64) -> OpParams {
    match kind {
        Kind::Contour => OpParams::RotaryContour {
            tolerance: 0.1,
            clearance_z: Height::fixed(clearance),
            retract_z: None,
            top_z: Height::fixed(14.0),
            bottom_z: Height::fixed(-14.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
        },
        // The crown and upper flanks: a laid bar's passes stay on the
        // half the head can reach from above (B never past +-90).
        Kind::Parallel => OpParams::RotaryParallel {
            tolerance: 0.1,
            stepover: Some(2.0),
            clearance_z: Height::fixed(clearance),
            retract_z: None,
            top_z: Height::fixed(6.0),
            bottom_z: Height::fixed(1.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
        },
        Kind::Offset => OpParams::RotaryOffset {
            links: camcraft_app::links::Links::ROUGH3D,
            tolerance: 0.1,
            stepover: 4.0,
            stock_to_leave: 0.5,
            clearance_z: Height::fixed(clearance),
            top_z: Height::fixed(14.0),
            bottom_z: Height::fixed(-14.0),
            stepdown: 0.5,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            strategy: camcraft_app::machining2d::PocketStrategy::Auto,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
        },
    }
}

/// A coaxial round bar of `dia` over the part's 32 mm length - the
/// stock a rotary job is clamped from (standing for the contour and
/// the rough, laid along x for the parallel).
fn world_for(kind: Kind, dia: f64, clearance: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let lying = kind == Kind::Parallel;
    let loc = if lying {
        Location { pos: Point3::new(-16.0, 0.0, 0.0), rot: quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2) }
    } else {
        Location { pos: Point3::new(0.0, 0.0, -16.0), rot: IDENTITY_Q }
    };
    let stock = SetStock::new(&world, StockShape::Cylinder { diameter: dia, height: 32.0 }, loc);
    h.execute(&world, stock.expect("bar stock"));
    let mesh = if lying { laid() } else { revolve(16) };
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(mesh)));
    let op = CreateOperation::new(&world, "OP", "BN-0600", SrcRef::Entity("bar".into()), params(kind, clearance));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    // The tilting head owns the letters; its Z stroke runs -500..0, so
    // G54 sits above the work (the tilt suite's parked-anchor law).
    let asset = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tilthead.mch");
    h.execute(&world, LoadMachine::from_file(&world, &asset).expect("mount"));
    let g54 = Location { pos: Point3::new(0.0, 0.0, 60.0), rot: IDENTITY_Q };
    h.execute(&world, SetWcsOrigin::new(&world, "G54", g54).expect("G54"));
    world
}

fn cache(world: &World) -> camcraft_app::schema::Toolpath {
    model::find_by_name(world, "OP").and_then(|e| model::toolpath_of(world, &e)).expect("cached")
}

fn status(world: &World) -> OpStatus {
    let e = model::find_by_name(world, "OP").expect("op");
    operation::status_of(world, &e)
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-rotary-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// Signed distance of a world point to the bar's meridian polyline
/// (negative inside): the analytic skin the 16-turn facets approach.
fn skin(p: Point3, lying: bool) -> f64 {
    let (rho, ax) = if lying { ((p.y * p.y + p.z * p.z).sqrt(), p.x) } else { ((p.x * p.x + p.y * p.y).sqrt(), p.z) };
    let mut best = f64::INFINITY;
    for w in MERIDIAN.windows(2) {
        let ((r0, z0), (r1, z1)) = (w[0], w[1]);
        let (dx, dz) = (r1 - r0, z1 - z0);
        let t = (((rho - r0) * dx + (ax - z0) * dz) / (dx * dx + dz * dz)).clamp(0.0, 1.0);
        best = best.min(((rho - r0 - t * dx).powi(2) + (ax - z0 - t * dz).powi(2)).sqrt());
    }
    let inside = rho < 6.0 && ax.abs() < 16.0 && (rho < 5.0 || ax < 2.0);
    if inside { -best } else { best }
}

/// The ball the stream places - tip plus r ALONG THE POSE - stands on
/// the skin: within the 16-gon's chord dip (r6 * (1 - cos 11.25°) =
/// 0.115 mm, the analytic circle vs the facets the ladder rode) plus
/// the kind's 0.1 tolerance of tangency, never a radius off it.
/// `leave` is the kind's planned wall (the rough's ladder floor stands
/// up to one infeed plus its leave above the skin).
fn ball_rides_the_skin(kind: Kind, lying: bool, leave: f64) {
    const SLACK: f64 = 0.115 + 0.1 + 0.01;
    let world = world_for(kind, 12.4, 25.0);
    let t = cache(&world).path;
    let mut n = 0usize;
    let mut far = 0usize;
    for m in t.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let Some(a) = m.axis else { continue };
        let c = Point3::new(m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]);
        let gap = skin(c, lying) - R;
        assert!(gap > -SLACK, "the ball sinks {gap:.3} mm into the skin at {:?}", m.to);
        if gap.abs() < 21.0 {
            n += 1;
            if gap > leave + SLACK {
                far += 1;
            }
        }
    }
    assert!(n > 50, "a whole bar of contact moves, got {n}");
    // Passes run off the bar's ends into air (the raster's overrun),
    // so "far" is measured against the moves near the work only - and
    // most of them must be ON the planned wall.
    assert!(far * 4 < n, "{far} of {n} cut moves stand off the planned wall - the pose turned about the tip");
}

#[test]
fn the_rotary_ball_is_the_planned_ball() {
    ball_rides_the_skin(Kind::Contour, false, 0.0);
    ball_rides_the_skin(Kind::Parallel, true, 0.0);
    ball_rides_the_skin(Kind::Offset, false, 1.0);
}

fn posts_clean(kind: Kind, tag: &str) {
    let world = world_for(kind, 12.4, 25.0);
    assert_eq!(status(&world), OpStatus::Ok, "{tag}: fresh");
    let c = cache(&world);
    assert!(c.warnings.is_empty(), "{tag}: a sensible rotary setup verifies silent: {:?}", &c.warnings[..c.warnings.len().min(6)]);
    let rapids_posed = c.path.moves.iter().filter(|m| m.kind == MoveKind::Rapid).all(|m| m.axis.is_some());
    assert!(rapids_posed, "{tag}: every rapid states the pose the machine holds (the tilt is modal)");
    let report = posting::verification_report(&world);
    assert!(report.problems.is_empty(), "{tag}: {:?}", report.problems);
    let path = tmp_nc(tag);
    let posted = posting::post_setup(&world, &path).expect("a clean rotary program posts");
    assert!(posted.program.contains("G43.4"), "{tag}: RTCP armed for the tilted records");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rotary_contour_on_a_round_bar_verifies_clean_and_posts() {
    posts_clean(Kind::Contour, "rc");
}

#[test]
fn rotary_parallel_on_a_laid_bar_verifies_clean_and_posts() {
    posts_clean(Kind::Parallel, "rp");
}

#[test]
fn rotary_offset_on_a_round_bar_verifies_clean_and_posts() {
    posts_clean(Kind::Offset, "ro");
}

fn blocks(world: &World, tag: &str) {
    let path = tmp_nc(tag);
    let err = posting::post_setup(world, &path).expect_err("a convicted rotary program never posts");
    assert!(!err.is_empty(), "{tag}: the refusal names its reason");
    assert!(!path.exists(), "{tag}: nothing written");
}

#[test]
fn a_finish_ring_buried_in_an_oversized_bar_convicts_its_shank() {
    // d50 over a d12 part: the flute reaches 15 mm out from the skin,
    // the shank band starts there - inside 25 mm of uncut bar.
    let world = world_for(Kind::Contour, 50.0, 40.0);
    let w = cache(&world).warnings;
    assert!(
        w.iter().any(|w| matches!(w, Warning::ShankHit { part: false, .. })),
        "the shank in uncut bar must convict: {:?}",
        &w[..w.len().min(6)]
    );
    blocks(&world, "rc-big");
}

#[test]
fn a_clearance_plane_inside_the_bar_convicts_the_rapids() {
    // Clearance 10 under a stock top of 16: the ring-to-ring hops run
    // through the bar's top that no ring above them ever cut.
    let world = world_for(Kind::Contour, 12.4, 10.0);
    let w = cache(&world).warnings;
    assert!(
        w.iter().any(|w| matches!(w, Warning::RapidInStock { .. })),
        "rapids through uncut stock must convict: {:?}",
        &w[..w.len().min(6)]
    );
    blocks(&world, "rc-low");
}

#[test]
fn a_rough_hopping_through_skin_it_never_peels_convicts() {
    // The wrap ladder peels from the PART's OD inward; a d20 bar's
    // skin from r6 to r10 is never cut, and the hops one infeed and a
    // radius outside the part's skin run straight through it.
    let world = world_for(Kind::Offset, 20.0, 25.0);
    let w = cache(&world).warnings;
    assert!(
        w.iter().any(|w| matches!(w, Warning::RapidInStock { .. })),
        "the wrap hops through uncut skin must convict: {:?}",
        &w[..w.len().min(6)]
    );
    blocks(&world, "ro-big");
}
