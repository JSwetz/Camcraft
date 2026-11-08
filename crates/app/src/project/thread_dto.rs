//! The cached threading-cycle record (issue #34, `crate::cl::ThreadCycle`)
//! in the project file. Our own format; the additive law of the comps
//! and notes keys: absent is none - every non-threading path, and every
//! cache before the record existed.

use serde::{Deserialize, Serialize};

use crate::cl::{lattice, Move, ThreadCycle};
use crate::geometry::Point3;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThreadCycleDto {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub major_diameter: f64,
    pub minor_diameter: f64,
    pub lead: f64,
    pub height: f64,
    pub first_depth: f64,
    pub min_depth: f64,
    pub finish_allowance: f64,
    pub infeed_angle: f64,
    pub spring_passes: u32,
    pub pull_out: f64,
    pub first: u64,
    pub last: u64,
}

impl ThreadCycleDto {
    pub(super) fn from_cycle(t: &ThreadCycle) -> Self {
        // The points take the lattice snap `MoveDto` gives the moves
        // they sit on, so the record still opens on its move on load.
        let lat = |p: Point3| [lattice(p.x), lattice(p.y), lattice(p.z)];
        ThreadCycleDto {
            start: lat(t.start),
            end: lat(t.end),
            major_diameter: t.major_diameter,
            minor_diameter: t.minor_diameter,
            lead: t.lead,
            height: t.height,
            first_depth: t.first_depth,
            min_depth: t.min_depth,
            finish_allowance: t.finish_allowance,
            infeed_angle: t.infeed_angle,
            spring_passes: t.spring_passes,
            pull_out: t.pull_out,
            first: t.first as u64,
            last: t.last as u64,
        }
    }

    /// The record over the loaded moves - `None` when it does not sit
    /// on them (the file's bytes are the law, the sanity door is the
    /// reader's: the passes then post as themselves).
    pub(super) fn to_cycle(&self, moves: &[Move]) -> Option<ThreadCycle> {
        let t = ThreadCycle {
            start: Point3::new(self.start[0], self.start[1], self.start[2]),
            end: Point3::new(self.end[0], self.end[1], self.end[2]),
            major_diameter: self.major_diameter,
            minor_diameter: self.minor_diameter,
            lead: self.lead,
            height: self.height,
            first_depth: self.first_depth,
            min_depth: self.min_depth,
            finish_allowance: self.finish_allowance,
            infeed_angle: self.infeed_angle,
            spring_passes: self.spring_passes,
            pull_out: self.pull_out,
            first: usize::try_from(self.first).unwrap_or(usize::MAX),
            last: usize::try_from(self.last).unwrap_or(usize::MAX),
        };
        t.is_sane(moves).then_some(t)
    }
}
