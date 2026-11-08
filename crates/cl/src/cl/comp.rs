//! Cutter radius compensation in the control (M13.11): the span of a
//! path a control offsets itself, and the geometry it is programmed
//! with. Provenance: RS-274 / ISO 6983 G41 (left) / G42 (right) / G40
//! (cancel) and the published startup / cancel block behaviour of
//! Fanuc-style controls - the offset vector is established over the
//! straight block that carries G41/G42 and withdrawn over the one that
//! carries G40, so both must be linear and at least the offset long.
//! Our own model; no vendor material consulted.

use super::moves::Move;

/// Which side of the programmed path the cutter stands on, looking
/// along the travel: `Left` is G41, `Right` is G42.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompSide {
    Left,
    Right,
}

/// One compensated stretch of a path. `moves[first..=last]` are the
/// CUTTER-CENTRE moves - what the machine actually does, what the
/// stock simulation, the verifier and every golden read - and
/// `programmed` is the text the post writes IN THEIR PLACE: the
/// engage block (the lead-in, carrying G41/G42), the part contour the
/// control offsets by `radius`, and the cancel block (the lead-out,
/// carrying G40). The two agree by construction: a Fanuc-style control
/// ramps the offset over the engage block and withdraws it over the
/// cancel block, so the tool centre runs `moves[first]` and
/// `moves[last]` exactly, and the offset contour between them is the
/// centre walk the generator proved. The post never emits a
/// compensated span through a script that cannot voice it - the engine
/// refuses by name.
#[derive(Clone, Debug, PartialEq)]
pub struct CompSpan {
    pub side: CompSide,
    /// The nominal cutter radius, mm: what the control's register must
    /// hold in control mode; in wear mode the radius the centre path
    /// was walked at (the register then holds only the wear delta).
    pub radius: f64,
    /// Wear compensation (#38): `programmed` is the cutter-centre walk
    /// itself, so the register must hold the wear delta, NOT the
    /// radius - a full radius there cuts the part a radius undersize.
    pub wear: bool,
    /// Index of the engage move (the lead-in) in the path.
    pub first: usize,
    /// Index of the cancel move (the lead-out) in the path, inclusive.
    pub last: usize,
    /// The programmed geometry replacing `first..=last`, chained
    /// (`programmed[0].from == moves[first].from`, the last `to` is
    /// `moves[last].to`); its first move carries the engage, its last
    /// the cancel.
    pub programmed: Vec<Move>,
}

impl CompSpan {
    /// Does the span sit inside a path of `len` moves, with a chained,
    /// non-empty programmed replacement? The door the dump and the
    /// loader check before trusting a span.
    #[must_use]
    pub fn is_sane(&self, moves: &[Move]) -> bool {
        let (Some(c0), Some(c1)) = (moves.get(self.first), moves.get(self.last)) else {
            return false;
        };
        let (Some(p0), Some(p1)) = (self.programmed.first(), self.programmed.last()) else {
            return false;
        };
        self.first <= self.last
            && self.radius.is_finite()
            && self.radius > 0.0
            && self.programmed.len() >= 2
            && p0.from == c0.from
            && p1.to == c1.to
            && p0.arc.is_none()
            && p1.arc.is_none()
            && self.programmed.windows(2).all(|w| w[0].to == w[1].from)
            && self.programmed.iter().all(|m| m.is_finite())
    }
}
