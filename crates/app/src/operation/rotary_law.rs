//! The rotary kinds' band and rapid verdict (M13): the radial field
//! law of `verify::rotary_hits` run with the same fitted axis the
//! generator posed about, the setup's stock solid and the part.
//! Provenance: our own wiring of the sim crate's law (its sources are
//! cited there); no vendor material consulted.

use crate::geometry::Triangle;
use crate::verify::{self, Warning};

use super::material::GOUGE_TOL;
use super::types::OpParams;

/// Field cell on the widest skin and sample step along a move, mm:
/// a quarter millimetre - finer than any band's clearance this law
/// is asked to resolve, and the field stays a few tens of thousands
/// of cells on a bar a hand holds.
const CELL: f64 = 0.25;
const STEP: f64 = 0.25;

/// The rotary kinds' finish budget (their `tolerance`), None for every
/// other kind - which is exactly where this law does not run.
pub(super) fn rotary_tolerance(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::RotaryContour { tolerance, .. }
        | OpParams::RotaryParallel { tolerance, .. }
        | OpParams::RotaryOffset { tolerance, .. } => Some(*tolerance),
        _ => None,
    }
}

/// Append the radial field law's warnings to `warns` (deduped). The
/// axis is re-fitted from the part - the fit is a pure function of
/// the facets, so it is the very axis the generator turned about; a
/// fit that fails here (it cannot, once the generator ran) fails
/// closed as a Gouge at the first cut, never a silent pass.
pub(super) fn rotary_into(
    warns: &mut Vec<Warning>,
    path: &crate::cl::Toolpath,
    part: &[Triangle],
    stock: Option<&[Triangle]>,
    tool: &crate::tools::Tool,
    tolerance: f64,
) {
    let hits = match crate::multiaxis::axis::fit(part) {
        Ok(ax) => verify::rotary_hits(
            path,
            &verify::RotaryAxis { origin: ax.origin, dir: [ax.dir.x, ax.dir.y, ax.dir.z] },
            stock,
            part,
            &verify::Cutter::of(tool),
            &verify::RotaryLaw {
                cell: CELL,
                step: STEP,
                allow: GOUGE_TOL,
                rapid_allow: tolerance.max(GOUGE_TOL),
            },
        ),
        Err(e) => {
            eprintln!("rotary verdict: the axis fit refused ({e}); the path is not judged clean");
            path.moves
                .iter()
                .position(|m| m.kind != crate::cl::MoveKind::Rapid)
                .map(|at| vec![Warning::Gouge { at }])
                .unwrap_or_default()
        }
    };
    for w in hits {
        if !warns.contains(&w) {
            warns.push(w);
        }
    }
}
