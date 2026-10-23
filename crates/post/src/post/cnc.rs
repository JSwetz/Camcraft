//! The intermediate stream the post consumes - our `.cnc` role.
//!
//! Provenance: our own line-oriented format, specced in
//! `docs/CNC-INTERMEDIATE.md`; only its *role* follows the published
//! Fusion 360 post architecture (behavior, not text, per the
//! Clean-Room Protocol). It is a superset of the golden `.cl` dumps:
//! a file with no `HSMW-CNC` magic parses by the `.cl` rules, so every
//! committed golden is a legal post input. Geometry stands on the µm
//! lattice the `.cl` dumps use; parameter reals are 6-decimal fixed.
//! Deterministic text in, deterministic record stream out - no
//! timestamps, no ordering luck.

use crate::cl::{CycleKind, MoveKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// One named `PARAM`, a `WARN` line, the tool block, a section
/// boundary, a motion record, or a cycle.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    /// A configuration parameter, in file order, before any section.
    Param { name: String, value: Value },
    /// A verification warning attached to the path - the post gate's
    /// evidence, carried in the stream so CLI and in-app paths share
    /// one source of truth.
    Warn { text: String },
    /// The tool block (`TOOL n name`).
    Tool { number: u32, name: String },
    /// A `SECTION`/`ENDSECTION` (legacy `START` ends no section; the
    /// parser opens one on the first record after `START` implicitly).
    Section { begin: bool },
    Move(MoveRec),
    Cycle(CycleRec),
}

/// One feed/rapid/plunge/arc record. Coordinates are millimeters
/// (parsed from µm integers, exact by the lattice law); `feed` is
/// mm/min (`None` for rapids). The record index in the stream is the
/// move index into the source path - legacy `.cl` files carry no
/// `from`, so chaining happens in the driver, not here.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveRec {
    pub kind: MoveKind,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub feed: Option<f64>,
    /// Arc: the absolute XY center.
    pub center: Option<(f64, f64)>,
    pub ccw: bool,
    /// Commanded absolute A/B/C angles, degrees (M3.3 stage D, RTCP).
    /// `None` - every record our generators write - means the tool
    /// stands as it always has: vertical, table at home. `Some` means
    /// the X/Y/Z above locate the TOOL TIP in the work offset while
    /// the machine turns to these angles; a post whose dialect has no
    /// RTCP output must refuse such a record by name, never post its
    /// XYZ as if the tool were vertical (the Prime Directive's exact
    /// shape: silently reinterpreted coordinates are a gouge).
    pub abc: Option<[f64; 3]>,
}

/// One canned-cycle record: the M1.6 `Cycle` in stream clothing. Zs and
/// peck are millimeters; `dwell` seconds (the stream carries ms);
/// `first_move` the cycle's move index - the agreement-law anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct CycleRec {
    pub kind: CycleKind,
    pub x: f64,
    pub y: f64,
    pub r_z: f64,
    pub bottom_z: f64,
    pub entry_z: f64,
    pub peck: f64,
    pub dwell: f64,
    pub first_move: usize,
    /// The cycle's cutting feed, mm/min. `dump` synthesizes it from
    /// the group's first cutting move; legacy `; CYCLE` markers
    /// carry none, so it is `None` there (the post falls back to the
    /// modal feed).
    pub feed: Option<f64>,
}

impl MoveRec {
    /// The property object the script sees for this record (names per
    /// `docs/CNC-INTERMEDIATE.md`; the driver adds the position keys).
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        let kind = match self.kind {
            MoveKind::Rapid => "Rapid",
            MoveKind::Feed => "Linear",
            MoveKind::Plunge => "Linear",
        };
        m.insert("command".into(), json!(kind));
        if let Some((cx, cy)) = self.center {
            m.insert("command".into(), json!("Circular"));
            m.insert(
                "CircularDirection".into(),
                json!(if self.ccw { "CCW" } else { "CW" }),
            );
            m.insert("IJKIncrement".into(), json!({ "I": cx, "J": cy }));
        }
        m.insert("X".into(), json!(self.x));
        m.insert("Y".into(), json!(self.y));
        m.insert("Z".into(), json!(self.z));
        m.insert("FeedRate".into(), json!(self.feed));
        if let Some([a, b, c]) = self.abc {
            m.insert("A".into(), json!(a));
            m.insert("B".into(), json!(b));
            m.insert("C".into(), json!(c));
        }
        if self.kind == MoveKind::Plunge {
            m.insert("LinearMovementIncludeOpts".into(), json!({ "L": [1] }));
        }
        m
    }
}

