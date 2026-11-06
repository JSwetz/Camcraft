//! The hidden-rest scan (M2.2d) and its budget, split from `scan.rs`
//! by the 400-line law (M13.1). Provenance: the parity-paired solid
//! truth of `solid_rest` (ray-crossing parity, the published
//! point-in-solid test) judged by the envelope's own distance law;
//! nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **Over budget is "not judged", never zero** (M13.1, paying the
//!   M2.2 debt the ROADMAP names): past its cell or claim budget the
//!   scan cannot answer, and it says so through the report's type
//!   (`HiddenRest::OverBudget`) instead of the old stated zero that
//!   read exactly like "no hidden rest". Every consumer shows the
//!   unjudged state by name.

use super::dexel::fy0;
use super::scan::dist_pt_seg3;

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// What the hidden-rest scan may spend: lattice cells (the bin
/// headers, ~30 MB at the shipped 1.2 M) and plane claims within
/// them (40 M). A stated budget, so a caller can name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanBudget {
    pub cells: usize,
    pub claims: usize,
}

impl ScanBudget {
    /// The budget every shipped verdict scans under.
    pub const SHIPPED: ScanBudget = ScanBudget { cells: 1_200_000, claims: 40_000_000 };
}

/// Whether the hidden-rest half of an envelope verdict was judged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HiddenRest {
    /// The scan ran (or had nothing to scan): `rest_mm` carries its
    /// answer, zero included as a real zero.
    #[default]
    Judged,
    /// The scan stopped at its budget and judged NOTHING: `rest_mm`
    /// speaks for the ridge law alone, and material hidden under an
    /// overhang may stand unreported. `cells` is the lattice, `claims`
    /// how far the binning got before it stopped.
    OverBudget { cells: usize, claims: usize },
}

/// The hidden-rest scan, paying M2.2b's open debt. Per column the
/// mesh's SOLID truth is paired (`solid_rest` law (a)); every
/// material top below the height field's own top minus the
/// quantization floor is occluded material - invisible to a
/// Z-following measure by construction. Each occluded top is then
/// judged by the ridge law's own distance truth: nearest ball
/// center minus the ball minus the floor, so the two reports are
/// one yardstick reading two depths, never two predicates. An odd
/// column is an open shell - unknown, and unknown never reports
/// material (law (b)). Work is binned facet-outer (the
/// `part_surface` shape); over budget the answer is
/// `Err(HiddenRest::OverBudget)` - not judged, said by name.
#[allow(clippy::too_many_arguments)] // the scan's whole geometry frame, threaded once from envelope_scan
pub(super) fn hidden_rest(
    facets: &[Triangle],
    segs: &[(Point3, Point3)],
    part: &[f64],
    (x0, y0, cell): (f64, f64, f64),
    nx: usize,
    ny: usize,
    r: f64,
    floor: f64,
    budget: ScanBudget,
) -> Result<(f64, Point2), HiddenRest> {
    if segs.is_empty() || facets.is_empty() {
        return Ok((0.0, Point2::new(0.0, 0.0))); // nothing swept, or nothing to hide: judged
    }
    if nx.saturating_mul(ny) > budget.cells {
        return Err(HiddenRest::OverBudget { cells: nx.saturating_mul(ny), claims: 0 });
    }
    let mut bins: Vec<Vec<crate::solid_rest::Claim>> = (0..nx * ny).map(|_| Vec::new()).collect();
    let mut claims = 0usize;
    for t in facets {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            continue;
        }
        let fx = |v: f64| ((v - x0) / cell).floor();
        let i0 = fx(a.x.min(b.x).min(c.x)).max(0.0) as usize;
        let i1 = fx(a.x.max(b.x).max(c.x)).clamp(0.0, nx as f64 - 1.0) as usize;
        let j0 = fy0(a.y.min(b.y).min(c.y), y0, cell).max(0.0) as usize;
        let j1 = fy0(a.y.max(b.y).max(c.y), y0, cell).clamp(0.0, ny as f64 - 1.0) as usize;
        if i0 >= nx || j0 >= ny {
            continue;
        }
        for iy in j0..=j1 {
            for ix in i0..=i1 {
                let (cx, cy) = (x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                if let Some(cl) = crate::solid_rest::plane_claim(&a, &b, &c, cx, cy) {
                    bins[ix + iy * nx].push(cl);
                    claims += 1;
                    if claims > budget.claims {
                        // over budget: NOT judged, and said so - never a zero
                        return Err(HiddenRest::OverBudget { cells: nx * ny, claims });
                    }
                }
            }
        }
    }
    let mut best = 0.0;
    let mut at = Point2::new(0.0, 0.0);
    for iy in 0..ny {
        for ix in 0..nx {
            let bin = &bins[ix + iy * nx];
            if bin.is_empty() {
                continue;
            }
            let t = part[ix + iy * nx];
            if !t.is_finite() {
                continue; // the field sees nothing here; nor has this scan's ground
            }
            let crate::solid_rest::Crossings::Paired(z) =
                crate::solid_rest::crossings_from_claims(bin)
            else {
                continue; // open shell: unknown never reports material
            };
            // Interval tops, high to low: the highest is the visible
            // surface; every lower one hides under a top above it.
            for c in z.as_chunks::<2>().0 {
                let hi = c[1];
                if hi >= t - floor {
                    continue; // not occluded: the ridge law already owns this top
                }
                let (cx, cy) = (x0 + (ix as f64 + 0.5) * cell, y0 + (iy as f64 + 0.5) * cell);
                let mut dmin = f64::INFINITY;
                for (sa, sb) in segs {
                    let d = dist_pt_seg3(cx, cy, hi, *sa, *sb);
                    if d < dmin {
                        dmin = d;
                    }
                }
                let rest = (dmin - r - floor).max(0.0);
                if rest > best {
                    best = rest;
                    at = Point2::new(cx, cy);
                }
            }
        }
    }
    Ok((best, at))
}

