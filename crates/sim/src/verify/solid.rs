//! The solid-containment law: a cutting move whose cutter CENTER stands
//! inside the finished part's own cross-section at the move's deepest
//! plane is a gouge - the tool is in steel the part keeps. Specced from
//! the shop rule "never cut below a feature's floor" (machining-handbook
//! hole/pocket depth practice) read against the model's planar sections,
//! the same `material_above` reading the model law of [`super::verify`]
//! uses (M2.1a). Containment only - no tangency distance - so it is the
//! law for kinds whose region carries exact rims in place of the mesh's
//! tessellated walls (drill, bore, thread mill): a matched tool at a
//! 16-facet wall's apothem is not a gouge, a plunge past a blind floor
//! is.

use std::collections::HashMap;

use super::laws::law_points;
use super::types::Warning;
use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;

/// The XY samples along one move: the move's own law points (arc chords)
/// plus its start, and a straight move subdivided every `step` so a
/// traverse crossing a solid column between two open ends is seen.
fn samples(m: &crate::cl::Move, chord_tol: f64, step: f64) -> Vec<Point2> {
    let a = Point2::new(m.from.x, m.from.y);
    let mut v = vec![a];
    if m.arc.is_some() {
        v.extend(law_points(m, chord_tol));
        return v;
    }
    let b = Point2::new(m.to.x, m.to.y);
    let len = (b.x - a.x).hypot(b.y - a.y);
    // Bounded: a non-finite or absurd length samples its ends only (the
    // NonFinite law owns the former), never an unbounded loop.
    let n = if len.is_finite() && step > 0.0 {
        ((len / step).ceil() as usize).clamp(1, 4096)
    } else {
        1
    };
    for k in 1..=n {
        let t = k as f64 / n as f64;
        v.push(Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
    }
    v
}

/// One `Gouge` per cutting move whose center enters the part's section
/// at the move's deepest plane; a section that cannot be proved fails
/// closed (a gouge, never a pass). Moves are reported ascending.
pub fn solid_hits(path: &Toolpath, facets: &[Triangle], chord_tol: f64, step: f64) -> Vec<Warning> {
    let mut cache: HashMap<u64, Option<Region>> = HashMap::new();
    let mut out = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid || !(m.from.is_finite() && m.to.is_finite()) {
            continue;
        }
        let z = m.from.z.min(m.to.z);
        let section = cache
            .entry(z.to_bits())
            .or_insert_with(|| crate::geometry::slice::material_above(facets, z).ok());
        let hit = match section {
            None => true,
            Some(s) => !s.is_empty() && samples(m, chord_tol, step).into_iter().any(|p| s.contains(p)),
        };
        if hit {
            out.push(Warning::Gouge { at: i });
        }
    }
    out
}