impl CycleRec {
    pub fn props(&self) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert("command".into(), json!("Drilling"));
        m.insert("CycleType".into(), json!(cycle_name(self.kind)));
        m.insert("X".into(), json!(self.x));
        m.insert("Y".into(), json!(self.y));
        m.insert("Z".into(), json!(self.bottom_z));
        m.insert("R".into(), json!(self.r_z));
        m.insert("EntryZ".into(), json!(self.entry_z));
        m.insert("Q".into(), json!(self.peck));
        m.insert("P".into(), json!(self.dwell));
        m.insert("FeedRate".into(), json!(self.feed));
        m
    }
}

/// The script-facing cycle name, matching the v1 `CYCLE` token spelling.
pub fn cycle_name(kind: CycleKind) -> &'static str {
    match kind {
        CycleKind::Spot => "SPOT",
        CycleKind::Drill => "DRILL",
        CycleKind::Peck => "PECK",
        CycleKind::Deep => "DEEP",
        CycleKind::Tap => "TAP",
        CycleKind::Bore => "BORE",
        CycleKind::BackBore => "BACKBORE",
    }
}

impl Record {
    /// The debugging name of one record (M3.3 stage D, post
    /// debugging): what a block of emitted lines stands for, in the
    /// trace pane and the `--trace` log. Positions are the stream's
    /// own millimeters; angles, when the record commands them, ride
    /// along - a trace that hides the tilt is not a trace.
    pub fn trace_label(&self) -> String {
        let pos = |x: f64, y: f64, z: f64| format!("({x:.3}, {y:.3}, {z:.3})");
        match self {
            Record::Param { name, .. } => format!("param {name}"),
            Record::Warn { text } => format!("warning: {text}"),
            Record::Tool { number, name } => format!("tool {number} · {name}"),
            Record::Section { begin: true } => "section".into(),
            Record::Section { begin: false } => "end section".into(),
            Record::Move(m) => {
                let kind = match m.kind {
                    MoveKind::Rapid => "rapid",
                    MoveKind::Feed => "feed",
                    MoveKind::Plunge => "plunge",
                };
                let mut s = format!("{kind} → {}", pos(m.x, m.y, m.z));
                if m.center.is_some() {
                    s = format!(
                        "arc {} → {}",
                        if m.ccw { "ccw" } else { "cw" },
                        pos(m.x, m.y, m.z)
                    );
                }
                if let Some(f) = m.feed {
                    s.push_str(&format!(" @ {f:.0} mm/min"));
                }
                if let Some([a, b, c]) = m.abc {
                    s.push_str(&format!(" A {a:.3}° B {b:.3}° C {c:.3}°"));
                }
                s
            }
            Record::Cycle(c) => format!(
                "cycle {} → {}",
                cycle_name(c.kind),
                pos(c.x, c.y, c.bottom_z)
            ),
        }
    }

    /// The `onCommand` name for this record, "" for structural ones.
    pub fn command_name(&self) -> &'static str {
        match self {
            Record::Move(m) => match m.props().get("command") {
                Some(v) => match v.as_str() {
                    Some("Rapid") => "Rapid",
                    Some("Circular") => "Circular",
                    _ => "Linear",
                },
                None => "",
            },
            Record::Cycle(_) => "Drilling",
            _ => "",
        }
    }
}

/// µm-integer formatting, the `.cl` golden law (`scale=0.001mm`):
/// value times a thousand, nearest, written as an integer.
fn q(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    ((v * 1000.0).round() as i64).to_string()
}

