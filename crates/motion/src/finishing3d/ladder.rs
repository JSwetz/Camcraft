//! The ring ladder's levels (M13.9): the slope-aware Z-level stepdown.
//!
//! Provenance: the ball-end cusp law of the machining handbooks (two
//! passes `s` apart along the surface leave a cusp `≈ s²/(8r)`), read
//! on a Z-level ladder: on ground of slope `φ` from horizontal two
//! rings `Δz` apart in Z are `s = Δz / sin φ` apart along the surface,
//! so `Δz = √(8·r·tol)·sin φ` holds the cusp at `tol` - the stated
//! form `√(8·r·tol)·cos α` with `α = 90° − φ` the surface's tilt from
//! vertical. At 45° it is `2√(r·tol)`, the ladder's law since M2.2a.
//! Our own derivation; no vendor material consulted.
//!
//! ## The law as landed
//!
//! - **Per band, from the part.** The ladder walks down from `top_z`;
//!   at each level the band one ceiling-step deep (`[z − Δz₄₅, z]`) is
//!   asked which facets a ring in it touches - a facet with upward
//!   normal `m` is touched at tip-Z `z` when the contact height
//!   `z + r·(1 − m_z)` lies in its Z range - and the shallowest of
//!   them writes the step: `Δz = √(8·r·tol)·√(1 − m_z²)`. Asking the
//!   widest band and taking its smallest answer is conservative: a
//!   facet that only the full-width band reaches can tighten the step,
//!   never a facet outside it loosen it.
//! - **The 45° law is the ceiling.** The slope law only ever tightens:
//!   on walls (`φ > 45°`) its `Δz` exceeds `2√(r·tol)` and the ceiling
//!   wins, so every fixture whose rings ride ≥45° ground - the cone,
//!   the plateau's walls, the box - keeps its ladder byte for byte.
//!   The ring scan's direction count already spends half the budget
//!   on the assumption of that ceiling (`dirs`), and a coarser ladder
//!   on walls would change every wall golden for no cusp it can buy.
//! - **Floors are not cusp ground.** A facet whose own law step falls
//!   under `tol/4` - the arc fitter's chord budget, the finest length
//!   the ring walk itself resolves - is a floor: it meets one ring at
//!   most (the grazing pass at its own height) and leaves rest, the
//!   raster's jurisdiction (`Flat`, `SteepShallow`), never a cusp
//!   between two rings. Reading it into the law would ask a stepdown
//!   under the walk's own resolution and the level cap would refuse a
//!   part for a floor's float noise. At `r = 3, tol = 0.1` the line is
//!   ~0.9° of slope.
//! - **Only the ladders that own the shallow ground read it.** The
//!   waterline (and Morph, whose rings are the waterline's) is the one
//!   finisher of every slope it touches, so its ladder answers for the
//!   cusp on all of them. Every pattern with a shallow family - the
//!   rasters, the polar and shadow families, Flow, Ramp - hands ground
//!   under 45° to that family's own cos-law spacing and keeps the rings
//!   for the flanks; tightening their ladder below the split would cut
//!   the same ground twice at a finer pitch for no promise the raster
//!   has not already made (and on a crown it shrinks the first ring
//!   under the flank reach, where the envelope scan reads the ring's
//!   own inside as scallop - its named owed item). `SteepShallow` and
//!   `Blend` ride rings only on the steep side of their border, where
//!   the ceiling already IS the slope law; a stated `steep_stepover`
//!   outranks every derived number, as it always did.
//! - **The level cap stays a named refusal.** A ladder past
//!   `MAX_LEVELS` is refused with its count, never coarsened.

use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::params::{Finish3DParams, FinishPattern, SPLIT_MZ, scallop_stepdown, scallop_stepover};

/// The hard level cap: a tolerance so fine that the rings would
/// number in the hundreds of thousands is REFUSED, not quietly
/// coarsened - silent coarsening would make the stated tolerance a lie.
const MAX_LEVELS: usize = 8192;

/// One facet's share of the law: the tip-Z window over which a ring
/// touches it, and the stepdown its slope asks for.
struct Rung {
    lo: f64,
    hi: f64,
    dz: f64,
}

