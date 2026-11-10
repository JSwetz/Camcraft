//! Locate the part in a fixture, or the fixture on the part (#561): a
//! mate list - a part face or point against a fixture face or point -
//! read in the frame of the setup the fixture belongs to, solved as the
//! shop's 3-2-1 ([`solve`]), then a typed nudge. The result is one rigid
//! transform in that setup frame that carries the part onto the fixture;
//! `LocatePart` applies it to the part, `LocateFixture` its inverse to
//! the fixture. Pure: nothing here writes the world.
//!
//! Provenance: the 3-2-1 locating principle and face/point datums from
//! machine-tool and fixture-design handbooks; the inside test for the
//! interpenetration warning is the fixture mesh's own ray-crossing law
//! (`camcraft_sim::fixture_mesh`). Our own code, no vendor material.
//!
//! ## The laws here
//!
//! - **The fixture's setup is the frame.** Fixtures are setup furniture
//!   and read their own `Location`; a part reads the setup's alignment
//!   (#326) composed on its document placement - the picks are read
//!   exactly where that setup shows them. The work offset is untouched.
//! - **Refused by name, before anything is built.** No such part or
//!   fixture, a pick on the wrong body, an edge or curved face, a face
//!   with no area, two mates on one point, faces that cannot oppose, a
//!   mate the earlier ones contradict, a non-finite nudge.
//! - **Interpenetration warns, it does not refuse.** A modelled vise is
//!   often drawn closed on a nominal part; the move is the user's call
//!   and the gate still judges every toolpath against the steel.

#[path = "fixture_locate_solve.rs"]
mod solve_impl;

pub use solve_impl::{LEN_TOL_MM, LocateOffset, MAX_MATES, NORMAL_TOL, solve};

use flecs_ecs::prelude::{Entity, World};

use crate::fixture::{self, MACHINE_TABLE};
use crate::geometry::{Point3, Triangle, transform_point};
use crate::model;
use crate::operation::GeoSel;
use crate::pick::mate_datum;
use crate::schema::Location;
use crate::setup_align;

/// One mate: a pick on the part against a pick on the fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct Mate {
    pub part: GeoSel,
    pub fixture: GeoSel,
}

/// Which body the locate moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mover {
    /// The part goes into the fixture (the stock follows it).
    #[default]
    Part,
    /// The fixture comes to the part (the part keeps its place).
    Fixture,
}

/// Everything a locate is asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocateSpec {
    pub part: String,
    pub fixture: String,
    pub mates: Vec<Mate>,
    pub offset: LocateOffset,
}

/// A solved locate: the fixture's declared setup binding and the rigid
/// transform (setup frame) that carries the part onto the fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub setup: Option<String>,
    pub delta: Location,
}

/// The fixture by name, refused by name when it is not one.
fn require_fixture(world: &World, name: &str) -> Result<Entity, String> {
    if name == MACHINE_TABLE {
        return Err("the machine table has no faces to mate - import the vise or fixture and mark it".into());
    }
    if name.trim().is_empty() {
        return Err("no fixture chosen - import one and mark it on the Fixtures page".into());
    }
    let e = model::find_by_name(world, name).ok_or_else(|| format!("no fixture named '{name}'"))?;
    if !model::is_fixture(world, &e) {
        return Err(format!("'{name}' is not a fixture - mark it on the Fixtures page first"));
    }
    Ok(e)
}

/// The part by name, refused by name when it is not one.
fn require_part(world: &World, name: &str) -> Result<Entity, String> {
    if name.trim().is_empty() {
        return Err("no part to locate - import a model first".into());
    }
    let e = model::find_by_name(world, name).ok_or_else(|| format!("no model named '{name}'"))?;
    if !setup_align::is_part(world, &e) {
        return Err(format!("'{name}' is not a part - pick the model the fixture holds"));
    }
    Ok(e)
}

/// Read and solve a locate. Every refusal names what to change.
pub fn plan(world: &World, spec: &LocateSpec) -> Result<Plan, String> {
    let fe = require_fixture(world, &spec.fixture)?;
    let pe = require_part(world, &spec.part)?;
    let setup = fixture::def_of(world, &fe).and_then(|d| d.setup);
    let part_pose = setup_align::pose_in(world, &pe, setup.as_deref()).unwrap_or_default();
    let fixture_pose = model::transform_of(world, &fe).unwrap_or_default();
    let mut pairs = Vec::with_capacity(spec.mates.len());
    for (i, m) in spec.mates.iter().enumerate() {
        let k = i + 1;
        if m.part.entity != spec.part {
            return Err(format!("mate {k}: {} of '{}' is not on the part '{}'", m.part.feature, m.part.entity, spec.part));
        }
        if m.fixture.entity != spec.fixture {
            return Err(format!(
                "mate {k}: {} of '{}' is not on the fixture '{}'",
                m.fixture.feature, m.fixture.entity, spec.fixture
            ));
        }
        let p = mate_datum(world, &m.part, part_pose).map_err(|e| format!("mate {k}: {e}"))?;
        let f = mate_datum(world, &m.fixture, fixture_pose).map_err(|e| format!("mate {k}: {e}"))?;
        pairs.push((p, f));
    }
    let delta = solve(&pairs, spec.offset)?;
    Ok(Plan { setup, delta })
}

