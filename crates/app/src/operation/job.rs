//! The world-free half of a generation (M5.3 B): [`collect_job`] reads
//! everything [`super::generate`] ever reads from the world and packs it
//! into a [`GenJob`] of plain owned data; [`super::run_job`] turns the
//! job into the path without ever naming a `World`. The type system is
//! the law here: `&World` is not `Send`, so a job that compiles into a
//! `Send` struct cannot smuggle a live read into a worker thread, and
//! the collect phase is where every read provably happened.
//!
//! Determinism, the whole point of the seam: a constructor's world is
//! frozen (constructors only read - the command writes nothing until
//! `apply`), so running the jobs of a batch in any order - or in
//! parallel - produces exactly the parts the serial loop produced.
//! `rayon`'s `into_par_iter().collect()` preserves input order, so even
//! the `parts`/`skipped` lists come out in tree order, byte-for-byte
//! the old sequential result.

use std::sync::Arc;

use super::*;

/// Everything one generation reads from the world, and nothing else.
/// The world-touching precomputes ride as stored `Result`s, taken at
/// their generation-time call sites, so a batch surfaces exactly the
/// error the serial path surfaces, in the same precedence.
pub struct GenJob {
    pub(super) tool: crate::tools::Tool,
    /// The op's tool NAME as the component stored it - the error
    /// strings of the tilted kinds quote it verbatim.
    pub(super) tool_name: String,
    pub(super) params: OpParams,
    pub(super) src: SrcRef,
    pub(super) material: Material,
    pub(super) frame: Option<crate::cl::TiltedFrame>,
    /// Resolved heights, UNshifted - the frame's `oz` fold is pure
    /// arithmetic and belongs to `run_job`.
    pub(super) hs: ResolvedHeights,
    pub(super) stock_facets: Option<Vec<Triangle>>,
    pub(super) stock_aabb: Option<(Point3, Point3)>,
    pub(super) section_z: Option<Result<f64, GenError>>,
    pub(super) picked_region: Option<Result<Region, GenError>>,
    pub(super) contour_sel:
        Option<Result<super::selection::SelectionGeometry, GenError>>,
    pub(super) drive_chains: Option<Result<Vec<Vec<Point3>>, GenError>>,
    pub(super) prior: Option<Result<Arc<crate::rest::PriorState>, GenError>>,
    pub(super) is_mill: bool,
    pub(super) spindle: Option<(f64, f64)>,
    pub(super) fixtures: Vec<crate::fixture::FixtureBox>,
    /// Whether `material` is a finished PART (a named model, or picks
    /// made only on models) rather than the setup's stock: the depth-
    /// aware verify judges the 2D kinds against its solid only then -
    /// cutting into stock is the job, cutting into the part never is.
    pub(super) part_solid: bool,
}

/// The tripwire phrase for a stored precompute whose arm never collected
/// it - a call-structure drift, which is a bug, and it names itself
/// rather than cutting a silent shadow (Prime Directive).
pub(super) fn unreached(what: &str) -> GenError {
    GenError::Invalid(format!("internal: the job's {what} was never collected"))
}

