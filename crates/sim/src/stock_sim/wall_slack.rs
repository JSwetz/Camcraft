//! The gouge verdict's lateral slack at a wall (#299).
//!
//! Provenance: our own design, from the tolerance geometry of a
//! height-field (Z-map) comparison as the published dexel literature
//! states it - a column verdict measures error along Z only, so at a
//! surface of slope angle `theta` its normal-distance tolerance is
//! `tol * cos(theta)`, which falls to zero at a vertical wall. Nothing
//! from vendor binaries or output.
//!
//! ## The law here
//!
//! A column whose centre sits a few microns inside a vertical wall is
//! judged against the wall's full height; the tool that finishes that
//! wall exactly (a flat end mill's edge on it, a ball tangent to it)
//! reaches the column's centre line, and the height verdict then
//! charges the whole wall as a gouge - a lateral error far below the
//! lattice's resolution read as one the depth of the wall. So a gouge
//! is judged against the LOWEST part top within `tol` of the column's
//! centre, sideways as well as down: the same tolerance the verdict
//! already grants in Z, granted in XY. The normal-distance tolerance
//! is then between `tol` and `sqrt(2) * tol` at every slope, instead of
//! shrinking to nothing at a wall.
//!
//! - **Only gouges.** Rest keeps the centre's own part top: slack
//!   there would read material standing on a floor next to a wall as
//!   the wall's own, hiding rest - and only the gouge side refuses a
//!   correct program.
//! - **Strict at the slack.** The disc is sampled at its centre and
//!   on a ring of eight directions at exactly `tol`, so a wall is
//!   found when it is within `tol * cos(22.5 deg)` (0.92 tol) of the
//!   centre in its worst orientation - the sampling only ever absolves
//!   less than the law allows, never more. A cut reaching past the
//!   slack into the part still convicts, at the centre's own depth.
//! - **Air absolves.** Within `tol` of the part's silhouette, the
//!   lowest top is air (`-inf`): the column is within tolerance of no
//!   part at all, so it is clean - the same as a column wholly in air,
//!   which the verdict never judged.
//! - **Paid only on a candidate.** The ring is read only when a column
//!   already reads below the centre's top by more than `tol`; a clean
//!   program costs nothing extra.

use super::deviate::Deviation;
use super::kernel::part_surface;

use crate::geometry::{Point3, Triangle};

/// A column the centre's height verdict would charge as a gouge,
/// waiting for the slack law: its index, the sim's top, `sim - part`
/// at the centre, and where it is (the sim's own surface).
#[derive(Clone, Copy, Debug)]
pub(super) struct Candidate {
    pub k: usize,
    pub sim: f64,
    pub v: f64,
    pub at: Point3,
}

/// The lowest part top within `slack` of each cell centre on the
/// lattice `(x0, y0, cell, nx, ny)`: `centre` (that lattice's own
/// `part_surface`) folded with the part surface read on the eight ring
/// offsets. `-inf` wins - air within the slack is air.
fn lowest_near(
    facets: &[Triangle],
    x0: f64,
    y0: f64,
    cell: f64,
    (nx, ny): (usize, usize),
    slack: f64,
    centre: &[f64],
) -> Vec<f64> {
    let mut lo = centre.to_vec();
    if !(slack.is_finite() && slack > 0.0) {
        return lo;
    }
    let d = slack * std::f64::consts::FRAC_1_SQRT_2;
    let ring = [
        (slack, 0.0),
        (-slack, 0.0),
        (0.0, slack),
        (0.0, -slack),
        (d, d),
        (d, -d),
        (-d, d),
        (-d, -d),
    ];
    for (dx, dy) in ring {
        let s = part_surface(facets, x0 + dx, y0 + dy, cell, nx, ny);
        for (l, v) in lo.iter_mut().zip(s) {
            if v < *l {
                *l = v;
            }
        }
    }
    lo
}

/// Settle the candidates on `d` (the lattice `(x0, y0)` with `d`'s own
/// cell and size): a candidate within `d.tol` of the lowest part top
/// near it is clean, its deviation the clamped `sim - lowest` (never
/// positive - it read below the centre's top, so it is not rest); the
/// rest are gouges at the centre's own depth, as before.
pub(super) fn settle(d: &mut Deviation, facets: &[Triangle], x0: f64, y0: f64, part: &[f64], cands: &[Candidate]) {
    if cands.is_empty() {
        return;
    }
    let lo = lowest_near(facets, x0, y0, d.cell, (d.cells_x, d.cells_y), d.tol, part);
    let a2 = d.cell * d.cell;
    for c in cands {
        let low = lo.get(c.k).copied().unwrap_or(f64::NEG_INFINITY);
        let near = c.sim - low;
        if near >= -d.tol {
            if let Some(slot) = d.dev.get_mut(c.k) {
                *slot = if near.is_finite() { near.min(0.0) } else { 0.0 };
            }
            continue;
        }
        if let Some(slot) = d.dev.get_mut(c.k) {
            *slot = c.v;
        }
        d.gouge_cells += 1;
        d.gouge_volume += -c.v * a2;
        if -c.v > d.max_gouge {
            d.max_gouge = -c.v;
            d.max_gouge_at = Some(c.at);
        }
    }
}
