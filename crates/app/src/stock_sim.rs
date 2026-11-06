//! Document stock: the world-facing half of `stock_sim`. The engine
//! (dexel grid, sweeps, measures, deviation, traverse timing) lives
//! in camcraft-sim (crates/sim/src/stock_sim.rs, M4.1 R4); what stays
//! here is exactly the part that walks the world - building the stock
//! solid from the `Stock`/`Setup` entities and driving a whole
//! document through the sim: the tuple seam the design note promised,
//! the engine consumes (facets, stock, tool) data, this module hands
//! it over.

pub use camcraft_sim::stock_sim::*;

use crate::geometry::Triangle;

/// The document's stock, as an axis-aligned grid in world. Box and
/// cylinder are stage A's analytic grids translated to the `Stock`
/// entity's place; a rotated stock would need the grid in the stock's
/// frame, and this kernel's law is one axis-aligned lattice - the
/// refusal says so instead of silently ignoring the rotation. Model
/// stock builds from its own world facets, the same mesh the viewport
/// draws. `simulate_document` and re-machining (M3.1 D) read the stock
/// through this law - one builder, two consumers, so a rest mask and
/// the deviation verdict always describe the same solid.
pub fn stock_grid(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    tool_r: f64,
) -> Result<Dexel, String> {
    use crate::geometry::{IDENTITY_Q, transform_point};
    use crate::stock::StockShape;
    use crate::model;

    let stock_name = crate::setup::stock_entity_name(world, setup);
    let stock_e = model::find_by_name(world, &stock_name)
        .ok_or_else(|| format!("no stock '{stock_name}' - the program sim judges material, not air"))?;
    let shape = model::stock_def_of(world, &stock_e)
        .ok_or("the stock entity lost its shape")?;
    let loc = model::transform_of(world, &stock_e).unwrap_or_default();
    if loc.rot != IDENTITY_Q {
        return Err("the program sim models an axis-aligned stock; this stock carries a rotation".into());
    }
    let stock_facets: Vec<Triangle> = match &shape {
        StockShape::FromModel { .. } => model::facets_of(world, &stock_e)
            .unwrap_or_default()
            .iter()
            .map(|t| Triangle::new(
                transform_point(t.a, loc.pos, loc.rot),
                transform_point(t.b, loc.pos, loc.rot),
                transform_point(t.c, loc.pos, loc.rot),
            ))
            .collect(),
        _ => Vec::new(),
    };
    let (span_x, span_y) = match &shape {
        StockShape::Box { dx, dy, .. } => (*dx, *dy),
        StockShape::Cylinder { diameter, .. } => (*diameter, *diameter),
        StockShape::FromModel { .. } => {
            let (mut x0, mut x1, mut y0, mut y1) =
                (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
            for t in &stock_facets {
                for v in [t.a, t.b, t.c] {
                    x0 = x0.min(v.x);
                    x1 = x1.max(v.x);
                    y0 = y0.min(v.y);
                    y1 = y1.max(v.y);
                }
            }
            ((x1 - x0).max(0.0), (y1 - y0).max(0.0))
        }
    };
    let cell = auto_cell(tool_r, span_x, span_y);
    let dex = match &shape {
        StockShape::FromModel { .. } => {
            Dexel::from_facets(&stock_facets, cell).map_err(|e| e.to_string())?
        }
        s => {
            let mut d = Dexel::from_shape(s, cell).map_err(|e| e.to_string())?;
            // The analytic grids are built origin-centered; the stock
            // entity's place moves the lattice itself, so the moves
            // stay in the world frame the viewport and post read.
            d.x0 += loc.pos.x;
            d.y0 += loc.pos.y;
            d.stock_z += loc.pos.z;
            d.translate_finite(loc.pos.z); // stored heights ride with it
            d
        }
    };
    Ok(dex)
}

/// Simulate the whole document's program: the operations in posting
/// order (through [`crate::posting::program_ops`] - so what the sim
/// judges IS what would post, one law shared, not two agreeing), each
/// op's CACHED moves through its tool's atom, against the stock as the
/// setup defines it. Move coordinates are pre-origin-shift world
/// millimeters, the same numbers the viewport draws and the post
/// shifts; the stock grid is built on that same frame. Refusals name
/// their reason (an op that never generated, a rotated stock, a grid
/// too fine) and change nothing.
pub fn simulate_document(
    world: &flecs_ecs::prelude::World,
) -> Result<ProgramSim, String> {
    let planned = plan_program(world)?;

    // The posed clause (M13.1 follow-up): the verdict below is the
    // COLUMN engine, whose atoms stand upright - a 5-axis lean or a
    // 3+2 frame's tilted moves would be cut as if vertical, a silent
    // wrong verdict. The field arm does cut a posed move exactly (the
    // pose rides `field_cut`'s solid), but its verdict reads heights
    // marched from above, and a tilted tool is precisely the tool that
    // can reach UNDER an overhang: a gouge there stands beneath an
    // untouched top and reads clean. So neither arm's height verdict
    // judges a posed program - refusal by name, which the post gate
    // shows as "not judged" (the generator-time verify, which reads
    // the lean, is what governs these ops).
    for p in &planned {
        if let Some(k) = first_posed(&p.moves) {
            return Err(format!("{}: move {k}: {POSED_REFUSAL}", p.name));
        }
    }

    let min_r = planned.iter().map(|p| p.radius).fold(f64::INFINITY, f64::min);
    // The block the program starts on is the FIRST setup's stock - the
    // implicit-setup law makes that the legacy `Stock` entity exactly,
    // so a single-setup document walks this identical to before.
    let mut dex = stock_grid(
        world,
        planned.first().and_then(|p| p.setup.as_deref()),
        min_r,
    )?;

    // M3.2 A, multi-setup: the flip transform between setups is M3.3's
    // engine, so until then a new setup's stock can only ADD material
    // where its own block stands - never move what the previous setup
    // already cut. That reads as a CLIP: inside the new block every
    // column drops to at most the new stock's top, outside it material
    // stands as left. The verdict then over-estimates leftover (a
    // re-clamped face reads as uncut) - visible as rest, never as a
    // gouge, the mask law's conservative direction. A later stock that
    // is rotated or from-model cannot be judged in this frame at all:
    // refusal, not a guess.
    let mut prev_setup = crate::setup::effective(
        world,
        planned.first().and_then(|p| p.setup.as_deref()),
    );
    for p in &planned {
        let eff = crate::setup::effective(world, p.setup.as_deref());
        if eff != prev_setup {
            clip_to_setup_stock(&mut dex, world, eff.as_deref(), min_r)?;
            prev_setup = eff;
        }
        let sim_ops = [SimOp { moves: &p.moves, radius: p.radius, ball: p.ball, cone: p.cone, corner: p.corner }];
        simulate(&mut dex, &sim_ops).map_err(|e| e.to_string())?;
    }

    let model_facets = part_facets(world);
    let dev = deviate(&dex, &model_facets, DEV_TOL).map_err(|e| e.to_string())?;
    let classes = color_classes(&dev);
    let surface = dex.surface(&classes);
    Ok(ProgramSim { dev, surface })
}

/// One operation of the program, resolved for the sim: its cached
/// moves and the atom its tool sweeps.
pub(crate) struct Planned {
    /// The operation's name - the sim timeline's key for it.
    pub name: String,
    pub moves: Vec<crate::cl::Move>,
    pub radius: f64,
    pub ball: bool,
    pub cone: Option<f64>,
    /// The bull nose's corner radius (M13.1), `bull_corner`'s law.
    pub corner: Option<f64>,
    pub setup: Option<String>,
}

/// The corner radius a tool's atom carries (M13.1): Some only for a
/// bull nose with a real corner, clamped to the tool radius (the
/// `Tool::is_sane` bound, restated so the atom can never be handed a
/// corner wider than its tool). A zero corner IS the flat end mill,
/// cut by the disc exactly; a non-finite one refuses by name.
pub(crate) fn bull_corner(tool: &crate::tools::Tool) -> Result<Option<f64>, String> {
    if tool.kind != crate::tools::ToolKind::BullNose {
        return Ok(None);
    }
    let (c, r) = (tool.corner_r, tool.diameter * 0.5);
    if !(c.is_finite() && r.is_finite() && c >= 0.0 && r > 0.0) {
        return Err(format!("bull nose carries no sane corner (corner radius {c}, radius {r})"));
    }
    Ok((c > 0.0).then_some(c.min(r)))
}

/// The program in posting order, each op resolved to its tool's atom
/// - the one plan both the column verdict and the field eye walk.
pub(crate) fn plan_program(world: &flecs_ecs::prelude::World) -> Result<Vec<Planned>, String> {
    use crate::{model, posting};

    let ops = posting::program_ops(world)?;

    // Tools and cached paths, resolved once, in program order. The
    // atom follows the tool kind (stage A's law): a ball nose sweeps
    // the exact sphere; a chamfer mill sweeps its exact cone (M2.4 B
    // - the disc would phantom-gouge every chamfer wall against the
    // full radius at the corner depth); a bull nose sweeps its exact
    // corner-rounded solid (M13.1 - the disc it rode before removed
    // the fillet's ring the real tool leaves); everything whose cut
    // IS its cylinder - end mills, drills, taps, bores - sweeps the
    // disc.
    let mut planned: Vec<Planned> = Vec::with_capacity(ops.len());
    for (name, e) in &ops {
        let op = model::operation_of(world, e)
            .ok_or_else(|| format!("{name}: lost its operation"))?;
        // The revolved clause (M5.2 G.2): a Turn-family path lives in
        // the half-plane - its numbers speak radius and axial
        // station, and a Z-downward height field would read a parting
        // kerf as a 38 mm plunge and the whole program as gouged. The
        // verdict that governs it is the revolved one, already binding
        // at generation; HERE the honest answer is that nothing was
        // judged. Refusal, not a guess - the rotated-stock precedent.
        if op.params.kind().info().family() == crate::operation::Family::Turn {
            return Err(format!(
                "{name}: a revolved path is not judgeable in the mill's dexel frame - its law is the revolved verdict at generation"
            ));
        }
        // The rotary clause (M13.1): between two stations a rotary
        // move turns the WORK, so the tool sweeps an annular sector
        // about the fitted axis - a torus-family solid neither stock
        // engine has. Cutting the station-to-station chord instead
        // would be a silent approximation of that sweep, so the
        // program is refused by name until the sector sweep exists
        // (ROADMAP M13.1, "a lathe or rotary cut = torus/annular-
        // sector sweeps"). Its own verdict stands at generation.
        if matches!(
            op.params,
            crate::operation::OpParams::RotaryContour { .. }
                | crate::operation::OpParams::RotaryParallel { .. }
                | crate::operation::OpParams::RotaryOffset { .. }
        ) {
            return Err(format!(
                "{name}: a rotary cut sweeps an annular sector about its axis, which the stock sim does not model - not judged here (its envelope verdict at generation stands)"
            ));
        }
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let cache = model::toolpath_of(world, e)
            .ok_or_else(|| format!("{name}: lost its toolpath"))?;
        // The cone's slope, cot of the half-angle from the axis -
        // the number the dexel atom reads directly, so the sim and
        // the congruence door in `generate` see the same steel.
        let cone = if tool.kind == crate::tools::ToolKind::Taper {
            let tan_half = (tool.tip_angle.to_radians() * 0.5).tan();
            if tan_half.is_finite() && tan_half > 0.0 {
                Some(1.0 / tan_half)
            } else {
                return Err(format!(
                    "{name}: chamfer mill {} carries no sane cone (tip angle {})",
                    op.tool, tool.tip_angle
                ));
            }
        } else {
            None
        };
        let corner = bull_corner(&tool).map_err(|e| format!("{name}: {e}"))?;
        planned.push(Planned {
            name: name.clone(),
            moves: cache.path.moves,
            radius: tool.diameter / 2.0,
            ball: tool.kind == crate::tools::ToolKind::BallNose,
            cone,
            corner,
            setup: op.setup,
        });
    }
    Ok(planned)
}

/// The part as the machine must leave it: every model's facets in
/// world, minus the stock (setup furniture, not design intent) - and
/// minus the workholding, which M3.2 B made models too. A vise is
/// steel the program must dodge, never a promise it must leave
/// standing at height: its facets belong to the collision gate, not
/// to the gouge verdict (counting them here would read a clamped job
/// as a part with a vise-shaped hole in it). Visibility is a display
/// choice; the verdict judges all the rest.
pub(crate) fn part_facets(world: &flecs_ecs::prelude::World) -> Vec<Triangle> {
    use crate::geometry::transform_point;
    use crate::model;

    let mut model_facets: Vec<Triangle> = Vec::new();
    for (_, e) in model::model_entities(world) {
        if model::is_fixture(world, &e) {
            continue;
        }
        let tris = model::facets_of(world, &e).unwrap_or_default();
        let l = model::transform_of(world, &e).unwrap_or_default();
        model_facets.extend(tris.iter().map(|t| Triangle::new(
            transform_point(t.a, l.pos, l.rot),
            transform_point(t.b, l.pos, l.rot),
            transform_point(t.c, l.pos, l.rot),
        )));
    }
    model_facets
}

/// Fold a newly-entered setup's stock into the standing grid: inside
/// the new block, no column may stand above that block's top; outside,
/// what the previous setup left stands untouched. See the law comment
/// in [`simulate_document`] - this is the conservative clip, not the
/// flip transform (M3.3). A setup with no stock entity yet clips
/// nothing; a rotated or from-model stock refuses, per the same law.
fn clip_to_setup_stock(
    dex: &mut Dexel,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    tool_r: f64,
) -> Result<(), String> {
    use crate::model;
    use crate::stock::StockShape;
    let Some(entity) = crate::setup::stock_entity(world, setup) else {
        return Ok(());
    };
    if matches!(model::stock_def_of(world, &entity), Some(StockShape::FromModel { .. })) {
        return Err(
            "the program sim cannot judge a later setup's from-model stock - the flip transform arrives with M3.3"
                .into(),
        );
    }
    let nd = stock_grid(world, setup, tool_r)?;
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let q = dex.center(ix, iy);
            let Some(k) = nd.col(q) else { continue };
            let t = nd.top_at(k);
            let i = ix + iy * dex.nx;
            if t.is_finite() {
                dex.lower(i, t);
            }
        }
    }
    Ok(())
}


// The engine-law unit tests (the ball atom, the reach law, the
// envelope gate), split out by the 400-line law (M13.1).
#[cfg(test)]
#[path = "stock_sim_tests.rs"]
mod tests;
