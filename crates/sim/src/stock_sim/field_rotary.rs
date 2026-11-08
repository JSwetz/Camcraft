//! The rotary chord pre-pass (M13.1, the rotary half of the field
//! verdict): a rotary program's moves carry the pose the head holds
//! at each station, and between two stations the PART turns about
//! its fitted axis while the tip runs its chord - in the part frame
//! the posed tool sweeps a helical band, a torus-family solid the
//! field's chord sweeps do not have in closed form. This pass chords
//! the turn: each station-to-station move is cut into sub-moves whose
//! tip is lerped on the move's straight line and whose pose is the
//! controller's letter lerp at the sub-chord's midpoint (the same
//! lerp `verify::rotary` and `verify::posed_steel` read: each letter
//! on its own line, [`crate::cl::pose`]), and every sub-chord is cut
//! by the existing posed sweep (`field_cut`, one axis per chord).
//!
//! Provenance: the sagitta chord law of this codebase's own
//! `arc_chords`, applied to a rigid turn; the modal pose law is
//! RS-274's. Our own design; nothing from vendor binaries.
//!
//! ## The law here, stated plainly
//!
//! - **This is the chord law at `pose_tol`, not a conservative closed
//!   form.** The sub-chord count is `n = ceil(dθ / (2·acos(1 −
//!   pose_tol / R_far)))`, `R_far` the farthest point of the tool from
//!   the rotary axis (the tip's radius at either station, plus the
//!   ball radius, plus the shank the column twin carries): a point
//!   turning at `R_far` deviates from its chord by at most the sag
//!   `pose_tol`. The error is NOT one-signed: the chorded solid can
//!   stand a sag inside or outside the true band, so the verdict it
//!   reads is within the sag of the truth, never a bound on it.
//! - **The ball's center is exact at every sub-chord end.** For a
//!   ball tool the sub-move's tip is moved by `r·(a(s) − a_mid)` so
//!   the sphere's center (`tip + r·axis`) sits on the true center
//!   curve at both ends; the chord between them sags by at most
//!   `r·(1 − cos α)` (`RotaryChordStats::center_sag`), which is
//!   `pose_tol · r / R_far` - the number the parity suite reads
//!   against. The flute above the ball sits on the mid pose, off the
//!   true pose by at most `α` - a displacement growing linearly up
//!   the axis (`L·α` at height `L`), which is why `R_far` counts the
//!   shank: at the shank's far end the displacement is the stated sag.
//! - **The flute-rotation bound (#20).** Any tool point at distance
//!   `w` from the tool's reference (the ball center, the flat tip)
//!   stands off its true-pose place by at most `w·2·sin(β/2)`, `β ≤
//!   half_angle` the angle between the true pose and the sub-chord's
//!   mid pose (each letter a rotation, so `β` is at most the letters'
//!   summed half-step). Only points inside the field can cut, so `w`
//!   is capped by `L`, the farthest field-bounds corner from the
//!   reference at either station (never more than the shank).
//!   `RotaryChordStats::flute_shift` states `L·2·sin(half/2)`; with
//!   the reference error (`center_sag`, zero for a flat tool) it is
//!   a two-sided Hausdorff bound: every point of the true sweep has a
//!   chorded point within `center_sag + flute_shift`, and back. A
//!   caller that needs the flank at a class finer than the chord law
//!   gives a flute tolerance ([`ChordLaw::flute_tol`],
//!   [`Stock::simulate_rotary_bounded`]) and the count also meets
//!   `L·2·sin(half/2) ≤ flute_tol` - more sub-chords, refused by name
//!   past [`MAX_SUBCHORDS`], never coarsened. The rigid-motion law is
//!   the textbook one (Craig, "Introduction to Robotics", ch. 2: a
//!   point at `w` from the pivot turned by `β` moves the chord
//!   `2·w·sin(β/2)`).
//! - **Nothing silent.** A turn that would need more than
//!   [`MAX_SUBCHORDS`] chords, an arc move that turns, a degenerate
//!   axis, a non-finite move or tolerance: refused by name before a
//!   cell is written. The column arm refuses the whole program
//!   ([`ROTARY_COLUMN_REFUSAL`]): its atoms stand upright.

