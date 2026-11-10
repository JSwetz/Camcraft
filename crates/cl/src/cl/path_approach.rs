//! The approach law of a clearance-seeded path (#690, #697), split from
//! `path.rs` (400-line law): a path seeded on the clearance plane always
//! opens with a rapid, so a post's first motion after the tool call is a
//! G00 there - never a feed out of the tool-change position.
//!
//! Provenance: #690's posted-program law; our own builder design.

use super::super::moves::{Move, MoveKind};
use super::PathBuilder;
use crate::geometry::Point3;

impl PathBuilder {
    /// A builder seeded at `at` on the clearance plane whose path always
    /// opens with a rapid (#690, #697). Posts write only a move's end, so
    /// the opening rapid is the program's approach - the G00 after the
    /// tool call. A generator's own rapid to its start is dropped when the
    /// start IS the seed (a zero-length move), and the first motion would
    /// then be a feed out of the tool-change position; so if the first
    /// move is not a rapid, a zero-length rapid at the seed is stated
    /// ahead of it. It lands before any move is recorded, so no cycle,
    /// comp span or note index ever shifts.
    pub fn approached(at: Point3) -> Self {
        PathBuilder { approach: true, ..Self::new(at) }
    }

    /// The approach law of [`approached`](Self::approached), run before
    /// the first appended move of `kind`.
    pub(super) fn open(&mut self, kind: MoveKind) {
        if std::mem::take(&mut self.approach) && self.moves.is_empty() && kind != MoveKind::Rapid {
            self.moves.push(Move::new(MoveKind::Rapid, self.at, self.at, None).attach_axis(self.axis));
            self.at_pose = self.axis;
        }
    }
}