/// The facets that can tighten the ladder below `ceiling`: steep
/// enough to be cusp ground (law step ≥ `tol/4`), shallow enough that
/// the ceiling does not already hold their cusp, and on the ring
/// family's side of `shallow_mz` (1.0 for the waterline's ladder:
/// every facet is its own; the split for every pattern with a shallow
/// family).
fn rungs(facets: &[Triangle], r: f64, tol: f64, ceiling: f64, shallow_mz: f64) -> Vec<Rung> {
    let s = scallop_stepover(r, tol);
    let floor = tol * 0.25;
    let mut out = Vec::new();
    for f in facets {
        let m = f.normal();
        if !m.is_finite() {
            continue;
        }
        let mz = m.z.abs().min(1.0);
        if mz >= shallow_mz {
            continue;
        }
        let dz = s * (1.0 - mz * mz).max(0.0).sqrt();
        if !(dz.is_finite() && dz >= floor && dz < ceiling) {
            continue;
        }
        let zmin = f.a.z.min(f.b.z).min(f.c.z);
        let zmax = f.a.z.max(f.b.z).max(f.c.z);
        let lift = r * (1.0 - mz);
        out.push(Rung { lo: zmin - lift, hi: zmax - lift, dz });
    }
    out
}

/// Tip-Z levels for the pass, top down, ending exactly at `bottom_z`
/// (the shared snap law), spaced by the slope law above.
pub(super) fn levels(p: &Finish3DParams, r: f64, facets: &[Triangle]) -> Result<Vec<f64>, GenError> {
    let span = p.top_z - p.bottom_z;
    if span < 0.0 {
        return Err(GenError::Invalid("waterline bottom is above its top".into()));
    }
    // The ladder's author: the scallop law, unless the partition's
    // own operation states a flank spacing (M2.5 A) - the same
    // "a stated number outranks a derived one, the verdict keeps
    // the promise" grammar the raster's stepover speaks.
    let (ceiling, shallow_mz) = match p.pattern {
        FinishPattern::SteepShallow => (
            p.steep_stepover.unwrap_or_else(|| scallop_stepdown(r, p.tolerance)),
            SPLIT_MZ,
        ),
        FinishPattern::Waterline | FinishPattern::Morph => (scallop_stepdown(r, p.tolerance), 1.0),
        _ => (scallop_stepdown(r, p.tolerance), SPLIT_MZ),
    };
    if span > 0.0 && ceiling <= span / MAX_LEVELS as f64 {
        return Err(GenError::Invalid(format!(
            "stepdown {:.6} with this span needs {} levels (cap {}): refine the spacing or the tolerance",
            ceiling,
            (span / ceiling.max(1e-12)).min(i64::MAX as f64) as i64,
            MAX_LEVELS,
        )));
    }
    if span <= 0.0 {
        return Ok(vec![p.bottom_z]);
    }
    let ceiling = ceiling.max(1e-9);
    let rungs = if p.steep_stepover.is_some() && matches!(p.pattern, FinishPattern::SteepShallow) {
        Vec::new() // a stated number is never silently changed
    } else {
        rungs(facets, r, p.tolerance, ceiling, shallow_mz)
    };
    let eps = span * 1e-9;
    let mut zs = Vec::new();
    let mut z = p.top_z;
    while z > p.bottom_z + eps {
        if zs.len() >= MAX_LEVELS {
            return Err(GenError::Invalid(format!(
                "the slope-aware ladder from {:.4} to {:.4} needs more than {} levels (cap {}): refine the tolerance or hand the shallow ground to a raster pattern",
                p.top_z, p.bottom_z, MAX_LEVELS, MAX_LEVELS,
            )));
        }
        zs.push(z);
        let band_lo = z - ceiling;
        let dz = rungs
            .iter()
            .filter(|g| g.lo <= z && g.hi >= band_lo)
            .fold(ceiling, |acc, g| acc.min(g.dz));
        z -= dz.max(1e-9);
    }
    zs.push(p.bottom_z);
    Ok(zs)
}
