//! Move simulation: the timeline law that turns generated toolpaths
//! into time.
//!
//! Provenance: our own design; time = distance ÷ rate is general
//! machining arithmetic (machine-tool handbooks), and the traverse
//! numbers ride the `.mch` kinematics specced in
//! `docs/CNC-INTERMEDIATE.md` - the same machine file the post reads,
//! so the simulation and the program disagree about nothing.
//!
//! The chain is the truth (M1.6's law): the sim walks the expanded
//! moves - rapids, feeds, plunge hops, tapping retracts - never the
//! cycle records, and so it sees exactly what verify saw and the
//! overlay draws. Cycles contribute one thing only: their dwell, the
//! seconds the machine waits at the bottom, which the move chain
//! cannot express (a zero-length move would poison the distance
//! laws). Between operations the sim inserts the connector rapid the
//! machine physically makes, so one document is one continuous
//! timeline.

use crate::cl::{ArcMove, Move, MoveKind, Toolpath};
use crate::geometry::Point3;
use serde_json::Value;
use std::f64::consts::PI;

/// Axis traverse rates, mm/min, read off the machine's X/Y/Z rapids.
/// The default stands in when a machine file names no usable axis:
/// the shipped 3-axis VMC's own numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traverse {
    pub xy: f64,
    pub z: f64,
}

impl Default for Traverse {
    fn default() -> Self {
        Self {
            xy: 6000.0,
            z: 4000.0,
        }
    }
}

fn rate(v: Option<&Value>) -> Option<f64> {
    let v = v?.as_f64()?;
    (v.is_finite() && v > 0.0).then_some(v)
}

impl Traverse {
    /// The rapid rates from a parsed `.mch`: XY takes the *slowest*
    /// horizontal rapid (a simultaneous two-axis move is paced by its
    /// slowest axis), Z its own. Unreadable files leave the defaults
    /// standing.
    #[must_use]
    pub fn from_machine(m: &Value) -> Self {
        let mut t = Self::default();
        let Some(axes) = m
            .get("kinematics")
            .and_then(|k| k.get("static"))
            .and_then(|s| s.get("axes"))
            .and_then(|a| a.as_array())
        else {
            return t;
        };
        let mut xy: Option<f64> = None;
        for ax in axes {
            let dir: Vec<f64> = ax
                .get("direction")
                .and_then(|d| d.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default();
            let Some(rapid) = ax.get("speeds").and_then(|s| rate(s.get("rapid"))) else {
                continue;
            };
            // A Z-dominant direction owns the Z rate; anything else
            // participates in the XY pace.
            let dz = dir.get(2).copied().unwrap_or(0.0).abs();
            if dz > 0.5 {
                t.z = rapid;
            } else {
                xy = Some(xy.map_or(rapid, |c| c.min(rapid)));
            }
        }
        if let Some(v) = xy {
            t.xy = v;
        }
        t
    }

    /// Simultaneous-axis rapid time: each axis travels on its own
    /// rate; the move ends when its slowest axis arrives.
    fn rapid_secs(&self, d: Point3) -> f64 {
        if !(self.xy > 0.0 && self.z > 0.0) {
            return 0.0;
        }
        60.0 * (d.x.abs() / self.xy)
            .max(d.y.abs() / self.xy)
            .max(d.z.abs() / self.z)
    }
}

/// One traversed move on the timeline. Arcs stay exact (the sweep is
/// interpolated by angle, never pre-flattened - display and verify
/// flatten, the timeline should not).
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub from: Point3,
    pub to: Point3,
    pub arc: Option<ArcMove>,
    /// Travel time in seconds (0 for a zero-length move).
    pub secs: f64,
    /// Standstill at the end - a cycle dwell, 0.0 for nearly every seg.
    pub dwell: f64,
    /// The feed the readout shows while moving; None means rapid.
    pub feed: Option<f64>,
    /// Index into [`Script::ops`].
    pub op: usize,
}

/// What the readout says at a point in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Readout {
    /// Nothing to show (empty script, or standing past the end).
    Idle,
    /// Moving under G0.
    Rapid,
    /// Moving at this feed, mm/min.
    Feed(f64),
    /// Standing at a cycle bottom for this many seconds.
    Dwell(f64),
}

