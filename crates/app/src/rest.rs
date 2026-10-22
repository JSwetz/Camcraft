//! Re-MACHining: what the previous operations already cut (M3.1 stage D).
//!
//! Provenance: our own design; tool-vs-tool rest roughing is standard
//! practice, specced from general machining knowledge - no vendor
//! implementation inspected. The grid arithmetic is all
//! [`crate::stock_sim`]'s; this module decides only who counts as
//! *before* an operation and what that means for its stamp.
//!
//! ## The laws
//!
//! - **Program order is the post's order.** [`crate::model::named_entities`]
//!   in name-sorted order, the same sequence `posting::program_ops`
//!   walks - what the machine will do first IS what was cut first. The
//!   one law's known wart (an operation named `Operation10` sorts
//!   before `Operation2`) is the post's wart too; fixing the sort is a
//!   roadmap question, forking it here would make re-machining disagree
//!   with the machine.
//! - **The mask only removes cuts, never adds them.** The rest state is
//!   the stock grid with every earlier operation's CACHED moves
//!   subtracted; a consumer's level targets are intersected with it, so
//!   a masked run is a subset of the unmasked one. Skipping a column
//!   whose material stands at or below the plane cuts air at worst -
//!   never a gouge. The model's own islands law keeps guarding the
//!   part, mask or no mask.
//! - **The digest reads the path on record, never an intention.** A
//!   predecessor's term is a hash of its CACHED moves, so an edited
//!   but unregenerated operation moves nobody: its parameters changed,
//!   but the path the program would post has not - and that is what
//!   the dependent's rest is computed from. That combination can never
//!   post anyway (`program_ops` refuses any Stale op in the program),
//!   so the dependent is asked to describe the program exactly as it
//!   would ship today. When the earlier op IS regenerated its cache
//!   moves, the dependent's rest digest moves with it, and the
//!   dependent reads Stale: the job converges within two
//!   `RegenerateAll` rounds (M3.1 C), each round honest about what it
//!   fixed. (A recorded `stamp` term was the first design; the stamp
//!   digests the parameters, and `EditOperationParams` drops it to
//!   None - an edit would have marked every dependent Stale for a path
//!   that had not moved.)
//! - **Inert data must not staleness-float (AGENTS.md, M1.6).** The
//!   digest is written only when the operation has predecessors and
//!   only into `Pocket3D` stamps: a first operation's stamp is
//!   bit-for-bit what it was before stage D, and an operation type
//!   that reads no mask digests none - a `Face` op is no more affected
//!   by an earlier pocket than it was yesterday.
//! - **Never-generated predecessors cut nothing** and contribute only
//!   their marker; the moment one generates, the dependent's digest
//!   moves and it goes Stale - the same one-way ratchet the whole
//!   associativity chain runs on.

use crate::cl::Move;
use crate::machining2d::GenError;
use crate::stock_sim::{self, Dexel, SimOp};
use crate::model;
use flecs_ecs::prelude::World;

/// What the program in front of one operation contributes: the grid it
/// leaves standing (`None` when nothing before it has cut, so no mask
/// is needed) and the digest of everything the mask depends on
/// (`None` when there are no predecessors at all - the first-operation
/// case, whose stamp must not churn).
pub struct PriorState {
    pub grid: Option<Dexel>,
    pub digest: Option<u64>,
}

/// One operation ahead of the consumer: name and the hash of its
/// recorded path (both digest terms), its tool atom (the grid depends
/// on it), and the cached moves themselves when it has cut anything.
struct Ahead {
    name: String,
    path: u64,
    radius: f64,
    ball: bool,
    cut: bool,
    moves: Vec<Move>,
}

/// Identity of a cached path: every field a move carries, bit-exact.
/// The generators are deterministic functions of what the stamp
/// digests, so equal paths here means the mask below builds identically.
fn path_hash(moves: &[Move]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"hsmw-rest-path-v1");
    h.write_u64(moves.len() as u64);
    for m in moves {
        h.write_u8(match m.kind {
            crate::cl::MoveKind::Rapid => 0,
            crate::cl::MoveKind::Feed => 1,
            crate::cl::MoveKind::Plunge => 2,
        });
        for p in [m.from, m.to] {
            h.write(&p.x.to_bits().to_le_bytes());
            h.write(&p.y.to_bits().to_le_bytes());
            h.write(&p.z.to_bits().to_le_bytes());
        }
        match m.feed {
            Some(f) => {
                h.write_u8(1);
                h.write(&f.to_bits().to_le_bytes());
            }
            None => h.write_u8(0),
        }
        match &m.arc {
            Some(a) => {
                h.write_u8(1);
                h.write(&a.center.x.to_bits().to_le_bytes());
                h.write(&a.center.y.to_bits().to_le_bytes());
                h.write_u8(u8::from(a.ccw));
            }
            None => h.write_u8(0),
        }
    }
    h.finish()
}

