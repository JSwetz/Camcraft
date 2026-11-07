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
//! - **Program order is the post's order - by CALL, not by reference.**
//!   [`crate::setup::operation_order`]: setup rank, then operation position
//!   within a setup - the very function `posting::program_ops` calls.
//!   What the machine will do first IS what was cut first; the sort's
//!   known wart (an operation named `Operation10` sorts before
//!   `Operation2` inside one setup) is shared so re-machining and the
//!   machine can never disagree.
//! - **Rest is same-setup.** A predecessor machines in the CONSUMER's
//!   setup or it cut a differently-placed block: cross-setup masks
//!   would need the flip transform (M3.3's orientation law), so an
//!   operation from another setup is simply not ahead here. Within one
//!   setup this is physically exact; across setups a later setup
//!   re-cuts conservatively - air, never a gouge (the mask law).
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
//!   dependent reads Stale: one `RegenerateAll` converges, its rounds
//!   re-reading each predecessor's new path (M3.1 C, #145). (A recorded `stamp` term was the first design; the stamp
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
    /// The operation's kind: on a lathe only the Facing kind is read
    /// ([`prior_plan`], #89 review).
    kind: crate::operation::OpType,
    path: u64,
    radius: f64,
    ball: bool,
    /// Cot of the half-angle, Some for a chamfer mill (M2.4 B) and a
    /// twist drill's point (#89 review: a pilot's cone stands):
    /// the rest grid must sweep the same cone the program sim does, or
    /// a predecessor's chamfer would evaporate a full disc of
    /// material that is really still standing.
    cone: Option<f64>,
    /// The bull nose's corner radius (M13.1, `stock_sim::bull_corner`):
    /// the rest grid sweeps the same corner the program sim does, so a
    /// bull predecessor's fillet ring reads as the rest it is.
    corner: Option<f64>,
    /// A turning insert's nose radius (issue #89): its moves carry the
    /// nose CENTRE, so the prior plan cuts it as a ball of that radius
    /// (tip = centre - nose) - the insert's real cutting edge, never
    /// the flat disc of its whole size. Only [`PriorPlan`] reads it.
    nose: Option<f64>,
    cut: bool,
    moves: Vec<Move>,
}

/// Identity of a cached path: every field a move carries, bit-exact.
/// The generators are deterministic functions of what the stamp
/// digests, so equal paths here means the mask below builds identically.
fn path_hash(moves: &[Move]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"camcraft-rest-path-v1");
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
                // M11.2 A: only a vertical plane adds a byte, so every
                // XY arc hashes exactly as it always did.
                if !a.plane.is_xy() {
                    h.write_u8(a.plane.g_code());
                }
            }
            None => h.write_u8(0),
        }
    }
    h.finish()
}

