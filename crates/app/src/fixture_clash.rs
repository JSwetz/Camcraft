//! Part-and-stock-versus-fixture interpenetration (#561, #582): the one
//! sampled check a locate, a Model-page `MoveModel` and a Fixtures-page
//! `MoveFixture` all run before they write, and the by-name warning it
//! gives - the body, the fixture and a point near the overlap.
//!
//! Provenance: the inside test is the fixture mesh's own ray-crossing law
//! (`camcraft_sim::fixture_mesh`); sampling facet centres a hair inside
//! their own body is our own design. No vendor material.
//!
//! ## The laws here
//!
//! - **A warning, never a refusal.** A modelled vise is often drawn
//!   closed on a nominal part; the move is the user's call and the gate
//!   still judges every toolpath against the steel.
//! - **Touching is not interpenetrating.** A sample counts only
//!   [`PENETRATION_MM`] deep inside the other closed body, so mated faces
//!   stay quiet.
//! - **An open mesh cannot say.** It is logged and judged clear.
//! - **The fixture's setup is the frame.** Fixtures and stock are setup
//!   furniture read at their own `Location`; a part reads the setup's
//!   alignment (#326) composed on its document placement.
//! - **No fixtures, no check.** A setup without tagged workholding costs
//!   nothing.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle, transform_point};
use crate::model;
use crate::schema::Location;
use crate::setup_align;

/// How deep (mm) a sample must sit inside the other body to count: the
/// mated faces touch by design, and touching is not interpenetrating.
pub const PENETRATION_MM: f64 = 0.05;
/// At most this many facet samples per body (a stride beyond).
const MAX_SAMPLES: usize = 20_000;

/// The hint a plain move's part warning ends with.
const MOVE_HINT: &str = "check the move";
/// The hint a plain move's stock warning ends with.
const MOVE_STOCK_HINT: &str = "check the move, or the stock's allowance";

/// `t` carried by the rigid transform `d`.
#[must_use]
pub fn place(t: Triangle, d: Location) -> Triangle {
    Triangle::new(
        transform_point(t.a, d.pos, d.rot),
        transform_point(t.b, d.pos, d.rot),
        transform_point(t.c, d.pos, d.rot),
    )
}

/// A point where `body` and the closed `steel` interpenetrate - a sample
/// of either sunk inside the other - if any.
#[must_use]
pub fn clash(body: &[Triangle], steel: &[Triangle]) -> Option<[f64; 3]> {
    sunk(body, steel).or_else(|| sunk(steel, body))
}

/// The part warning, by name, with the caller's hint.
#[must_use]
pub fn part_message(part: &str, fixture: &str, hit: [f64; 3], hint: &str) -> String {
    format!(
        "'{part}' would sit inside the fixture '{fixture}' (near {:.2}, {:.2}, {:.2}) - {hint}",
        hit[0], hit[1], hit[2]
    )
}

/// The stock warning, by name, with the caller's hint.
#[must_use]
pub fn stock_message(stock: &str, fixture: &str, hit: [f64; 3], hint: &str) -> String {
    format!(
        "the stock '{stock}' would sink into the fixture '{fixture}' (near {:.2}, {:.2}, {:.2}) - {hint}",
        hit[0], hit[1], hit[2]
    )
}

/// Warnings joined into one status line (duplicates dropped).
#[must_use]
pub fn join(mut all: Vec<String>) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    all.retain(|w| seen.insert(w.clone()));
    (!all.is_empty()).then(|| all.join("; "))
}

/// Every setup context of the document: each setup, or the implicit one.
fn contexts(world: &World) -> Vec<Option<String>> {
    let names = crate::setup::names(world);
    if names.is_empty() { vec![None] } else { names.into_iter().map(Some).collect() }
}

/// After a Model-page move (#582): the part `part` placed at `at` (its
/// document `Location`) and the following stock bodies `stocks` where the
/// move leaves them, judged against every fixture of every setup. `None`
/// when clear or when no setup has a fixture.
#[must_use]
pub fn after_part_move(world: &World, part: &str, at: Location, stocks: &[(String, Vec<Triangle>)]) -> Option<String> {
    let entity = model::find_by_name(world, part)?;
    if !setup_align::is_part(world, &entity) {
        return None;
    }
    let mut warnings = Vec::new();
    let mut tris: Option<std::sync::Arc<[Triangle]>> = None;
    for ctx in contexts(world) {
        let steel_list = crate::fixture::tagged(world, ctx.as_deref());
        if steel_list.is_empty() {
            continue;
        }
        if tris.is_none() {
            tris = model::facets_of(world, &entity);
        }
        // A part without facets has no body to judge; its stock still is,
        // and warnings from earlier setups stand.
        let pose = setup_align::alignment(world, ctx.as_deref()).map_or(at, |a| setup_align::compose(a, at));
        let body: Option<Vec<Triangle>> = tris.as_ref().map(|raw| raw.iter().map(|t| place(*t, pose)).collect());
        let own = crate::setup::stock_entity_name(world, ctx.as_deref());
        for (fname, fe) in steel_list {
            let Some(steel) = model::world_facets(world, &fe) else { continue };
            if let Some(hit) = body.as_deref().and_then(|b| clash(b, &steel)) {
                warnings.push(part_message(part, &fname, hit, MOVE_HINT));
            }
            for (sname, sbody) in stocks.iter().filter(|(n, _)| *n == own) {
                if let Some(hit) = clash(sbody, &steel) {
                    warnings.push(stock_message(sname, &fname, hit, MOVE_STOCK_HINT));
                }
            }
        }
    }
    join(warnings)
}

/// After a Fixtures-page move (#582): the fixture `fixture` placed at
/// `at`, judged against every part and the stock of its setup.
#[must_use]
pub fn after_fixture_move(world: &World, fixture: &str, at: Location) -> Option<String> {
    let fe = model::find_by_name(world, fixture)?;
    let ctx = crate::fixture::def_of(world, &fe).and_then(|d| d.setup);
    let steel: Vec<Triangle> = model::facets_of(world, &fe)?.iter().map(|t| place(*t, at)).collect();
    let mut warnings = Vec::new();
    for (pname, pe) in model::model_entities(world) {
        if !setup_align::is_part(world, &pe) {
            continue;
        }
        let Some(body) = setup_align::facets_in(world, &pe, ctx.as_deref()) else { continue };
        if let Some(hit) = clash(&body, &steel) {
            warnings.push(part_message(&pname, fixture, hit, "check the fixture's place"));
        }
    }
    let own = crate::setup::stock_entity_name(world, ctx.as_deref());
    if let Some(body) = model::find_by_name(world, &own).and_then(|e| model::world_facets(world, &e))
        && let Some(hit) = clash(&body, &steel)
    {
        warnings.push(stock_message(&own, fixture, hit, "check the fixture's place, or the stock's allowance"));
    }
    join(warnings)
}

/// A sample of `body` sunk inside the closed `other`, if any.
fn sunk(body: &[Triangle], other: &[Triangle]) -> Option<[f64; 3]> {
    let mesh = match camcraft_sim::fixture_mesh::FixtureMesh::build(other) {
        Ok(m) => m,
        Err(why) => {
            tracing::info!("fixture interpenetration not judged: {why}");
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
