//! The lathe clause of the document stock (#19, M13.1 owed): which
//! Turn-family operations the field eye cuts through the revolved
//! stadium (`Stock::simulate_lathe_about`), about which spindle line,
//! and why the column VERDICT still refuses them by name. Provenance:
//! our own engines, the M5.2 E nose-centre convention and the M5.2
//! meridian law; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **The plan carries the nose and the frame.** A turning op's moves
//!   are the insert's nose-CENTRE path, written by the kernels in the
//!   plane of the axis `turning::Frame::about` fits to the op's own
//!   source facets. The plan re-fits that same frame from the same
//!   facets (`operation::source_facets` - `material_of`'s read, one
//!   law), so the sweep revolves about the line the generator turned
//!   about, never an assumed world Z.
//! - **The nose stadium is the insert's solid; a blade's is not.**
//!   A turning insert (and a boring bar's ground nose) sweeps its nose
//!   circle - exactly what the field's revolved stadium cuts, with
//!   meridian parity (`lathe_field_sim.rs`). A grooving/parting blade,
//!   a threading V and a form gauge cut a meridian footprint the nose
//!   circle under-claims; they refuse by name ([`LATHE_BLADE_REFUSAL`],
//!   #21) instead of drawing a ghost the eye would call the part.
//! - **The eye cuts; the verdict refuses.** The column verdict
//!   (`simulate_document`) reads Z heights from above: a parting kerf
//!   reads as a plunge and an undercut groove as nothing - so a lathe
//!   program is not judged there, by name ([`LATHE_VERDICT_REFUSAL`]);
//!   the revolved verdict at generation governs, and the post gate
//!   shows "not judged". Rapids touch nothing on the field (RS-274 G0,
//!   every arm's law): a rapid through steel is the generation-time
//!   rapid verdict's to refuse, not the picture's to hide.

use crate::stock_sim::{LatheAxis, LATHE_COLUMN_REFUSAL};
use crate::tools::{Tool, ToolKind};

/// The column verdict's refusal of a turning program, said once.
pub const LATHE_VERDICT_REFUSAL: &str = "a turning program is cut by the field eye about its fitted spindle, but the column verdict reads Z heights from above (a parting kerf reads as a plunge) - not judged here; the revolved verdict at generation stands";

/// The refusal of a turning tool whose meridian footprint is not the
/// nose circle, said once (#21 owes the blade-profile sweep).
pub const LATHE_BLADE_REFUSAL: &str = "a grooving/parting blade, threading insert or form gauge cuts a meridian footprint the nose-circle sweep cannot model (the blade-profile sweep is owed, #21) - not simulated, rather than drawn as a ghost";

/// What a turning op's plan carries into the field: the nose radius
/// its stadium dilates by and the spindle line it revolves about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LathePlan {
    pub nose: f64,
    pub axis: LatheAxis,
}

/// The lathe plan of `op`: `None` for an op outside the Turn family,
/// the plan for a nose tool, and the refusal by name for a blade or
/// for a source no axis fits.
pub(crate) fn lathe_plan(
    world: &flecs_ecs::prelude::World,
    name: &str,
    op: &crate::schema::Operation,
    tool: &Tool,
) -> Result<Option<LathePlan>, String> {
    if op.params.kind().info().family() != crate::operation::Family::Turn {
        return Ok(None);
    }
    match tool.kind {
        ToolKind::Turning | ToolKind::Bore => {}
        ToolKind::Groove | ToolKind::Thread | ToolKind::Form => {
            return Err(format!("{name}: {LATHE_BLADE_REFUSAL}"));
        }
        k => {
            return Err(format!("{name}: a turning op cut by a {k} has no lathe solid - {LATHE_COLUMN_REFUSAL}"));
        }
    }
    let nose = tool.nose_radius;
    if !(nose.is_finite() && nose > 0.0) {
        return Err(format!("{name}: the insert carries no sane nose radius ({nose}) - not simulated"));
    }
    let facets = crate::operation::source_facets(world, op.setup.as_deref(), &op.src)
        .map_err(|e| format!("{name}: the lathe frame needs the op's source: {e}"))?;
    let frame = crate::turning::profile::Frame::about(&facets)
        .map_err(|e| format!("{name}: no spindle fits the op's source, so its turn has no frame to cut in: {e}"))?;
    let (o, p) = (frame.point(0.0, 0.0), frame.point(0.0, 1.0));
    let axis = LatheAxis::new(o, [p.x - o.x, p.y - o.y, p.z - o.z]).map_err(|e| format!("{name}: {e}"))?;
    Ok(Some(LathePlan { nose, axis }))
}
