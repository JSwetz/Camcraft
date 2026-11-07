//! The Probe operation's generation and its protected-move law (#108):
//! op params to [`crate::probing`]'s cycle, then every stylus move
//! judged against the model - a ball that comes nearer than its radius
//! to the part anywhere before (or after) the expected contact is a
//! stylus driven through material, refused as a gouge. A probe removes
//! no stock: the sim never carves it (`stock_sim` skips probe paths).
//! Provenance: NIST RS274NGC G38.2 semantics and touch-probe practice;
//! the swept-ball distance is Ericson's segment-triangle test (the
//! sim's `dist_seg_tri`). Our own design; no vendor material consulted.

use crate::cl::Toolpath;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::probing::{probe_cycle, ProbeParams};
use crate::tools::{Tool, ToolKind};
use crate::verify::Warning;

use super::types::OpParams;

/// How much nearer than its radius the ball may come to the model
/// before the law convicts: the touch itself is tangent (exactly r),
/// so this is the facet lattice's numeric slack, not a cut allowance.
pub const STYLUS_SLACK: f64 = 0.01;

/// The cycle's numbers from the op's, with the resolved safe plane.
pub(super) fn probe_params(params: &OpParams, tool: &Tool, clearance_z: f64) -> Result<ProbeParams, GenError> {
    let OpParams::Probe {
        mode,
        dir,
        dir_y,
        target,
        radius,
        approach,
        overtravel,
        backoff,
        fast_feed,
        slow_feed,
        depth,
        inset,
        tolerance,
        set_wcs,
        ..
    } = params
    else {
        return Err(GenError::Invalid("not a probe operation".into()));
    };
    if tool.kind != ToolKind::Probe {
        return Err(GenError::Invalid(format!(
            "a probe operation mounts a touch probe, not a {}",
            tool.kind
        )));
    }
    if !(tolerance.is_finite() && *tolerance > 0.0) {
        return Err(GenError::Invalid("the probe tolerance must be positive".into()));
    }
    if set_wcs.is_some_and(|s| s > 5) {
        return Err(GenError::Invalid("the work offset to set must be one of G54..G59".into()));
    }
    Ok(ProbeParams {
        mode: *mode,
        dir: *dir,
        dir_y: *dir_y,
        target: Point3::new(target[0], target[1], target[2]),
        radius: *radius,
        ball_diameter: tool.diameter,
        approach: *approach,
        overtravel: *overtravel,
        backoff: *backoff,
        fast_feed: *fast_feed,
        slow_feed: *slow_feed,
        depth: *depth,
        inset: *inset,
        clearance_z,
    })
}

/// Generate a probe op's path and judge it against `facets` (the model).
pub(super) fn generate_probe(
    params: &OpParams,
    tool: &Tool,
    clearance_z: f64,
    facets: &[Triangle],
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let p = probe_params(params, tool, clearance_z)?;
    let path = probe_cycle(&p)?;
    let warns = stylus_hits(&path, tool.diameter * 0.5, facets);
    Ok((path, warns))
}

/// The protected-move law: every move of the stylus, the touches
/// included, keeps the ball at least its radius (less the slack) off
/// every facet. One warning per offending move.
#[must_use]
pub fn stylus_hits(path: &Toolpath, ball_r: f64, facets: &[Triangle]) -> Vec<Warning> {
    let floor = ball_r - STYLUS_SLACK;
    path.moves
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            facets
                .iter()
                .any(|t| crate::stock_sim::dist_seg_tri(m.from, m.to, t) < floor)
        })
        .map(|(at, _)| Warning::Gouge { at })
        .collect()
}
