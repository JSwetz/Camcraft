//! The centre-cutting law on the 3-D finishes (M11.3 owed (5)): a tool
//! that is not centre-cutting never feeds axially into solid on a
//! finish's entry descent or between-pass drop. The finishes carry no
//! approach, so the law judges the finished path (`plunge_law::audit`);
//! the fallback is the finishes' own sloped entries - the certified
//! diagonal chains (`ramp`) and the ramped first contacts
//! (`entry_ramp`) - and where those still leave a plunge, the operation
//! is refused by name.
//!
//! Provenance: the centre-cutting / non-centre-cutting end mill
//! distinction of machining handbooks (such a tool may ramp into
//! material but not plunge); the retry-then-refuse ladder is this
//! crate's own. No vendor material consulted.

use super::params::Finish3DParams;
use crate::approach::NOT_CENTER_CUTTING;
use crate::cl::Toolpath;
use crate::machining2d::GenError;
use crate::plunge_law::{self, Solid};

/// Run the finish `run` under the centre-cutting law. A centre-cutting
/// tool's path is returned untouched (byte-identical: `run` is called
/// once). Otherwise a path that plunges into `solid` is regenerated
/// with the sloped entries forced on; if that path still plunges, the
/// operation is refused by name.
pub fn guard<F>(center_cutting: bool, solid: &Solid, fp: &Finish3DParams, run: F) -> Result<Toolpath, GenError>
where
    F: Fn(&Finish3DParams) -> Result<Toolpath, GenError>,
{
    guard_as("the finish", center_cutting, solid, fp, run)
}

/// [`guard`] for a kind built on finish params that names itself in
/// the refusal (M11.3 owed: the multiaxis kinds - 5-Axis Contour,
/// Parallel and Flow, the swarf and rotary twins - whose posed descents
/// the law reads along the tool's own axis, `plunge_law::posed_axial`).
/// `kind` reads as the subject of "{kind}'s entry descent".
pub fn guard_as<F>(
    kind: &str,
    center_cutting: bool,
    solid: &Solid,
    fp: &Finish3DParams,
    run: F,
) -> Result<Toolpath, GenError>
where
    F: Fn(&Finish3DParams) -> Result<Toolpath, GenError>,
{
    guard_shaped(kind, center_cutting, solid, false, fp, run)
}

/// [`guard_as`] with the tool's shape said: `ball` says the tool is a
/// ball, whose posed re-entries into an already-cut sphere are no
/// plunge (`plunge_law::reenters_cut_sphere`, #17).
pub fn guard_shaped<F>(
    kind: &str,
    center_cutting: bool,
    solid: &Solid,
    ball: bool,
    fp: &Finish3DParams,
    run: F,
) -> Result<Toolpath, GenError>
where
    F: Fn(&Finish3DParams) -> Result<Toolpath, GenError>,
{
    let path = run(fp)?;
    let r = fp.tool_diameter * 0.5;
    if center_cutting || plunge_law::first_solid_plunge_as(&path, r, solid, ball).is_none() {
        return Ok(path);
    }
    if fp.ramp && fp.entry_ramp {
        plunge_law::audit_as(&path, false, r, solid, ball, &format!("{kind}'s entry descent"))?;
        return Ok(path);
    }
    let sloped = Finish3DParams { ramp: true, entry_ramp: true, ..*fp };
    let mut path = run(&sloped)?;
    plunge_law::audit_as(&path, false, r, solid, ball, &format!("{kind}'s entry descent, even with ramped entries,"))?;
    path.notes.push(format!("{NOT_CENTER_CUTTING} - {kind}'s entries ramp instead of plunging"));
    Ok(path)
}
