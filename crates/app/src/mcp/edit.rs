//! The write side of the MCP tools (#128). Every change here is a real
//! command executed through the session's History - the same commands
//! the app's panels issue - so `undo` / `redo` walk exactly what the
//! agent did, and every construction-time refusal (bad numbers, a tool
//! the kind cannot cut with, a name taken) comes back as its text.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::tools::{opt_str, req_str};
use crate::commands::{
    AddTool, CommandError, CreateOperation, DeleteOperation, EditOperationLinks, EditOperationParams, SetStock,
};
use crate::model;
use crate::operation::{self, Height, HeightReference, KindInfo, OpParams, SrcRef};
use crate::project;
use crate::script::{inventory, Env};
use crate::stock::StockShape;

use serde_json::{json, Map, Value};

fn cmd_err(e: CommandError) -> String {
    e.to_string()
}

/// The draft every new operation is seeded from: a facing pass in
/// references (25 above the stock top, skimming the top), carried into
/// the kind through the registry's retype law - the same seed shape the
/// app's quick-add uses, so an agent's new op and a clicked one agree.
pub(super) fn seed(kind: usize) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    operation::retype_params(&base, kind)
}

/// A kind by its label ("Pocket", any case) or its file key ("pocket").
fn kind_index(name: &str) -> Result<usize, String> {
    let want = name.trim().to_ascii_lowercase();
    (0..KindInfo::ALL.len())
        .find(|&i| {
            KindInfo::ALL.get(i).is_some_and(|k| k.label.to_ascii_lowercase() == want)
                || super::job::kind_defaults(i).0 == want
        })
        .ok_or_else(|| format!("unknown operation kind '{name}' - see list_operation_types"))
}

/// An operation by its stable key or its display label.
pub(super) fn find_op(env: &Env, name: &str) -> Result<String, String> {
    inventory(env)
        .into_iter()
        .find(|(k, e, _)| k == name || model::operation_label(&env.world, k, e) == name)
        .map(|(k, _, _)| k)
        .ok_or_else(|| format!("no operation named '{name}'"))
}

/// Apply a params patch (JSON merge patch, RFC 7386, over the kind's
/// field object) and read the result back as domain params. A key the
/// kind does not have is refused by name - serde would drop it, and a
/// silently dropped edit reads as "done as asked".
fn patched(base: &OpParams, patch: &Value) -> Result<OpParams, String> {
    let Value::Object(patch) = patch else {
        return Err("\"params\" must be an object of fields".into());
    };
    let Value::Object(tagged) = project::params_to_json(base) else {
        return Err("the operation's parameters could not be read".into());
    };
    let Some((key, Value::Object(mut fields))) = tagged.into_iter().next() else {
        return Err("the operation's parameters could not be read".into());
    };
    for (k, v) in patch {
        merge(fields.entry(k.clone()).or_insert(Value::Null), v);
        if v.is_null() {
            fields.remove(k);
        }
    }
    let mut tagged = Map::new();
    tagged.insert(key.clone(), Value::Object(fields));
    let next = project::params_from_json(&Value::Object(tagged))
        .map_err(|e| format!("bad params patch: {e}"))?;
    let Value::Object(back) = project::params_to_json(&next) else {
        return Err("the patched parameters could not be read back".into());
    };
    let back = back.get(&key).and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(path) = lost(patch, &back, "") {
        return Err(format!("bad params patch: unknown field {path} - a {key} operation has no field \"{path}\"; see list_operation_types"));
    }
    Ok(next)
}

/// The first key path of `patch` (dotted, e.g. `clearance_z.ofset`) the
/// typed round-trip did not keep, at any depth. A null value is a
/// removal (RFC 7386) and has nothing to keep; an object is checked key
/// by key against what came back; any other value is a leaf that must
/// simply be present.
fn lost(patch: &Map<String, Value>, back: &Map<String, Value>, prefix: &str) -> Option<String> {
    for (k, v) in patch {
        if v.is_null() {
            continue;
        }
        let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        match (v, back.get(k)) {
            (_, None) => return Some(path),
            (Value::Object(p), Some(Value::Object(b))) => {
                if let Some(deeper) = lost(p, b, &path) {
                    return Some(deeper);
                }
            }
            // An object patch whose field came back as a scalar or null
            // lost every key it carried.
            (Value::Object(p), Some(_)) => {
                if let Some(first) = p.iter().find(|(_, x)| !x.is_null()).map(|(n, _)| n) {
                    return Some(format!("{path}.{first}"));
                }
            }
            _ => {}
        }
    }
    None
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    t.remove(k);
                } else {
                    merge(t.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

pub(super) fn add_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let catalog = opt_str(a, "catalog")?;
    let (default_name, tool) = match (catalog, a.get("definition")) {
        (Some(c), None) => {
            let t = crate::tools::catalog()
                .into_iter()
                .find(|(n, _)| *n == c)
                .map(|(_, t)| t)
                .ok_or_else(|| format!("no catalog tool named '{c}' - see list_tool_catalog"))?;
            (Some(c.to_owned()), t)
        }
        (None, Some(d)) => (None, project::tool_from_json(d).map_err(|e| format!("bad tool definition: {e}"))?),
        _ => return Err("give exactly one of \"catalog\" or \"definition\"".into()),
    };
    let name = match opt_str(a, "name")? {
        Some(n) => n.to_owned(),
        None => default_name.ok_or("\"name\" is required with a definition")?,
    };
    let cmd = AddTool::new(&env.world, &name, tool).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "tool": name }))
}

