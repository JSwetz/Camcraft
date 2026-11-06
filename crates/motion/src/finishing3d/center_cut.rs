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
    let path = run(fp)?;
    let r = fp.tool_diameter * 0.5;
    if center_cutting || plunge_law::first_solid_plunge(&path, r, solid).is_none() {
        return Ok(path);
    }
    if fp.ramp && fp.entry_ramp {
        plunge_law::audit(&path, false, r, solid, "the finish's entry descent")?;
        return Ok(path);
    }
    let sloped = Finish3DParams { ramp: true, entry_ramp: true, ..*fp };
    let mut path = run(&sloped)?;
    plunge_law::audit(&path, false, r, solid, "the finish's entry descent, even with ramped entries,")?;
    path.notes.push(format!("{NOT_CENTER_CUTTING} - the finish's entries ramp instead of plunging"));
    Ok(path)
}
