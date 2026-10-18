//! The post driver: records in, hooks out, lines back.
//!
//! Provenance: the call order below is the lifecycle contract of the
//! published Fusion 360 post architecture (behavior only, per the
//! Clean-Room Protocol); this Rust is ours, and so is the intermediate
//! it reads. The control dialect lives in the script, never here.

use super::cnc::{self, Record};
use super::js::{is_false, Script};
use super::machine;
use super::PostError;
use crate::cl::MoveKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// One program line, and which stream record wrote it (None for lines
/// outside any record - the header block a post emits in `onOpen`).
#[derive(Clone, Debug, PartialEq)]
pub struct LineInfo {
    pub line: usize,
    pub record: Option<usize>,
}

/// The finished program plus the evidence a reviewer needs.
#[derive(Clone, Debug)]
pub struct PostOutput {
    /// The G-code, one line per entry, newline-terminated.
    pub program: String,
    /// Program line -> stream record, the viewer's sync map (the
    /// record's index doubles as the move index for motion records).
    pub line_map: Vec<LineInfo>,
    /// `WARN` records plus every `message()` the script logged - the
    /// validation gate's counted warnings, in file order.
    pub warnings: Vec<String>,
    /// Records the script swallowed by returning false from
    /// `onCommand` (a post filtering coolant-only records is legal;
    /// silent loss is not - so it is counted).
    pub swallowed: usize,
}

/// Post a stream text with one script. The machine value is the raw
/// `.mch` JSON (flattened here); pass `Value::Null` for none.
pub fn run(cps: &str, mch: &Value, stream: &str) -> Result<PostOutput, PostError> {
    let records = super::normalize_program_order(cnc::parse(stream)?);
    let mut s = Script::load(cps)?;
    s.set_machine(&machine::script_props(mch))?;
    s.call("onOpen", "")?;

    let mut warnings = Vec::new();
    let mut base: BTreeMap<String, Value> = BTreeMap::new();
    let mut pos = (0.0f64, 0.0f64, 0.0f64);
    let mut line_map: Vec<LineInfo> = Vec::new();
    let mut swallowed = 0usize;

    for (i, r) in records.iter().enumerate() {
        let before = s.out_len()?;
        match r {
            Record::Param { name, value } => {
                base.insert(name.clone(), value.clone());
                match name.as_str() {
                    "StartX" => pos.0 = value.as_f64().unwrap_or(0.0),
                    "StartY" => pos.1 = value.as_f64().unwrap_or(0.0),
                    "StartZ" => pos.2 = value.as_f64().unwrap_or(0.0),
                    _ => {}
                }
                s.call("onParameter", &format!("{}, {}", json!(name), value))?;
                if name == "SpindleSpeed" {
                    s.call("onSpindleSpeed", "")?;
                }
            }
            Record::Warn { text } => warnings.push(text.clone()),
            Record::Tool { number, name } => {
                base.insert("ToolNumber".into(), json!(*number));
                base.insert("ToolName".into(), json!(name));
            }
            Record::Section { begin } => {
                s.call(if *begin { "onSection" } else { "onSectionEnd" }, "")?;
            }
            Record::Move(m) => {
                let p = m.props();
                s.set_properties(&merged(&base, &p, pos))?;
                s.set_current(pos.0, pos.1, pos.2)?;
                let cmd = p.get("command").cloned().unwrap_or(Value::Null);
                if is_false(&s.call("onCommand", &cmd.to_string())?) {
                    swallowed += 1;
                } else {
                    let hook = if m.center.is_some() {
                        "onCircular"
                    } else if m.kind == MoveKind::Rapid {
                        "onRapid"
                    } else if m.kind == MoveKind::Plunge && s.hook_exists("onPlunge") {
                        "onPlunge"
                    } else {
                        "onLinear"
                    };
                    s.call(hook, "")?;
                }
                pos = (m.x, m.y, m.z);
            }
            Record::Cycle(c) => {
                s.set_properties(&merged(&base, &c.props(), pos))?;
                s.set_current(pos.0, pos.1, pos.2)?;
                if is_false(&s.call("onCommand", &json!("Drilling").to_string())?) {
                    swallowed += 1;
                } else {
                    s.call("onCycle", "")?;
                    s.call("onCyclePoint", "")?;
                    if c.dwell > 0.0 {
                        s.call("onDwell", &json!(c.dwell).to_string())?;
                    }
                    s.call("onCycleEnd", "")?;
                }
                // The canned cycle retracts to its R plane; the chain
                // continues from there.
                pos = (c.x, c.y, c.r_z);
            }
        }
        let after = s.out_len()?;
        for line in before..after {
            line_map.push(LineInfo { line, record: Some(i) });
        }
    }

    let before = s.out_len()?;
    s.call("onClose", "")?;
    let after = s.out_len()?;
    for line in before..after {
        line_map.push(LineInfo { line, record: None });
    }

    let (out, msgs) = s.drain()?;
    warnings.extend(msgs);
    let mut program = String::new();
    for l in &out {
        program.push_str(l);
        program.push('\n');
    }
    Ok(PostOutput {
        program,
        line_map,
        warnings,
        swallowed,
    })
}

/// Params + this record's properties + the live position keys.
fn merged(
    base: &BTreeMap<String, Value>,
    rec: &BTreeMap<String, Value>,
    pos: (f64, f64, f64),
) -> BTreeMap<String, Value> {
    let mut m = base.clone();
    m.insert("StartX".into(), json!(pos.0));
    m.insert("StartY".into(), json!(pos.1));
    m.insert("StartZ".into(), json!(pos.2));
    for (k, v) in rec {
        m.insert(k.clone(), v.clone());
    }
    m
}