pub(super) fn set_stock(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let given: Vec<&String> = a.keys().collect();
    if given.len() != 1 {
        return Err("give exactly one of \"box\", \"cylinder\" or \"from_model\"".into());
    }
    let shape = match (a.get("box"), a.get("cylinder"), opt_str(a, "from_model")?) {
        (Some(b), None, None) => project::stock_from_json(&json!({ "box": b })),
        (None, Some(c), None) => project::stock_from_json(&json!({ "cylinder": c })),
        (None, None, Some(m)) => Ok(StockShape::FromModel { source: m.to_owned() }),
        _ => Err("give exactly one stock shape".into()),
    }
    .map_err(|e| format!("bad stock: {e}"))?;
    let cmd = SetStock::new(&env.world, shape.clone(), Default::default()).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "stock": project::stock_to_json(&shape) }))
}

pub(super) fn add_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let kind = kind_index(req_str(a, "kind")?)?;
    let tool = req_str(a, "tool")?;
    let label = KindInfo::ALL.get(kind).map_or("Operation", |k| k.label);
    let name = match opt_str(a, "name")? {
        Some(n) => n.to_owned(),
        None => (1..)
            .map(|n| format!("{label}{n}"))
            .find(|c| model::find_by_name(&env.world, c).is_none())
            .unwrap_or_else(|| label.to_owned()),
    };
    let src = match opt_str(a, "source")? {
        None => SrcRef::Stock,
        Some(s) if s.eq_ignore_ascii_case("stock") => SrcRef::Stock,
        Some(s) => SrcRef::Entity(s.to_owned()),
    };
    let mut params = seed(kind);
    // The seed helix follows the tool, as the app's quick-add does.
    if let Some(t) = model::find_by_name(&env.world, tool).and_then(|e| model::tool_of(&env.world, &e)) {
        operation::seed_helix_for_tool(&mut params, t.diameter);
    }
    if let Some(patch) = a.get("params") {
        params = patched(&params, patch)?;
    }
    let cmd = CreateOperation::new(&env.world, &name, tool, src, params).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "operation": name, "kind": label, "status": "never" }))
}

pub(super) fn edit_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let patch = a.get("params").ok_or("\"params\" is required")?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let next = patched(&op.params, patch)?;
    let cmd = EditOperationParams::new(&env.world, &key, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let params = model::operation_of(&env.world, &e).map(|o| project::params_to_json(&o.params));
    Ok(json!({ "operation": key, "status": "stale", "params": params }))
}

/// The app's Detect holes button over MCP (#312): the operation's
/// source read for holes now (`operation::detect_holes`, the button's
/// own rule) and stored as one undoable parameter edit. Answers with
/// how many axial and radial holes went in.
pub(super) fn detect_holes(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let next = operation::detect_holes(&env.world, &key, &op.params, op.setup.as_deref(), &op.src)?;
    let (holes, radial) = match &next {
        OpParams::Drill { holes, cross, .. } => (holes.len(), cross.len()),
        OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => (holes.len(), 0),
        _ => (0, 0),
    };
    let cmd = EditOperationParams::new(&env.world, &key, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "operation": key, "holes": holes, "radial_holes": radial, "status": "stale" }))
}

pub(super) fn delete_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let cmd = DeleteOperation::new(&env.world, &key).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "deleted": key }))
}

pub(super) fn set_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let tool = req_str(a, "tool")?;
    let cmd = EditOperationLinks::new(&env.world, &key, Some(tool), None).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "operation": key, "tool": tool }))
}

fn history_state(env: &Env) -> Value {
    json!({ "can_undo": env.history.can_undo(), "can_redo": env.history.can_redo() })
}

pub(super) fn undo(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    if !env.history.undo(&env.world) {
        return Err("nothing to undo".into());
    }
    let mut out = history_state(env);
    out["undone"] = json!(true);
    Ok(out)
}

pub(super) fn redo(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    if !env.history.redo(&env.world) {
        return Err("nothing to redo".into());
    }
    let mut out = history_state(env);
    out["redone"] = json!(true);
    Ok(out)
}
