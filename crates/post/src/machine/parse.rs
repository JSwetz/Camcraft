//! The `.mch` reader: JSON in, Machine out, with the file-form grammar
//! documented in `assets/machines/AGENTS.md`. Refusals name their
//! reason (Prime Directive); unstated numbers read `None`.
//!
//! Provenance: our own parser, specced from the observable shape of a
//! machine file and RS-274/ISO 6983 axis-lettering convention.

use super::def::{AxisLimits, Coolant, Kind, Machine};

/// The number a machine field reads as, with the unstated-limit law:
/// absent, null, non-numeric, or exactly zero reads `None`; a
/// negative or NaN reads `Some` only for travel bounds (where a
/// negative stroke limit is the point), and `None` for rates (where
/// it is a broken file, checked at the point of use).
fn rate(v: Option<&serde_json::Value>) -> Option<f64> {
    let x = v?.as_f64()?;
    if x.is_finite() && x > 0.0 {
        Some(x)
    } else {
        None
    }
}

/// Length-unit factor: a file declaring inches multiplies every
/// distance - and every distance-per-minute - by 25.4 on the way in,
/// so the whole checker stays mm-only.
fn mm_factor(units: Option<&serde_json::Value>) -> Option<f64> {
    match units?.get("length")?.as_str()? {
        "inch" | "inches" => Some(25.4),
        _ => Some(1.0),
    }
}

/// Walk a part list collecting every `"type": "linear"` and (since
/// M3.3 stage C) `"type": "rotary"` part by name, depth-first (the
/// standard shape nests Y inside X inside Z - the chain is the
/// kinematics, but the limit table is flat; a rotary trunnion hangs
/// off the table the same way). The flag says which kind of motion
/// the part states, not a letter the reader has to stereotype.
fn collect_axes(
    parts: Option<&serde_json::Value>,
    out: &mut Vec<(String, serde_json::Value, bool)>,
) {
    let Some(arr) = parts.and_then(|p| p.as_array()) else { return };
    for p in arr {
        let ty = p.get("type").and_then(|t| t.as_str());
        let rotary = ty == Some("rotary");
        let name = p
            .get("name")
            .and_then(|n| n.as_str())
            .or_else(|| p.get("id").and_then(|n| n.as_str()))
            .unwrap_or("")
            .to_owned();
        if (ty == Some("linear") || rotary) && !name.is_empty() {
            out.push((name, p.clone(), rotary));
        }
        collect_axes(p.get("parts"), out);
    }
}

/// The coolants a tool station states, in either shape the grammar
/// takes: bare name strings (a named option, no code - posts
/// nothing), or `{name, mCode}` objects. The code may itself be a
/// string ("M8") or a bare number (8), normalized to the word form.
fn parse_coolants(v: Option<&serde_json::Value>) -> Vec<Coolant> {
    let Some(arr) = v.and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| {
            if let Some(name) = e.as_str() {
                return Some(Coolant {
                    name: name.to_owned(),
                    m_code: String::new(),
                });
            }
            let name = e.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let code = match e.get("mCode").or_else(|| e.get("mcode")) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => format!("M{n}"),
                _ => String::new(),
            };
            (!name.is_empty()).then_some(Coolant {
                name: name.to_owned(),
                m_code: code,
            })
        })
        .collect()
}

/// How many `type: "head"` parts anywhere in the tree state a
/// spindle (M5.3 C). The limit numbers still read the first match
/// (`find_type` above) - this is the inventory for the handoff
/// check, so it counts them all.
fn count_spindles(parts: Option<&serde_json::Value>) -> usize {
    let Some(arr) = parts.and_then(|p| p.as_array()) else {
        return 0;
    };
    arr.iter()
        .map(|p| {
            usize::from(
                p.get("type").and_then(|t| t.as_str()) == Some("head")
                    && p.get("spindle").is_some_and(|s| !s.is_null()),
            ) + count_spindles(p.get("parts"))
        })
        .sum()
}

/// First part anywhere in the tree with the given `type`, depth-first.
fn find_type<'a>(parts: &'a serde_json::Value, ty: &str) -> Option<&'a serde_json::Value> {
    let arr = parts.as_array()?;
    for p in arr {
        if p.get("type").and_then(|t| t.as_str()) == Some(ty) {
            return Some(p);
        }
        if let Some(hit) = find_type(p.get("parts")?, ty) {
            return Some(hit);
        }
    }
    None
}

