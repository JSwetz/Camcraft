//! The machine definition - our `.mch` role.
//!
//! Provenance: a JSON file carrying capability flags and a kinematics
//! tree (head → axes → table), a layout specced from Autodesk's
//! published post-processor documentation (behavior only, per the
//! Clean-Room Protocol). The grammar and every key name are ours.

use serde_json::{json, Value};

/// Parse the file text (syntax only; absent fields default at read
/// time, so a minimal file is legal).
pub fn from_json(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

/// The static-grammar mirror of a checker-parsed machine (M3.3 A).
/// [`crate::machine::Machine`] is what a vendor `.mch` parses to -
/// limits the checker judges; the script's `machine` global and the
/// sim's traverse timer walk `kinematics.static` instead. This says
/// the same numbers in that grammar, stating only what the file
/// stated: an unstated limit is absent, never a zero that lies (the
/// posts' `> 0` guards keep silence faithful). Coolants: a parsed
/// `.mch` carries no coolant list, so the mirror states none and the
/// post omits the M-code (honest absence, the unstated-limit law in
/// coolant clothes; stage C grows the checker grammar to carry it).
/// Rotary axes are not mirrored - no 3-axis post reads them, and the
/// traverse paces only prismatic motion (stage D, RTCP).
#[must_use]
pub fn mirror(m: &crate::machine::Machine) -> Value {
    let mut axes: Vec<Value> = Vec::new();
    for a in &m.axes {
        let direction = match a.name.as_str() {
            "X" => [1.0, 0.0, 0.0],
            "Y" => [0.0, 1.0, 0.0],
            "Z" => [0.0, 0.0, 1.0],
            _ => continue,
        };
        let mut speeds = serde_json::Map::new();
        if let Some(r) = a.rapid {
            speeds.insert("rapid".to_owned(), json!(r));
        }
        if let Some(f) = a.feed {
            speeds.insert("normal".to_owned(), json!(f));
        }
        axes.push(json!({
            "letter": a.name,
            "direction": direction,
            "speeds": Value::Object(speeds),
        }));
    }
    // Rates the script reads as ceilings: the fastest stated axis.
    let rapid = m.axes.iter().filter_map(|a| a.rapid).fold(0.0_f64, f64::max);
    let feed = m.axes.iter().filter_map(|a| a.feed).fold(0.0_f64, f64::max);
    let mut head = serde_json::Map::new();
    if let Some(s) = m.spindle_max {
        head.insert("spindle".to_owned(), json!({"max": s}));
    }
    json!({
        "name": m.model,
        "kinematics": { "static": { "axes": axes, "head": Value::Object(head) } },
        "rapidRate": rapid,
        "feedRateMax": feed,
    })
}

/// Walk a path of object keys; `None` on any missing or mismatched hop.
fn get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k)?;
    }
    Some(cur)
}

fn f64_or(v: Option<&Value>, default: f64) -> f64 {
    v.and_then(|x| x.as_f64()).unwrap_or(default)
}

/// The flattened object the script's `machine` global sees, per the
/// format doc: name, rate maxima, spindle bound, coolant names.
pub fn script_props(m: &Value) -> Value {
    // An axes-array form contributes the max rapid across axes (the
    // post uses rapidRate as a ceiling, so the max is the permissive
    // bound).
    let axes_rapid = get(m, &["kinematics", "static", "axes"])
        .and_then(Value::as_array)
        .map(|axes| {
            axes.iter()
                .map(|a| f64_or(get(a, &["speeds", "rapid"]), 0.0))
                .fold(0.0_f64, f64::max)
        })
        .unwrap_or(0.0);
    let rapid = f64_or(get(m, &["rapidRate"]), 0.0).max(axes_rapid);
    let feed = f64_or(get(m, &["feedRateMax"]), 0.0);
    let spindle = f64_or(
        get(m, &["kinematics", "static", "head", "spindle", "max"]).or(get(m, &["spindleMax"])),
        0.0,
    );
    let coolants: Vec<Value> = get(m, &["kinematics", "static", "head", "toolStation", "coolants"])
        .or(get(m, &["coolants"]))
        .and_then(Value::as_array)
        .map(|c| {
            c.iter()
                .map(|e| {
                    e.as_str().map(Value::from).unwrap_or_else(|| {
                        json!({
                            "name": get(e, &["name"]).and_then(Value::as_str).unwrap_or(""),
                            "mCode": get(e, &["mCode"]).and_then(Value::as_str).unwrap_or(""),
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "name": get(m, &["name"]).and_then(Value::as_str).unwrap_or(""),
        "capabilities": get(m, &["capabilities"]).cloned().unwrap_or_else(|| json!({})),
        "rapidRate": f64::max(rapid, axes_rapid),
        "feedRateMax": feed,
        "spindleMax": spindle,
        "coolants": coolants,
    })
}
