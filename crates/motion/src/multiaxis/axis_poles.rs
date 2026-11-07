//! Candidate axes from a part's flat poles (issue #152). The normal
//! moment of `axis.rs` reads the axis off its extreme eigenvectors,
//! but a turned bar near L/D 1..2 balances it: its end caps' axial
//! area 2*pi*R^2 matches the walls' radial moment pi*R*h, all three
//! eigenvalues come out near equal and the eigenvectors are arbitrary.
//! The moment then cannot tell the bar from a sphere.
//!
//! The faces can. A facing flat of a revolve is a POLE: a cluster of
//! facets sharing one normal, and that normal is the axis (a revolved
//! surface's normal at a point on the axis lies along it - the facing
//! cut of a lathe). A sphere has no such cluster: its facets each
//! face their own way. So when the moment is isotropic, the
//! directions that carry a real share of the area as one flat are
//! offered as candidates, and the same concurrency and coarseness
//! judgement of `axis.rs` decides (its line re-centred by least
//! trimmed squares when an off-centre feature drags it, see
//! [`try_pole`]). A cube's faces are poles too, but four bearings
//! about any of them fail the coarseness gate, so it is still refused
//! by name as before.
//!
//! Provenance: principal-axis analysis of the normal (Gauss-map)
//! distribution - a flat maps to one point of the Gauss sphere,
//! a revolve's lateral zone to a great or small circle about its axis
//! (do Carmo, Differential Geometry of Curves and Surfaces, the Gauss
//! map; Pottmann & Randrup 1998 on revolved-surface fitting); least
//! trimmed squares (Rousseeuw, 1984). Written from those sources and
//! our own design notes; no vendor material.

use std::collections::HashMap;

use crate::geometry::Point3;

use super::axis::{judge, line, try_axis, Axis, Datum};
use super::axis_robust::dist;
use super::linalg::unit;

/// Least share of the whole area one flat must carry to be offered:
/// a lathe's facing ends carry R/(R+h) of a plain bar (a fifth at
/// L/D 2); a sphere's single facets, a thousandth.
const POLE_SHARE: f64 = 0.05;
/// The Gauss-map cell, in radians: far below the bearing step of any
/// mesh the coarseness gate passes, far above an f32 STL's rounding.
const CELL: f64 = 1e-4;
/// Re-centring rounds of [`try_pole`]: the kept half settles in a few.
const LTS_ROUNDS: usize = 8;
/// At most this many flats are tried, largest first.
const MAX_POLES: usize = 3;

/// The pole directions of `data`, largest area first: the normals
/// (sign-folded, a flat and its opposite face are one axis) that one
/// cluster of facets shares with at least [`POLE_SHARE`] of
/// `a_total`.
pub(super) fn candidates(data: &[Datum], a_total: f64) -> Vec<Point3> {
    let mut cells: HashMap<(i64, i64, i64), (f64, Point3)> = HashMap::new();
    for d in data {
        let n = fold(d.n);
        let q = |c: f64| (c / CELL).round();
        let (kx, ky, kz) = (q(n.x), q(n.y), q(n.z));
        if !(kx.is_finite() && ky.is_finite() && kz.is_finite()) {
            continue;
        }
        let e = cells.entry((kx as i64, ky as i64, kz as i64)).or_insert((0.0, Point3::new(0.0, 0.0, 0.0)));
        e.0 += d.a;
        e.1 = Point3::new(e.1.x + d.a * n.x, e.1.y + d.a * n.y, e.1.z + d.a * n.z);
    }
    let mut poles: Vec<((i64, i64, i64), f64, Point3)> = cells
        .into_iter()
        .filter(|(_, (a, _))| *a >= POLE_SHARE * a_total)
        .filter_map(|(k, (a, s))| unit(s).map(|u| (k, a, u)))
        .collect();
    // Largest first; the cell key breaks ties so the order is the
    // same on every run (a HashMap iterates in no fixed order).
    poles.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
    poles.into_iter().take(MAX_POLES).map(|(_, _, u)| u).collect()
}

/// Judge the pole direction `u` (`axis::try_axis`). A short bar's
/// plain least-squares line is dragged by an off-centre feature (one
/// radial hole on an r10 x 40 bar pulls it 0.4 mm, past the sagitta
/// band, so half the wall misses it): when the plain line fails, it is
/// re-centred by least trimmed squares - refitted on the half of the
/// normal lines that miss it least, until that half stops changing -
/// and the re-centred line must still pass the full judgement on every
/// facet, so this can only find an axis the part earns.
pub(super) fn try_pole(data: &[Datum], u: Point3, a_total: f64) -> Option<(f64, Axis)> {
    if let Some(sol) = try_axis(data, u, a_total) {
        return Some(sol);
    }
    let (mut ax, rho) = line(data, u, a_total)?;
    let mut keep: Vec<bool> = Vec::new();
    for _ in 0..LTS_ROUNDS {
        let miss: Vec<Option<f64>> = data.iter().map(|d| dist(d, ax)).collect();
        let mut sorted: Vec<f64> = miss.iter().flatten().copied().collect();
        if sorted.len() < 2 {
            return None;
        }
        sorted.sort_by(|a, b| a.total_cmp(b));
        let med = sorted[sorted.len() / 2];
        let next: Vec<bool> = miss.iter().map(|m| m.is_none_or(|e| e <= med)).collect();
        if next == keep {
            break;
        }
        let kept: Vec<Datum> = data
            .iter()
            .zip(&next)
            .filter(|(_, k)| **k)
            .map(|(d, _)| Datum { n: d.n, p: d.p, a: d.a })
            .collect();
        let a_kept: f64 = kept.iter().map(|d| d.a).sum();
        ax = line(&kept, ax.dir, a_kept)?.0;
        keep = next;
    }
    judge(data, ax, rho, a_total).map(|r| (r, ax))
}

/// `n` or `-n`, whichever has its largest component positive - the
/// largest, not the first nonzero, so a flat's rounding noise in a
/// near-zero component never splits it into two cells.
fn fold(n: Point3) -> Point3 {
    let (ax, ay, az) = (n.x.abs(), n.y.abs(), n.z.abs());
    let big = if ax >= ay && ax >= az {
        n.x
    } else if ay >= az {
        n.y
    } else {
        n.z
    };
    if big < 0.0 {
        Point3::new(-n.x, -n.y, -n.z)
    } else {
        n
    }
}

#[cfg(test)]
#[path = "axis_poles_tests.rs"]
mod tests;
