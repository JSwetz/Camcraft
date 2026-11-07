//! One NC file per operation (#111): the program cut into standalone
//! programs, one per operation, so an operator can rerun or prove out
//! a single op on its own.
//!
//! Provenance: our own design (issue #111, RECOMMENDED.md); the gate is
//! `posting`'s own, and each file's shape (header, tool change, safe
//! start, end) is the dialect's whole-program shape from the shipped
//! `.cps` scripts - public RS-274 / ISO 6983 behaviour only, per the
//! Clean-Room Protocol.
//!
//! The law: every file is judged before any is written. The program
//! gate (freshness, tools, gouge, limits, fixtures, probe) runs once
//! over the same op list, each op's own post must come back with no
//! counted warning, and only then are the files written - into a
//! scratch folder first, then moved into place. A refusal anywhere
//! writes nothing: never a folder with ops 1 and 3 but not 2.

use super::{machine_value, op_streams, script_source, verification_report, viewer_map, Posted};
use crate::post::{cnc, engine};
use flecs_ecs::prelude::World;
use std::path::{Path, PathBuf};

/// One written program of a per-operation post.
#[derive(Debug)]
pub struct PostedFile {
    /// Where it was written.
    pub path: PathBuf,
    /// The operation it carries (entity name).
    pub op: String,
    /// The program as posted, the same evidence a whole-job post gives.
    pub posted: Posted,
}

/// Longest sanitised operation name in a file name: a file system
/// limit is never the reason a post fails.
const NAME_MAX: usize = 48;

/// A name cut to the portable file-name characters (ASCII letters,
/// digits, `-`, `_`; everything else becomes `_`, runs collapse, ends
/// are trimmed). Empty after that is `fallback`.
fn sanitise(name: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' };
        if c == '_' && out.ends_with('_') {
            continue;
        }
        out.push(c);
    }
    let cut: String = out.trim_matches('_').chars().take(NAME_MAX).collect();
    let cut = cut.trim_end_matches('_');
    if cut.is_empty() { fallback.to_owned() } else { cut.to_owned() }
}

/// The file stem of operation `index` (1-based, program order):
/// `<program>_<nn>_<op>`, both names sanitised, `nn` at least two
/// digits so a folder listing sorts in program order.
#[must_use]
pub fn file_stem(program: &str, index: usize, op_name: &str) -> String {
    format!("{}_{index:02}_{}", sanitise(program, "program"), sanitise(op_name, "op"))
}

/// Post every program operation into `dir` as its own standalone
/// program, through the document's post. See [`post_per_op_script`].
pub fn post_per_op(world: &World, dir: &Path, program: &str) -> Result<Vec<PostedFile>, String> {
    let (_label, cps) = script_source(world)?;
    post_per_op_script(world, dir, program, &cps)
}

/// [`post_per_op`] at a given script. Suppressed ops write no file
/// (`program_ops` leaves them out). Each op's stream posts alone, as
/// program number 1, 2, 3... (the O-word a numbering dialect writes),
/// so each file opens with the header and units, calls its own tool,
/// states its work offset, spindle and coolant, and ends with the
/// dialect's spindle stop, coolant off, home and program end.
pub fn post_per_op_script(
    world: &World,
    dir: &Path,
    program: &str,
    cps: &str,
) -> Result<Vec<PostedFile>, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let streams = op_streams(world)?;
    let report = verification_report(world);
    if !report.problems.is_empty() {
        return Err(format!("post blocked - {}", report.problems.join("; ")));
    }
    let ext = engine::extension_of(cps).map_err(|e| format!("post engine: {e}"))?;
    let ext = sanitise(&ext, "nc");
    let mch = machine_value(world);
    let mut files = Vec::with_capacity(streams.len());
    for (k, (name, records)) in streams.iter().enumerate() {
        let index = k + 1;
        let number = u32::try_from(index).map_err(|_| "too many operations to number".to_owned())?;
        let out = engine::run_numbered(cps, &mch, &cnc::serialize(records), number)
            .map_err(|e| format!("{name}: post engine: {e}"))?;
        if !out.warnings.is_empty() {
            return Err(format!(
                "post blocked - {name}: {} warning(s): {}",
                out.warnings.len(),
                out.warnings.join("; ")
            ));
        }
        let line_pos = viewer_map(records, &out);
        files.push(PostedFile {
            path: dir.join(format!("{}.{ext}", file_stem(program, index, name))),
            op: name.clone(),
            posted: Posted {
                ops: 1,
                lines: out.program.lines().count(),
                program: out.program,
                line_pos,
                blocks: out.blocks,
                not_judged: report.not_judged.clone(),
            },
        });
    }
    write_all(dir, program, &files)?;
    Ok(files)
}

/// Write every file or none: all into a scratch folder beside the
/// targets, then each renamed into place. A failed rename takes back
/// the ones already moved, so a refusal leaves no partial set.
fn write_all(dir: &Path, program: &str, files: &[PostedFile]) -> Result<(), String> {
    let scratch = dir.join(format!(".{}.camcraft-post-{}", sanitise(program, "program"), std::process::id()));
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).map_err(|e| format!("cannot clear {}: {e}", scratch.display()))?;
    }
    std::fs::create_dir(&scratch).map_err(|e| format!("cannot write into {}: {e}", dir.display()))?;
    let staged = files.iter().try_for_each(|f| {
        let name = f.path.file_name().ok_or_else(|| format!("{}: no file name", f.path.display()))?;
        std::fs::write(scratch.join(name), f.posted.program.as_bytes())
            .map_err(|e| format!("cannot write {}: {e}", f.path.display()))
    });
    if let Err(err) = staged {
        let _ = std::fs::remove_dir_all(&scratch);
        return Err(err);
    }
    let mut moved: Vec<&Path> = Vec::with_capacity(files.len());
    for f in files {
        let Some(name) = f.path.file_name() else { continue };
        if let Err(e) = std::fs::rename(scratch.join(name), &f.path) {
            for done in &moved {
                let _ = std::fs::remove_file(done); // never a partial set
            }
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(format!("cannot write {}: {e}", f.path.display()));
        }
        moved.push(&f.path);
    }
    // Every file is in place: an empty scratch folder left behind is
    // untidy, not a wrong program - never a refusal after the write.
    let _ = std::fs::remove_dir(&scratch);
    Ok(())
}
