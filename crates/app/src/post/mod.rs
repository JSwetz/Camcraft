//! Post processing: the engine core of our CPS architecture.
//!
//! Provenance: three file roles and a lifecycle-hook architecture
//! specced from Autodesk's published post-processor documentation
//! (behavior only, never text, per the Clean-Room Protocol) - the
//! `.cps` script, the `.mch` machine, and our own intermediate, all
//! consumed by this Rust engine (a from-scratch boa_engine host; no
//! vendor binary or code is involved). Formats:
//! `docs/CNC-INTERMEDIATE.md`.

pub mod cnc;
pub mod engine;
pub mod js;
pub mod machine;

pub use engine::{LineInfo, PostOutput};

use crate::cl::Toolpath;
use cnc::{CycleRec, MoveRec, Record};
use serde_json::{json, Value};

/// Engine failures, all of them clean refusals.
#[derive(Debug, thiserror::Error)]
pub enum PostError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("intermediate: {0}")]
    Stream(#[from] cnc::CncError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("post script: {0}")]
    Script(String),
}

/// Post a stream text with one script (the CLI's whole job, and the
/// in-app gate's core call).
pub fn post(cps: &str, mch: &Value, stream: &str) -> Result<PostOutput, PostError> {
    engine::run(cps, mch, stream)
}

/// Everything a `Toolpath` needs alongside it to become a stream -
/// the op's resolved context. Built by callers from the world (tool
/// library rank, resolved heights, WCS slot); pure data here, so the
/// dump and its tests never touch ECS.
#[derive(Clone, Debug)]
pub struct Dump<'a> {
    pub op_name: &'a str,
    /// 1-based rank among named tools (our tool-number law).
    pub tool_number: u32,
    pub tool_name: &'a str,
    pub tool_diameter: f64,
    pub rpm: f64,
    pub clearance_z: f64,
    pub retract_z: f64,
    pub top_z: f64,
    pub wcs: &'a str,
    pub origin: [f64; 3],
    /// Stock bounds in mm: left, right, front, back (XY), plus top Z.
    pub stock: [f64; 4],
    pub stock_top_z: f64,
    /// Verification warnings attached to the path (the gate's
    /// evidence, riding the stream).
    pub warnings: Vec<String>,
}

impl<'a> Default for Dump<'a> {
    fn default() -> Self {
        Self {
            op_name: "",
            tool_number: 0,
            tool_name: "",
            tool_diameter: 0.0,
            rpm: 0.0,
            clearance_z: 0.0,
            retract_z: 0.0,
            top_z: 0.0,
            wcs: "G54",
            origin: [0.0; 3],
            stock: [0.0; 4],
            stock_top_z: 0.0,
            warnings: Vec::new(),
        }
    }
}

