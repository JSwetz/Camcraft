//! The program sim across setups (#317): the machined body one setup
//! leaves is carried into the next through the transform between the
//! two setups (`setup_relative::transform_between`, #316), so a later
//! setup starts from what the earlier one cut instead of a fresh block.
//!
//! Provenance: our own design per #308's research decision (one body,
//! viewed through each setup's transform, never re-sampled onto each
//! setup's grid); the engine is camcraft-sim's `stock_sim::Body`. Plain
//! re-clamp practice; no vendor material consulted.
//!
//! ## The laws here
//!
//! - **When the body is carried.** Entering a setup that holds the part
//!   differently from the one before it, or one placed relative to it
//!   (#316's link, "Same as" included), carries the body. Two setups
//!   holding the part alike and not linked keep the M3.2 A CLIP (the
//!   new block's top lowers what stands; nothing moves) - the default
//!   #320 will turn into a per-setup choice.
//! - **The carried body is the incoming stock.** A carried setup's own
//!   stock entity is not read by the sim: the material the machine
//!   holds is what the previous setup left.
//! - **Refuse what is not verified.** A turn the column lattice cannot
//!   carry exactly (side-on, or off the quarter turns) and a lathe or
//!   mill-turn machine refuse by name, naming both setups. Lathe,
//!   rotary and posed programs are refused before the walk, as before.
//! - **One machine (#404).** A carry into a setup whose machine (its own
//!   or the document's, #343) differs from the setup before it refuses
//!   by name ([`CARRY_MACHINE_REFUSAL`]) in [`entry`], so the rest
//!   reading (#318) refuses it too. The program walk refuses every setup
//!   change onto another machine, the clip included: the clip keeps
//!   what the earlier setup left standing. (A rest chain that meets a
//!   clip starts from the setup's own block and reads nothing across, so
//!   it does not refuse.) The machine kind gate reads the two setups'
//!   machine, never the active setup's.

use crate::stock_sim::{Body, Dexel, LatticeTurn, Planned, SimOp, simulate, stock_grid};

/// The refusal for a program that moves the stock between setups on
/// different machines (#404). The program sim judges one machine's
/// program, the post writes one (#343's one-mount law), so the material
/// is never handed across a machine change on a guess.
pub const CARRY_MACHINE_REFUSAL: &str = "the setups are on different machines - the program sim carries the stock only between setups on one machine; simulate each machine's setups on their own";

/// How the program enters a setup from the setup before it.
pub(crate) enum Entry {
    /// Same hold, not linked: the conservative clip to the new block.
    Clip,
    /// The body moves through the transform.
    Carry(LatticeTurn),
}

/// The rotation matrix of a unit quaternion, row-major (`p' = r p`).
fn matrix_of(rot: [f64; 4]) -> [[f64; 3]; 3] {
    use crate::geometry::{Point3, transform_point};
    let col = |e: Point3| transform_point(e, Point3::ZERO, rot);
    let (x, y, z) = (col(Point3::new(1.0, 0.0, 0.0)), col(Point3::new(0.0, 1.0, 0.0)), col(Point3::new(0.0, 0.0, 1.0)));
    [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]]
}

fn carry_head(from: &str, to: &str) -> String {
    format!("carrying the machined stock from setup '{from}' into setup '{to}'")
}

/// The machine both setups mount (#343: each its own, else the
/// document's), or [`CARRY_MACHINE_REFUSAL`] naming both setups and both
/// machines when they differ (#404).
///
/// # Errors
/// The two setups mount different machines.
pub(crate) fn one_machine(
    world: &flecs_ecs::prelude::World,
    from: &str,
    to: &str,
) -> Result<(String, Option<crate::machine::Machine>), String> {
    use crate::setup_mount::{machine_label, machine_of};
    let (m_from, m_to) = (machine_of(world, Some(from)), machine_of(world, Some(to)));
    if m_from != m_to {
        return Err(format!(
            "{}: {CARRY_MACHINE_REFUSAL} ('{from}' on {}, '{to}' on {})",
            carry_head(from, to),
            machine_label(&m_from),
            machine_label(&m_to),
        ));
    }
    Ok(m_to)
}

/// Decide how the program enters setup `to` from setup `from`.
///
/// # Errors
/// A transform that cannot be carried exactly, a machine kind the carry
/// is not verified on, or a setup `transform_between` refuses - each
/// named with both setups.
pub(crate) fn entry(world: &flecs_ecs::prelude::World, from: Option<&str>, to: Option<&str>) -> Result<Entry, String> {
    use crate::{setup_align, setup_relative};
    let (Some(from), Some(to)) = (from, to) else { return Ok(Entry::Clip) };
    let linked = setup_relative::relative_to(world, to).as_deref() == Some(from);
    if !linked && setup_relative::same(setup_align::stored(world, from), setup_align::stored(world, to)) {
        return Ok(Entry::Clip);
    }
    let head = carry_head(from, to);
    // #404: the machine is the two setups' own (#343), never the active
    // setup's - the body never crosses a machine change.
    let m_to = one_machine(world, from, to)?;
    if crate::machine::kind(m_to.1.as_ref()) != crate::machine::Kind::Mill {
        return Err(format!("{head}: not supported yet on a lathe or mill-turn machine"));
    }
    let t = setup_relative::transform_between(world, from, to).map_err(|e| format!("{head}: {e}"))?;
    LatticeTurn::from_matrix(matrix_of(t.rot), [t.pos.x, t.pos.y, t.pos.z])
        .map(Entry::Carry)
        .map_err(|e| format!("{head}: {e}"))
}

