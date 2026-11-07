//! The program ahead of one operation: who machines before it in its
//! setup, and the digest of the paths and tool atoms its rest depends on.
//!
//! Provenance: our own design (M3.1 stage D), split out of `rest.rs` for
//! AGENTS.md's 400-line limit (issue #127); general machining knowledge,
//! no vendor implementation inspected. The laws live in `rest.rs`'s header.

use crate::cl::Move;
use crate::model;
use crate::stock_sim;
use flecs_ecs::prelude::World;

/// One operation ahead of the consumer: name and the hash of its
/// recorded path (both digest terms), its tool atom (the grid depends
/// on it), and the cached moves themselves when it has cut anything.
pub(super) struct Ahead {
    pub(super) name: String,
    /// The operation's kind: on a lathe only the Facing kind is read
    /// ([`super::prior_plan`], #89 review).
    pub(super) kind: crate::operation::OpType,
    path: u64,
    pub(super) radius: f64,
    pub(super) ball: bool,
    /// Cot of the half-angle, Some for a chamfer mill (M2.4 B) and a
    /// twist drill's point (#89 review: a pilot's cone stands):
    /// the rest grid must sweep the same cone the program sim does, or
    /// a predecessor's chamfer would evaporate a full disc of
    /// material that is really still standing.
    pub(super) cone: Option<f64>,
    /// The bull nose's corner radius (M13.1, `stock_sim::bull_corner`):
    /// the rest grid sweeps the same corner the program sim does, so a
    /// bull predecessor's fillet ring reads as the rest it is.
    pub(super) corner: Option<f64>,
    /// A turning insert's nose radius (issue #89): its moves carry the
    /// nose CENTRE, so the prior plan cuts it as a ball of that radius
    /// (tip = centre - nose) - the insert's real cutting edge, never
    /// the flat disc of its whole size. Only [`super::PriorPlan`] reads it.
    pub(super) nose: Option<f64>,
    pub(super) cut: bool,
    pub(super) moves: Vec<Move>,
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
pub(super) fn ahead_of(world: &World, name: &str, consumer_setup: Option<&str>) -> Vec<Ahead> {
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
pub(super) fn digest_of(ahead: &[Ahead]) -> u64 {
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