/// The writer side: the stream as text, `records` in driver order. The
/// canonical v1 spelling keys every coordinate (`X 1000`, spaced, as
/// `docs/CNC-INTERMEDIATE.md` specs it); the parser also accepts the
/// joined (`X1000`) and legacy positional forms.
pub fn serialize(records: &[Record]) -> String {
    let mut s = String::from("; HSMW-CNC v1\n");
    for r in records {
        match r {
            Record::Param { name, value } => {
                let v = match value {
                    Value::String(t) => t.clone(),
                    Value::Number(n) if n.is_f64() => format!("{:.6}", n.as_f64().unwrap_or(0.0)),
                    other => other.to_string(),
                };
                s.push_str(&format!("PARAM {name} {v}\n"));
            }
            Record::Warn { text } => s.push_str(&format!("WARN {text}\n")),
            Record::Tool { number, name } => s.push_str(&format!("TOOL {number} {name}\n")),
            Record::Section { begin } => {
                s.push_str(if *begin { "SECTION\n" } else { "ENDSECTION\n" });
            }
            Record::Move(m) => {
                let f = m
                    .feed
                    .map(|f| format!(" F {}", (f * 1000.0).round() as i64))
                    .unwrap_or_default();
                // Commanded angles (stage D): milli-degrees on the
                // same lattice law as the microns above - the angle
                // a record states is the angle it parses back to.
                let abc = m
                    .abc
                    .map(|[a, b, c]| format!(" A {} B {} C {}", q(a), q(b), q(c)))
                    .unwrap_or_default();
                match m.center {
                    Some((cx, cy)) => s.push_str(&format!(
                        "Arc {} X {} Y {} Z {}{f}{abc} I {} J {}\n",
                        if m.ccw { "CCW" } else { "CW" },
                        q(m.x),
                        q(m.y),
                        q(m.z),
                        q(cx),
                        q(cy),
                    )),
                    None => s.push_str(&format!(
                        "{} X {} Y {} Z {}{f}{abc}\n",
                        match m.kind {
                            MoveKind::Rapid => "Rapid",
                            MoveKind::Feed => "Feed",
                            MoveKind::Plunge => "Plunge",
                        },
                        q(m.x),
                        q(m.y),
                        q(m.z),
                    )),
                }
            }
            Record::Cycle(c) => {
                let f = c
                    .feed
                    .map(|f| format!(" F {}", (f * 1000.0).round() as i64))
                    .unwrap_or_default();
                s.push_str(&format!(
                    "CYCLE {} X {} Y {} Z {} R {} ENTRY {} Q {} P {}{} AT {}\n",
                    cycle_name(c.kind),
                    q(c.x),
                    q(c.y),
                    q(c.bottom_z),
                    q(c.r_z),
                    q(c.entry_z),
                    q(c.peck),
                    (c.dwell * 1000.0).round() as i64,
                    f,
                    c.first_move,
                ))
            }
        }
    }
    s
}

/// Parse errors: the Prime Directive's shape - a malformed stream is a
/// clean refusal, never a partial record set.
#[derive(Debug, thiserror::Error)]
pub enum CncError {
    #[error("line {line}: {why}")]
    Syntax { line: usize, why: String },
}

/// Split one word into (key, value-text): `X: 5`, `X=5`, or the joined
/// `X5` / `RZ-1000` spelling where the key is the leading run of ASCII
/// letters and the value starts at the first digit or sign.
fn cnc_word(word: &str) -> Option<(&str, &str)> {
    if let Some((k, v)) = word.split_once([':', '=']) {
        return Some((k, v));
    }
    let split = word
        .find(|c: char| c == '-' || c.is_ascii_digit())
        .unwrap_or(word.len());
    if split > 0
        && split < word.len()
        && word[..split].bytes().all(|b| b.is_ascii_alphabetic())
    {
        Some((&word[..split], &word[split..]))
    } else {
        None
    }
}