/// The stock as the walk holds it: the plain column grid until the
/// first carry, the two-sided body after.
pub(crate) enum Held {
    Plain(Dexel),
    Carried(Body),
}

impl Held {
    pub(crate) fn dex_mut(&mut self) -> &mut Dexel {
        match self {
            Held::Plain(d) => d,
            Held::Carried(b) => &mut b.dex,
        }
    }
}

/// Where the program's walk stands: the stock, the setup frame it is
/// held in, and whether the program crossed into another setup.
pub(crate) struct Walked {
    pub held: Held,
    pub frame: Option<String>,
    pub spanned: bool,
}

/// The first setup's stock read as a body: its floor is the stock's
/// bottom (the box and cylinder from their placement, a from-model
/// stock from its mesh read from below). Upright cuts never move the
/// floor, so it is read when the first carry needs it.
pub(crate) fn first_body(world: &flecs_ecs::prelude::World, setup: Option<&str>, dex: Dexel) -> Result<Body, String> {
    use crate::{model, stock::StockShape};
    let name = crate::setup::stock_entity_name(world, setup);
    let e = model::find_by_name(world, &name).ok_or_else(|| format!("no stock '{name}' to carry"))?;
    let shape = model::stock_def_of(world, &e).ok_or("the stock entity lost its shape")?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    Ok(match shape {
        StockShape::FromModel { .. } => Body::on_mesh(dex, &model::world_facets(world, &e).unwrap_or_default()),
        _ => Body::on_floor(dex, loc.pos.z),
    })
}

/// Walk the program through the stock, carrying or clipping at every
/// setup change. `until`: stop on entering that setup (its first op not
/// cut), returning the stock it starts from.
pub(crate) fn walk(
    world: &flecs_ecs::prelude::World,
    planned: &[Planned],
    until: Option<&str>,
) -> Result<Walked, String> {
    let min_r = planned.iter().map(|p| p.radius).fold(f64::INFINITY, f64::min);
    // The block the program starts on is the FIRST setup's stock - the
    // implicit-setup law makes that the legacy `Stock` entity exactly,
    // so a single-setup document walks this identical to before.
    let first = planned.first().and_then(|p| p.setup.as_deref());
    let first_eff = crate::setup::effective(world, first);
    let mut held = Held::Plain(stock_grid(world, first, min_r)?);
    let mut frame = first_eff.clone();
    let mut spanned = false;
    for p in planned {
        let eff = crate::setup::effective(world, p.setup.as_deref());
        if eff != frame {
            spanned = true;
            // #404: the program sim judges one machine's program - a
            // setup change onto another machine refuses here, whether the
            // entry would carry or clip (the clip keeps what the earlier
            // setup left standing, so it hands material across too).
            if let (Some(from), Some(to)) = (frame.as_deref(), eff.as_deref()) {
                one_machine(world, from, to)?;
            }
            match entry(world, frame.as_deref(), eff.as_deref())? {
                Entry::Clip => clip_to_setup_stock(held.dex_mut(), world, eff.as_deref(), min_r)?,
                Entry::Carry(turn) => {
                    let body = match held {
                        Held::Plain(d) => first_body(world, first_eff.as_deref(), d)?,
                        Held::Carried(b) => b,
                    };
                    held = Held::Carried(body.carry(&turn)?);
                }
            }
            frame = eff;
        }
        if until.is_some() && frame.as_deref() == until {
            return Ok(Walked { held, frame, spanned });
        }
        let sim_ops = [SimOp { moves: &p.moves, radius: p.radius, ball: p.ball, cone: p.cone, corner: p.corner }];
        simulate(held.dex_mut(), &sim_ops).map_err(|e| e.to_string())?;
    }
    if let Some(u) = until {
        return Err(format!("the program never enters setup '{u}'"));
    }
    Ok(Walked { held, frame, spanned })
}

/// The stock setup `setup` starts from when the program reaches it: the
/// body the setups before it leave, in `setup`'s frame (#317; #320's
/// incoming-stock display and #318's rest read it). The program's first
/// setup starts from its own stock.
///
/// # Errors
/// Every refusal of the program sim, and a setup the program never
/// enters.
pub fn incoming_body(world: &flecs_ecs::prelude::World, setup: &str) -> Result<Body, String> {
    let planned = crate::stock_sim::plan_program(world)?;
    crate::stock_sim::refuse_unjudged(&planned)?;
    let w = walk(world, &planned, Some(setup))?;
    match w.held {
        Held::Carried(b) => Ok(b),
        Held::Plain(d) => {
            let first = planned.first().and_then(|p| p.setup.as_deref());
            first_body(world, crate::setup::effective(world, first).as_deref(), d)
        }
    }
}

/// Fold a newly-entered setup's stock into the standing grid: inside
/// the new block, no column may stand above that block's top; outside,
/// what the previous setup left stands untouched - the conservative
/// clip for two setups that hold the part alike, not the carry (#317). A setup with no stock entity yet clips
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
            "the program sim cannot clip to a later setup's from-model stock - place that setup relative to the one before it so the machined stock is carried instead"
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
