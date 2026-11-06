//! Law (a), Feed-Feed crossings by the orientation predicate - textbook computational geometry (O'Rourke, "Computational Geometry in C", public material) (split from `engine.rs` by M13.7; structure only).

use super::laws::proper_cross;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;

/// (a) Crossing: Feed-Feed pairs, proper XY crossings at shared depth.
/// Each feed contributes its chords (one for a straight move, so a
/// straight path walks exactly the pair set it always did); reports
/// are per move pair, ascending and deduped - indices hold stable
/// whatever the chord count.
pub(super) fn crossings(path: &Toolpath, cx: &VerifyContext, chord_tol: f64) -> Vec<Warning> {
    let mut crossed: Vec<[usize; 2]> = Vec::new();
    // Mode::Envelope stands this law down by design (M2.2c): a
    // crossed raster pattern makes two Feed chords cross in XY at
    // shared depth wherever its two families share a point, and the
    // stepover's own overlap already has neighboring lines covering
    // the same material - that intersection is the pattern's
    // substance, not a path defect. The waterline ring never made a
    // proper crossing, so only the multi-direction patterns reveal
    // the predicate's limits. What a harmful chord would be - one
    // biting into the surface or sailing above it - law (f) measures
    // directly, on the swept ball against the model, move by move.
    // Mode::Revolve stands it down too (M5.2 C): a turning path's
    // radial plunges all share the axis in XY, so every pair of
    // centers in a groove's ladder "crosses" at the very line the
    // part spins on - the XY reading of the predicate says nothing
    // true about a meridian path, which the revolve verdict judges.
    // Mode::Radial stands it down too (M5.2 F2): two live-tool holes
    // drilled at one height across one another ARE the cross-drilled
    // intersection the part asks for - the crossing point is kerf in
    // both bores, not a collision.
    if !matches!(cx.mode, Mode::Envelope | Mode::Revolve | Mode::Radial | Mode::Rotary) {
        let mut feeds: Vec<(usize, Point2, Point2, f64, f64)> = Vec::new();
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind != MoveKind::Feed {
                continue;
            }
            for (a, b) in m.chords(chord_tol) {
                feeds.push((i, a, b, m.from.z, m.to.z));
            }
        }
        for (ai, &(ia, a0, a1, za0, za1)) in feeds.iter().enumerate() {
            for &(ib, b0, b1, zb0, zb1) in feeds.iter().skip(ai + 1) {
                if ia == ib {
                    continue; // chords of one move share their joints
                }
                // Two planar chords cross only at a shared plane, exactly
                // as always. When a chord descends (a ramp diagonal, a
                // helix arc chord), a Z-interval that touches the other's
                // plane at a single endpoint is a GRAZE of the already-cut
                // floor at the dive's own start or end depth - which is
                // what every 3-D entry does by construction, over XY the
                // level law already proved open. For a non-planar pair
                // only a shared depth RANGE (two descents whose bands
                // overlap) counts as crossing. (M2.1a's superset law is
                // what makes the graze safe; without it this would be a
                // gouge.)
                let planar_pair = za0 == za1 && zb0 == zb1;
                let z_hit = if planar_pair {
                    za0 == zb0
                } else {
                    za0.max(za1).min(zb0.max(zb1)) - za0.min(za1).max(zb0.min(zb1)) > 0.0
                };
                if z_hit && proper_cross(a0, a1, b0, b1) {
                    crossed.push([ia.min(ib), ia.max(ib)]);
                }
            }
        }
    }
    crossed.sort_unstable();
    crossed.dedup();
    crossed.into_iter().map(|moves| Warning::Crossing { moves }).collect()
}
