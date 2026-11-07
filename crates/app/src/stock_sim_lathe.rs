//! The lathe clause of the document stock (#19, M13.1 owed): which
//! Turn-family operations the field eye cuts through the revolved
//! stadium (`Stock::simulate_lathe_about`), about which spindle line,
//! and why the column VERDICT still refuses them by name. Provenance:
//! our own engines, the M5.2 E nose-centre convention and the M5.2
//! meridian law; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **The plan carries the footprint and the frame.** A turning op's
//!   moves are the insert's nose-CENTRE path (a blade's tip at its
//!   band's centre), written by the kernels in the
//!   plane of the axis `turning::Frame::about` fits to the op's own
//!   source facets. The plan re-fits that same frame from the same
//!   facets (`operation::source_facets` - `material_of`'s read, one
//!   law), so the sweep revolves about the line the generator turned
//!   about, never an assumed world Z. The field revolves about that
//!   FITTED axis; generation guarantees it is the machine spindle the
//!   lathe post turns about (world Z through the work offset - #62
//!   refuses at generation any turning op whose fitted axis is not),
//!   so the picture and the posted program turn about one line.
//! - **Each tool cuts with its own footprint (#21).** A turning insert
//!   (and a boring bar's ground nose) sweeps its nose circle - the
//!   field's revolved stadium, with meridian parity
//!   (`lathe_field_sim.rs`). A grooving/parting blade sweeps its
//!   rectangle (`width` along the axis, corners by `corner_r`), a
//!   threading insert its V of `tip_angle` cut off at the `width`
//!   crest, a form gauge its crown of `nose_radius` across `width` -
//!   each with its tip at the band's centre and its body `flute_len`
//!   outward, the reading #103's meridian rapid law
//!   (`operation::turning_rapids`) holds the same blade to. A gauge
//!   with no sane crown falls back to its band's rectangle, the
//!   envelope that never under-claims it.
//! - **The footprint is the whole modelled solid.** Only the nose
//!   stadium or the blade's footprint is swept: an insert's flanks and
//!   the holder are not
//!   (unlike the mill field, which sweeps the shank above the flute),
//!   so a flank or holder rub is the revolved verdict's at generation
//!   to refuse, not the picture's to show.
//! - **The eye cuts; the verdict refuses.** The column verdict
//!   (`simulate_document`) reads Z heights from above: a parting kerf
//!   reads as a plunge and an undercut groove as nothing - so a lathe
//!   program is not judged there, by name ([`LATHE_VERDICT_REFUSAL`]);
//!   the revolved verdict at generation governs, and the post gate
//!   shows "not judged". Rapids touch nothing on the field (RS-274 G0,
//!   every arm's law): a rapid through steel is the generation-time
//!   rapid verdict's to refuse, not the picture's to hide.

use crate::stock_sim::{LatheAxis, LatheProfile};
use crate::tools::{Tool, ToolKind};

/// The column verdict's refusal of a turning program, said once.
pub const LATHE_VERDICT_REFUSAL: &str = "a turning program is cut by the field eye about its fitted spindle, but the column verdict reads Z heights from above (a parting kerf reads as a plunge) - not judged here; the revolved verdict at generation stands";

/// What a turning op's plan carries into the field: the footprint
/// its moves sweep and the spindle line it revolves about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LathePlan {
    pub profile: LatheProfile,
    pub axis: LatheAxis,
}

impl LathePlan {
    /// The footprint's finest cutting feature, the radius the display
    /// cell law (`auto_cell`) sizes the picture by: the nose, or half
    /// a blade's band.
    #[must_use]
    pub fn feature(&self) -> f64 {
        match self.profile {
            LatheProfile::Nose { radius } => radius,
            LatheProfile::Blade { width, .. } | LatheProfile::Thread { width, .. } | LatheProfile::Form { width, .. } => 0.5 * width,
        }
    }
}

/// The footprint `tool` cuts with: `None` for a tool that is no
/// turning steel.
fn profile_of(tool: &Tool) -> Option<LatheProfile> {
    let height = tool.flute_len;
    Some(match tool.kind {
        ToolKind::Turning | ToolKind::Bore => LatheProfile::Nose { radius: tool.nose_radius },
        ToolKind::Groove => LatheProfile::Blade { width: tool.width, corner: tool.corner_r.min(0.5 * tool.width), height },
        ToolKind::Thread => LatheProfile::Thread { width: tool.width, angle: tool.tip_angle, height },
        ToolKind::Form if tool.nose_radius.is_finite() && tool.width <= 2.0 * tool.nose_radius => {
            LatheProfile::Form { width: tool.width, crown: tool.nose_radius, height }
        }
        ToolKind::Form => LatheProfile::Blade { width: tool.width, corner: 0.0, height },
        _ => return None,
    })
}

/// The lathe plan of `op`: `None` for an op outside the Turn family,
/// the plan for a turning tool, and the refusal by name for a
/// non-turning tool or for a source no axis fits.
pub(crate) fn lathe_plan(
    world: &flecs_ecs::prelude::World,
    name: &str,
    op: &crate::schema::Operation,
    tool: &Tool,
) -> Result<Option<LathePlan>, String> {
    if op.params.kind().info().family() != crate::operation::Family::Turn {
        return Ok(None);
    }
    let Some(profile) = profile_of(tool) else {
        return Err(format!(
            "{name}: a turning op cut by a {} - a milling/drilling tool on the lathe has no revolved solid the field can sweep - not simulated",
            tool.kind
        ));
    };
    // The footprint's own gate, said here by the op's name rather than
    // deep in the field walk.
    if let LatheProfile::Nose { radius } = profile
        && !(radius.is_finite() && radius > 0.0)
    {
        return Err(format!("{name}: the insert carries no sane nose radius ({radius}) - not simulated"));
    }
    let facets = crate::operation::source_facets(world, op.setup.as_deref(), &op.src)
        .map_err(|e| format!("{name}: the lathe frame needs the op's source: {e}"))?;
    let frame = crate::turning::profile::Frame::about(&facets)
        .map_err(|e| format!("{name}: no spindle fits the op's source, so its turn has no frame to cut in: {e}"))?;
    let (o, p) = (frame.point(0.0, 0.0), frame.point(0.0, 1.0));
    let axis = LatheAxis::new(o, [p.x - o.x, p.y - o.y, p.z - o.z]).map_err(|e| format!("{name}: {e}"))?;
    Ok(Some(LathePlan { profile, axis }))
}