/// The machine's state at one instant of the script.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub pos: Point3,
    /// The operation being executed, if any.
    pub op: Option<usize>,
    pub readout: Readout,
    /// The timeline is spent (t at or past total).
    pub done: bool,
}

/// A whole document's timeline. `cum[i]` is the *end* of `segs[i]`'s
/// window (travel plus dwell) and `total == cum.last()`; segment `i`
/// starts at `cum[i-1]` (0.0 for the first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    /// Operation names in document order, the ones the script runs.
    pub ops: Vec<String>,
    pub segs: Vec<Seg>,
    pub cum: Vec<f64>,
    pub total: f64,
    /// Past [`MAX_SIM_SEGS`] the build stops; the UI must say so.
    pub truncated: bool,
}

/// The overlay vertex budget's sibling: a pathological cache (millions
/// of file-loaded moves) must not turn every frame into a
/// million-segment allocation. Past it the script ends early and
/// `truncated` speaks.
pub const MAX_SIM_SEGS: usize = 1 << 20;

impl Script {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// The machine at time `t` (seconds). Out-of-range t clamps:
    /// before 0 is the untouched start, past the end stands at the
    /// final point with `done`.
    #[must_use]
    pub fn at(&self, t: f64) -> Frame {
        let Some(first) = self.segs.first() else {
            return Frame {
                pos: Point3::new(0.0, 0.0, 0.0),
                op: None,
                readout: Readout::Idle,
                done: true,
            };
        };
        if !t.is_finite() {
            return self.at(self.total);
        }
        if t <= 0.0 {
            return Frame {
                pos: first.from,
                op: Some(first.op),
                readout: Readout::Idle,
                done: false,
            };
        }
        if t >= self.total {
            // The program is over; the machine stands where it ended.
            // Checked against the total directly - the window walk
            // below cannot be trusted for one ULP at the tail.
            let last = self.segs.last().unwrap_or(first);
            return Frame {
                pos: last.to,
                op: Some(last.op),
                readout: Readout::Idle,
                done: true,
            };
        }
        // The first segment whose window ends after t; nothing past
        // the end has one, and the clamp makes the last hold.
        let i = self.cum.partition_point(|&e| e <= t).min(self.segs.len() - 1);
        let s = &self.segs[i];
        let start = if i == 0 { 0.0 } else { self.cum[i - 1] };
        let local = t - start;
        if local < s.secs {
            let f = if s.secs > 0.0 {
                (local / s.secs).clamp(0.0, 1.0)
            } else {
                1.0
            };
            Frame {
                pos: at_seg(s, f),
                op: Some(s.op),
                readout: match s.feed {
                    Some(v) => Readout::Feed(v),
                    None => Readout::Rapid,
                },
                done: false,
            }
        } else if local - s.secs < s.dwell {
            Frame {
                pos: s.to,
                op: Some(s.op),
                readout: Readout::Dwell(s.dwell),
                done: false,
            }
        } else {
            // Past every window (or a float whisper of one at the
            // tail): the machine stands where it ended. `done` reads
            // the total directly, so no rounding of the window walk
            // can claim it early.
            Frame {
                pos: s.to,
                op: Some(s.op),
                readout: Readout::Idle,
                done: t >= self.total,
            }
        }
    }
}

