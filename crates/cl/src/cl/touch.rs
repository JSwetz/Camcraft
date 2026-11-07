//! Probing touches (#108): the stretches of a path a control runs as
//! "move until the stylus trips", and the point the trip is expected
//! at. Provenance: NIST RS274NGC (Kramer et al., NISTIR 6556) G38.2 -
//! straight probe toward a target, stop on contact, error if none - and
//! general touch-probe practice (fast seek, back off, slow measure).
//! Our own model; no vendor material consulted.
//!
//! The moves stay the truth the sim and the verifier read: a touch's
//! move runs to its TARGET (the overtravel end), which is the farthest
//! the stylus may go. The post writes those moves as probing blocks,
//! never as plain feeds - a probe move posted as G1 drives the stylus
//! through the part, the silent wrong answer.

use crate::geometry::Point3;

use super::moves::{Move, MoveKind};

/// One probing move of a path: `moves[index]` is the straight feed
/// from the approach point to the EXPECTED trip (the stylus ball
/// centre on contact with a part exactly where the model says), and
/// `target` is the point the control is told to probe toward - the
/// expected trip plus the overtravel, on the same line. The control
/// stops wherever the stylus trips; the moves state the nominal.
/// `measure` is `Some(k)` for the slow re-probe whose trip point is
/// the k-th measurement of the operation, `None` for the fast seek
/// that only finds the surface before the back-off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeTouch {
    pub index: usize,
    pub measure: Option<u32>,
    pub target: Point3,
}

impl ProbeTouch {
    /// Does the touch name a straight feed of `moves` whose line runs
    /// on to `target` past its end? The door the dump and the loader
    /// check before trusting a touch.
    #[must_use]
    pub fn is_sane(&self, moves: &[Move]) -> bool {
        let Some(m) = moves.get(self.index) else {
            return false;
        };
        if m.arc.is_some() || m.kind == MoveKind::Rapid || m.feed.is_none() || !self.target.is_finite() {
            return false;
        }
        let d = m.to - m.from;
        let len2 = d.dot(d);
        if !(len2.is_finite() && len2 > 1e-12) {
            return false;
        }
        // The target lies on the move's line, beyond its end.
        let t = (self.target - m.from).dot(d) / len2;
        let foot = Point3::new(m.from.x + d.x * t, m.from.y + d.y * t, m.from.z + d.z * t);
        let off = foot - self.target;
        t > 1.0 && off.dot(off).sqrt() < 1e-3
    }

    /// The unit travel direction of the touch, when it is sane.
    #[must_use]
    pub fn direction(&self, moves: &[Move]) -> Option<Point3> {
        let m = moves.get(self.index)?;
        let d = m.to - m.from;
        let n = d.dot(d).sqrt();
        (n.is_finite() && n > 1e-9).then(|| Point3::new(d.x / n, d.y / n, d.z / n))
    }
}
