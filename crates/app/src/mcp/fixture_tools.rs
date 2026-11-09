//! The MCP fixture tools (#181): mark an imported model as workholding,
//! place it, unmark it, and read the verifier's live fixture findings
//! back as data. Marking and moving are the fixture commands the app's
//! fixtures page issues, one History step each; the findings are the
//! post gate's own workholding walk (`posting::fixture_findings`), so
//! this tool and the post can never disagree about the steel.
//!
//! Provenance: our own tool surface over this repo's fixture commands
//! and verifier; fixtures as imported solids is plain shop practice.

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point, setup_or_active};
use super::tools::{req_str, Tool};
use crate::commands::{MoveFixture, TagFixture, UntagFixture};
use crate::fixture;
use crate::model;
use crate::schema::Location;
use crate::script::{fnum, Env};
use crate::setup;
use crate::verify::Warning;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "add_fixture", description: "Mark an imported model as a fixture (workholding steel the verifier checks every toolpath against) of a setup - the active one by default. Import the model with import_model first.", schema: s_add_fixture, run: add_fixture },
    Tool { name: "remove_fixture", description: "Unmark a fixture: the model stays, the verifier stops checking against it.", schema: s_fixture_name, run: remove_fixture },
    Tool { name: "move_fixture", description: "Place a fixture: position [x, y, z] in mm and an optional unit quaternion rotation [x, y, z, w] (omitted keeps the current one).", schema: s_move_fixture, run: move_fixture },
    Tool { name: "get_fixture_findings", description: "The verifier's fixture check, live: every setup's fixtures and machine table with their boxes, and each collision of a toolpath with them (op, move index, struck part, message). Needs every operation generated.", schema: s_empty, run: get_fixture_findings },
];

fn add_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "model")?;
    let target = setup_or_active(env, a)?;
    let cmd = TagFixture::new(&env.world, name, target.as_deref()).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "fixture": name, "setup": target }))
}

fn remove_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let cmd = UntagFixture::new(&env.world, name).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "removed": name }))
}

fn move_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let pos = opt_nums::<3>(a, "position")?.ok_or("\"position\" is required")?;
    let current = model::find_by_name(&env.world, name).and_then(|e| model::transform_of(&env.world, &e));
    let rot = match opt_nums::<4>(a, "rotation")? {
        Some(q) => q,
        None => current.unwrap_or_default().rot,
    };
    let cmd = MoveFixture::new(&env.world, name, Location { pos: point(pos), rot }).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let placed = model::find_by_name(&env.world, name).and_then(|e| model::transform_of(&env.world, &e));
    let mut out = loc_view(&placed.unwrap_or_default());
    out["fixture"] = json!(name);
    Ok(out)
}

fn bounds(min: [f64; 3], max: [f64; 3]) -> Value {
    json!({ "min": min.map(fnum), "max": max.map(fnum) })
}

/// Every setup context's steel: the tagged models, then the table.
fn workholding_view(env: &Env) -> Vec<Value> {
    let w = &env.world;
    let mut contexts: Vec<Option<String>> = setup::names(w).into_iter().map(Some).collect();
    if contexts.is_empty() {
        contexts.push(None);
    }
    contexts
        .iter()
        .map(|s| {
            let d = s.as_deref();
            let fixtures: Vec<Value> = fixture::tagged(w, d)
                .iter()
                .map(|(n, e)| {
                    let b = model::entity_world_aabb(w, e);
                    json!({
                        "name": n,
                        "fixed": fixture::def_of(w, e).is_some_and(|f| f.fixed),
                        "bounds": b.map(|b| bounds([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])),
                    })
                })
                .collect();
            let table = fixture::table_box(w, d).map(|b| bounds([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z]));
            let machine = crate::machine::of_setup(w, d).1.map(|m| m.model);
            json!({ "setup": s, "machine": machine, "fixtures": fixtures, "table": table })
        })
        .collect()
}

fn get_fixture_findings(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let w = &env.world;
    let mut out = json!({
        "workholding": workholding_view(env),
        // The active setup's machine (kept for older callers); each
        // workholding entry names its own setup's machine (#388).
        "machine": crate::machine::of(w).1.map(|m| m.model),
    });
    match crate::posting::fixture_findings(w) {
        Ok(found) => {
            let findings: Vec<Value> = found
                .hits
                .iter()
                .map(|(key, hit)| {
                    let label = model::find_by_name(w, key).map_or_else(|| key.clone(), |e| model::operation_label(w, key, &e));
                    let mut f = super::job::finding(&label, hit);
                    f["key"] = json!(key);
                    if let Warning::FixtureHit { fixture_part, .. } = hit {
                        f["part"] = serde_json::to_value(fixture_part).unwrap_or(Value::Null);
                    }
                    f
                })
                .collect();
            out["judged"] = json!(true);
            out["ok"] = json!(findings.is_empty() && found.unjudged.is_empty());
            out["findings"] = Value::Array(findings);
            // #368: an op the walk could not judge is a problem, by name.
            out["unjudged"] = json!(found.unjudged);
        }
        // No verdict over stale paths: said, never read as "clear".
        Err(reason) => {
            out["judged"] = json!(false);
            out["ok"] = json!(false);
            out["findings"] = json!([]);
            out["skipped"] = json!(reason);
        }
    }
    Ok(out)
}

// ---- input schemas ----------------------------------------------------

fn s_empty() -> Value {
    obj(json!({}), &[])
}

fn s_add_fixture() -> Value {
    obj(
        json!({
            "model": { "type": "string", "description": "An imported model's name." },
            "setup": { "type": "string", "description": "The declared setup it holds the part in; defaults to the active one." }
        }),
        &["model"],
    )
}

fn s_fixture_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "A fixture's name." } }), &["name"])
}

fn s_move_fixture() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "A fixture's name." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "World position [x, y, z] in mm." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w]; omitted keeps the current rotation." }
        }),
        &["name", "position"],
    )
}