use super::backend::{named_op, spanning_flank, Stock};
use super::cut::{check_ops, SimOp};
use super::kernel::StockError;

use crate::cl::{angles_to_axis, axis_to_angles, Move, MoveKind};
use crate::geometry::{Aabb, Point3};
use crate::verify::RotaryAxis;

/// The column engine's refusal of a rotary program, said once.
pub const ROTARY_COLUMN_REFUSAL: &str = "a rotary program turns the part under a posed tool - the column engine's atoms stand upright and have no turning sweep; the field arm chords it (Stock::simulate_rotary)";

/// Sub-chords one move may need before the pass refuses by name.
pub const MAX_SUBCHORDS: usize = 1 << 16;

/// What the pre-pass cost and promised.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RotaryChordStats {
    /// Sub-chords emitted for the turning moves (pass-through moves
    /// not counted).
    pub sub_chords: usize,
    /// Moves that turned and were chorded.
    pub turning_moves: usize,
    /// The widest half-turn one sub-chord spans, radians.
    pub half_angle: f64,
    /// The ball center's worst chord sag, mm: `r·(1 − cos half_angle)`,
    /// zero for a flat tool (whose tip is exact and whose disc edge
    /// is off by up to `r·half_angle` instead).
    pub center_sag: f64,
    /// The flute-rotation bound, mm: `L·2·sin(half_angle/2)`, the
    /// farthest any tool point inside the field stands from its
    /// true-pose place (module docs). With `center_sag`, the two-sided
    /// distance between the chorded sweep and the true one.
    pub flute_shift: f64,
}

/// The chord law's inputs: tool radius, the flute length the sweep
/// carries, the sag tolerance at the farthest reach, the flute-
/// rotation tolerance (`None`: stated in the stats, not enforced) and
/// the box only inside which a tool point can cut (`None`: the shank
/// alone caps the flute's reach).
#[derive(Clone, Copy, Debug)]
pub struct ChordLaw {
    pub r: f64,
    pub shank: f64,
    pub pose_tol: f64,
    pub ball: bool,
    pub flute_tol: Option<f64>,
    pub reach: Option<Aabb>,
}

const UP: [f64; 3] = [0.0, 0.0, 1.0];

impl Stock {
    /// Run a rotary program: every op's moves chorded by the turn law
    /// above, then cut by [`Stock::simulate`]'s posed sweeps. `ax` is
    /// the part's fitted rotary axis in the moves' own frame,
    /// `pose_tol` the chord sag allowed at the tool's farthest point.
    pub fn simulate_rotary(
        &mut self,
        ops: &[SimOp],
        ax: &RotaryAxis,
        pose_tol: f64,
    ) -> Result<RotaryChordStats, StockError> {
        self.simulate_rotary_bounded(ops, ax, pose_tol, None)
    }

    /// [`Stock::simulate_rotary`] with the flute-rotation bound
    /// enforced when `flute_tol` is given: every sub-chord short
    /// enough that no tool point inside the field stands farther than
    /// `flute_tol` from its true-pose place.
    pub fn simulate_rotary_bounded(
        &mut self,
        ops: &[SimOp],
        ax: &RotaryAxis,
        pose_tol: f64,
        flute_tol: Option<f64>,
    ) -> Result<RotaryChordStats, StockError> {
        let Some(f) = self.field() else {
            return Err(StockError::Invalid(ROTARY_COLUMN_REFUSAL.into()));
        };
        check_ops(ops)?;
        let shank = spanning_flank(f);
        let reach = Some(f.bounds());
        let mut stats = RotaryChordStats::default();
        let mut chorded: Vec<Vec<Move>> = Vec::with_capacity(ops.len());
        for (i, op) in ops.iter().enumerate() {
            let law = ChordLaw { r: op.radius, shank, pose_tol, ball: op.ball, flute_tol, reach };
            let (moves, s) = rotary_chords_law(op.moves, ax, &law).map_err(|e| named_op(i, e))?;
            stats.sub_chords += s.sub_chords;
            stats.turning_moves += s.turning_moves;
            stats.half_angle = stats.half_angle.max(s.half_angle);
            stats.center_sag = stats.center_sag.max(s.center_sag);
            stats.flute_shift = stats.flute_shift.max(s.flute_shift);
            chorded.push(moves);
        }
        let sim_ops: Vec<SimOp> = ops.iter().zip(&chorded).map(|(op, m)| SimOp { moves: m, ..*op }).collect();
        self.simulate(&sim_ops)?;
        Ok(stats)
    }
}

