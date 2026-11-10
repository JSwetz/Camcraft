//! The MCP part-placement tool (#716): move and turn an imported part
//! through the same undoable `MoveModel` the GUI's model move issues, so
//! a headless session can bring a part's bottom to Z0 or stand it up.
//! Every refusal is the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's `MoveModel`
//! command; no vendor material consulted.

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point};
use super::tools::{req_str, Tool};
use crate::commands::MoveModel;
use crate::model;
use crate::schema::Location;
use crate::script::Env;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "move_model",
    description: "Place an imported part: position [x, y, z] in mm and an optional unit quaternion rotation [x, y, z, w] (omitted keeps the current one). Stock made from the part, or the stock of a one-part document, moves with it and is listed under \"stock\". One undo step. Refused by name: a fixture (move_fixture places it), a work offset (set_wcs), the machine table, an unknown name, a rotation that is not a quaternion.",
    schema: s_move_model,
    run: move_model,
}];

/// The named entity's current placement; identity when it has none.
fn pose_of(env: &Env, name: &str) -> Location {
    model::find_by_name(&env.world, name)
        .and_then(|e| model::transform_of(&env.world, &e))
        .unwrap_or_default()
}

fn move_model(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let pos = opt_nums::<3>(a, "position")?.ok_or("\"position\" is required")?;
    let rot = match opt_nums::<4>(a, "rotation")? {
        Some(q) => q,
        None => pose_of(env, name).rot,
    };
    let cmd = MoveModel::new(&env.world, name, Location { pos: point(pos), rot }).map_err(cmd_err)?;
    // `execute` takes the command, so read who rides along first.
    let followers: Vec<String> = cmd.following_stock().map(str::to_owned).collect();
    env.history.execute(&env.world, cmd);
    let stock: Vec<Value> = followers
        .iter()
        .map(|s| {
            let mut view = loc_view(&pose_of(env, s));
            view["name"] = json!(s);
            view
        })
        .collect();
    let mut out = loc_view(&pose_of(env, name));
    out["model"] = json!(name);
    out["stock"] = json!(stock);
    Ok(out)
}

fn s_move_model() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "An imported part's name (not a fixture or work offset)." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "World position [x, y, z] in mm." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w]; omitted keeps the current rotation." }
        }),
        &["name", "position"],
    )
}
