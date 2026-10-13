//! Cutter-location (CL) data: a linked sequence of machine moves.
//!
//! Provenance: our own minimal CL model. Move kinds map to the G0/G1
//! vocabulary of ISO 6983 / RS-274 (public standards); the rapid/feed/plunge
//! split is general machining practice. Arcs (tangent lead-ins) are
//! tessellated to Feed moves at tolerance rather than stored here - exact
//! ARC posting belongs to the post-processor.
//!
//! Invariant: every move's `from` equals the previous move's `to` (the path
//! is a chain, no gaps); generators build through [`PathBuilder`], which
//! maintains it by construction.

use crate::geometry::Point3;

/// G0 rapid, G1 feed, G1 with a plunge (reduced) feed - the three verbs a
/// 2.5-D path needs. The distinction drives display color now and post
/// vocabulary later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKind {
    Rapid,
    Feed,
    Plunge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub kind: MoveKind,
    pub from: Point3,
    pub to: Point3,
    /// mm/min while in contact with material; None for rapids.
    pub feed: Option<f64>,
}

impl Move {
    pub fn new(kind: MoveKind, from: Point3, to: Point3, feed: Option<f64>) -> Self {
        Move { kind, from, to, feed }
    }

    pub fn is_finite(self) -> bool {
        self.from.is_finite() && self.to.is_finite()
            && self.feed.is_none_or(|f| f.is_finite() && f > 0.0)
    }
}

/// A generated path: linked moves, nothing else. Caching, staleness and
/// serialization wrap this at the schema/project layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub moves: Vec<Move>,
}

impl Toolpath {
    pub fn new(moves: Vec<Move>) -> Self {
        Toolpath { moves }
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
    }

    /// True when every move starts where its predecessor ended.
    pub fn is_chained(&self) -> bool {
        self.moves.windows(2).all(|w| w[0].to == w[1].from)
    }
}

/// Chains moves onto a Toolpath, maintaining the link invariant by
/// construction: a move's `from` is always the cursor.
pub struct PathBuilder {
    at: Point3,
    moves: Vec<Move>,
}

impl PathBuilder {
    pub fn new(at: Point3) -> Self {
        PathBuilder { at, moves: Vec::new() }
    }

    pub fn at(&self) -> Point3 {
        self.at
    }

    /// Append a move to `to`; degenerate zero-length moves are dropped (a
    /// "move" to where you already are posts nothing real and only pollutes
    /// golden comparisons).
    pub fn push(&mut self, kind: MoveKind, to: Point3, feed: Option<f64>) {
        if to.x == self.at.x && to.y == self.at.y && to.z == self.at.z {
            return;
        }
        self.moves.push(Move::new(kind, self.at, to, feed));
        self.at = to;
    }

    pub fn finish(self) -> Toolpath {
        Toolpath { moves: self.moves }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn builder_chains_and_drops_no_motion() {
        let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
        b.push(MoveKind::Plunge, p(0.0, 0.0, 1.0), Some(100.0));
        b.push(MoveKind::Feed, p(0.0, 0.0, 1.0), Some(500.0)); // no motion
        b.push(MoveKind::Feed, p(10.0, 0.0, 1.0), Some(500.0));
        let t = b.finish();
        assert_eq!(t.len(), 2);
        assert!(t.is_chained());
        assert_eq!(t.start(), Some(p(0.0, 0.0, 5.0)));
        assert_eq!(t.end(), Some(p(10.0, 0.0, 1.0)));
        assert!(t.all_finite());
    }
}