fn num(tok: &str) -> Result<f64, ()> {
    // Integer microns by our own law; a decimal parse accepts other
    // spellings. Non-finite text fails the gate below rather than
    // poisoning a record.
    let v = tok.parse::<f64>().map_err(|_| ())?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(())
    }
}

/// One motion word, in any of the three legal spellings: a bare number
/// (legacy positional), a pure key whose value is the next token, or a
/// key with the value attached or `:`/`=`-separated.
enum MotionWord {
    Value(f64),
    Keyed(&'static str, f64),
}

fn motion_words<'a>(
    it: &mut std::iter::Peekable<std::str::SplitWhitespace<'a>>,
    line_no: usize,
) -> Result<Vec<MotionWord>, CncError> {
    let fail = |why: String| CncError::Syntax { line: line_no, why };
    let mut out = Vec::new();
    while let Some(w) = it.next() {
        if let Ok(v) = num(w) {
            out.push(MotionWord::Value(v));
            continue;
        }
        if !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphabetic()) {
            // Pure key: the value is the next token (the docs' spaced
            // canonical form `X 1000` arrives this way).
            let Some(key) = normalize_key(w) else {
                return Err(fail(format!("unknown word {w:?}")));
            };
            let Some(vtok) = it.next() else {
                return Err(fail(format!("word {w:?} has no value")));
            };
            let Ok(v) = num(vtok) else {
                return Err(fail(format!("bad value {vtok:?} for {w:?}")));
            };
            out.push(MotionWord::Keyed(key, v));
            continue;
        }
        let Some((k, v)) = cnc_word(w) else {
            return Err(fail(format!("move word {w:?}")));
        };
        let Ok(n) = num(v) else {
            return Err(fail(format!("move value {w:?}")));
        };
        let Some(key) = normalize_key(k) else {
            return Err(fail(format!("unknown word {k:?}")));
        };
        out.push(MotionWord::Keyed(key, n));
    }
    Ok(out)
}

fn normalize_key(k: &str) -> Option<&'static str> {
    Some(match k.to_ascii_uppercase().as_str() {
        "X" => "X",
        "Y" => "Y",
        "Z" => "Z",
        "F" => "F",
        "I" => "I",
        "J" => "J",
        "S" => "S",
        // Stage D: commanded rotary angles, absolute degrees written
        // in milli-degrees. Keyed words only - a positional value
        // never means an angle (the legacy law is unchanged).
        "A" => "A",
        "B" => "B",
        "C" => "C",
        _ => return None,
    })
}

/// One cycle's word stream: `KIND [X.. Y.. Z..|BZ.. R..|RZ.. ENTRY..
/// Q..|PECK.. P..|DWELL.. AT..]`, each word spaced (`X 1000`), joined
/// (`X1000`), or `:`/`=`-separated. `default_move` is the `AT` value
/// when the stream carries none: the cycle's position in the file.
/// Shared by the v1 `CYCLE` record and the legacy `; CYCLE` marker.
fn parse_cycle_rec(
    it: &mut std::iter::Peekable<std::str::SplitWhitespace<'_>>,
    default_move: usize,
) -> Result<CycleRec, String> {
    let Some(k) = it.next() else {
        return Err("CYCLE needs a kind".into());
    };
    let Some(kind) = cycle_kind(k) else {
        return Err(format!("unknown cycle kind {k:?}"));
    };
    let mut r = CycleRec {
        kind,
        x: 0.0,
        y: 0.0,
        r_z: 0.0,
        bottom_z: 0.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: default_move,
        feed: None,
    };
    while let Some(w) = it.next() {
        // A pure-alphabetic token is a spaced key; its value follows.
        let (k, vtok) = if !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphabetic()) {
            let Some(v) = it.next() else {
                return Err(format!("cycle word {w:?} has no value"));
            };
            (w, v)
        } else {
            let Some((k, v)) = cnc_word(w) else {
                return Err(format!("cycle word {w:?}"));
            };
            (k, v)
        };
        let Ok(n) = num(vtok) else {
            return Err(format!("cycle value {vtok:?}"));
        };
        match k.to_ascii_uppercase().as_str() {
            "X" => r.x = n / 1000.0,
            "Y" => r.y = n / 1000.0,
            "Z" | "BZ" => r.bottom_z = n / 1000.0,
            "R" | "RZ" => r.r_z = n / 1000.0,
            "Q" | "PECK" => r.peck = n / 1000.0,
            "P" | "DWELL" => r.dwell = n / 1000.0,
            "F" => r.feed = Some(n / 1000.0),
            "ENTRY" => r.entry_z = n / 1000.0,
            "AT" => r.first_move = n.max(0.0) as usize,
            k => return Err(format!("unknown cycle word {k:?}")),
        }
    }
    Ok(r)
}