/// Parse a `.mch` machine file. Malformed JSON refuses; a well-formed
/// file that describes nothing answers with an all-`None` Machine -
/// an empty machine checks nothing, which is the truth rather than a
/// silently stricter one.
pub fn parse_mch(text: &str) -> Result<Machine, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("machine file: {e}"))?;
    if !v.is_object() {
        return Err("machine file: expected a machine object".to_owned());
    }

    let general = v.pointer("/general");
    let mut m = Machine {
        model: text_of(general.and_then(|g| g.get("model"))),
        vendor: text_of(general.and_then(|g| g.get("vendor"))),
        ..Default::default()
    };

    // The declared kind (M3.3 C): a stated word must be a known one.
    // A file that declares "drill_press" is not silently demoted to
    // the inference - the declaration was the point of writing it, so
    // an unreadable one refuses at the front door.
    match general.and_then(|g| g.get("kind")) {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(w)) if w.is_empty() => {}
        Some(serde_json::Value::String(w)) => {
            if Kind::from_word(w).is_none() {
                return Err(format!(
                    "machine file: kind {w:?} is not mill, lathe or mill-turn"
                ));
            }
            m.kind = Some(w.clone());
        }
        Some(other) => {
            return Err(format!("machine file: kind must be a word, not {other}"));
        }
    }

    let kine = v.pointer("/kinematics/default/parts");
    let f = mm_factor(v.pointer("/kinematics/default/units")).unwrap_or(1.0);
    // #131: keep the stated unit for the posts (the numbers stay mm).
    let length = v.pointer("/kinematics/default/units/length").and_then(|u| u.as_str());
    if matches!(length, Some("inch" | "inches")) {
        m.units = Some("inch".to_owned());
    }

    if let Some(parts) = kine {
        if let Some(head) = find_type(parts, "head") {
            let sp = head.get("spindle");
            m.spindle_min = rate(sp.and_then(|s| s.get("min_speed")));
            m.spindle_max = rate(sp.and_then(|s| s.get("max_speed")));
            m.spindle_power = rate(sp.and_then(|s| s.get("power")));
            let ts = head.get("tool_station");
            m.max_tool_length = scale(rate(ts.and_then(|t| t.get("max_tool_length"))), f);
            m.max_tool_diameter = scale(rate(ts.and_then(|t| t.get("max_tool_diameter"))), f);
            m.spindle_d = scale(rate(sp.and_then(|s| s.get("body_diameter"))), f);
            m.spindle_len = scale(rate(sp.and_then(|s| s.get("body_length"))), f);
            m.coolants = parse_coolants(ts.and_then(|t| t.get("coolants")));
        }
        if m.coolants.is_empty() {
            // Some files keep the option list at the tooling root
            // rather than on the station; either home reads.
            m.coolants = parse_coolants(v.pointer("/tooling/coolants"));
        }
        // The spindle inventory (M5.3 C): heads that state a spindle,
        // counted whole-tree; no head states one, no count is stated.
        let nsp = count_spindles(Some(parts));
        m.spindles = (nsp > 0).then_some(nsp);
        // The changer (stage C's capacity number): the tooling tree's
        // `number_of_tools`, a count the file states directly.
        m.tool_count = v
            .pointer("/tooling/number_of_tools")
            .and_then(|n| n.as_u64())
            .map(|n| n as usize)
            .filter(|n| *n > 0);

        let mut listed: Vec<(String, serde_json::Value, bool)> = Vec::new();
        collect_axes(Some(parts), &mut listed);

        // Controller speeds, keyed by axis name; the kinematics list
        // decides WHICH axes exist, the controller says how fast they
        // go, and a `"travel"` object (our extension) says how far.
        let ctrl = v.pointer("/controller/synced_configuration/parts");
        let mut axes: Vec<AxisLimits> = Vec::new();
        for (name, part, rotary) in &listed {
            let c = ctrl.and_then(|c| c.get(name.as_str()));
            // Travel: a length scales with the file's units, an
            // angle is an angle (the length factor would lie). Rates
            // the same law: mm/min scales, deg/min does not.
            let scale_v = if *rotary { 1.0 } else { f };
            let travel = part.get("travel").and_then(|t| {
                let lo = t.get("min")?.as_f64()?;
                let hi = t.get("max")?.as_f64()?;
                if lo.is_finite() && hi.is_finite() && hi > lo {
                    Some((lo * scale_v, hi * scale_v))
                } else {
                    None
                }
            });
            let resolution = scale(rate(part.get("resolution")), scale_v);
            // A rotary's stated rotation direction (stage D): three
            // finite numbers or a refusal that names the axis - a
            // garbled tilt vector must never quietly become "no
            // tilt", because the whole RTCP judgment rides on it.
            let mut axis = None;
            if let Some(a) = part.get("axis") {
                let d = a.as_array().filter(|v| v.len() == 3).map(|v| {
                    [
                        v[0].as_f64(),
                        v[1].as_f64(),
                        v[2].as_f64(),
                    ]
                });
                match d {
                    Some([Some(x), Some(y), Some(z)]) => {
                        if *rotary && x == 0.0 && y == 0.0 && z == 0.0 {
                            return Err(format!(
                                "machine file: rotary axis {name} states the zero direction"
                            ));
                        }
                        axis = Some([x, y, z]);
                    }
                    _ => {
                        return Err(format!(
                            "machine file: axis {name} direction must be three finite numbers"
                        ));
                    }
                }
            }
            axes.push(AxisLimits {
                name: name.clone(),
                rotary: *rotary,
                feed: scale(rate(c.and_then(|c| c.get("max_normal_speed"))), scale_v),
                rapid: scale(rate(c.and_then(|c| c.get("max_rapid_speed"))), scale_v),
                travel,
                resolution,
                axis,
            });
        }
        axes.sort_by_key(|a| ctrl_coord(ctrl, &a.name));
        m.axes = axes;
    }
    Ok(m)
}

fn text_of(v: Option<&serde_json::Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("unknown").to_owned()
}

fn scale(v: Option<f64>, f: f64) -> Option<f64> {
    v.map(|x| x * f)
}

fn ctrl_coord(ctrl: Option<&serde_json::Value>, name: &str) -> i64 {
    ctrl.and_then(|c| c.get(name))
        .and_then(|a| a.get("coordinate"))
        .and_then(|x| x.as_i64())
        .unwrap_or(i64::MAX)
}

