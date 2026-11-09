//! Edge blends (round, chamfer) through solidcore: picks resolved to edges by
//! sampling each edge's curve, then the port's `fillet_edges` (SmApiFilletEdges)
//! and a result law around it. The pick distance is sampling plus a
//! golden-section refinement (Kiefer 1953).
//! Provenance: calls solidcore 1.0.1's public fillet API as documented in its
//! rustdoc (blend::fillet_executive::api) and its public tests (the
//! nearest-edge pick follows tests/issue_398_shell_box_rounds_chamfer_fillet.rs);
//! the guards are our own design.

use solidcore::EdgeId;
use solidcore::blend::fillet_executive::api::fillet_edges;
use solidcore::math::Point3;
use solidcore::topology::{Brep, Edge};

use crate::solidcore_impl::{SolidcoreKernel, VOLUME_ACCURACY, checked, failed};
use crate::{Blend, Kernel, KernelError, Measure, P3, PICK_TOL, finite_point, positive_len};

/// Samples per edge for the nearest-edge search.
const SAMPLES: u32 = 256;
/// Golden-section steps after sampling (0.618^60 of one sample interval).
const REFINE_STEPS: u32 = 60;

/// SmApiFilletEdges cross-section codes (0 linear, any other circular).
const XSECT_LINEAR: u32 = 0;
const XSECT_CIRCULAR: u32 = 1;

fn invalid(detail: String) -> KernelError {
    KernelError::InvalidInput {
        op: "blend_edges",
        detail,
    }
}

/// Distance from `p` to the nearest sample of the edge's curve, `None` for
/// an edge without a curve or a curve that does not evaluate.
fn edge_distance(k: &solidcore::Kernel, e: &Edge, p: Point3) -> Option<f64> {
    let c = e.curve.as_ref()?;
    let (t0, t1) = (e.interval.min(), e.interval.max());
    let dist = |t: f64| -> Option<f64> {
        let q = c.evaluate_point(k, t).ok()?;
        Some(((q.x - p.x).powi(2) + (q.y - p.y).powi(2) + (q.z - p.z).powi(2)).sqrt())
    };
    let step = (t1 - t0) / f64::from(SAMPLES);
    let mut best = (f64::INFINITY, t0);
    for i in 0..=SAMPLES {
        let t = t0 + step * f64::from(i);
        let d = dist(t)?;
        if d < best.0 {
            best = (d, t);
        }
    }
    // Golden-section refinement over the bracketing sample interval: the
    // distance along a smooth curve is unimodal between two samples this
    // close (Kiefer 1953; any numerical-methods text).
    let (mut a, mut b) = ((best.1 - step).max(t0), (best.1 + step).min(t1));
    let g = (5.0_f64.sqrt() - 1.0) / 2.0;
    for _ in 0..REFINE_STEPS {
        let (x1, x2) = (b - g * (b - a), a + g * (b - a));
        if dist(x1)? < dist(x2)? {
            b = x2;
        } else {
            a = x1;
        }
    }
    let refined = dist(0.5 * (a + b))?.min(best.0);
    refined.is_finite().then_some(refined)
}

/// The edge each pick names: the nearest one, which must lie within
/// [`PICK_TOL`]. Refused: a pick within tolerance of two edges (a vertex, or
/// a rim point on a cylinder's seam - which one was meant is a guess), and a
/// pick naming an edge another pick already named.
fn resolve_picks(
    k: &solidcore::Kernel,
    b: &Brep,
    picks: &[P3],
) -> Result<Vec<EdgeId>, KernelError> {
    if picks.is_empty() {
        return Err(invalid("no edge picks".into()));
    }
    let mut out: Vec<EdgeId> = Vec::with_capacity(picks.len());
    for &pick in picks {
        finite_point("blend_edges", pick)?;
        let p = Point3::new(pick[0], pick[1], pick[2]);
        let mut near: Vec<(f64, EdgeId)> = b
            .edges()
            .iter()
            .filter_map(|(id, e)| edge_distance(k, e, p).map(|d| (d, id)))
            .collect();
        near.sort_by(|a, c| a.0.total_cmp(&c.0));
        if let Some((d2, _)) = near.get(1)
            && *d2 <= PICK_TOL
        {
            return Err(invalid(format!(
                "pick {pick:?} is ambiguous: it lies on more than one edge (second at {d2:.3e})"
            )));
        }
        match near.first().copied() {
            Some((d, id)) if d <= PICK_TOL => {
                if out.contains(&id) {
                    return Err(invalid(format!(
                        "pick {pick:?} names an edge already picked"
                    )));
                }
                out.push(id);
            }
            Some((d, _)) => {
                return Err(invalid(format!(
                    "pick {pick:?} is {d:.3e} from the nearest edge (tolerance {PICK_TOL})"
                )));
            }
            None => return Err(invalid(format!("pick {pick:?}: the solid has no edges"))),
        }
    }
    Ok(out)
}

/// The result law: a closed manifold solid (`checked`), inside the source's
/// bounding box (a blend only trims or fills between the faces it joins),
/// and a volume that actually moved (a blend that changed nothing did not
/// happen). A backstop, not a proof: a blend that ate a neighbouring face
/// but stayed inside the box passes this law - the bench's closed forms are
/// what prove the port.
fn result_law(before: &Measure, after: &Measure, b: Brep, tol: f64) -> Result<Brep, KernelError> {
    let outside =
        (0..3).any(|i| after.min[i] < before.min[i] - tol || after.max[i] > before.max[i] + tol);
    // Ten times the integrator's relative accuracy: below that a change is noise.
    let unmoved = (after.volume - before.volume).abs() <= 10.0 * VOLUME_ACCURACY * before.volume;
    if outside || unmoved {
        Err(KernelError::InvalidResult {
            kernel: "solidcore",
            op: "blend_edges",
            detail: format!(
                "blend left the source envelope or did not change the volume: {before:?} -> {after:?}"
            ),
        })
    } else {
        Ok(b)
    }
}

/// [`Kernel::blend_edges`] for solidcore.
pub(crate) fn blend_edges(
    sk: &mut SolidcoreKernel,
    mut s: Brep,
    picks: &[P3],
    radius: f64,
    kind: Blend,
) -> Result<Brep, KernelError> {
    positive_len("blend_edges", "radius", radius)?;
    let before = sk.measure(&mut s)?;
    let thinnest = (0..3)
        .map(|i| before.max[i] - before.min[i])
        .fold(f64::INFINITY, f64::min);
    if radius * 2.0 >= thinnest {
        return Err(invalid(format!(
            "radius {radius} does not fit a solid whose thinnest extent is {thinnest}"
        )));
    }
    let edges = resolve_picks(&sk.k, &s, picks)?;
    let xsect = match kind {
        Blend::Round => XSECT_CIRCULAR,
        Blend::Chamfer => XSECT_LINEAR,
    };
    fillet_edges(&mut sk.k, &mut s, &edges, radius, xsect, 1, 1.0)
        .map_err(|e| failed("blend_edges", &e))?;
    let mut r = checked("blend_edges", s)?;
    let after = sk.measure(&mut r)?;
    result_law(&before, &after, r, 1e-9 * thinnest.max(1.0))
}
