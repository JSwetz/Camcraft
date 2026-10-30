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
