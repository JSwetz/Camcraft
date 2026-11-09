//! Rest across setups (#318): the program ahead of an operation, cut
//! into per-setup segments, reaching back through every setup boundary
//! the machined body is CARRIED across (#317's `stock_sim::carry`).
//!
//! Provenance: our own design per #308's research decision ("stock from
//! the preceding setup" read through the setup transform); general
//! machining knowledge, no vendor material consulted.
//!
//! ## The laws here
//!
//! - **One law with the program sim.** A boundary counts exactly when the
//!   program sim carries the body across it (`carry::entry` decides, by
//!   call). A setup entered by the M3.2 A clip (same hold, not linked)
//!   breaks the chain: its rest starts from its own block as before -
//!   whether a fresh block or the same part is loaded there is not stated
//!   until #320, and the fresh block is the side that never assumes
//!   material gone.
//! - **A refused carry refuses the rest.** If the body cannot be carried
//!   into the consumer's setup (a turn the lattice cannot hold, a lathe),
//!   the stock that setup starts from is unknown: the rest reading is
//!   refused by name ([`REST_CARRY_REFUSAL`]), never read as a fresh block.
//! - **A single setup reads exactly as before.** With no carried boundary
//!   the chain is the same-setup walk and no extra term is written, so
//!   single-setup stamps and paths are bit-for-bit unchanged.

use super::ahead::{all_ahead, Ahead};
use crate::stock_sim::carry::{self, Entry};
use crate::stock_sim::{Dexel, LatticeTurn, SimOp};
use flecs_ecs::prelude::World;

/// The refusal for a rest reading whose incoming stock cannot be carried.
pub const REST_CARRY_REFUSAL: &str = "this operation reads the stock earlier setups left, and that stock cannot be carried into its setup";

/// The operations of one setup in the chain, and the turn the body takes
/// entering it (`None` for the setup the chain starts on).
pub(super) struct Segment {
    pub(super) setup: Option<String>,
    pub(super) turn: Option<LatticeTurn>,
    pub(super) ops: Vec<Ahead>,
}

/// The program ahead of one operation as its rest reads it.
pub(super) struct Chain {
    pub(super) segments: Vec<Segment>,
}

impl Chain {
    /// Does the chain reach back across a carried boundary?
    pub(super) fn crosses(&self) -> bool {
        self.segments.len() > 1
    }

    /// The setup whose block the chain starts on.
    pub(super) fn origin(&self) -> Option<&str> {
        self.segments.first().and_then(|s| s.setup.as_deref())
    }

    pub(super) fn ops(&self) -> impl Iterator<Item = &Ahead> {
        self.segments.iter().flat_map(|s| s.ops.iter())
    }

    pub(super) fn into_ops(self) -> Vec<Ahead> {
        self.segments.into_iter().flat_map(|s| s.ops).collect()
    }

    /// The chain's own digest term, beside the predecessors' paths: the
    /// setups crossed, each turn, and the origin setup's stock (the block
    /// the carried body was cut from). Only written when the chain crosses.
    pub(super) fn term(&self, world: &World, h: &mut impl std::hash::Hasher) {
        h.write(b"camcraft-rest-across-v1");
        for s in &self.segments {
            h.write(s.setup.as_deref().unwrap_or("").as_bytes());
            h.write_u8(0);
            match &s.turn {
                // Debug prints every float round-trip exact.
                Some(t) => h.write(format!("{t:?}").as_bytes()),
                None => h.write_u8(0),
            }
        }
        if crate::operation::write_setup_stock(world, self.origin(), h).is_none() {
            h.write_u8(0xfe);
        }
    }
}

/// The chain ahead of `name` (consumer setup `setup`).
///
/// # Errors
/// [`REST_CARRY_REFUSAL`] with the carry's own reason (naming both
/// setups) when a boundary the chain must cross refuses.
pub(super) fn chain_of(world: &World, name: &str, setup: Option<&str>, with_moves: bool) -> Result<Chain, String> {
    let consumer = crate::setup::effective(world, setup);
    let mut segments: Vec<Segment> = Vec::new();
    for a in all_ahead(world, name, with_moves) {
        match segments.last_mut() {
            Some(s) if s.setup == a.setup => s.ops.push(a),
            _ => segments.push(Segment { setup: a.setup.clone(), turn: None, ops: vec![a] }),
        }
    }
    if segments.last().is_none_or(|s| s.setup != consumer) {
        segments.push(Segment { setup: consumer, turn: None, ops: Vec::new() });
    }
    // Back from the consumer: every carried boundary extends the chain,
    // the first clip ends it.
    let mut start = segments.len() - 1;
    while start > 0 {
        let (from, to) = (segments[start - 1].setup.as_deref(), segments[start].setup.as_deref());
        match carry::entry(world, from, to) {
            Ok(Entry::Clip) => break,
            Ok(Entry::Carry(t)) => {
                segments[start].turn = Some(t);
                start -= 1;
            }
            Err(e) => return Err(format!("{REST_CARRY_REFUSAL}: {e}")),
        }
    }
    let segments = segments.split_off(start);
    Ok(Chain { segments })
}

/// The column grid the crossing chain leaves standing, in the consumer's
/// frame: the origin setup's block, each segment's cut moves swept, the
/// body carried at every boundary. The tops are read as the rest grid -
/// a column cut through on its far side still reads its standing top
/// (more material, never less).
pub(super) fn carried_grid(world: &World, chain: &Chain, min_r: f64) -> Result<Dexel, String> {
    let origin = crate::setup::effective(world, chain.origin());
    let dex = crate::stock_sim::stock_grid(world, origin.as_deref(), min_r)?;
    let mut body = carry::first_body(world, origin.as_deref(), dex)?;
    for s in &chain.segments {
        if let Some(t) = &s.turn {
            body = body.carry(t)?;
        }
        let ops: Vec<SimOp> = s
            .ops
            .iter()
            .filter(|a| a.cut)
            .map(|a| SimOp { moves: &a.moves, radius: a.radius, ball: a.ball, cone: a.cone, corner: a.corner })
            .collect();
        crate::stock_sim::simulate(&mut body.dex, &ops).map_err(|e| e.to_string())?;
    }
    Ok(body.dex)
}
