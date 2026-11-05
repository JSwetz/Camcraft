//! The machined volume unfolding with the playhead (M7.2 C): the
//! display field cut move by move up to the transport's position,
//! seeking backward from a checkpoint. Provenance: the M7.1 field
//! engine's order-independent subtraction (a running max of swept-tool
//! distances, so any prefix of the program is cut exactly by its own
//! moves); the checkpoint journal is plain periodic snapshotting.
//! Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Playback is the field's truth unfolding.** The stock shown at
//!   move `k` is the uncut stock minus exactly the program's first `k`
//!   moves - the same cut `stock_view::field_view` makes for all of
//!   them, so the last frame of playback IS the Deviate picture.
//! - **Backward seek re-applies from a checkpoint.** A snapshot of the
//!   cut stock is kept every `stride` moves on the way forward; seeking
//!   back clones the nearest one at or before the target and cuts the
//!   rest. The snapshot count is bounded (`MAX_CHECKPOINTS`), so memory
//!   is a constant multiple of one field, never proportional to moves.
//!   (Dirty-brick deltas instead of whole snapshots are M7.3's cost law.)
//! - **Only completed moves cut.** The move under the playhead has not
//!   finished, so it has not removed its material yet; a seek to the
//!   end cuts everything.
//! - **The timeline and the plan agree by name, or nothing plays.**
//!   The transport's script and the posting plan are walked by
//!   different callers; an operation the plan does not hold is a
//!   refusal, never a silent skip that would show material the
//!   machine removes as standing.

use crate::sim::Script;
use crate::stock_sim::{SimOp, Stock};
use crate::stock_view::{display_parts, display_target, FieldView, DEFAULT_TEXELS};

/// Snapshots kept at most - the backward seek's memory bound.
pub const MAX_CHECKPOINTS: usize = 32;

/// The smallest checkpoint stride, in moves.
const MIN_STRIDE: usize = 64;

struct PlayOp {
    name: String,
    moves: Vec<crate::cl::Move>,
    radius: f64,
    ball: bool,
    cone: Option<f64>,
    /// Global index of this op's first move.
    start: usize,
}

/// The playback field: the uncut stock, the plan, the stock cut to
/// `at` moves, and the checkpoint journal.
pub struct FieldPlay {
    ops: Vec<PlayOp>,
    total: usize,
    stride: usize,
    /// (moves cut, the stock after them), ascending; [0] is uncut.
    checkpoints: Vec<(usize, Stock)>,
    cur: Stock,
    at: usize,
    target: Option<Stock>,
    target_note: Option<String>,
    cell: f64,
}

impl FieldPlay {
    /// Build the playback for the document's program at the default
    /// display budget. `judged`: carry the model's field so the eye
    /// classes each hit (the Deviate cell is armed); otherwise the
    /// picture is unjudged material.
    pub fn new(world: &flecs_ecs::prelude::World, judged: bool) -> Result<Self, String> {
        Self::with_budget(world, judged, DEFAULT_TEXELS)
    }

    /// `new` at a display budget of `texels` cells (the
    /// `field_view_with` twin: coarser is cheaper, never less honest).
    pub fn with_budget(world: &flecs_ecs::prelude::World, judged: bool, texels: f64) -> Result<Self, String> {
        let (stock, planned, cell) = display_parts(world, texels)?;
        let mut ops = Vec::with_capacity(planned.len());
        let mut total = 0usize;
        for p in planned {
            let n = p.moves.len();
            ops.push(PlayOp { name: p.name, moves: p.moves, radius: p.radius, ball: p.ball, cone: p.cone, start: total });
            total = total.saturating_add(n);
        }
        let (target, target_note) = if judged {
            display_target(world, cell)
        } else {
            (None, Some("playback shows material, not a verdict - arm Deviate to judge".to_string()))
        };
        let stride = total.div_ceil(MAX_CHECKPOINTS).max(MIN_STRIDE);
        Ok(FieldPlay {
            ops,
            total,
            stride,
            checkpoints: vec![(0, stock.clone())],
            cur: stock,
            at: 0,
            target,
            target_note,
            cell,
        })
    }

    /// Moves in the whole program.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Moves the shown stock has been cut by.
    #[must_use]
    pub const fn at(&self) -> usize {
        self.at
    }