/// The generator side: a toolpath plus its resolved context, as
/// records in driver order. Cycles are interleaved before their
/// group's first move, so the stream reads in program order whether
/// it came from here or from a legacy `.cl` golden (whose trailing
/// `; CYCLE` markers `engine::run` normalizes the same way).
#[must_use]
pub fn dump(t: &Toolpath, d: &Dump) -> Vec<Record> {
    let mut out = Vec::new();
    if let Some(p) = t.start() {
        out.push(Record::Param { name: "StartX".into(), value: json!(p.x) });
        out.push(Record::Param { name: "StartY".into(), value: json!(p.y) });
        out.push(Record::Param { name: "StartZ".into(), value: json!(p.z) });
    }
    out.push(Record::Param { name: "OperationName".into(), value: json!(d.op_name) });
    out.push(Record::Param { name: "ToolDiameter".into(), value: json!(d.tool_diameter) });
    out.push(Record::Param { name: "SpindleSpeed".into(), value: json!(d.rpm) });
    out.push(Record::Param { name: "WorkOffset".into(), value: json!(d.wcs) });
    out.push(Record::Param { name: "OriginX".into(), value: json!(d.origin[0]) });
    out.push(Record::Param { name: "OriginY".into(), value: json!(d.origin[1]) });
    out.push(Record::Param { name: "OriginZ".into(), value: json!(d.origin[2]) });
    out.push(Record::Param { name: "ClearancePlane".into(), value: json!(d.clearance_z) });
    out.push(Record::Param { name: "RetractPlane".into(), value: json!(d.retract_z) });
    out.push(Record::Param { name: "TopOfStock".into(), value: json!(d.top_z) });
    out.push(Record::Param { name: "StockXYLeft".into(), value: json!(d.stock[0]) });
    out.push(Record::Param { name: "StockXYRight".into(), value: json!(d.stock[1]) });
    out.push(Record::Param { name: "StockXYFront".into(), value: json!(d.stock[2]) });
    out.push(Record::Param { name: "StockXYBack".into(), value: json!(d.stock[3]) });
    out.push(Record::Param { name: "StockZTop".into(), value: json!(d.stock_top_z) });
    for w in &d.warnings {
        out.push(Record::Warn { text: w.clone() });
    }
    if d.tool_number > 0 && !d.tool_name.is_empty() {
        out.push(Record::Tool { number: d.tool_number, name: d.tool_name.into() });
    }
    out.push(Record::Section { begin: true });

    // Hole groups lead with their cycle record: bucket cycles by the
    // move index they anchor at.
    let mut owned: Vec<CycleRec> = t
        .cycles
        .iter()
        .map(|c| CycleRec {
            kind: c.kind,
            x: c.xy.x,
            y: c.xy.y,
            r_z: c.r_z,
            bottom_z: c.bottom_z,
            entry_z: c.entry_z,
            peck: c.peck,
            dwell: c.dwell,
            first_move: c.first_move,
        })
        .collect();
    owned.sort_by_key(|c| c.first_move);
    let cycles: Vec<&CycleRec> = owned.iter().collect();

    let mut ci = 0usize;
    for (mi, m) in t.moves.iter().enumerate() {
        while ci < cycles.len() && cycles[ci].first_move <= mi {
            out.push(Record::Cycle(cycles[ci].clone()));
            ci += 1;
        }
        out.push(Record::Move(MoveRec {
            kind: m.kind,
            x: m.to.x,
            y: m.to.y,
            z: m.to.z,
            feed: m.feed,
            center: m.arc.map(|a| (a.center.x, a.center.y)),
            ccw: m.arc.map(|a| a.ccw).unwrap_or(false),
        }));
    }
    for c in &cycles[ci..] {
        out.push(Record::Cycle((*c).clone()));
    }
    out.push(Record::Section { begin: false });
    out
}

/// Normalize any parsed stream to program order: move cycle records
/// to just before their `AT`-anchored move. Idempotent on `dump`
/// output; the fix for legacy goldens that append their markers.
#[must_use]
pub fn normalize_program_order(records: Vec<Record>) -> Vec<Record> {
    let has_cycles = records.iter().any(|r| matches!(r, Record::Cycle(_)));
    if !has_cycles {
        return records;
    }
    let mut cycles: Vec<CycleRec> = records
        .iter()
        .filter_map(|r| match r {
            Record::Cycle(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    cycles.sort_by_key(|c| c.first_move); // stable: holes keep stream order
    let mut out = Vec::with_capacity(records.len());
    let mut mi = 0usize;
    let mut ci = 0usize;
    for r in &records {
        if matches!(r, Record::Cycle(_)) {
            continue;
        }
        if matches!(r, Record::Move(_)) {
            while ci < cycles.len() && cycles[ci].first_move <= mi {
                out.push(Record::Cycle(cycles[ci].clone()));
                ci += 1;
            }
            mi += 1;
        }
        out.push(r.clone());
    }
    for c in &cycles[ci..] {
        out.push(Record::Cycle(c.clone()));
    }
    out
}
