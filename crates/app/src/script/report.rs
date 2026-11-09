//! `cam.report()` - the job on one page. Every number is read from
//! the same seams the Setup sheet draws (the tool list's time law,
//! the operation order), never recomputed here: a report that
//! invented its own arithmetic would be a second truth, and the
//! batch lane exists to avoid exactly that.

use super::fnum;
use super::api::ops_view;
use super::Env;

use crate::tool_list;

use serde_json::{json, Value};

pub(crate) fn summary(env: &Env) -> Value {
    let rows = tool_list::program(&env.world);
    let (mut minutes, mut cut, mut rapid) = (0.0f64, 0.0f64, 0.0f64);
    let tools: Vec<Value> = rows
        .iter()
        .map(|r| {
            minutes += r.minutes;
            cut += r.cut_mm;
            rapid += r.rapid_mm;
            json!({
                "tool": r.tool,
                "ops": r.ops,
                "cut_mm": fnum(r.cut_mm),
                "rapid_mm": fnum(r.rapid_mm),
                "minutes": fnum(r.minutes),
                "changes": r.changes,
            })
        })
        .collect();
    json!({
        "document": env.doc_path.as_ref().map(|p| p.display().to_string()),
        "posted": env.posted.as_ref().map(|p| p.display().to_string()),
        "operations": ops_view(env),
        "tools": tools,
        "minutes": fnum(minutes),
        "cut_mm": fnum(cut),
        "rapid_mm": fnum(rapid),
    })
}

/// `cam.post`'s answer (#388): one file reads as it always did (path,
/// ops, lines); a program over several mounts adds `files` (every path,
/// program order) and `mounts` (each file's setups, machine, post, ops,
/// lines - and its text when asked), `path` naming the first.
pub(crate) fn posted_view(env: &mut Env, files: Vec<crate::posting::PostedMount>, with_program: bool) -> Value {
    let several = files.len() > 1;
    let ops: usize = files.iter().map(|f| f.posted.ops).sum();
    let lines: usize = files.iter().map(|f| f.posted.lines).sum();
    let first = files.first();
    env.posted = first.map(|f| f.path.clone());
    let mut out = json!({
        "path": first.map(|f| f.path.display().to_string()).unwrap_or_default(),
        "ops": ops,
        "lines": lines,
    });
    if let Some(line) = first.and_then(|f| f.posted.not_judged.as_ref()) {
        out["not_judged"] = json!(line);
    }
    if !several {
        if with_program && let Some(f) = first {
            out["program"] = json!(f.posted.program);
        }
        return out;
    }
    out["files"] = json!(files.iter().map(|f| f.path.display().to_string()).collect::<Vec<_>>());
    let mounts: Vec<Value> = files
        .iter()
        .map(|f| {
            let mut m = json!({
                "path": f.path.display().to_string(),
                "setups": f.mount.setups,
                "machine": crate::setup_mount::machine_label(&f.mount.machine),
                "post": crate::posting::post_ref_label(&f.mount.post),
                "ops": f.posted.ops,
                "lines": f.posted.lines,
            });
            if with_program {
                m["program"] = json!(f.posted.program);
            }
            m
        })
        .collect();
    out["mounts"] = Value::Array(mounts);
    out
}
