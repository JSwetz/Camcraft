//! One setup's tool list as a file (#389): CSV for a spreadsheet or a
//! tool-crib import, plain text for the bench. One row per tool, in the
//! order the tools first enter the spindle ([`crate::tool_list::for_setup`],
//! the same walk the post makes): T number, name, size, kind, holder and
//! the operations using it.
//!
//! A pure read of the world - the UI picks a path and writes the string,
//! nothing enters the history (the setup-sheet law). Lengths are mm with
//! three decimals, the setup sheet's own voice. A tool the operations
//! name but the library no longer holds still gets its row (`?` number,
//! `missing` size) - the list must show what the program would load.
//!
//! Provenance: our own design; a tool list is standard shop paperwork,
//! CSV per RFC 4180 (quote a field holding a comma, quote or line break,
//! double an embedded quote). No vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::setup_sheet::mm;
use crate::tool_numbers;

/// One exported tool row, the formatted cells.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportRow {
    pub number: String,
    pub name: String,
    pub size: String,
    pub kind: String,
    pub holder: String,
    pub ops: Vec<String>,
}

const HEADER: [&str; 6] = ["T", "Tool", "Diameter mm", "Kind", "Holder", "Operations"];

/// The rows of `ctx`'s tool list (`None` - the implicit first setup).
#[must_use]
pub fn rows(world: &World, ctx: Option<&str>) -> Vec<ExportRow> {
    crate::tool_list::for_setup(world, ctx)
        .into_iter()
        .map(|row| {
            let tool = model::find_by_name(world, &row.tool).and_then(|e| model::tool_of(world, &e));
            let ops = row
                .ops
                .iter()
                .map(|key| match model::find_by_name(world, key) {
                    Some(e) => model::operation_label(world, key, &e),
                    None => key.clone(),
                })
                .collect();
            ExportRow {
                number: tool_numbers::number_of(world, &row.tool).map_or_else(|| "?".to_owned(), |n| n.to_string()),
                size: tool.as_ref().map_or_else(|| "missing".to_owned(), |t| mm(t.diameter)),
                kind: tool.as_ref().map_or_else(String::new, |t| t.kind.to_string()),
                holder: tool.as_ref().map_or_else(String::new, |t| holder(t.holder_d, t.holder_len)),
                name: row.tool,
                ops,
            }
        })
        .collect()
}

/// The holder cell: `D x L` in mm, `none` when the tool states none.
fn holder(d: f64, len: f64) -> String {
    if d.is_finite() && len.is_finite() && d > 0.0 && len > 0.0 {
        format!("{} x {}", mm(d), mm(len))
    } else {
        "none".to_owned()
    }
}

/// The setup's tool list as CSV: a header line, then one line per tool;
/// the operations share one cell, separated by `; `. CRLF line ends, as
/// RFC 4180 states them.
#[must_use]
pub fn csv(world: &World, ctx: Option<&str>) -> String {
    let mut out = String::new();
    line(&mut out, HEADER.iter().map(|h| (*h).to_owned()).collect());
    for r in rows(world, ctx) {
        let ops = r.ops.join("; ");
        line(&mut out, vec![r.number, r.name, r.size, r.kind, r.holder, ops]);
    }
    out
}

fn line(out: &mut String, cells: Vec<String>) {
    let cells: Vec<String> = cells.iter().map(|c| cell(c)).collect();
    out.push_str(&cells.join(","));
    out.push_str("\r\n");
}

/// One RFC 4180 field.
fn cell(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_owned()
    }
}

/// The setup's tool list as plain text: a title naming the setup, then
/// fixed columns, one tool per line.
#[must_use]
pub fn text(world: &World, ctx: Option<&str>) -> String {
    let title = crate::setup::effective(world, ctx).unwrap_or_else(|| "Setup".to_owned());
    let mut out = format!("TOOL LIST - {title}\n");
    let rows = rows(world, ctx);
    if rows.is_empty() {
        out.push_str("(no operation in this setup uses a tool)\n");
        return out;
    }
    out.push_str(&format!(
        "{:<4} {:<16} {:>12} {:<16} {:<20} {}\n",
        HEADER[0], HEADER[1], HEADER[2], HEADER[3], HEADER[4], HEADER[5]
    ));
    for r in &rows {
        out.push_str(&format!(
            "{:<4} {:<16} {:>12} {:<16} {:<20} {}\n",
            r.number,
            r.name,
            r.size,
            r.kind,
            r.holder,
            r.ops.join(", ")
        ));
    }
    out.push_str(&format!("{} tool(s)\n", rows.len()));
    out
}

/// The export for a file path: CSV for a `.csv` extension (any case),
/// plain text otherwise.
#[must_use]
pub fn for_path(world: &World, ctx: Option<&str>, path: &std::path::Path) -> String {
    let is_csv = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if is_csv {
        csv(world, ctx)
    } else {
        text(world, ctx)
    }
}