/// Parse a stream (our v1 form, or any legacy `.cl` golden).
pub fn parse(text: &str) -> Result<Vec<Record>, CncError> {
    let mut out = Vec::new();
    let mut move_idx = 0usize;
    let mut last_z = 0.0f64;
    let mut in_section = false;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        let fail = |why: String| CncError::Syntax { line: lineno + 1, why };
        if line.is_empty() {
            continue;
        }
        // Comments: the CNC magic header opens the v1 dialect; a `.cl`
        // file is simply one without it (and its `; HSMW-CL` header is
        // an ordinary comment here). A `; CYCLE` line is the v3 golden
        // marker, parsed exactly like a v1 CYCLE record.
        if let Some(rest) = line.strip_prefix(";") {
            let t = rest.trim_start();
            if t.starts_with("HSMW-CNC") || t.starts_with("HSMW-CL") {
                continue; // version token
            }
            if let Some(c) = t.strip_prefix("CYCLE") {
                let mut wit = c.split_whitespace().peekable();
                let r = parse_cycle_rec(&mut wit, move_idx).map_err(fail)?;
                out.push(Record::Cycle(r));
                continue;
            }
            continue; // any other comment is nothing to the post
        }
        let mut it = line.split_whitespace().peekable();
        let head = it.next().unwrap_or("");
        match head {
            "PARAM" => {
                let Some(name) = it.next() else {
                    return Err(fail("PARAM needs a name and a value".into()));
                };
                // The value is the rest of the line (whitespace run
                // collapsed to single spaces): strings may carry
                // spaces - operation names do.
                let parts: Vec<&str> = it.collect();
                if parts.is_empty() {
                    return Err(fail("PARAM needs a name and a value".into()));
                }
                out.push(Record::Param { name: name.into(), value: param_value(&parts.join(" ")) });
            }
            "WARN" => {
                let Some(text) = line.strip_prefix("WARN") else {
                    return Err(fail("WARN needs text".into()));
                };
                let text = text.trim();
                if text.is_empty() {
                    return Err(fail("WARN needs text".into()));
                }
                out.push(Record::Warn { text: text.into() });
            }
            "TOOL" => {
                let Some(n) = it.next() else {
                    return Err(fail("TOOL needs a number and a name".into()));
                };
                let Ok(number) = n.parse::<u32>() else {
                    return Err(fail(format!("TOOL number {n:?}")));
                };
                // The name is the rest of the line (runs collapsed):
                // library names carry spaces ("1/4 INCH 4FL").
                let parts: Vec<&str> = it.collect();
                if parts.is_empty() {
                    return Err(fail("TOOL needs a number and a name".into()));
                }
                out.push(Record::Tool { number, name: parts.join(" ") });
            }
            "SECTION" => {
                out.push(Record::Section { begin: true });
                in_section = true;
            }
            "ENDSECTION" => {
                if in_section {
                    out.push(Record::Section { begin: false });
                }
                in_section = false;
            }
            "END" => {
                // Legacy close: a section we never opened is still a
                // boundary worth reporting, but only after records.
                if move_idx > 0 || out.iter().any(|r| matches!(r, Record::Cycle(_))) {
                    out.push(Record::Section { begin: false });
                }
                in_section = false;
            }
            "START" => {
                // Legacy position line: X Y Z, microns; Z chains on.
                let nums: Vec<f64> = match it.map(num).collect::<Result<Vec<_>, _>>() {
                    Ok(v) => v,
                    Err(_) => return Err(fail("START needs numbers".into())),
                };
                last_z = nums.last().map_or(0.0, |v| v / 1000.0);
            }
            "Rapid" | "Feed" | "Plunge" | "Arc" => {
                let mut ccw = false;
                if head == "Arc" {
                    // v1 spells the direction CW/CCW as a word; legacy
                    // carries it in the S token instead.
                    let peeked = it.peek().copied().unwrap_or("");
                    if peeked == "CW" || peeked == "CCW" {
                        ccw = peeked == "CCW";
                        it.next();
                    }
                }
                let words = motion_words(&mut it, lineno + 1)?;
                let mut x = None;
                let mut y = None;
                let mut z = None;
                let mut feed = None;
                let mut ci = None;
                let mut cj = None;
                let mut abc = [0.0f64; 3];
                let mut tilted = false;
                let mut pos = 0usize;
                for w in words {
                    match w {
                        MotionWord::Value(v) => {
                            match pos {
                                0 => x = Some(v / 1000.0),
                                1 => y = Some(v / 1000.0),
                                2 => z = Some(v / 1000.0),
                                _ => return Err(fail("too many positional values".into())),
                            }
                            pos += 1;
                        }
                        MotionWord::Keyed(k, v) => match k {
                            "X" => x = Some(v / 1000.0),
                            "Y" => y = Some(v / 1000.0),
                            "Z" => z = Some(v / 1000.0),
                            "F" => feed = Some(v / 1000.0),
                            "I" => ci = Some(v / 1000.0),
                            "J" => cj = Some(v / 1000.0),
                            "S" => ccw = v >= 0.0,
                            "A" => {
                                abc[0] = v / 1000.0;
                                tilted = true;
                            }
                            "B" => {
                                abc[1] = v / 1000.0;
                                tilted = true;
                            }
                            "C" => {
                                abc[2] = v / 1000.0;
                                tilted = true;
                            }
                            _ => {}
                        },
                    }
                }
                let (Some(x), Some(y)) = (x, y) else {
                    return Err(fail("move needs at least X and Y".into()));
                };
                let z = z.unwrap_or(last_z);
                last_z = z;
                let center = match (ci, cj) {
                    (Some(i), Some(j)) => Some((i, j)),
                    (None, None) => None,
                    _ => return Err(fail("arc center needs both I and J".into())),
                };
                if head == "Arc" && center.is_none() {
                    return Err(fail("arc needs a center (I and J)".into()));
                }
                out.push(Record::Move(MoveRec {
                    kind: match head {
                        "Rapid" => MoveKind::Rapid,
                        "Plunge" => MoveKind::Plunge,
                        _ => MoveKind::Feed,
                    },
                    x,
                    y,
                    z,
                    feed,
                    center,
                    ccw,
                    abc: tilted.then_some(abc),
                }));
                move_idx += 1;
                in_section = true;
            }
            "CYCLE" => {
                let r = parse_cycle_rec(&mut it, move_idx).map_err(fail)?;
                out.push(Record::Cycle(r));
                in_section = true;
            }
            other => {
                return Err(fail(format!("unknown record {other:?}")));
            }
        }
    }
    Ok(out)
}

fn cycle_kind(name: &str) -> Option<CycleKind> {
    Some(match name.to_ascii_uppercase().as_str() {
        "SPOT" => CycleKind::Spot,
        "DRILL" => CycleKind::Drill,
        "PECK" => CycleKind::Peck,
        "DEEP" => CycleKind::Deep,
        "TAP" => CycleKind::Tap,
        "BORE" => CycleKind::Bore,
        "BACKBORE" => CycleKind::BackBore,
        _ => return None,
    })
}

fn param_value(v: &str) -> Value {
    let t = v.trim();
    if t.eq_ignore_ascii_case("true") {
        return json!(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return json!(false);
    }
    if let Ok(n) = t.parse::<f64>()
        && n.is_finite()
    {
        return json!(n);
    }
    json!(t.trim_matches('"'))
}