/// The operations in program order ahead of `name` in the consumer's
/// setup. The ordering law is shared with the post BY CALL - both walk
/// [`crate::setup::operation_order`] - and the sequence is cut at the
/// consumer, same-setup filtered (module header). `posting::program_ops`
/// itself is not called: it refuses whole programs over a `Never`
/// operation, and an operation later in the program is nobody's
/// business here.
fn ahead_of(world: &World, name: &str, consumer_setup: Option<&str>) -> Vec<Ahead> {
    let ops = crate::setup::operation_order(world);
    let consumer = crate::setup::effective(world, consumer_setup);
    let Some(self_ix) = ops
        .iter()
        .position(|(n, _)| n == name)
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(self_ix);
    for (n, e) in &ops[..self_ix] {
        let Some(op) = model::operation_of(world, e) else {
            continue; // in the operation list; lost a beat later
        };
        if crate::setup::effective(world, op.setup.as_deref()) != consumer {
            continue; // another setup's cut is not this block's rest
        }
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
            kind: op.params.kind(),
            path: if cut { path_hash(&moves) } else { 0 },
            radius: tool.as_ref().map_or(0.0, |t| t.diameter * 0.5),
            ball: tool
                .as_ref()
                .is_some_and(|t| t.kind == crate::tools::ToolKind::BallNose),
            cone: tool
                .as_ref()
                .filter(|t| matches!(t.kind, crate::tools::ToolKind::Taper | crate::tools::ToolKind::Drill))
                .map(|t| (t.tip_angle.to_radians() * 0.5).tan())
                .filter(|x| x.is_finite() && *x > 0.0)
                .map(|x| 1.0 / x),
            // A nonsense corner falls back to the disc: over-reported
            // removal, the side `Tool::is_sane` keeps unreachable.
            corner: tool
                .as_ref()
                .and_then(|t| stock_sim::bull_corner(t).ok().flatten()),
            nose: tool
                .as_ref()
                .filter(|t| t.kind == crate::tools::ToolKind::Turning)
                .map(|t| t.nose_radius)
                .filter(|n| n.is_finite() && *n > 0.0),
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
    h.write(b"camcraft-rest-v1");
    for a in ahead {
        h.write(a.name.as_bytes());
        if a.cut {
            h.write_u8(1);
            h.write_u64(a.path);
            h.write(&a.radius.to_bits().to_le_bytes());
            h.write_u8(u8::from(a.ball));
            // The cone's slope is part of the atom the grid depends
            // on, so a re-coned chamfer mill churns the stamp.
            match a.cone {
                Some(c) => {
                    h.write_u8(1);
                    h.write(&c.to_bits().to_le_bytes());
                }
                None => h.write_u8(0),
            }
            // The bull's corner (M13.1) is atom too; only a bull
            // writes the byte, so every other stamp hashes unchanged.
            if let Some(c) = a.corner {
                h.write_u8(2);
                h.write(&c.to_bits().to_le_bytes());
            }
            // The insert's nose is atom too (#89 review).
            if let Some(n) = a.nose {
                h.write_u8(3);
                h.write(&n.to_bits().to_le_bytes());
            }
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
pub fn prior_digest(world: &World, name: &str, setup: Option<&str>) -> Option<u64> {
    let ahead = ahead_of(world, name, setup);
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
    setup: Option<&str>,
    tool_r: f64,
) -> Result<PriorState, GenError> {
    let ahead = ahead_of(world, name, setup);
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
    let mut dex = stock_sim::stock_grid(world, setup, min_r)
        .map_err(|e| GenError::Invalid(format!("rest from previous operations: {e}")))?;
    let ops: Vec<SimOp> = ahead
        .iter()
        .filter(|a| a.cut)
        .map(|a| SimOp {
            moves: &a.moves,
            radius: a.radius,
            ball: a.ball,
            cone: a.cone,
            corner: a.corner,
        })
        .collect();
    stock_sim::simulate(&mut dex, &ops)
        .map_err(|e| GenError::Invalid(format!("rest from previous operations: {e}")))?;
    Ok(PriorState {
        grid: Some(dex),
        digest: Some(digest),
    })
}

/// The program ahead of one operation, collected but not yet cut
/// (issue #96 review): the stock grid and the predecessors' cached
/// moves with their atoms. Verify's vertical-rapid law needs the rest
/// surface only when it has convicted a dive, so the simulation - the
/// costly half of [`prior_state`] - runs on the job thread, and only
/// then ([`PriorPlan::simulate`]).
pub struct PriorPlan {
    grid: Dexel,
    ahead: Vec<Ahead>,
}

impl PriorPlan {
    /// The grid the plan's operations leave standing; a refusal (a
    /// posed predecessor the column grid cannot cut) is the caller's
    /// "prior unknown" - the raw stock, never an acquittal.
    pub fn simulate(self) -> Result<Dexel, String> {
        let mut dex = self.grid;
        // A turning insert cuts with its nose: the centre path lowered
        // to the tip, swept as a ball (issue #89).
        let tips: Vec<Vec<Move>> = self
            .ahead
            .iter()
            .map(|a| match a.nose {
                Some(n) => a
                    .moves
                    .iter()
                    // A vertical-plane arc's centre moves with its ends:
                    // lowering only the ends would bend it (review nit).
                    .filter(|m| m.arc.as_ref().is_none_or(|a| a.plane.is_xy()))
                    .map(|m| {
                        let mut m = *m;
                        m.from.z -= n;
                        m.to.z -= n;
                        m
                    })
                    .collect(),
                None => Vec::new(),
            })
            .collect();
        let ops: Vec<SimOp> = self
            .ahead
            .iter()
            .zip(&tips)
            .map(|(a, tip)| match a.nose {
                Some(n) => SimOp { moves: tip, radius: n, ball: true, cone: None, corner: None },
                None => SimOp { moves: &a.moves, radius: a.radius, ball: a.ball, cone: a.cone, corner: a.corner },
            })
            .collect();
        stock_sim::simulate(&mut dex, &ops).map_err(|e| e.to_string())?;
        Ok(dex)
    }
}

/// The program ahead of `name` in its setup, collected for verify's
/// prior reading: `Ok(None)` when nothing ahead has cut (the raw stock
/// IS the prior), an error when the stock grid cannot be built (the
/// caller treats that as an unknown prior - raw stock).
pub fn prior_plan(world: &World, name: &str, setup: Option<&str>, tool_r: f64) -> Result<Option<PriorPlan>, String> {
    let ahead: Vec<Ahead> = ahead_of(world, name, setup).into_iter().filter(|a| a.cut).collect();
    if ahead.is_empty() {
        return Ok(None);
    }
    // #89 review (B1/B2): on a lathe the column grid credits a turning
    // move under standing material (a parting kerf, an ID groove behind
    // a lip) with everything above it, and a bar or blade cut as a full
    // disc would be revolved round the bar. Only the Facing kind, cut
    // with its nose, never undercuts: any other kind ahead is unknown.
    // The same holds for a turning kind on a mill-turn: the column grid
    // reads its undercuts the same way, so outside a lathe every turning
    // kind but nose-cut Facing ahead is unknown too.
    let lathe = crate::machine::kind_of(world) == crate::machine::Kind::Lathe;
    let facing = |a: &Ahead| a.kind == crate::operation::OpType::Facing && a.nose.is_some();
    let turning = |a: &Ahead| a.kind.info().family() == crate::operation::Family::Turn;
    if let Some(a) = ahead.iter().find(|a| !facing(a) && (lathe || turning(a))) {
        return Err(format!(
            "operation '{}' ahead is a turning pass other than facing - the column grid cannot read its removal",
            a.name
        ));
    }
    let min_r = ahead.iter().map(|a| a.nose.unwrap_or(a.radius)).filter(|r| *r > 0.0).fold(tool_r, f64::min);
    let grid = stock_sim::stock_grid(world, setup, min_r)?;
    Ok(Some(PriorPlan { grid, ahead }))
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
pub fn prior_stale(world: &World, name: &str, setup: Option<&str>) -> Option<String> {
    ahead_of(world, name, setup)
        .into_iter()
        .find(|a| a.cut)
        .map(|a| {
            format!(
                "operation '{}' ahead of it in the program changed - regenerate the program in order",
                a.name
            )
        })
}

/// A twist drill ahead in the setup, as the turned family's rapid law
/// credits it (#103): its cut moves, the drill's radius and the cot of
/// its point's half-angle. A drill that states no point (tip angle 0 or
/// 180) is read as the standard 118-degree point: a flat point would
/// credit the full radius down to the tip - MORE removed (review S3).
pub struct DrillAhead {
    pub radius: f64,
    pub cone: Option<f64>,
    pub moves: Vec<Move>,
}

/// cot(59 deg): the 118-degree twist drill point (machine-tool handbooks).
const STANDARD_POINT_COT: f64 = 0.600_860_619_027_560_4;

/// Every Drill operation ahead of `name` in its setup that has cut. Read
/// on a lathe too - the axial hole is a cylinder about the spindle, so
/// the meridian law reads it without the column grid (the prior plan's
/// facing-only rule is unchanged).
pub fn drills_ahead(world: &World, name: &str, setup: Option<&str>) -> Vec<DrillAhead> {
    ahead_of(world, name, setup)
        .into_iter()
        .filter(|a| a.cut && a.kind == crate::operation::OpType::Drill && a.radius > 0.0)
        .map(|a| DrillAhead { radius: a.radius, cone: a.cone.or(Some(STANDARD_POINT_COT)), moves: a.moves })
        .collect()
}
