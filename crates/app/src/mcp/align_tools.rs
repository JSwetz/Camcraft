//! The MCP part-alignment tools: turn the part in a setup's work plane
//! from picks (#557) - a straight edge, or two points, for +Y and/or +X -
//! through the same undoable `AlignPartToSetup::turn_in_plane` the Setups
//! page's "Axes in the plane" rows queue; and lay it on a plane through
//! three picked points (#555), through `AlignPartToSetup::lay_on_points`
//! like the "Lay on a plane - pick three points" row. Every refusal is
//! the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's setup alignment
//! command; no vendor material consulted.

use super::pick::{pick_list_schema, picks_of};
use super::setup_tools::{cmd_err, loc_view, obj, setup_or_active};
use super::tools::{Tool, opt_str};
use crate::commands::AlignPartToSetup;
use crate::cl::ArcPlane;
use crate::pick::{AxisPick, InPlaneAxes, InPlaneAxis, PlaneLay, PointsLay, in_plane_turn_deg, points_side};
use crate::script::{Env, fnum};
use crate::setup_align;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "set_part_axes",
        description: "Turn the part in a setup's work plane (its XY) so a picked straight edge, or two picked points, runs along +Y and/or +X. Picks are list_features ids: one edge chain, or two vertices (first to second). The direction is projected into the plane; flip_y / flip_x reverse it; with both axes given, primary (default \"y\") is kept exactly and the other is only checked. Composes onto the setup's current part alignment; one undo step. Refused by name: a face, a curved edge, coincident points, a direction along Z, parallel or left-handed X and Y.",
        schema: s_set_part_axes,
        run: set_part_axes,
    },
    Tool {
        name: "lay_part_on_points",
        description: "Lay the part on a setup's G17 (XY), G18 (ZX) or G19 (YZ) plane through three picked points (list_features vertex ids). The plane through them is turned so its outside (the side away from the part's material) faces the plane's axis (+Z for G17), or the opposite way with flip (G17 + flip: the points rest on the table, part above), and point 1 is set onto the plane's level (place, default true). frame (3-2-1) also turns the part so point 1 -> 2 runs along +X (+Y for G19) and moves point 1 to the setup origin. Absolute like the face pick; one undo step; the work offset is untouched. Refused by name: not three vertices, points closer than 0.001 mm, (nearly) collinear points (under 1 degree of spread), a pick off the part, no model.",
        schema: s_lay_part_on_points,
        run: lay_part_on_points,
    },
];

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

fn lay_part_on_points(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let plane = match opt_str(a, "plane")? {
        None | Some("g17") => ArcPlane::Xy,
        Some("g18") => ArcPlane::Xz,
        Some("g19") => ArcPlane::Yz,
        Some(other) => return Err(format!("\"plane\" must be \"g17\", \"g18\" or \"g19\", got \"{other}\"")),
    };
    let place = match a.get("place") {
        None | Some(Value::Null) => true,
        _ => flag(a, "place")?,
    };
    let picks = picks_of(env, a.get("points"), "points")?;
    let pl = PointsLay { lay: PlaneLay { plane, flip: flag(a, "flip")?, place }, frame: flag(a, "frame")? };
    let side = points_side(&env.world, &picks)?;
    let cmd = AlignPartToSetup::lay_on_points(&env.world, &setup, &picks, pl).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let held = setup_align::stored(&env.world, &setup);
    Ok(json!({
        "setup": setup,
        "outside": side,
        "alignment": held.as_ref().map(loc_view),
        "description": setup_align::describe(held),
    }))
}

fn s_lay_part_on_points() -> Value {
    obj(
        json!({
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." },
            "points": pick_list_schema("Exactly three vertices (points 1, 2, 3)."),
            "plane": { "type": "string", "enum": ["g17", "g18", "g19"], "description": "The plane to lay on (default g17)." },
            "flip": { "type": "boolean", "description": "Face the points' outside the opposite way (-Z for G17: rest the points on the table)." },
            "place": { "type": "boolean", "description": "Set point 1 onto the plane's level (default true); false keeps the part where it is." },
            "frame": { "type": "boolean", "description": "3-2-1: point 1 to the origin, point 1 -> 2 along +X (+Y for G19)." }
        }),
        &["points"],
    )
}