    /// Whether the model's field rides along (see `new`).
    #[must_use]
    pub const fn judged(&self) -> bool {
        self.target.is_some()
    }

    /// Snapshots held (bounded by `MAX_CHECKPOINTS` + 1).
    #[must_use]
    pub fn checkpoints(&self) -> usize {
        self.checkpoints.len()
    }

    /// The global move index the playhead `p` stands at, or `None`
    /// when the plan holds no op of that name. An op's timeline is its
    /// moves, one segment each, behind at most one connector rapid (the
    /// air between programs, cut by nothing): a span one longer than
    /// the op's moves carries that connector, and it is not a move.
    #[must_use]
    pub fn index_at(&self, p: &Progress) -> Option<usize> {
        let op = self.ops.iter().find(|o| o.name == p.op)?;
        let connector = usize::from(p.span == op.moves.len() + 1);
        let done = p.segs_done.saturating_sub(connector).min(op.moves.len());
        Some(op.start.saturating_add(done))
    }

    /// Cut (or restore) the shown stock to exactly `k` moves; `Ok(true)`
    /// when the picture changed. `k` past the end clamps to the end.
    pub fn seek(&mut self, k: usize) -> Result<bool, String> {
        let k = k.min(self.total);
        if k == self.at {
            return Ok(false);
        }
        if k < self.at {
            let (at, snap) = self
                .checkpoints
                .iter()
                .rev()
                .find(|(n, _)| *n <= k)
                .ok_or("playback lost its uncut stock")?;
            self.cur = snap.clone();
            self.at = *at;
        }
        // Forward in stride-sized runs, snapshotting each boundary
        // passed for the first time.
        while self.at < k {
            let boundary = (self.at / self.stride + 1).saturating_mul(self.stride);
            let to = k.min(boundary);
            self.cut(self.at, to)?;
            self.at = to;
            let last = self.checkpoints.last().map_or(0, |c| c.0);
            if to == boundary && to > last && self.checkpoints.len() <= MAX_CHECKPOINTS {
                self.checkpoints.push((to, self.cur.clone()));
            }
        }
        Ok(true)
    }

    /// The shown stock as the eye's view (a snapshot: the eye holds it
    /// while playback moves on).
    #[must_use]
    pub fn view(&self) -> FieldView {
        FieldView {
            stock: self.cur.clone(),
            target: self.target.clone(),
            target_note: self.target_note.clone(),
            cell: self.cell,
        }
    }

    /// Subtract global moves `[from, to)` from the shown stock.
    fn cut(&mut self, from: usize, to: usize) -> Result<(), String> {
        for op in &self.ops {
            let end = op.start.saturating_add(op.moves.len());
            let (a, b) = (from.max(op.start), to.min(end));
            if a >= b {
                continue;
            }
            let moves = op.moves.get(a - op.start..b - op.start).ok_or("playback: a move range fell outside its op")?;
            let sim = [SimOp { moves, radius: op.radius, ball: op.ball, cone: op.cone }];
            self.cur
                .simulate_display(&sim)
                .map_err(|e| format!("playback could not cut {}: {e}", op.name))?;
        }
        Ok(())
    }
}

/// Where the transport's playhead stands, in the timeline's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// The op under the playhead.
    pub op: String,
    /// Its segments completed (a connector rapid counts here).
    pub segs_done: usize,
    /// Its segments in all.
    pub span: usize,
}

/// The playhead at `t` as a [`Progress`]: `None` for an empty
/// timeline. Before the first segment completes nothing is done; at or
/// past the end every segment of the last op is. Only completed
/// segments count - the one under the playhead has not cut yet.
#[must_use]
pub fn progress(sc: &Script, t: f64) -> Option<Progress> {
    let last = sc.segs.len().checked_sub(1)?;
    let t = if t.is_finite() { t } else { 0.0 };
    // The segment the playhead is inside: the first whose end is past t.
    let i = sc.cum.partition_point(|&c| c <= t).min(last);
    let seg = sc.segs.get(i)?;
    let op = sc.ops.get(seg.op)?.clone();
    let first = sc.segs.iter().position(|s| s.op == seg.op)?;
    let end = sc.segs.iter().rposition(|s| s.op == seg.op)?;
    let finished = if t >= sc.total { i + 1 } else { i };
    let span = end + 1 - first;
    Some(Progress { op, segs_done: finished.saturating_sub(first).min(span), span })
}
