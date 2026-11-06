//! Setup-sheet tool list: one row per tool with its operations, cut and
//! rapid lengths, minutes and changes, plus the total (M3.2 stage D).
//! Provenance: our own text; machine-shop setup-sheet convention,
//! behavior only - see the module doc in `setup_sheet/mod.rs`.

use flecs_ecs::prelude::World;

use super::{minutes, mm};
use crate::model;

pub(super) fn render_tool_list(world: &World, ctx: Option<&str>, out: &mut String) {
    out.push_str("TOOL LIST\n");
    out.push_str(&format!(
        "  {:<14} {:<30} {:>10} {:>10} {:>7} {:>5}\n",
        "TOOL", "OPERATIONS", "CUT mm", "RAPID mm", "MIN", "CHG"
    ));
    let rows = crate::tool_list::for_setup(world, ctx);
    if rows.is_empty() {
        out.push_str("  (no tools in this setup)\n\n");
        return;
    }
    let mut cut = 0.0f64;
    let mut rapid = 0.0f64;
    for row in &rows {
        cut += row.cut_mm;
        rapid += row.rapid_mm;
        out.push_str(&format!(
            "  {:<14} {:<30} {:>10} {:>10} {:>7} {:>5}\n",
            row.tool,
            row.ops.join(", "),
            mm(row.cut_mm),
            mm(row.rapid_mm),
            minutes(row.minutes),
            row.changes,
        ));
        match model::find_by_name(world, &row.tool).and_then(|e| model::tool_of(world, &e)) {
            Some(t) => {
                let mut geo = format!(
                    "      D {}  {}  FLUTE {}",
                    mm(t.diameter),
                    t.kind,
                    mm(t.flute_len)
                );
                if t.tip_angle != 0.0 {
                    geo.push_str(&format!("  TIP {}", mm(t.tip_angle)));
                }
                if t.pitch != 0.0 {
                    geo.push_str(&format!("  PITCH {}", mm(t.pitch)));
                }
                out.push_str(&geo);
                out.push('\n');
            }
            None => out.push_str(&format!("      (tool '{}' is no longer in the document)\n", row.tool)),
        }
    }
    // The total time speaks with tool_list's own voice (setup_minutes:
    // the sum of the rows, tool-change allowance included), so the
    // footer and the stage-C report can never disagree.
    out.push_str(&format!(
        "  TOTAL: {} tool(s), cut {} mm, rapid {} mm, {}\n\n",
        rows.len(),
        mm(cut),
        mm(rapid),
        minutes(crate::tool_list::setup_minutes(world, ctx)),
    ));
}