/// One pick read where the locate reads it - the frame of the setup
/// `fixture` belongs to - for the mate list's plain words (#561 review):
/// a face's outward normal, a point's coordinates. Refused by name as
/// [`mate_datum`] refuses.
pub fn read_pick(world: &World, fixture: &str, g: &GeoSel) -> Result<crate::pick::MateDatum, String> {
    let setup = model::find_by_name(world, fixture).and_then(|fe| fixture::def_of(world, &fe)).and_then(|d| d.setup);
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| format!("no model named '{}'", g.entity))?;
    let pose = setup_align::pose_in(world, &e, setup.as_deref()).unwrap_or_default();
    mate_datum(world, g, pose)
}

/// Is this transform the identity (nothing would move)?
#[must_use]
pub fn is_still(delta: Location) -> bool {
    let p = delta.pos;
    p.x.abs() <= 1e-9 && p.y.abs() <= 1e-9 && p.z.abs() <= 1e-9 && setup_align::is_identity_rot(delta.rot)
}

/// How deep (mm) a sample must sit inside the other body to count: the
/// mated faces touch by design, and touching is not interpenetrating.
pub const PENETRATION_MM: f64 = 0.05;
/// At most this many facet samples per body (a stride beyond).
const MAX_SAMPLES: usize = 20_000;

/// The warning, by name, when the part would sit inside the fixture after
/// `delta` (relative placement - the same whichever body moves). A
/// sampled check: each facet's centre pushed [`PENETRATION_MM`] into its
/// own body, tested inside the other's closed mesh. `None` when clear, or
/// when a mesh is open and cannot say (logged).
#[must_use]
pub fn interpenetration(world: &World, spec: &LocateSpec, plan: &Plan) -> Option<String> {
    let fe = model::find_by_name(world, &spec.fixture)?;
    let pe = model::find_by_name(world, &spec.part)?;
    let part: Vec<Triangle> = setup_align::facets_in(world, &pe, plan.setup.as_deref())?
        .iter()
        .map(|t| place(*t, plan.delta))
        .collect();
    let steel = model::world_facets(world, &fe)?;
    let hit = sunk(&part, &steel).or_else(|| sunk(&steel, &part))?;
    Some(format!(
        "'{}' would sit inside the fixture '{}' (near {:.2}, {:.2}, {:.2}) - check the mates and the offset",
        spec.part, spec.fixture, hit[0], hit[1], hit[2]
    ))
}

/// The warning, by name, when the setup's stock would sink into the
/// fixture where the locate leaves it (#561) - a blank with an allowance
/// under the part mated on the floor goes through the floor. `stocks`
/// are the moved stock bodies (setup frame) with the fixture where it
/// will stand; the same sampled check as [`interpenetration`].
#[must_use]
pub fn stock_interpenetration(world: &World, spec: &LocateSpec, plan: &Plan, stocks: &[(String, Vec<Triangle>)]) -> Option<String> {
    let fe = model::find_by_name(world, &spec.fixture)?;
    let steel = model::world_facets(world, &fe)?;
    let own = crate::setup::stock_entity_name(world, plan.setup.as_deref());
    stocks.iter().filter(|(n, _)| *n == own).find_map(|(name, body)| {
        let hit = sunk(body, &steel).or_else(|| sunk(&steel, body))?;
        Some(format!(
            "the stock '{name}' would sink into the fixture '{}' (near {:.2}, {:.2}, {:.2}) - check its allowance \
             under the mated faces, or set the stock again",
            spec.fixture, hit[0], hit[1], hit[2]
        ))
    })
}

/// The setup's stock as it stands, carried by `delta` - the relative
/// placement a `LocateFixture` gives it against the moved fixture.
#[must_use]
pub fn stock_carried(world: &World, plan: &Plan) -> Vec<(String, Vec<Triangle>)> {
    let name = crate::setup::stock_entity_name(world, plan.setup.as_deref());
    model::find_by_name(world, &name)
        .and_then(|e| model::world_facets(world, &e))
        .map(|t| vec![(name, t.into_iter().map(|t| place(t, plan.delta)).collect())])
        .unwrap_or_default()
}

/// Both warnings, joined.
#[must_use]
pub fn join_warnings(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(format!("{a}; {b}")),
        (a, b) => a.or(b),
    }
}

fn place(t: Triangle, d: Location) -> Triangle {
    Triangle::new(
        transform_point(t.a, d.pos, d.rot),
        transform_point(t.b, d.pos, d.rot),
        transform_point(t.c, d.pos, d.rot),
    )
}

/// A sample of `body` sunk inside the closed `other`, if any.
fn sunk(body: &[Triangle], other: &[Triangle]) -> Option<[f64; 3]> {
    let mesh = match camcraft_sim::fixture_mesh::FixtureMesh::build(other) {
        Ok(m) => m,
        Err(why) => {
            tracing::info!("locate interpenetration not judged: {why}");
            return None;
        }
    };
    let stride = body.len().div_ceil(MAX_SAMPLES).max(1);
    body.iter().step_by(stride).find_map(|t| {
        let n = (t.b - t.a).cross(t.c - t.a);
        let l = n.dot(n).sqrt();
        if !(l.is_finite() && l > 1e-12) {
            return None;
        }
        // Outward by winding: step against it, into the body.
        let s = -PENETRATION_MM / l;
        let c = Point3::new(
            (t.a.x + t.b.x + t.c.x) / 3.0 + n.x * s,
            (t.a.y + t.b.y + t.c.y) / 3.0 + n.y * s,
            (t.a.z + t.b.z + t.c.z) / 3.0 + n.z * s,
        );
        mesh.contains(c).then_some([c.x, c.y, c.z])
    })
}
