//! The raster's rim rows (#46): a last line where the shallow ground ends, so a fixed-phase line grid cannot stop a stepover short of the rim.
//!
//! The raster lays its lines at `vmin + j·step` - a phase set by the
//! padded bounds, not by the ground. Where the ground ends at an open
//! rim (a shallow face meeting a steep wall, a mesh edge) the last
//! line that still finds tangent ground can stand up to one whole
//! step inside it, and the strip between that line's contact and the
//! rim is touched by nothing else: the strict facet tangency answers
//! nothing past the face, and the ring that grazes the rim edge from
//! outside sits nearly edge-on to the face. On the tilted wedge
//! (26.57°, r 3, step 1.386) the high rim's last line contacted 1.2 mm
//! short of it and left a 0.17 mm cusp against a 0.1 promise.
//!
//! The law: between the outermost line that found ground and its
//! empty neighbour outward, bisect the line offset to where ground
//! ends, and lay one more line there - so the rim is at most a bisect
//! tolerance from a tangent contact. It adds a line only when the gap
//! is worth one (more than a quarter step), never moves a grid line,
//! and every sample of the new line is the same march, so it is
//! exactly as safe as any other line.
//!
//! Provenance: the boundary pass of a zig-zag (direction-parallel)
//! finish - covering the region's edge with a pass of its own - is
//! textbook CAM practice (machining handbooks' raster finishing);
//! the bisection is our own. No vendor material consulted.

/// The bisection's resolution against the scallop tolerance: the rim
/// ends up within `tol/20` of a contact - a sagitta far under `tol`.
const BISECT_OF_TOL: f64 = 0.05;
/// A gap narrower than a quarter step leaves a rim sagitta of under a
/// sixteenth of the line's own scallop: not worth a line.
const MIN_GAP_OF_STEP: f64 = 0.25;

/// The rim rows of one raster pass: the line offsets (`low`, `high`)
/// to add outside the outermost grid lines with ground, `vs` the grid
/// offsets in order, `filled[j]` whether line `j` found any ground,
/// `vmax` the padded bound past the last line, and `probe(v)` whether
/// a line at `v` finds ground. `None` on a side that needs no row.
pub(super) fn rim_rows(
    vs: &[f64],
    filled: &[bool],
    vmax: f64,
    step: f64,
    tol: f64,
    probe: &mut impl FnMut(f64) -> bool,
) -> (Option<f64>, Option<f64>) {
    let (Some(jf), Some(jl)) = (filled.iter().position(|f| *f), filled.iter().rposition(|f| *f)) else {
        return (None, None);
    };
    let res = (tol * BISECT_OF_TOL).max(1e-6);
    let gap = step * MIN_GAP_OF_STEP;
    let low = match jf.checked_sub(1).and_then(|j| vs.get(j)) {
        Some(&out) => vs.get(jf).and_then(|&inner| edge(inner, out, res, probe)),
        None => None,
    }
    .filter(|v| vs.get(jf).is_some_and(|inner| inner - v > gap));
    let high = vs.get(jl).and_then(|&inner| {
        let out = vs.get(jl + 1).copied().unwrap_or(vmax);
        if out - inner <= gap {
            return None;
        }
        if vs.get(jl + 1).is_none() && probe(out) {
            return Some(out); // ground right to the padded bound
        }
        edge(inner, out, res, probe).filter(|v| v - inner > gap)
    });
    (low, high)
}

/// The last offset from `inner` (ground) toward `out` (none) that
/// still finds ground, to `res`. Capped at 64 halvings: a finite
/// span reaches any sane `res` long before.
fn edge(inner: f64, out: f64, res: f64, probe: &mut impl FnMut(f64) -> bool) -> Option<f64> {
    if !(inner.is_finite() && out.is_finite()) {
        return None;
    }
    let (mut hit, mut miss) = (inner, out);
    for _ in 0..64 {
        if (miss - hit).abs() <= res {
            break;
        }
        let mid = 0.5 * (hit + miss);
        if probe(mid) {
            hit = mid;
        } else {
            miss = mid;
        }
    }
    (hit != inner).then_some(hit)
}
