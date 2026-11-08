//! The threading intent record (issue #34): what a thread's cycle is,
//! standing beside the explicit helical passes that cut it.
//!
//! Provenance: the quantities a lathe control's multi-pass threading
//! cycle asks for - start and end of the thread, major / minor
//! diameter, lead, thread height, first-pass and minimum depth, finish
//! allowance, infeed angle, spring passes, pull-out - as public lathe
//! programming references and machine-tool handbooks list them
//! (behaviour only). Our own model; no vendor material consulted. The
//! explicit passes remain the truth for verification, display and
//! simulation (the drilling `Cycle` law); the record is what a post
//! with a threading cycle writes in their place.

use crate::geometry::Point3;

use super::moves::Move;

/// One thread's machining intent: the passes `moves[first..=last]`
/// are what the machine does in G32 form; a cycle-capable post writes
/// one threading cycle from these numbers instead. Points are world
/// coordinates on the stream's lattice; lengths are mm, radial
/// quantities are RADIUS values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadCycle {
    /// The cycle's start point: clearance radius, thread start station
    /// (the tool returns here after every pass).
    pub start: Point3,
    /// The thread's end at the root: minor radius, last station.
    pub end: Point3,
    /// Crest diameter (2 x the blank's crest radius in the window).
    pub major_diameter: f64,
    /// Root diameter (2 x the deepest pass's tip radius).
    pub minor_diameter: f64,
    /// Axial travel per spindle revolution (single start: the pitch).
    pub lead: f64,
    /// Thread height, radial (crest to root).
    pub height: f64,
    /// Depth of the first pass, radial.
    pub first_depth: f64,
    /// The smallest depth increment a pass may take, radial.
    pub min_depth: f64,
    /// Radial stock the finishing pass removes (0: the last roughing
    /// rung is the root).
    pub finish_allowance: f64,
    /// Included angle of the compound infeed, degrees; 0 is straight
    /// radial infeed (every pass plunges square to the axis).
    pub infeed_angle: f64,
    /// Extra passes at full depth after the finishing pass.
    pub spring_passes: u32,
    /// Axial length of the angled pull-out at the thread's end (0: a
    /// square, radial retract).
    pub pull_out: f64,
    /// The passes' first move (the rapid onto the start point).
    pub first: usize,
    /// The passes' last move (the retract after the deepest pass),
    /// inclusive.
    pub last: usize,
}

impl ThreadCycle {
    /// Does the record sit on a path of these moves: an in-range span
    /// opening on its start point and finite numbers that state a
    /// thread? The door the dump and the loader check before trusting
    /// a record; out-of-range CYCLE words are the post's to refuse.
    #[must_use]
    pub fn is_sane(&self, moves: &[Move]) -> bool {
        let (Some(m0), Some(_)) = (moves.get(self.first), moves.get(self.last)) else {
            return false;
        };
        let nums = [
            self.major_diameter,
            self.minor_diameter,
            self.lead,
            self.height,
            self.first_depth,
            self.min_depth,
            self.finish_allowance,
            self.infeed_angle,
            self.pull_out,
        ];
        self.first <= self.last
            && m0.to == self.start
            && self.start.is_finite()
            && self.end.is_finite()
            && nums.iter().all(|v| v.is_finite())
            && self.lead > 0.0
            && self.height > 0.0
    }
}
