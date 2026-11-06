//! Law (b), the section/tangency gouge law d(center, boundary) >= r - leave - tol, and the wiring of the solid law (b2) it shares a domain with - the shop practice of holding the cutter radius to the finish wall (machine-tool handbooks) on our own section model (split from `engine.rs` by M13.7; structure only).

use super::laws::{dist2_seg, law_points};
use super::rest::Rest;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};

/// Sample spacing of the solid law (b2) along each move, mm - the
/// operation layer's containment step.
const SOLID_STEP: f64 = 0.25;

/// Whether laws (b) and (b2) speak in `mode`.
///
/// Mode::Envelope stands this law down by design: a waterline ring
/// is tangent - its centers hold exactly r to the model at the
/// tangent line while sitting nearer than r to the level silhouette
/// elsewhere - and tangency is not gouging. Law (f) measures the
/// swept ball instead. Mode::Revolve stands down for the same
/// reason read the other way: a lathe's centers sit ON the
/// section's silhouette by design - the tool tip traces the
/// meridian - so "inside the section" is where a turned path
/// legitimately lives; the revolve verdict owns that geometry.
/// Mode::Radial stands down the same way read again (M5.2 F2): a
/// radial plunge's centers RUN THROUGH the silhouette wall - head
/// on, d = 0, every feed point of the bore inside the finish
/// circle it came to make. Tangency is the mill's law; the door
/// the kernel measured on the mesh is the radial one.
/// Mode::Cone stands down on the same reading from the vertical
/// mill's side (M2.5 E B): a chamfer ride's axis runs ON the wall
/// loop it rules (d = 0 by construction) and a deburr's tip
/// stands inside the corner material the ride exists to remove -
/// both facts are the generatrix law, not near misses. The reach
/// laws stay, unlike Engrave's: the cone's shank is wider than
/// the cone and the wall beside the edge is steel.
pub(super) fn judged(mode: Mode) -> bool {
    !matches!(
        mode,
        Mode::Engrave | Mode::Envelope | Mode::Revolve | Mode::Radial | Mode::Cone | Mode::Rotary
    )
}

/// (b) Gouge: tangency law d(center, boundary) >= r - leave - tol,
/// held at every law point of the move (see law_points). For a
/// model-driven path the boundary is level-dependent: the region's
/// silhouette walls plus the model's cross-section strictly above
/// the move's deepest plane. That section is exact for a fixed-Z
/// level pass, and conservative for a ramp or helix, whose whole XY
/// the generator already proved inside the deepest level's center
/// area (the chord and swept-disc laws) - a column that is only an
/// island deeper is not a hole here, and one the ladder must never
/// enter is. A center inside the section is a full collision, not
/// a near miss, so containment joins the distance test.
///
/// The section is read where the cutter is a full-radius cylinder
/// (M13.7): `lift` above the tip, the height the real tip reaches
/// `r` (0 for a square end, so its verdict is unchanged). Material
/// in the tip band below it is the solid law's (b2, [`solid`]) -
/// judged exactly instead of as a square rim.
pub(super) fn tangency(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    lift: f64,
    chord_tol: f64,
    warns: &mut Vec<Warning>,
) {
    let r = cx.tool.radius;
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let section = cx.model.map(|f| rest.islands_at(f, m.from.z.min(m.to.z) + lift));
        if let Some(None) = section {
            warns.push(Warning::Gouge { at: i }); // cannot prove the slice
            continue;
        }
        let section = section.flatten();
        let mut gouged = false;
        for p in law_points(m, chord_tol) {
            let mut d2 = f64::INFINITY;
            for l in cx.region.loops() {
                let pts = l.points();
                for k in 0..pts.len() {
                    d2 = d2.min(dist2_seg(p, pts[k], pts[(k + 1) % pts.len()]));
                }
            }
            if let Some(s) = &section {
                if s.contains(p) {
                    gouged = true;
                    break;
                }
                for l in s.loops() {
                    let pts = l.points();
                    for k in 0..pts.len() {
                        d2 = d2.min(dist2_seg(p, pts[k], pts[(k + 1) % pts.len()]));
                    }
                }
            }
            // The part is gouged below `r`, less only a NEGATIVE leave
            // (a deliberate overcut) - b2's own allowance. A positive
            // leave never licenses cutting into the finished wall: the
            // old `r - leave` let a 0.5 mm leave sink 0.5 mm unconvicted.
            if d2.sqrt() < r - (-cx.stock_to_leave).max(0.0) - cx.gouge_tol {
                gouged = true;
                break;
            }
        }
        if gouged {
            warns.push(Warning::Gouge { at: i });
        }
    }
}

/// (b2) The solid law: the plane sections of (b) are blind to what
/// no level slices - a blind floor between two planes, an
/// overhang above the ring, an undercut lip - so a model-driven
/// path also answers to the mesh's solid at every sampled tip,
/// with the op's real tip (M13.7) when the context carries one
/// that agrees with `cx.tool`, else the square stack (never
/// smaller than the real tip, so it can only over-report).
pub(super) fn solid(
    path: &Toolpath,
    cx: &VerifyContext,
    cutter: &super::Cutter,
    warns: &mut Vec<Warning>,
) {
    if let Some(f) = cx.model {
        let law = super::SolidLaw {
            step: SOLID_STEP,
            allow: cx.gouge_tol + (-cx.stock_to_leave).max(0.0),
            hole_walls: false,
        };
        for w in super::solid_hits(path, f, cutter, &law) {
            if !warns.contains(&w) {
                warns.push(w);
            }
        }
    }
}
