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

/// One toolpath record's stretch of program lines (M3.3 stage D,
/// post debugging): the block the trace pane selects and `--trace`
/// logs. `first..last` is half-open into the program's line numbers;
/// `record` is `None` for the header `onOpen` wrote and the footer
/// `onClose` wrote - lines no toolpath record owns.
#[derive(Clone, Debug)]
pub struct TraceBlock {
    pub record: Option<usize>,
    pub label: String,
    pub first: usize,
    pub last: usize,
}

/// The finished program plus the evidence a reviewer needs.
#[derive(Clone, Debug)]
pub struct PostOutput {
    /// The G-code, one line per entry, newline-terminated.
    pub program: String,
    /// Program line -> stream record, the viewer's sync map (the
    /// record's index doubles as the move index for motion records).
    pub line_map: Vec<LineInfo>,
    /// Block-by-block trace: every record that wrote lines (and every
    /// motion record `onCommand` swallowed, which writes none - loss
    /// stays visible, never silent). Records that legitimately emit
    /// nothing (a `PARAM` a post only stores) are absent, so the
    /// trace reads as program, not as the stream echoed back.
    pub blocks: Vec<TraceBlock>,
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
    let head = s.out_len()?;

    let mut warnings = Vec::new();
    let mut base: BTreeMap<String, Value> = BTreeMap::new();
    let mut pos = (0.0f64, 0.0f64, 0.0f64);
    let mut line_map: Vec<LineInfo> = Vec::new();
    let mut blocks: Vec<TraceBlock> = Vec::new();
    if head > 0 {
        blocks.push(TraceBlock { record: None, label: "header".into(), first: 0, last: head });
    }
    let mut swallowed = 0usize;

    for (i, r) in records.iter().enumerate() {
        let before = s.out_len()?;
        let mut swallow = false;
        match r {
            Record::Param { name, value } => {
                base.insert(name.clone(), value.clone());
                match name.as_str() {
                    "StartX" => pos.0 = value.as_f64().unwrap_or(0.0),
                    "StartY" => pos.1 = value.as_f64().unwrap_or(0.0),
                    "StartZ" => pos.2 = value.as_f64().unwrap_or(0.0),
                    _ => {}
                }
                // Publish the running param set so `onParameter` (and
                // `onSpindleSpeed` just below) can actually read the
                // value the hook names.
                s.set_properties(&merged(&base, &BTreeMap::new(), pos))?;
                s.call("onParameter", &format!("{}, {}", json!(name), value))?;
                if name == "SpindleSpeed" {
                    s.call("onSpindleSpeed", "")?;
                }
            }
            Record::Warn { text } => {
                warnings.push(text.clone());
                // The comment contract (M3.3 A): a warning is also
                // commentary the script may want to voice. The
                // PRELUDE's `onComment` default echoes it verbatim -
                // and the echo only ever lands in a REFUSED output,
                // because the warning count that blocks the write
                // (`posting::post_setup`) reads the same list this
                // arm grows. So the released program is byte-for-byte
                // what it was; a script that overrides `onComment`
                // can restyle or silence its comments, but the data
                // is never withheld from it.
                s.call("onComment", &json!(text).to_string())?;
            }
            Record::Tool { number, name } => {
                base.insert("ToolNumber".into(), json!(*number));
                base.insert("ToolName".into(), json!(name));
                s.set_properties(&merged(&base, &BTreeMap::new(), pos))?;
                s.call("onToolChange", &format!("{}, {}", json!(number), json!(name)))?;
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
                    swallow = true;
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
                    swallow = true;
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
            Record::Part { loading, advance } => {
                // A lifecycle event, like SECTION: it commands no
                // motion, so `onCommand` never sees it - the script's
                // onPartLoading/onPartUnloading is the whole contract,
                // and the PRELUDE's defaults voice the intent as a
                // comment so a program from a bare script still says
                // what it asks the machine to do.
                let mut p = BTreeMap::new();
                p.insert(
                    "command".into(),
                    json!(if *loading { "PartLoading" } else { "PartUnloading" }),
                );
                if *loading {
                    p.insert("Advance".into(), json!(advance));
                }
                s.set_properties(&merged(&base, &p, pos))?;
                s.call(if *loading { "onPartLoading" } else { "onPartUnloading" }, "")?;
            }
            Record::Transfer { from, to } => {
                // The handoff step (M5.3 C): same lifecycle law as
                // the feeder records - no motion, so `onCommand`
                // never sees it, and the script's onPartTransfer is
                // the whole contract. The stations ride as From/To
                // so a dialect can name its own handshake; the
                // engine invents nothing.
                let mut p = BTreeMap::new();
                p.insert("command".into(), json!("PartTransfer"));
                p.insert("From".into(), json!(from));
                p.insert("To".into(), json!(to));
                s.set_properties(&merged(&base, &p, pos))?;
                s.call("onPartTransfer", "")?;
            }
            Record::Comp { side, radius_mm } => {
                // Cutter compensation (M13.11): a lifecycle record like
                // its siblings - no motion, `onCommand` never sees it -
                // but NOT comment-only by default: the records after it
                // are the part contour, and a script that cannot say
                // G41/G42 would post them as the centre path - an
                // undersize part. So a script without the hook is
                // refused here, by name, as a counted warning.
                if !s.hook_exists("onRadiusCompensation") {
                    warnings.push(
                        "control cutter compensation: this post script does not define \
                         onRadiusCompensation - it cannot write G41/G42, so the program is refused"
                            .into(),
                    );
                } else {
                    let mut p = BTreeMap::new();
                    p.insert("command".into(), json!("RadiusCompensation"));
                    let word = match side {
                        Some(crate::cl::CompSide::Left) => "left",
                        Some(crate::cl::CompSide::Right) => "right",
                        None => "off",
                    };
                    p.insert("Compensation".into(), json!(word));
                    p.insert("CompensationRadius".into(), json!(radius_mm));
                    s.set_properties(&merged(&base, &p, pos))?;
                    s.call("onRadiusCompensation", &json!(word).to_string())?;
                }
            }
            Record::Probe { tol_mm } => {
                // The fourth lifecycle name (M5.3 D): the measurement
                // ask rides as a property, the same law as its
                // siblings - no motion, `onCommand` never sees it,
                // and onProbe is the whole contract. What protocol
                // the probe speaks is the script's dialect; the
                // engine only says that a measurement was asked for,
                // against what budget.
                let mut p = BTreeMap::new();
                p.insert("command".into(), json!("Probe"));
                p.insert("Tolerance".into(), json!(tol_mm));
                s.set_properties(&merged(&base, &p, pos))?;
                s.call("onProbe", "")?;
            }
        }
        let after = s.out_len()?;
        for line in before..after {
            line_map.push(LineInfo { line, record: Some(i) });
        }
        // A swallowed motion record writes nothing, and the trace
        // still shows it - the loss is the story.
        if after > before || swallow {
            let mut label = r.trace_label();
            if after == before {
                label.push_str(" (no output)");
            }
            blocks.push(TraceBlock { record: Some(i), label, first: before, last: after });
        }
    }

    let before = s.out_len()?;
    s.call("onClose", "")?;
    let after = s.out_len()?;
    for line in before..after {
        line_map.push(LineInfo { line, record: None });
    }
    if after > before {
        blocks.push(TraceBlock { record: None, label: "end".into(), first: before, last: after });
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
        blocks,
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