/// The operations in program order ahead of `name`. The ordering law
/// is shared with `posting::program_ops` by reference, not by call:
/// that function refuses whole programs over a `Never` operation, and
/// an operation later in the program is nobody's business here.
fn ahead_of(world: &World, name: &str) -> Vec<Ahead> {
    let ops = model::named_entities(world);
    let Some(self_ix) = ops
        .iter()
        .position(|(n, e)| n == name && model::is_operation(world, e))
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(self_ix);
    for (n, e) in &ops[..self_ix] {
        let Some(op) = model::operation_of(world, e) else {
            continue; // named and is_operation said operation; lost a beat later
        };
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t));
        let cache = model::toolpath_of(world, e);
        // The cached PATH is the truth for the digest: an edited op's
        // moved parameters are its OWN staleness, and its cache still
        // says exactly what the program would post today. Cutting
        // nothing - never generated, or an empty path - is what
        // contributes no mask.
        let moves = cache.map_or_else(Vec::new, |c| c.path.moves);
        let cut = !moves.is_empty();
        out.push(Ahead {
            name: n.clone(),
            path: if cut { path_hash(&moves) } else { 0 },
            radius: tool.as_ref().map_or(0.0, |t| t.diameter * 0.5),
            ball: tool
                .as_ref()
                .is_some_and(|t| t.kind == crate::tools::ToolKind::BallNose),
            cut,
            moves,
        });
    }
    out
}

/// One predecessor's contribution to the digest: a never-cutting
/// operation names itself and nothing else (its tool's size is as
/// irrelevant to this stamp as its moves - inert data law), a cutting
/// one contributes the path it would post and the atom that would
/// carve it.
fn digest_of(ahead: &[Ahead]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"hsmw-rest-v1");
    for a in ahead {
        h.write(a.name.as_bytes());
        if a.cut {
            h.write_u8(1);
            h.write_u64(a.path);
            h.write(&a.radius.to_bits().to_le_bytes());
            h.write_u8(u8::from(a.ball));
        } else {
            h.write_u8(0);
        }
    }
    h.finish()
}

/// The cheap half of the story, for the stamp: `Some(digest)` exactly
/// when this operation has predecessors whose work it may consume.
/// Reads names, stamps and tool diameters - no grid, no mesh, so
/// `status_of` can ask every frame (M3.1 C's live-questions law).
#[must_use]
pub fn prior_digest(world: &World, name: &str) -> Option<u64> {
    let ahead = ahead_of(world, name);
    (!ahead.is_empty()).then(|| digest_of(&ahead))
}

/// The grid the previous operations leave standing, and the digest it
/// came from. `tool_r` is the consumer's own radius: the lattice must
/// resolve its passes, and the predecessors' atoms can only make it
/// finer. Refusals are generation errors with the reason named - a
/// program that rest-machines cannot quietly pretend the stock
/// was readable.
pub fn prior_state(
    world: &World,
    name: &str,
    tool_r: f64,
) -> Result<PriorState, GenError> {
    let ahead = ahead_of(world, name);
    if ahead.is_empty() {
        return Ok(PriorState {
            grid: None,
            digest: None,
        });
    }
    let digest = digest_of(&ahead);
    if !ahead.iter().any(|a| a.cut) {
        // Nothing cut before us: the stock as it came is the rest as
        // it stands, and no mask is a cheaper way to say that.
        return Ok(PriorState {
            grid: None,
            digest: Some(digest),
        });
    }
    let min_r = ahead
        .iter()
        .filter(|a| a.cut && a.radius > 0.0)
        .map(|a| a.radius)
        .fold(tool_r, f64::min);
    let mut dex = stock_sim::stock_grid(world, min_r)
        .map_err(|e| GenError::Invalid(format!("rest from previous operations: {e}")))?;
    let ops: Vec<SimOp> = ahead
        .iter()
        .filter(|a| a.cut)
        .map(|a| SimOp {
            moves: &a.moves,
            radius: a.radius,
            ball: a.ball,
        })
        .collect();
    stock_sim::simulate(&mut dex, &ops)
        .map_err(|e| GenError::Invalid(format!("rest from previous operations: {e}")))?;
    Ok(PriorState {
        grid: Some(dex),
        digest: Some(digest),
    })
}

/// The staleness story re-machining adds to `stale_reason`'s list of
/// live questions - asked LAST, after every question the operation can
/// answer about its own inputs. If the dependent still reads Stale
/// with all of those clean, its rest digest moved, and the only terms
/// that can move are a cutting predecessor's path or atom: a never
/// generated predecessor contributes a constant. So naming the first
/// cutting predecessor IS the honest attribution, whether that
/// operation itself reads Ok (it was generated after the dependent
/// was) or Stale (something ahead moved it, or the batch simply has
/// not run). The order law above is what makes "ahead" mean what the
/// machine means.
pub fn prior_stale(world: &World, name: &str) -> Option<String> {
    ahead_of(world, name)
        .into_iter()
        .find(|a| a.cut)
        .map(|a| {
            format!(
                "operation '{}' ahead of it in the program changed - regenerate the program in order",
                a.name
            )
        })
}
