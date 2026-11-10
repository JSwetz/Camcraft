//! The MCP part-alignment tool (#557): turn the part in a setup's work
//! plane from picks - a straight edge, or two points, for +Y and/or +X -
//! through the same undoable `AlignPartToSetup::turn_in_plane` the Setups
//! page's "Axes in the plane" rows queue. Every refusal is the command's
//! own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's setup alignment
//! command; no vendor material consulted.

use super::pick::{pick_list_schema, picks_of};
use super::setup_tools::{cmd_err, loc_view, obj, setup_or_active};
use super::tools::{Tool, opt_str};
use crate::commands::AlignPartToSetup;
use crate::pick::{AxisPick, InPlaneAxes, InPlaneAxis, in_plane_turn_deg};
use crate::script::{Env, fnum};
use crate::setup_align;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_part_axes",
    description: "Turn the part in a setup's work plane (its XY) so a picked straight edge, or two picked points, runs along +Y and/or +X. Picks are list_features ids: one edge chain, or two vertices (first to second). The direction is projected into the plane; flip_y / flip_x reverse it; with both axes given, primary (default \"y\") is kept exactly and the other is only checked. Composes onto the setup's current part alignment; one undo step. Refused by name: a face, a curved edge, coincident points, a direction along Z, parallel or left-handed X and Y.",
    schema: s_set_part_axes,
    run: set_part_axes,
}];

fn flag(a: &Map<String, Value>, key: &str) -> Result<bool, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("\"{key}\" must be true or false, got {other}")),
    }
}

fn axis_of(env: &Env, a: &Map<String, Value>, key: &str, flip: &str) -> Result<Option<AxisPick>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        v => Ok(Some(AxisPick { picks: picks_of(env, v, key)?, flip: flag(a, flip)? })),
    }
}

fn set_part_axes(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let primary = match opt_str(a, "primary")? {
        None | Some("y") => InPlaneAxis::Y,
        Some("x") => InPlaneAxis::X,
        Some(other) => return Err(format!("\"primary\" must be \"y\" or \"x\", got \"{other}\"")),
    };
    let axes = InPlaneAxes { y: axis_of(env, a, "y", "flip_y")?, x: axis_of(env, a, "x", "flip_x")?, primary };
    let turn = in_plane_turn_deg(&env.world, &setup, &axes)?;
    let cmd = AlignPartToSetup::turn_in_plane(&env.world, &setup, &axes).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let held = setup_align::stored(&env.world, &setup);
    Ok(json!({
        "setup": setup,
        "turn_deg": fnum(turn),
        "alignment": held.as_ref().map(loc_view),
        "description": setup_align::describe(held),
    }))
}

fn s_set_part_axes() -> Value {
    obj(
        json!({
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." },
            "y": pick_list_schema("+Y: one straight edge chain, or two vertices (first to second)."),
            "x": pick_list_schema("+X: one straight edge chain, or two vertices (first to second)."),
            "flip_y": { "type": "boolean", "description": "Take the +Y pick the other way." },
            "flip_x": { "type": "boolean", "description": "Take the +X pick the other way." },
            "primary": { "type": "string", "enum": ["y", "x"], "description": "With both axes given, the one kept exactly (default y)." }
        }),
        &[],
    )
}
