//! The lathe control point at post time (#630): which operations post
//! a nose-centre path, which nose contacts their cut needs, whether the
//! tool's orientation can present them, and the converted path.
//!
//! The toolpath the generator caches stays the nose-centre truth - the
//! verifier, the stock sim and the gouge laws judge it unchanged. Only
//! the stream handed to the post is converted, here, once, for every
//! lathe script: [`crate::post::nose_tip`] does the geometry.
//!
//! The contacts per operation family (the module law of
//! `camcraft_tools::orientation`): an outside turn rides diameters
//! from the -X side of the nose; facing cuts the free-end face from
//! its -Z side and runs inward, leading with -X; boring rides the
//! hole wall from the +X side. An orientation whose nose cannot
//! present every contact is refused by name, listing the ones that
//! fit - never posted as a cut that rubs or a tip touched off on the
//! wrong corner. Grooving, parting, threading and form gauges are not
//! nose-centre paths (a blade's band, a V, a crown) and are posted as
//! they always were.
//!
//! Provenance: LinuxCNC Lathe User Information (control point and
//! tool orientation, public docs); behaviour of PathPilot's own
//! orientation refusal, read as a syntax/behaviour reference only.
//! Our own design otherwise.

use std::borrow::Cow;

use crate::cl::Toolpath;
use crate::operation::OpParams;
use crate::orientation::{self, Contact};
use crate::tools::{Tool, ToolKind};

/// The nose contacts an operation's cut needs, or None when the
/// operation does not post a nose-centre path at all.
#[must_use]
pub fn nose_contacts(params: &OpParams, tool: &Tool) -> Option<&'static [Contact]> {
    if !matches!(tool.kind, ToolKind::Turning | ToolKind::Bore) {
        return None;
    }
    match params {
        OpParams::Facing { .. } => Some(&[Contact::MinusZ, Contact::MinusX]),
        OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        | OpParams::MultiPassTurning { .. } => Some(&[Contact::MinusX]),
        OpParams::BoringTurning { .. } => Some(&[Contact::PlusX]),
        _ => None,
    }
}

/// The orientation `tool` posts `params` with, or a refusal by name
/// when its nose cannot present the cut's contacts. Ok(None) for an
/// operation that posts no nose centre.
pub fn checked_orientation(params: &OpParams, tool_name: &str, tool: &Tool) -> Result<Option<u8>, String> {
    let Some(need) = nose_contacts(params, tool) else { return Ok(None) };
    let Some(q) = tool.lathe_orientation().filter(|q| orientation::is_valid(*q)) else {
        return Err(format!("tool '{tool_name}' states no lathe orientation (1-9) - set it in the tool editor"));
    };
    if need.iter().all(|c| orientation::covers(q, *c)) {
        return Ok(Some(q));
    }
    let needs: Vec<String> = need.iter().map(ToString::to_string).collect();
    let fit: Vec<String> = orientation::fitting(need).iter().map(ToString::to_string).collect();
    Err(format!(
        "a {} cut needs the {} nose contact{}, which call for orientation [{}]; tool '{tool_name}' is orientation {q} - \
         set the tool's orientation to match how it is mounted, or pick a tool that is",
        params.kind(),
        needs.join(" and "),
        if need.len() > 1 { "s" } else { "" },
        fit.join(", "),
    ))
}

/// The path the post is handed, and the orientation it was converted
/// for: the control-point path for a nose-centre operation, the cached
/// path itself for every other one.
pub fn posted_path<'a>(
    params: &OpParams,
    tool_name: &str,
    tool: &Tool,
    path: &'a Toolpath,
    origin: [f64; 3],
) -> Result<(Cow<'a, Toolpath>, Option<u8>), String> {
    let Some(q) = checked_orientation(params, tool_name, tool)? else {
        return Ok((Cow::Borrowed(path), None));
    };
    let (ox, oz) = orientation::tip_offset(q).unwrap_or((0.0, 0.0));
    let r = tool.nose_radius;
    if !r.is_finite() || r < 0.0 {
        return Err(format!("tool '{tool_name}' has no usable nose radius"));
    }
    let posted = crate::post::nose_tip::control_point_path(path, (r * ox, r * oz), origin)
        .map_err(|e| format!("the control point for orientation {q}: {e}"))?;
    Ok((Cow::Owned(posted), Some(q)))
}
