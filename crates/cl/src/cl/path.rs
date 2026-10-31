//! The path container and its builder: the chain, its cycles, and the
//! cursor that maintains the link invariant by construction.

use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::geometry::units::Unit;

use super::moves::{Move, MoveKind};
use super::cycle::Cycle;
use super::pose::{is_vertical, normalize_axis};

/// A generated path: linked moves, plus — for drilling paths only — the
/// per-hole cycle records the post needs. Caching, staleness and
/// serialization wrap this at the schema/project layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub moves: Vec<Move>,
    /// Empty for every non-drilling path; drilling keeps these in hole
    /// order and in agreement with `moves` (there is a law, and a test).
    pub cycles: Vec<Cycle>,
    /// The generator's own statements about the path (M8.1): what it
    /// substituted and why - the entry-law fallbacks the moves alone
    /// cannot show. Never geometry, never posted to G-code; the status
    /// line says them verbatim, so a silent fallback cannot exist.
    /// Carried by the cache as an additive key (absent = none).
    pub notes: Vec<String>,
}

impl Toolpath {
    pub fn new(moves: Vec<Move>) -> Self {
        Toolpath {
            moves,
            cycles: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Where the machine is when the path starts (first move's `from`).
    pub fn start(&self) -> Option<Point3> {
        self.moves.first().map(|m| m.from)
    }

    pub fn end(&self) -> Option<Point3> {
        self.moves.last().map(|m| m.to)
    }

    pub fn all_finite(&self) -> bool {
        self.moves.iter().all(|m| m.is_finite())
            && self.cycles.iter().all(|c| {
                c.xy.x.is_finite()
                    && c.xy.y.is_finite()
                    && c.r_z.is_finite()
                    && c.entry_z.is_finite()
                    && c.bottom_z.is_finite()
                    && c.peck.is_finite()
                    && c.dwell.is_finite()
            })
    }

    /// True when every move starts where its predecessor ended.
    pub fn is_chained(&self) -> bool {
        self.moves.windows(2).all(|w| w[0].to == w[1].from)
    }

    /// Does any move traverse an arc? Decides the golden format
    /// version, among other things.
    pub fn has_arcs(&self) -> bool {
        self.moves.iter().any(|m| m.arc.is_some())
    }

    /// How many moves are arcs (the status line's honest count).
    pub fn arc_count(&self) -> usize {
        self.moves.iter().filter(|m| m.arc.is_some()).count()
    }

    /// True when no move asks the tool to lean: every stated axis (if
    /// any) is the Z column itself. The generators' own verdict, and
    /// the consumer's shortcut for "today's ordinary path" (M5.1 A).
    pub fn is_prismatic(&self) -> bool {
        self.moves.iter().all(|m| m.axis.is_none_or(|a| is_vertical(&a)))
    }

    /// The status line after a successful generate, counted from the
    /// moves themselves: plunge count with the plunge feed the path
    /// actually carries, the number of distinct cut levels (feeds at
    /// that height, whatever lifted the tool there), arc moves, and the
    /// verify warnings. Nothing a generator merely intended - only what
    /// the path shows.
    #[must_use]
    pub fn status_summary(&self, name: &str, warnings: usize, unit: Unit) -> String {
        let n = self.moves.len();
        let p = self.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
        let a = self.arc_count();
        let mut levels: Vec<f64> = Vec::new();
        for m in &self.moves {
            if m.kind == MoveKind::Feed && !levels.contains(&m.to.z) {
                levels.push(m.to.z);
            }
        }
        let feed = self
            .moves
            .iter()
            .find(|m| m.kind == MoveKind::Plunge)
            .and_then(|m| m.feed)
            .map_or(String::new(), |f| format!(" @ {}", crate::geometry::units::feed(f, unit)));
        // The generator's substitutions ride the same line (M8.1): the
        // status that shows a clean move count also states what the
        // entry family could not do and where it dived instead.
        let notes = if self.notes.is_empty() {
            String::new()
        } else {
            format!(" - {}", self.notes.join(" - "))
        };
        format!(
            "generated {name}: {n} moves ({p} plunge(s){feed}, {} level(s), {a} arc(s)), {warnings} warning(s){notes}",
            levels.len()
        )
    }
}

/// Chains moves onto a Toolpath, maintaining the link invariant by
/// construction: a move's `from` is always the cursor.
pub struct PathBuilder {
    at: Point3,
    /// The pose the NEXT move will carry (M5.1 stage A): None until a
    /// generator asks the tool to lean. `at_pose` is the pose the last
    /// move actually held - what makes a pose change at a standing
    /// point REAL motion (a rotation the post must command), and what
    /// keeps the builder's no-motion drop from silently swallowing one.
    axis: Option<[f64; 3]>,
    at_pose: Option<[f64; 3]>,
    moves: Vec<Move>,
    cycles: Vec<Cycle>,
    notes: Vec<String>,
}

impl PathBuilder {
    pub fn new(at: Point3) -> Self {
        PathBuilder {
            at,
            axis: None,
            at_pose: None,
            moves: Vec::new(),
            cycles: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// State something about the path being built (M8.1): a
    /// substitution the moves alone cannot show - an entry motion that
    /// could not be honored and the motion taken in its place. Notes
    /// ride the finished path to the status line; they are never
    /// geometry and never posted. Log, never silence.
    pub fn note(&mut self, msg: impl Into<String>) {
        self.notes.push(msg.into());
    }

    pub fn at(&self) -> Point3 {
        self.at
    }

    /// Moves laid down so far - a cycle generator snapshots this to
    /// stamp the cycle's `first_move` once the tool stands at the cycle
    /// entry plane, so the group boundary never swallows positioning
    /// rapids.
    pub fn moves_len(&self) -> usize {
        self.moves.len()
    }

    /// Declare the tool pose the following moves hold (M5.1 stage A):
    /// `Some(vector)` normalizes through [`Move::with_axis`]'s law
    /// (a direction-stating vector, refused LOUDLY otherwise), `None`
    /// returns to no angle stated. The pose rides every move pushed
    /// from here on, until said otherwise - a generator tilts by
    /// calling this, then walking.
    pub fn set_axis(&mut self, axis: Option<[f64; 3]>) {
        self.axis = axis.and_then(|a| match normalize_axis(a) {
            Some(n) => Some(n),
            None => {
                eprintln!("cl: refusing a builder axis of {a:?}: not a direction");
                None
            }
        });
    }

    /// Record a hole's cycle intent alongside the moves built so far.
    /// The expanded chain stays the physical truth; this is what lets
    /// the post emit one canned block instead of the chain's letters.
    pub fn push_cycle(&mut self, cycle: Cycle) {
        self.cycles.push(cycle);
    }

    /// Append a move to `to`; degenerate no-motion moves are dropped -
    /// and no motion means the endpoint is where we stand AND the pose
    /// is the one the last move held. A pose change at a standing tip
    /// is the table or head rotating: real motion, it posts (M5.1 A).
    pub fn push(&mut self, kind: MoveKind, to: Point3, feed: Option<f64>) {
        if to.x == self.at.x && to.y == self.at.y && to.z == self.at.z && self.axis == self.at_pose {
            return;
        }
        self.moves.push(Move::new(kind, self.at, to, feed).attach_axis(self.axis));
        self.at = to;
        self.at_pose = self.axis;
    }

    /// Append a move to `to` traversing the circular arc about `center`,
    /// travelling `ccw`. Endpoints stay the cursor and `to` exactly, so
    /// the chaining invariant holds by construction the same way
    /// [`push`](Self::push) holds it. The pose follows the same law:
    /// every arc of a tilted path is a circle the tool walks in its own
    /// tilted plane, XY-projected like any other arc at stage A.
    ///
    /// Refusals, never normalizations: a zero-sweep arc (no XY motion)
    /// is dropped like any no-motion move - which is why `fit_arcs`
    /// splits full circles before they reach this call, so a ring is
    /// never silently erased - and a non-finite center falls back to the
    /// straight move: the motion always posts, only its exact shape gets
    /// the honest simpler answer. A Z change between the endpoints is a
    /// helix (M2.1); XY motion is what makes it an arc.
    pub fn push_arc(
        &mut self,
        kind: MoveKind,
        to: Point3,
        feed: Option<f64>,
        center: Point2,
        ccw: bool,
    ) {
        let arcable = (to.x != self.at.x || to.y != self.at.y)
            && center.x.is_finite()
            && center.y.is_finite();
        if !arcable {
            self.push(kind, to, feed);
            return;
        }
        self.moves
            .push(Move::new(kind, self.at, to, feed).with_arc(center, ccw).attach_axis(self.axis));
        self.at = to;
        self.at_pose = self.axis;
    }

    pub fn finish(self) -> Toolpath {
        Toolpath {
            moves: self.moves,
            cycles: self.cycles,
            notes: self.notes,
        }
    }
}
