//! The rotary suite's turned bar (M13.1, the rotary lift): the
//! revolved d12.4 bar standing on +Z (Contour, Offset) or laid along
//! +X (Parallel), its cylinder stock, the BN-0600 ball, and each
//! rotary kind's params - the fixture `rotary_field_parity.rs` reads
//! the field against the radial law on. Moved here by the 400-line
//! law; a pure move. Provenance: our own fixtures and engines;
//! nothing from vendor binaries.

use std::time::Instant;

use flecs_ecs::prelude::World;

use camcraft_app::cl::Move;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{quat_from_axis_angle, Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{Backend, RotaryChordStats, SimOp, Stock, FIELD_BAND};
use camcraft_app::stock_view::subdivide;
use camcraft_app::tools::catalog;
use camcraft_app::verify::RotaryAxis;

/// The ball's radius (BN-0600).
pub const R: f64 = 3.0;
/// The stock bar's radius.
pub const BAR_R: f64 = 6.2;
/// Half the bar's length.
pub const HALF_L: f64 = 16.0;
/// The field's cell.
pub const CELL: f64 = 0.5;
/// The bar's meridian, (rho, z): a d12 shank necked to d10 past z = 2.
pub const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

pub fn revolve(n: usize) -> Vec<Triangle> {
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

pub fn laid() -> Vec<Triangle> {
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    revolve(16).iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Contour,
    Parallel,
    Offset,
}

impl Kind {
    /// Whether the bar lies along +X (Parallel) or stands on +Z.
    #[must_use]
    pub fn lying(self) -> bool {
        self == Kind::Parallel
    }
}

/// The rotary suite's params; the standing kinds run their rings to
/// the bar's end (top 16) so the end cap is cut and every column in
/// the neck's annulus reads a sphere-cut floor from above.
pub fn params(kind: Kind) -> OpParams {
    match kind {
        Kind::Contour => OpParams::RotaryContour {
            tolerance: 0.1,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(16.0),
            bottom_z: Height::fixed(-14.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
        },
        Kind::Parallel => OpParams::RotaryParallel {
            tolerance: 0.1,
            stepover: Some(2.0),
            clearance_z: Height::fixed(25.0),
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
            clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(16.0),
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

/// A world with the bar as stock and model, the ball on the rack, and
/// the kind's op "OP" generated.
pub fn world_for(kind: Kind) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let lying = kind.lying();
    let loc = if lying {
        Location { pos: Point3::new(-HALF_L, 0.0, 0.0), rot: quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2) }
    } else {
        Location { pos: Point3::new(0.0, 0.0, -HALF_L), rot: IDENTITY_Q }
    };
    let stock = SetStock::new(&world, StockShape::Cylinder { diameter: 2.0 * BAR_R, height: 2.0 * HALF_L }, loc);
    h.execute(&world, stock.expect("bar stock"));
    let mesh = if lying { laid() } else { revolve(16) };
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(mesh)));
    let op = CreateOperation::new(&world, "OP", "BN-0600", SrcRef::Entity("bar".into()), params(kind));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

/// The op's cached moves and the axis the plan carries.
pub fn program(world: &World) -> (Vec<Move>, RotaryAxis) {
    let e = model::find_by_name(world, "OP").expect("op");
    let moves = model::toolpath_of(world, &e).expect("cached").path.moves;
    let op = model::operation_of(world, &e).expect("operation");
    (moves, camcraft_app::fixture::rotary_axis_of(world, &op).expect("a fitted axis"))
}

/// The setup's stock as the eye builds it (the entity's own mesh).
pub fn field_stock(world: &World, cell: f64) -> Stock {
    let name = camcraft_app::setup::stock_entity_name(world, None);
    let e = model::find_by_name(world, &name).expect("stock");
    let f = model::world_facets(world, &e).expect("stock mesh");
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &subdivide(&f, cell), cell).expect("the bar as a field")
}

/// The program chorded through the field at `pose_tol`: the cut
/// stock, the pre-pass's stats, the wall time.
pub fn run(world: &World, cell: f64, pose_tol: f64) -> (Stock, RotaryChordStats, std::time::Duration) {
    let (moves, ax) = program(world);
    let mut s = field_stock(world, cell);
    let op = [SimOp { moves: &moves, radius: R, ball: true, cone: None, corner: None }];
    let t0 = Instant::now();
    let stats = s.simulate_rotary(&op, &ax, pose_tol).expect("the field chords the turn");
    (s, stats, t0.elapsed())
}
