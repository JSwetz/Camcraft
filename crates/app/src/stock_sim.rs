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
        let sim_ops = [SimOp { moves: &p.moves, radius: p.radius, ball: p.ball, cone: p.cone }];
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
    pub setup: Option<String>,
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
    // full radius at the corner depth); everything whose cut IS its
    // cylinder - end mills, drills, taps, bores - and the bull nose
    // with its stated corner bias sweep the disc.
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
        planned.push(Planned {
            name: name.clone(),
            moves: cache.path.moves,
            radius: tool.diameter / 2.0,
            ball: tool.kind == crate::tools::ToolKind::BallNose,
            cone,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;
    use crate::geometry::Point3;

    fn p3(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn static_ball_is_the_sphere_section() {
        // A motionless ball is the exact circle of its section: bottom
        // at `z_c - sqrt(r^2 - d^2)`, nothing beyond `r`, no sampling
        // error anywhere - the identity the swept law generalizes.
        let mut dex = Dexel::block(-4.5, -4.5, 4.5, 4.5, 100.0, 0.5).unwrap();
        dex.cut_ball(p3(0.0, 0.0, 5.0), p3(0.0, 0.0, 5.0), 2.0);
        let r = 2.0;
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                let q = dex.center(ix, iy);
                let d = q.x.hypot(q.y);
                let want = if d <= r {
                    5.0 - (r * r - d * d).sqrt()
                } else {
                    100.0
                };
                assert!(
                    (dex.top_at(ix + iy * dex.nx) - want).abs() <= 1e-12,
                    "cell {q:?} d={d}: got {}, want {want}",
                    dex.top_at(ix + iy * dex.nx)
                );
            }
        }
    }

    #[test]
    fn oblique_sweep_finds_the_interior_stationary_point() {
        // Endpoints and the XY-nearest instant are NOT enough on an
        // oblique move: the convex bottom's minimum sits before the
        // midpoint when the center climbs. f(t) = (5 + 7t) -
        // sqrt(9 - (10 - 20t)^2) bottoms at u = 21/sqrt(449) off the
        // midpoint, 5.3204 - strictly under f(t_c) = 5.5.
        let mut dex = Dexel::block(-13.8, -0.2, 13.8, 0.2, 100.0, 0.4).unwrap();
        dex.cut_ball(p3(-10.0, 0.0, 5.0), p3(10.0, 0.0, 12.0), 3.0);
        let q = p3(0.0, 0.0, 0.0);
        let k = dex.col(Point2::new(q.x, q.y)).unwrap();
        let u = 21.0 / 449.0f64.sqrt();
        let want = 5.0 + 7.0 * (10.0 - u) / 20.0 - 20.0 * u / 7.0;
        assert!(
            (dex.top_at(k) - want).abs() <= 1e-9,
            "got {}, want {want} (t_c-only would say 5.5)",
            dex.top_at(k)
        );
        assert!(dex.top_at(k) < 5.5 - 0.05, "the quadratic root never ran");
    }

    #[test]
    fn planar_sweep_is_a_constant_offset_profile() {
        // k = 0 (flat feed): the bottom at any column is the section at
        // the perpendicular distance - the double root collapses onto
        // t_c and the clamp evaluation owns it.
        let mut dex = Dexel::block(-0.25, 0.5, 0.25, 1.0, 100.0, 0.5).unwrap();
        dex.cut_ball(p3(-10.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), 3.0);
        let k = dex.col(Point2::new(0.0, 0.75)).unwrap();
        let want = 5.0 - (9.0f64 - 0.75 * 0.75).sqrt();
        assert!(
            (dex.top_at(k) - want).abs() <= 1e-12,
            "got {}, want {want}",
            dex.top_at(k)
        );
    }

    #[test]
    fn out_of_reach_cells_are_never_touched() {
        // The phantom-window law: a sweep whose disc misses a column
        // leaves its stock top exactly as it stood - the reach test,
        // not a margin, decides.
        let mut dex = Dexel::block(-20.0, -1.0, 20.0, 1.0, 7.0, 0.5).unwrap();
        dex.cut_ball(p3(-10.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), 3.0);
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                let q = dex.center(ix, iy);
                let touched = q.x.abs() <= 10.0 + 3.0 && q.y.abs() <= 3.0;
                if !touched {
                    assert_eq!(dex.top_at(ix + iy * dex.nx), 7.0, "phantom cut at {q:?}");
                }
            }
        }
        // And the reach law itself, directly: a perpendicular offset
        // just under r grazes; just over r gives nothing at all.
        assert!(reach(0.0, 2.999, 20.0, 0.0, 3.0).is_some());
        assert!(reach(0.0, 3.001, 20.0, 0.0, 3.0).is_none());
        // w is an offset from the chord's START, so the middle of the
        // window is at w = (10, 0): the disc covers the middle 6 of a
        // 20-long chord, t 0.35..0.65. (At w = (0,0) the point is the
        // start endpoint itself and the clamped half-window is 0.15.)
        let (t0, t1) = reach(10.0, 0.0, 20.0, 0.0, 3.0).unwrap();
        assert!((t1 - t0 - 0.3).abs() <= 1e-12, "window {t0}..{t1}");
    }

    #[test]
    fn non_finite_inputs_cut_nothing_and_never_panic() {
        let mut dex = Dexel::block(-5.0, -5.0, 5.0, 5.0, 9.0, 1.0).unwrap();
        dex.cut_ball(p3(0.0, 0.0, f64::NAN), p3(1.0, 0.0, 0.0), 2.0);
        dex.cut_ball(p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 1.0), f64::INFINITY);
        dex.cut_chord(p3(0.0, 0.0, 1.0), p3(f64::NAN, 0.0, 1.0), 2.0);
        assert!(dex.tops().all(|v| v == 9.0), "garbage cut something");
    }

    #[test]
    fn envelope_scan_states_its_grid_and_refuses_a_bad_ball() {
        let facets = vec![Triangle::new(
            p3(-5.0, -5.0, 0.0),
            p3(5.0, -5.0, 0.0),
            p3(5.0, 5.0, 0.0),
        )];
        assert!(envelope_scan(&facets, &[], 0.0, 0.01).is_err());
        assert!(envelope_scan(&facets, &[], -1.0, 0.01).is_err());
        assert!(envelope_scan(&facets, &[], 2.0, f64::NAN).is_err());
        // A non-finite vertex is an error, not an empty report.
        let mut bad = facets.clone();
        bad[0].c = p3(f64::NAN, 5.0, 0.0);
        assert!(envelope_scan(&bad, &[], 2.0, 0.01).is_err());
        // An empty model has no footprint to measure on: silence.
        let rep = envelope_scan(&[], &[], 2.0, 0.01).unwrap();
        assert_eq!(rep, EnvelopeReport::default());
    }
}