/// The pre-pass itself: `moves` with every turning cut replaced by
/// its sub-chords (rapids, non-turning moves pass through, stamped
/// with the pose they hold - the field reads an unstamped move as
/// upright, the controller as modal).
pub fn rotary_chords(
    moves: &[Move],
    ax: &RotaryAxis,
    r: f64,
    shank: f64,
    pose_tol: f64,
    ball: bool,
) -> Result<(Vec<Move>, RotaryChordStats), StockError> {
    rotary_chords_law(moves, ax, &ChordLaw { r, shank, pose_tol, ball, flute_tol: None, reach: None })
}

/// [`rotary_chords`] under the full [`ChordLaw`]: the flute-rotation
/// bound stated, and enforced when `law.flute_tol` is given.
pub fn rotary_chords_law(moves: &[Move], ax: &RotaryAxis, law: &ChordLaw) -> Result<(Vec<Move>, RotaryChordStats), StockError> {
    let ChordLaw { r, shank, pose_tol, ball, flute_tol, reach } = *law;
    if let Some(t) = flute_tol.filter(|t| !(t.is_finite() && *t > 0.0)) {
        return Err(StockError::Invalid(format!("rotary chords: flute tolerance {t} must be finite and positive")));
    }
    let dir = unit(ax.dir).ok_or_else(|| StockError::Invalid("rotary chords: the rotary axis direction is not a finite nonzero vector".into()))?;
    if !ax.origin.is_finite() {
        return Err(StockError::Invalid("rotary chords: the rotary axis origin is not finite".into()));
    }
    if !(pose_tol.is_finite() && pose_tol > 0.0) || !(r.is_finite() && r > 0.0) || !(shank.is_finite() && shank >= 0.0) {
        return Err(StockError::Invalid(format!(
            "rotary chords: pose tolerance {pose_tol}, radius {r} and shank {shank} must be finite and positive"
        )));
    }
    let mut out: Vec<Move> = Vec::with_capacity(moves.len());
    let mut stats = RotaryChordStats::default();
    let mut held: Option<[f64; 3]> = moves.first().and_then(|m| m.axis);
    for (k, m) in moves.iter().enumerate() {
        if !m.is_finite() {
            return Err(StockError::Invalid(format!("rotary chords: move {k} has a non-finite endpoint")));
        }
        let from_pose = held;
        let to_pose = m.axis.or(held);
        held = to_pose;
        let (fp, tp) = (
            unit(from_pose.unwrap_or(UP)).ok_or_else(|| bad_axis(k))?,
            unit(to_pose.unwrap_or(UP)).ok_or_else(|| bad_axis(k))?,
        );
        if m.kind == MoveKind::Rapid || fp == tp {
            out.push(Move { axis: to_pose, ..*m });
            continue;
        }
        if m.arc.is_some() {
            return Err(StockError::Invalid(format!(
                "rotary chords: move {k} turns the part along an arc - the chord law reads a straight tip; segment it"
            )));
        }
        let (la, lb) = (axis_to_angles(fp), axis_to_angles(tp));
        let dth = ((la[0] - lb[0]).abs() + (la[1] - lb[1]).abs()).to_radians();
        let r_far = off_axis(ax, dir, m.from).max(off_axis(ax, dir, m.to)) + r + shank;
        let alpha = (1.0 - pose_tol / r_far).clamp(-1.0, 1.0).acos();
        if !(alpha.is_finite() && alpha > 0.0 && dth.is_finite()) {
            return Err(StockError::Invalid(format!(
                "rotary chords: move {k} turns {dth} rad at reach {r_far}: no finite chord half-angle at tolerance {pose_tol}"
            )));
        }
        let flute = flute_reach(m, r, shank, ball, reach);
        // The flute law: L·2·sin(β/2) ≤ flute_tol at β the half-step.
        let beta = match flute_tol {
            Some(t) if flute > 0.0 => 2.0 * (t / (2.0 * flute)).min(1.0).asin(),
            _ => f64::INFINITY,
        };
        let n_f = (dth / (2.0 * alpha.min(beta))).ceil();
        if !(n_f.is_finite() && n_f <= MAX_SUBCHORDS as f64) {
            return Err(StockError::Invalid(format!(
                "rotary chords: move {k} turns {dth:.4} rad at reach {r_far:.2} and would need {n_f} sub-chords at tolerance {pose_tol}; at most {MAX_SUBCHORDS} - refused, never coarsened"
            )));
        }
        let n = (n_f as usize).max(1);
        let half = dth / (2.0 * n as f64);
        stats.turning_moves += 1;
        stats.sub_chords += n;
        stats.half_angle = stats.half_angle.max(half);
        if ball {
            stats.center_sag = stats.center_sag.max(r * (1.0 - half.cos()));
        }
        stats.flute_shift = stats.flute_shift.max(flute * 2.0 * (0.5 * half).sin());
        for j in 0..n {
            let (s0, s1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
            let a_mid = pose(la, lb, 0.5 * (s0 + s1));
            let (mut p0, mut p1) = (lerp(m.from, m.to, s0), lerp(m.from, m.to, s1));
            if ball {
                // The sphere's center exact at both ends: tip + r·a(s)
                // == tip' + r·a_mid.
                let (a0, a1) = (pose(la, lb, s0), pose(la, lb, s1));
                p0 = shift(p0, a0, a_mid, r);
                p1 = shift(p1, a1, a_mid, r);
            }
            out.push(Move { kind: m.kind, from: p0, to: p1, feed: m.feed, arc: None, axis: Some(a_mid) });
        }
    }
    Ok((out, stats))
}

/// `L`: how far from the tool's reference (ball center, flat tip) a
/// tool point can be and still cut - the shank plus the radius,
/// capped by the farthest corner of the reach box from the reference
/// over the move (every point of a box lies within its farthest
/// corner's distance of any point; the tip runs the chord between the
/// stations, convex, so its ends bound it; a ball's center stands
/// within `r` of the tip).
fn flute_reach(m: &Move, r: f64, shank: f64, ball: bool, reach: Option<Aabb>) -> f64 {
    let full = shank + r;
    let Some(b) = reach else { return full };
    let far = |p: Point3| {
        let d = |v: f64, lo: f64, hi: f64| (v - lo).abs().max((hi - v).abs());
        d(p.x, b.min.x, b.max.x).hypot(d(p.y, b.min.y, b.max.y)).hypot(d(p.z, b.min.z, b.max.z))
    };
    let corner = far(m.from).max(far(m.to)) + if ball { r } else { 0.0 };
    if corner.is_finite() {
        corner.min(full)
    } else {
        full
    }
}

fn bad_axis(k: usize) -> StockError {
    StockError::Invalid(format!("rotary chords: move {k} states a pose that is not a finite nonzero direction"))
}

/// The axis the letters hold `s` of the way from `a` to `b` (each
/// letter on its own line); upright when no vector holds them.
fn pose(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    let l = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s, a[2] + (b[2] - a[2]) * s];
    angles_to_axis(l).unwrap_or(UP)
}

fn shift(p: Point3, a: [f64; 3], a_mid: [f64; 3], r: f64) -> Point3 {
    Point3::new(p.x + r * (a[0] - a_mid[0]), p.y + r * (a[1] - a_mid[1]), p.z + r * (a[2] - a_mid[2]))
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Distance of `p` from the rotary axis line (unit `d`).
fn off_axis(ax: &RotaryAxis, d: [f64; 3], p: Point3) -> f64 {
    let w = [p.x - ax.origin.x, p.y - ax.origin.y, p.z - ax.origin.z];
    let c = [w[1] * d[2] - w[2] * d[1], w[2] * d[0] - w[0] * d[2], w[0] * d[1] - w[1] * d[0]];
    (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}