/// The point at fraction `f` along a segment (arc-aware; the angle
/// law is `arc_chords`' exact survivor - measured off `from`, so no
/// rounding accumulates).
fn at_seg(s: &Seg, f: f64) -> Point3 {
    let straight = |f: f64| {
        Point3::new(
            s.from.x + (s.to.x - s.from.x) * f,
            s.from.y + (s.to.y - s.from.y) * f,
            s.from.z + (s.to.z - s.from.z) * f,
        )
    };
    let Some(arc) = s.arc else {
        return straight(f);
    };
    let o = arc.center;
    let ra = ((s.from.x - o.x).powi(2) + (s.from.y - o.y).powi(2)).sqrt();
    let rb = ((s.to.x - o.x).powi(2) + (s.to.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    let a0 = (s.from.y - o.y).atan2(s.from.x - o.x);
    let a1 = (s.to.y - o.y).atan2(s.to.x - o.x);
    let mut d = if arc.ccw { a1 - a0 } else { a0 - a1 };
    d = d.rem_euclid(2.0 * PI);
    if !r.is_finite() || !d.is_finite() || r <= f64::EPSILON || d <= 0.0 {
        // The straight-chord fallback of arc_chords, matched: a
        // degenerate arc still moves somewhere honest.
        return straight(f);
    }
    let th = a0 + if arc.ccw { d * f } else { -d * f };
    // Arcs are planar by construction (push_arc's law); Z lerps.
    Point3::new(
        o.x + r * th.cos(),
        o.y + r * th.sin(),
        s.from.z + (s.to.z - s.from.z) * f,
    )
}

/// The XY sweep of an arc about `o` in direction `ccw`, as
/// `(radius, sweep)` - `arc_chords`' angle law in one place. A
/// degenerate answer (radius 0, non-finite) is `(0.0, 0.0)`.
fn arc_sweep(a: Point3, b: Point3, o: crate::geo2d::Point2, ccw: bool) -> (f64, f64) {
    let ra = ((a.x - o.x).powi(2) + (a.y - o.y).powi(2)).sqrt();
    let rb = ((b.x - o.x).powi(2) + (b.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    let a0 = (a.y - o.y).atan2(a.x - o.x);
    let a1 = (b.y - o.y).atan2(b.x - o.x);
    let mut d = if ccw { a1 - a0 } else { a0 - a1 };
    d = d.rem_euclid(2.0 * PI);
    if r.is_finite() && d.is_finite() && r > f64::EPSILON && d > 0.0 {
        (r, d)
    } else {
        (0.0, 0.0)
    }
}

/// Distance of a move: the exact arc length over Z, or the straight
/// 3-D distance - the same geometric law `chords` flattens for
/// display and verify.
fn path_len(m: &Move) -> f64 {
    let dz = m.to.z - m.from.z;
    let dxy = || {
        ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt()
    };
    match m.arc {
        None => (dxy().powi(2) + dz * dz).sqrt(),
        Some(arc) => {
            let (r, d) = arc_sweep(m.from, m.to, arc.center, arc.ccw);
            let xy = if d > 0.0 { r * d } else { dxy() };
            (xy * xy + dz * dz).sqrt()
        }
    }
}

fn delta(a: Point3, b: Point3) -> Point3 {
    Point3::new(b.x - a.x, b.y - a.y, b.z - a.z)
}

/// One move's travel time: feeds divide through the feed word, G0
/// divides through the axis rapids, and a move with nothing to say
/// (no usable feed where a feed belongs) falls back to the rapid law
/// - never zero, never a hang.
fn move_secs(m: &Move, tr: &Traverse) -> f64 {
    match m.kind {
        MoveKind::Rapid => tr.rapid_secs(delta(m.from, m.to)),
        MoveKind::Feed | MoveKind::Plunge => match m.feed {
            Some(f) if f.is_finite() && f > 0.0 => 60.0 * path_len(m) / f,
            _ => tr.rapid_secs(delta(m.from, m.to)),
        },
    }
}

/// The timeline for a run of operations: `(name, path)` pairs in
/// document order (the caller has walked the tree; hidden and
/// never-generated ones are already left out).
#[must_use]
pub fn script<'a>(
    ops: impl IntoIterator<Item = (&'a str, &'a Toolpath)>,
    tr: &Traverse,
) -> Script {
    let mut sc = Script::default();
    let mut cum: f64 = 0.0;
    let mut last: Option<Point3> = None;
    for (name, t) in ops {
        let Some(path_start) = t.start() else {
            continue; // an empty path contributes no time and no name
        };
        if !path_start.is_finite() {
            // Nowhere to stand at the start is not a slow path, it is
            // a corrupt cache; it contributes nothing, and says so.
            eprintln!("sim: {name} starts nowhere finite and is left out");
            continue;
        }
        if sc.segs.len() >= MAX_SIM_SEGS {
            sc.truncated = true;
            break;
        }
        let oi = sc.ops.len();
        sc.ops.push(name.to_owned());
        // The connector the machine makes between programs: a rapid
        // from where the last one ended, attributed to the operation
        // it arrives for (its tool is in the spindle).
        if let Some(prev) = last
            && prev != path_start
        {
            let secs = tr.rapid_secs(delta(prev, path_start));
            sc.segs.push(Seg {
                from: prev,
                to: path_start,
                arc: None,
                secs,
                dwell: 0.0,
                feed: None,
                op: oi,
            });
            cum += secs;
            sc.cum.push(cum);
        }
        // One seg per move, index-aligned with the move list: the
        // dwell pass below addresses segments by move index.
        let seg0 = sc.segs.len();
        let mut here = path_start;
        for m in &t.moves {
            if sc.segs.len() >= MAX_SIM_SEGS {
                sc.truncated = true;
                break;
            }
            let (from, to) = if m.is_finite() {
                (m.from, m.to)
            } else {
                // The cache is validated on its way here; a corrupt
                // move stands still and says so, keeping the
                // move-index alignment the dwell pass depends on.
                eprintln!("sim: a non-finite move of {name} stands in as no motion");
                (here, here)
            };
            here = to;
            let secs = if m.is_finite() {
                move_secs(m, tr)
            } else {
                0.0
            };
            sc.segs.push(Seg {
                from,
                to,
                arc: if m.is_finite() { m.arc } else { None },
                secs,
                dwell: 0.0,
                feed: if m.kind == MoveKind::Rapid {
                    None
                } else {
                    m.feed
                },
                op: oi,
            });
            cum += secs;
            sc.cum.push(cum);
        }
        // Dwells ride the move that reaches the cycle bottom (the
        // agreement law guarantees the chain got there; the fallback
        // seats them at the group's last move, honestly).
        let end = sc.segs.len(); // may sit short of seg0 + moves.len() when truncated
        for (ci, c) in t.cycles.iter().enumerate() {
            if c.dwell <= 0.0 || !c.dwell.is_finite() {
                continue;
            }
            let g0 = c.first_move.min(t.moves.len());
            let g1 = t.cycles.get(ci + 1).map_or(t.moves.len(), |n| {
                n.first_move.max(g0).min(t.moves.len())
            });
            if g0 >= g1 {
                continue;
            }
            let anchor = t
                .moves
                .get(g0..g1)
                .and_then(|s| {
                    s.iter().position(|m| {
                        m.kind != MoveKind::Rapid && (m.to.z - c.bottom_z).abs() <= 1e-6
                    })
                })
                .map_or(g1 - 1, |k| g0 + k);
            let si = seg0 + anchor;
            if si < end {
                sc.segs[si].dwell += c.dwell;
                for slot in &mut sc.cum[si..] {
                    *slot += c.dwell;
                }
                cum += c.dwell;
            }
        }
        last = Some(here); // the last finite point the timeline saw
    }
    sc.total = cum;
    sc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Cycle, CycleKind};
    use crate::geo2d::Point2;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn near(a: Point3, b: Point3, tol: f64) -> bool {
        (a.x - b.x).abs() < tol && (a.y - b.y).abs() < tol && (a.z - b.z).abs() < tol
    }

    #[test]
    fn rapid_secs_take_the_slowest_axis() {
        let tr = Traverse {
            xy: 6000.0,
            z: 4000.0,
        };
        // 60 mm XY arrives at 0.6 s; the 10 mm Z leg at 0.15 s - the
        // move is over at 0.6.
        assert!((tr.rapid_secs(delta(p(0.0, 0.0, 0.0), p(60.0, 0.0, -10.0))) - 0.6).abs() < 1e-9);
        // A pure-Z move is paced by Z alone.
        assert!((tr.rapid_secs(delta(p(0.0, 0.0, 0.0), p(0.0, 0.0, 40.0))) - 0.6).abs() < 1e-9);
        assert_eq!(tr.rapid_secs(delta(p(5.0, 5.0, 5.0), p(5.0, 5.0, 5.0))), 0.0);
    }

    #[test]
    fn feed_time_is_distance_over_feed() {
        let tr = Traverse::default();
        let m = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(30.0, 0.0, 0.0), Some(500.0));
        assert!((move_secs(&m, &tr) - 3.6).abs() < 1e-9); // 30 mm at 500 mm/min
        // A feed move that lost its feed word falls back to G0 pace.
        let lost = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(60.0, 0.0, 0.0), None);
        assert!((move_secs(&lost, &tr) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn script_runs_a_path_end_to_end() {
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(10.0, 0.0, 10.0), None));
        t.moves.push(Move::new(
            MoveKind::Plunge,
            p(10.0, 0.0, 10.0),
            p(10.0, 0.0, 0.0),
            Some(100.0),
        ));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(10.0, 0.0, 0.0),
            p(20.0, 0.0, 0.0),
            Some(600.0),
        ));
        let sc = script([("C1", &t)], &tr);
        assert_eq!(sc.ops.len(), 1);
        assert_eq!(sc.segs.len(), 3);
        assert!(near(sc.at(0.0).pos, p(0.0, 0.0, 10.0), 1e-12));
        assert_eq!(sc.at(0.0).op, Some(0));
        assert!(!sc.at(0.0).done);
        // Mid-plunge is halfway down, and says its feed.
        let plunge = sc.segs[1].secs; // 60 s·mm/100 mm = 6.0
        assert!((plunge - 6.0).abs() < 1e-9);
        let f = sc.at(sc.cum[0] + plunge / 2.0);
        assert!((f.pos.z - 5.0).abs() < 1e-9);
        assert!(matches!(f.readout, Readout::Feed(v) if (v - 100.0).abs() < 1e-9));
        // The whole thing, then past the end.
        let e = sc.at(sc.total + 1.0);
        assert!(near(e.pos, p(20.0, 0.0, 0.0), 1e-9));
        assert!(e.done);
        assert_eq!(e.readout, Readout::Idle);
    }

    #[test]
    fn windows_never_overlap_and_boundaries_are_continuous() {
        // The cum[i] contract at(): at a boundary t == cum[i], the
        // next segment's from wins - and chaining makes that equal
        // the previous segment's to.
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 5.0), p(30.0, 0.0, 5.0), None));
        t.moves.push(Move::new(
            MoveKind::Feed,
            p(30.0, 0.0, 5.0),
            p(30.0, 40.0, 5.0),
            Some(1000.0),
        ));
        let sc = script([("A", &t)], &tr);
        let b = sc.at(sc.cum[0] - 1e-12);
        let c = sc.at(sc.cum[0]);
        assert!(near(b.pos, c.pos, 1e-6), "the boundary stands in one place");
        assert!(matches!(c.readout, Readout::Feed(_)), "after the boundary it cuts");
    }

    #[test]
    fn connector_rapids_join_operations() {
        let tr = Traverse::default();
        let mut a = Toolpath::default();
        a.moves.push(Move::new(
            MoveKind::Feed,
            p(0.0, 0.0, 0.0),
            p(10.0, 0.0, 0.0),
            Some(1000.0),
        ));
        let mut b = Toolpath::default();
        b.moves.push(Move::new(
            MoveKind::Feed,
            p(50.0, 0.0, 0.0),
            p(60.0, 0.0, 0.0),
            Some(1000.0),
        ));
        let sc = script([("A", &a), ("B", &b)], &tr);
        assert_eq!(sc.segs.len(), 3); // move, connector, move
        assert_eq!(sc.segs[1].op, 1); // connector arrives for B
        assert_eq!(sc.segs[1].feed, None);
        assert!((sc.segs[1].secs - 0.4).abs() < 1e-9); // 40 mm XY at 6000
        assert_eq!(sc.at(0.0).op, Some(0));
        assert_eq!(sc.at(sc.total - 1e-9).op, Some(1));
    }

    #[test]
    fn dwell_sits_after_the_move_that_reaches_bottom() {
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(0.0, 0.0, 10.0), None));
        t.moves.push(Move::new(
            MoveKind::Plunge,
            p(0.0, 0.0, 10.0),
            p(0.0, 0.0, -5.0),
            Some(100.0),
        ));
        t.moves.push(Move::new(
            MoveKind::Rapid,
            p(0.0, 0.0, -5.0),
            p(0.0, 0.0, 10.0),
            None,
        ));
        t.cycles.push(Cycle {
            kind: CycleKind::Drill,
            xy: Point2::new(0.0, 0.0),
            r_z: 10.0,
            bottom_z: -5.0,
            entry_z: 0.0,
            peck: 0.0,
            dwell: 1.0,
            first_move: 0,
        });
        let sc = script([("D1", &t)], &tr);
        assert!((sc.segs[1].dwell - 1.0).abs() < 1e-12);
        // The dwell window belongs to the plunge's end: at its middle
        // the tool stands at the bottom and the readout says DWELL.
        let mid = sc.cum[1] - sc.segs[1].dwell / 2.0;
        let f = sc.at(mid);
        assert!(near(f.pos, p(0.0, 0.0, -5.0), 1e-12));
        assert!(matches!(f.readout, Readout::Dwell(v) if (v - 1.0).abs() < 1e-12));
        // And it shifted the retract's whole window later.
        assert!(sc.cum[2] > sc.cum[1]);
    }

    #[test]
    fn arc_positions_are_exact_on_the_circle() {
        let tr = Traverse::default();
        let mut t = Toolpath::default();
        // Quarter ccw arc, r = 10 about the origin, from +X to +Y.
        let m = Move::new(
            MoveKind::Feed,
            p(10.0, 0.0, 0.0),
            p(0.0, 10.0, 0.0),
            Some(100.0 * 10.0 * PI / 2.0), // arc length × 100: secs = 0.6
        )
        .with_arc(Point2::new(0.0, 0.0), true);
        assert!((move_secs(&m, &tr) - 0.6).abs() < 1e-9);
        t.moves.push(m);
        let sc = script([("A", &t)], &tr);
        let a = sc.at(0.0);
        let m2 = sc.at(0.3);
        let b = sc.at(0.6);
        assert!(near(a.pos, p(10.0, 0.0, 0.0), 1e-9));
        assert!(
            near(m2.pos, p(10.0 / 2f64.sqrt(), 10.0 / 2f64.sqrt(), 0.0), 1e-9),
            "the halfway point is 45 degrees, not a chord midpoint"
        );
        assert!(near(b.pos, p(0.0, 10.0, 0.0), 1e-9));
        // The circle: every sample keeps its radius.
        for k in 0..=10 {
            let r = sc.at(0.6 * k as f64 / 10.0);
            let rr = (r.pos.x * r.pos.x + r.pos.y * r.pos.y).sqrt();
            assert!((rr - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn empty_and_pathological_input_stay_calm() {
        let tr = Traverse::default();
        let s = script([], &tr);
        assert_eq!(s.total, 0.0);
        assert_eq!(s.at(1.0).readout, Readout::Idle);
        assert!(s.at(1.0).done);
        let empty = Toolpath::default();
        assert!(script([("E", &empty)], &tr).segs.is_empty());
        // Non-finite moves stand in as no motion: the position never
        // goes NaN, no time passes, nothing panics.
        let mut bad = Toolpath::default();
        bad.moves.push(Move::new(
            MoveKind::Rapid,
            p(1.0, 2.0, 3.0),
            p(f64::NAN, 0.0, 0.0),
            None,
        ));
        bad.moves.push(Move::new(
            MoveKind::Feed,
            p(f64::NAN, 0.0, 0.0),
            p(1.0, 2.0, 3.0),
            Some(100.0),
        ));
        let sc = script([("X", &bad)], &tr);
        assert_eq!(sc.segs.len(), 2); // alignment kept: one seg per move
        assert_eq!(sc.segs[0].secs, 0.0);
        assert_eq!(sc.total, 0.0);
        let f = sc.at(1e9);
        assert!(near(f.pos, p(1.0, 2.0, 3.0), 1e-12));
        assert!(f.done);
        // A path starting nowhere finite is left out entirely, loudly.
        let mut rot = Toolpath::default();
        rot.moves.push(Move::new(
            MoveKind::Feed,
            p(f64::NAN, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            Some(100.0),
        ));
        let sc = script([("R", &rot)], &tr);
        assert!(sc.is_empty());
        assert_eq!(sc.at(0.0).readout, Readout::Idle);
    }

    #[test]
    fn traverse_reads_the_machine_file() {
        let m: Value =
            serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch")).unwrap();
        let tr = Traverse::from_machine(&m);
        assert_eq!(
            tr,
            Traverse {
                xy: 6000.0,
                z: 4000.0
            }
        );
        assert_eq!(Traverse::from_machine(&Value::Null), Traverse::default());
    }
}