/// Read the world once, as `generate` would, and pack the answers.
/// Errors keep their generation-time precedence: everything that can
/// refuse before a single region is built (tool, material, heights,
/// the tilt refusals) refuses HERE, exactly as the old head refused;
/// everything an arm used to refuse at lives on as a stored `Result`
/// for `run_job` to take at that exact statement.
pub fn collect_job(world: &World, op: &Operation, op_name: &str) -> Result<GenJob, GenError> {
    // Tool: the diameter drives the path, the body drives the reach law.
    let Some(tool_entity) = model::find_by_name(world, &op.tool) else {
        return Err(GenError::Invalid(format!("no tool named '{}'", op.tool)));
    };
    let Some(tool) = model::tool_of(world, &tool_entity) else {
        return Err(GenError::Invalid(format!("'{}' is not a tool", op.tool)));
    };
    let setup = op.setup.as_deref();
    let material = material_of(world, setup, &op.src)?;
    let stock_name = crate::setup::stock_entity_name(world, setup);
    let part_solid = !material.facets.is_empty()
        && match &op.src {
            SrcRef::Stock => false,
            SrcRef::Entity(n) => *n != stock_name,
            SrcRef::Selection(items) => {
                !items.is_empty() && items.iter().all(|g| g.entity != stock_name)
            }
        };
    // Setup stock, when it is not the source itself: the swarf column
    // around a smaller model (the head read at `generate` - same call).
    let stock_facets = match &op.src {
        SrcRef::Stock => None,
        SrcRef::Entity(_) | SrcRef::Selection(_) => {
            model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
                .and_then(|e| model::world_facets(world, &e))
        }
    };
    let stock_aabb = model::stock_world_aabb_for(world, setup).map(|b| (b.min, b.max));

    // The height seam, refused by name before anything else (as the old
    // head refused it, `resolve_params` being the first fallible read
    // after the material).
    let hs = resolve_params(world, setup, Some(&op.src), &op.params).map_err(GenError::Invalid)?;

    // The 3+2 wrap (M5.1 B), same refusals, same order: non-finite
    // letters first, then the tilted-selection lock.
    let frame = match op.params.tilt() {
        None => None,
        Some(t) => match t.frame() {
            Some(f) => Some(f),
            // Only a non-finite letter could get here (the parameter
            // door refuses it); naming it beats inventing a plane.
            None => {
                return Err(GenError::Invalid(
                    "the 3+2 index commands no direction - its angles are not finite".into(),
                ));
            }
        },
    };
    // The one refusal the wrap owes (M5.1 B): a picked selection is
    // read as world geometry by the selection walk - projecting a pick
    // onto a tilted plane is not implemented, and a shadow of the pick
    // is not what was picked. Refused by name, never silently cut as
    // a projection.
    if frame.is_some() && matches!(op.src, SrcRef::Selection(_)) {
        return Err(GenError::Invalid(
            "a tilted plane cannot yet index a picked selection - pick a whole model instead"
                .into(),
        ));
    }

    // The arm-scoped world reads, precomputed under the same guards
    // that decide which arm runs them; each result surfaces at its
    // arm's own statement in `run_job`.
    let section_z = section_z_of(world, setup, Some(&op.src), &op.params);
    let picked_region = match (&op.params, &op.src) {
        // Contour's Selection pair is resolved as raw geometry below
        // (its arm never asks for a region first); every other kind
        // whose picks author the region takes selection_region's
        // answer.
        (params, SrcRef::Selection(items))
            if selection_drives_region(params)
                && !matches!(params, OpParams::Contour { .. }) =>
        {
            Some(selection_region(world, setup, items, params))
        }
        _ => None,
    };
    let contour_sel = match (&op.params, &op.src) {
        (OpParams::Contour { .. }, SrcRef::Selection(items)) => {
            Some(selection_geometry_impl(world, setup, items, &op.params, None))
        }
        _ => None,
    };
    let drive_chains = match &op.params {
        OpParams::MultiAxisContour { .. } => Some(drive_chains_of(world, op)),
        _ => None,
    };
    let prior = match &op.params {
        OpParams::Pocket3D { .. } | OpParams::MultiAxisRough { .. } => {
            // The mask the rest laws stand on, resolved NOW on the
            // caller's thread; the Arc is so a run never clones a
            // height field (Dexel is big, and borrowed anyway).
            Some(
                crate::rest::prior_state(world, op_name, setup, tool.diameter * 0.5)
                    .map(Arc::new),
            )
        }
        _ => None,
    };
    let is_mill = crate::machine::kind_of(world) == crate::machine::Kind::Mill;
    let fixtures = crate::fixture::boxes_for(world, setup);
    let spindle = crate::machine::of(world)
        .1
        .and_then(|m| m.spindle_d.zip(m.spindle_len))
        .map(|(d, l)| (d * 0.5, l));

    Ok(GenJob {
        tool,
        tool_name: op.tool.clone(),
        params: op.params.clone(),
        src: op.src.clone(),
        material,
        frame,
        hs,
        stock_facets,
        stock_aabb,
        section_z,
        picked_region,
        contour_sel,
        drive_chains,
        prior,
        is_mill,
        spindle,
        fixtures,
        part_solid,
    })
}

/// The old `region_for_src`'s law told without a world: the picks' answer when
/// the picks drive, otherwise the pure region build with the section
/// height `section_z_of` already resolved. The `take`s mean a stored
/// error surfaces exactly where the live call used to make it.
pub(super) fn region_for_job(
    picked_region: &mut Option<Result<Region, GenError>>,
    section_z: &mut Option<Result<f64, GenError>>,
    src: &SrcRef,
    params: &OpParams,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Region, GenError> {
    if matches!(src, SrcRef::Selection(_)) && selection_drives_region(params) {
        // The Contour arm never comes through here (its picks resolve
        // as geometry, not as a region) - so the guard that decided
        // collection decides the take.
        debug_assert!(!matches!(params, OpParams::Contour { .. }));
        return picked_region
            .take()
            .ok_or_else(|| unreached("picked selection region"))?;
    }
    region_core(
        material,
        params,
        frame,
        section_z.take().unwrap_or(Ok(0.0)),
    )
}
