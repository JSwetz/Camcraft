//! The solid law: no sampled position of a move may put the tool
//! assembly - cutting atom (flat, ball, bull nose, point cone) plus
//! shank and holder - inside the finished part's SOLID. Read off the
//! mesh itself ([`super::probe`], [`super::solid_index`]), never off
//! plane sections: material a plane at the move's depth does not cut -
//! a blind floor between two levels, a slab overhanging the ring, the
//! underside of an undercut lip - is still steel the tool cannot enter.
//! Specced from the shop rule "never cut the part you keep" read
//! against the closed-shell solid (point-in-solid by ray parity and the
//! swept-atom distance law, both cited in [`super::swept`]). A straight
//! move is judged as one exact sweep, an XY arc by its chords' sweeps
//! with the cutter widened by their sagitta; only a vertical-plane arc
//! is sampled (every `step`), its stated limit.
//!
//! Two readers: [`super::verify`]'s model law (b2), with the square
//! stack of its `ToolGeom` - never smaller than the real cutter - and
//! the operation layer's depth law via [`solid_hits`], with the tool's
//! own tip. A cutting move that enters the solid is a [`Warning::Gouge`];
//! a rapid that does is a [`Warning::RapidInStock`] - the part is stock
//! too, and steel at rapid feed is a crash.

use super::probe::{Cutter, Frame};
use super::solid_index::SolidIndex;
use super::types::Warning;
use crate::cl::{MoveKind, Toolpath, is_vertical, normalize_axis};
use crate::geometry::{Point3, Triangle};

/// How the solid law samples and what it excuses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolidLaw {
    /// Sample spacing along a vertical-plane arc, mm (every other move
    /// is swept exactly).
    pub step: f64,
    /// Penetration granted, radial and at the tip, mm: the gouge
    /// tolerance (plus any negative stock-to-leave the op asked for).
    pub allow: f64,
    /// The containment kinds (drill, bore, thread mill): their regions
    /// carry exact rims for the mesh's tessellated hole walls, so a
    /// facet entered only by its chord's dip between two vertices that
    /// stand outside the tool is the rim, not a gouge.
    pub hole_walls: bool,
}

/// Sagitta of the chords an XY arc is swept along, mm; every atom is
/// widened by it, so the chord sweep encloses the arc's (conservative by
/// at most this much, never blind).
const ARC_SAG: f64 = 0.002;

/// The swept pieces of one move as (start tip, end tip, widening): a
/// straight move is itself - one exact sweep; an XY arc or helix its
/// chords at [`ARC_SAG`]; a vertical-plane arc (whose chord deviation is
/// not horizontal) its on-arc points every `step`, the stated sampling
/// limit. Bounded, never an unbounded loop on absurd lengths.
fn pieces(m: &crate::cl::Move, step: f64) -> Vec<(Point3, Point3, f64)> {
    match m.arc {
        None => vec![(m.from, m.to, 0.0)],
        Some(arc) if arc.plane.is_xy() => m.segments3(ARC_SAG).into_iter().map(|(a, b)| (a, b, ARC_SAG)).collect(),
        Some(arc) => {
            let len = arc.length(m.from, m.to).filter(|l| l.is_finite()).unwrap_or(0.0);
            let n = if step > 0.0 { ((len / step).ceil() as usize).clamp(1, 4096) } else { 1 };
            let mut v = vec![(m.from, m.from, 0.0)];
            for k in 1..=n {
                match arc.point_at(m.from, m.to, k as f64 / n as f64) {
                    Some(p) => v.push((p, p, 0.0)),
                    // Degenerate arc: its straight chords, swept exactly.
                    None => return m.segments3(ARC_SAG).into_iter().map(|(a, b)| (a, b, ARC_SAG)).collect(),
                }
            }
            v
        }
    }
}

/// One warning per move whose assembly enters the solid, ascending. A
/// mesh or tool that cannot be read honestly fails closed: a Gouge at
/// the first cutting move, never a pass.
pub fn solid_hits(path: &Toolpath, facets: &[Triangle], cutter: &Cutter, law: &SolidLaw) -> Vec<Warning> {
    let first_cut = path.moves.iter().position(|m| m.kind != MoveKind::Rapid);
    if facets.is_empty() {
        return Vec::new();
    }
    let atoms = cutter.atoms();
    let allow = if law.allow.is_finite() { law.allow.max(0.0) } else { 0.0 };
    let index = SolidIndex::build(facets);
    let (Some(mut index), false) = (index, atoms.is_empty()) else {
        return first_cut.map(|at| vec![Warning::Gouge { at }]).unwrap_or_default();
    };
    // The fluted length is the cutting edge; the bands above it are the
    // shank and holder. A cut that only the bands make is not a gouge -
    // the edge never passed the finish surface - but steel rubbing the
    // part, said as such (ShankHit on the part), and said once.
    let fl = cutter.flute_len;
    let (edge, body): (Vec<_>, Vec<_>) = atoms.iter().copied().partition(|a| a.z0 < fl);
    let mut out = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if !(m.from.is_finite() && m.to.is_finite()) {
            continue; // the NonFinite law owns it
        }
        let axis = m.axis.and_then(normalize_axis).filter(|a| !is_vertical(a));
        let mut hits = |set: &[super::probe::Atom]| {
            !set.is_empty()
                && pieces(m, law.step).into_iter().any(|(a, b, grow)| {
                    let fr = Frame::new(a, axis);
                    index.hits(&fr, fr.local(b), set, allow, grow, law.hole_walls)
                })
        };
        if m.kind == MoveKind::Rapid {
            if hits(&atoms) {
                out.push(Warning::RapidInStock { at: i });
            }
        } else if hits(&edge) {
            out.push(Warning::Gouge { at: i });
        } else if hits(&body) {
            out.push(Warning::ShankHit { at: i, part: true });
        }
    }
    out
}
