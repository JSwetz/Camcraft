//! The MCP tool table (#128): name, description, JSON input schema and
//! the Rust handler, one row each - `tools/list` and `tools/call` read
//! the same rows, so a listed tool always runs and a running one is
//! always listed (the script host's VERBS law, at the MCP door).
//!
//! Provenance: our own tool surface over this repo's commands and the
//! script verbs; schemas are JSON Schema as the MCP spec carries them.

use super::{area, edit, features, job, pick, pockets, render};
use crate::script::{api, Env};

use serde_json::{json, Map, Value};

pub(super) type Handler = fn(&mut Env, &Map<String, Value>) -> Result<Value, String>;

pub(super) struct Tool {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) schema: fn() -> Value,
    pub(super) run: Handler,
}

const TOOLS: &[Tool] = &[
    Tool { name: "open_project", description: "Open a .camproj project file, replacing the current document and clearing undo history.", schema: s_path, run: open_project },
    Tool { name: "save_project", description: "Save the document as a .camproj file. Without a path, saves over the file last opened or saved.", schema: s_opt_path, run: save_project },
    Tool { name: "import_model", description: "Import a mesh model (STL, STEP or OBJ) through the app's importers. A name already used by a model updates that model.", schema: s_import, run: import_model },
    Tool { name: "get_job", description: "The whole document as JSON: setups, stock, work offsets, models, tools, and operations with their parameters and status.", schema: s_empty, run: job::get_job },
    Tool { name: "list_operation_types", description: "Every operation kind with its parameter fields, JSON types and default values, and the tool kinds it accepts.", schema: s_empty, run: job::list_operation_types },
    Tool { name: "list_tool_catalog", description: "The built-in tool catalog: names and definitions add_tool can copy into the document.", schema: s_empty, run: job::list_tool_catalog },
    Tool { name: "add_tool", description: "Add a tool to the document, from the catalog by name or from a full definition (the shape get_job shows).", schema: s_add_tool, run: edit::add_tool },
    Tool { name: "export_tool_library", description: "Write the document's tools to a tool library file (.camtools), the same format the Tool library page's Export writes. Refuses a document with no tools or a path that cannot be written.", schema: s_library, run: export_tool_library },
    Tool { name: "import_tool_library", description: "Import a tool library file into the document as one undo step, through the same command as the Tool library page's Import: an identical tool is skipped, a clashing name arrives as \"Name (2)\", a malformed or insane file is refused naming the tool.", schema: s_library, run: import_tool_library },
    Tool { name: "set_stock", description: "Set the active setup's stock: a box, a cylinder, or a copy of a model.", schema: s_stock, run: edit::set_stock },
    Tool { name: "add_operation", description: "Add an operation of a kind (see list_operation_types) with a tool, on the stock or a model, seeded with the kind's defaults and an optional params patch. Not generated until generate runs.", schema: s_add_op, run: edit::add_operation },
    Tool { name: "edit_operation", description: "Patch an operation's parameters: the given fields replace the current ones (JSON merge patch over the kind's fields; null clears an optional field). One undo step.", schema: s_edit_op, run: edit::edit_operation },
    Tool { name: "detect_holes", description: "Fill a Drill, Bore or Thread operation's hole list from its source model's holes (the app's Detect holes button). One undo step.", schema: s_name, run: edit::detect_holes },
    Tool { name: "detect_pockets", description: "A model's recognised pockets and slots (closed or open pockets, straight slots with open, round or square ends): boundary, floor and top Z, depth, tightest inside corner, slot width and open sides. With create, adds the operation that machines one of them (a Pocket over its floor, or a Slot along its centreline). One undo step.", schema: pockets::s_detect_pockets, run: pockets::detect_pockets },
    Tool { name: "delete_operation", description: "Delete an operation and its cached toolpath.", schema: s_name, run: edit::delete_operation },
    Tool { name: "set_tool", description: "Assign a document tool to an operation (the tool must suit the operation's kind).", schema: s_set_tool, run: edit::set_tool },
    Tool { name: "list_features", description: "A model's pickable features with stable ids: faces (each boundary loop with the edge chains that run it), edge chains, welded smooth faces and vertices, in world coordinates.", schema: pick::s_list_features, run: features::list_features },
    Tool { name: "get_operation_geometry", description: "What an operation machines: its source (stock, a model, or picked features), its area controls, and what the picks resolve to.", schema: pick::s_op, run: pick::get_operation_geometry },
    Tool { name: "set_operation_geometry", description: "Set the features an operation machines (faces, edge chains, loops) by list_features ids. One undo step; undo restores the previous source.", schema: pick::s_set_geometry, run: pick::set_operation_geometry },
    Tool { name: "set_operation_area", description: "Replace an operation's area controls (containment, offset, corner, extension, extra include/exclude boundaries, open boundary). One undo step.", schema: area::s_set_area, run: area::set_operation_area },
    Tool { name: "undo", description: "Undo the last document change.", schema: s_empty, run: edit::undo },
    Tool { name: "redo", description: "Redo the last undone change.", schema: s_empty, run: edit::redo },
    Tool { name: "generate", description: "Generate toolpaths: one named operation (always regenerated) or, with no name, every operation that is not current.", schema: s_opt_name, run: generate },
    Tool { name: "verify", description: "Verify the job without posting: per-operation status and findings (op, move index, kind, message, whether it blocks the post) plus the program-level post gate.", schema: s_opt_name, run: job::verify },
    Tool { name: "post", description: "Post the program to an NC file through a post processor (a built-in name or a .cps path; default the document's own). Returns the written files, or refuses with the reasons.", schema: s_post, run: post },
    Tool { name: "render_stock", description: "Render the program's simulated stock as a PNG (an image content block), painted by deviation class (clean, rest, gouge, unjudged), optionally with the toolpath over it; plus rest and gouge volume and the bounds of the stock and the rest. Generate first.", schema: render::schema, run: render::render_stock },
];

/// Every row: this table, then the setup furniture's (#181).
fn all() -> impl Iterator<Item = &'static Tool> {
    TOOLS.iter().chain(super::setup_tools::TOOLS).chain(super::fixture_tools::TOOLS).chain(super::align_tools::TOOLS)
}

/// The `tools/list` rows.
pub(super) fn list() -> Vec<Value> {
    all()
        .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": (t.schema)() }))
        .collect()
}

/// Run one tool by name. An unknown name is a refusal like any other.
pub(super) fn call(env: &mut Env, name: &str, args: &Value) -> Result<Value, String> {
    let Some(tool) = all().find(|t| t.name == name) else {
        let names: Vec<&str> = all().map(|t| t.name).collect();
        return Err(format!("unknown tool '{name}' - tools: {}", names.join(", ")));
    };
    let empty = Map::new();
    let map = args.as_object().unwrap_or(&empty);
    // Typos are refused, never ignored: an argument the schema does not
    // name would otherwise read as "done as asked".
    let schema = (tool.schema)();
    let known = schema["properties"].as_object();
    for key in map.keys() {
        if known.is_none_or(|k| !k.contains_key(key)) {
            return Err(format!("{name}: unknown argument \"{key}\""));
        }
    }
    (tool.run)(env, map).map_err(|e| format!("{name}: {e}"))
}

// ---- argument readers -------------------------------------------------

pub(super) fn req_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    match a.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s),
        None | Some(Value::Null) => Err(format!("\"{key}\" is required")),
        Some(other) => Err(format!("\"{key}\" must be a non-empty string, got {other}")),
    }
}

pub(super) fn opt_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(format!("\"{key}\" must be a string, got {other}")),
    }
}

// ---- handlers that are a script verb, re-dressed ----------------------

fn open_project(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let path = req_str(a, "path")?;
    api::v_load(env, vec![json!(path)])
}

fn save_project(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let args = opt_str(a, "path")?.map(|p| vec![json!(p)]).unwrap_or_default();
    api::v_save(env, args)
}

fn import_model(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let mut args = vec![json!(req_str(a, "path")?)];
    if let Some(n) = opt_str(a, "name")? {
        args.push(json!(n));
    }
    api::v_import(env, args)
}

fn export_tool_library(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    api::v_export_tools(env, vec![json!(req_str(a, "path")?)])
}

fn import_tool_library(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    api::v_import_tools(env, vec![json!(req_str(a, "path")?)])
}

fn generate(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let args = opt_str(a, "name")?.map(|n| vec![json!(n)]).unwrap_or_default();
    let ops = api::v_generate(env, args)?;
    Ok(json!({ "generated": ops }))
}

fn post(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let to = req_str(a, "output")?;
    let mut opts = json!({ "to": to });
    if let Some(p) = opt_str(a, "post")? {
        opts["cps"] = json!(p);
    }
    let mut out = api::v_post(env, vec![opts])?;
    // #388: a program over several mounts names its files itself;
    // otherwise what v_post itself reports as written, not the path asked for.
    if out.get("files").is_none() {
        out["files"] = json!([out.get("path").cloned().unwrap_or_else(|| json!(to))]);
    }
    Ok(out)
}

// ---- input schemas ----------------------------------------------------

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn s_empty() -> Value {
    obj(json!({}), &[])
}

fn s_path() -> Value {
    obj(json!({ "path": { "type": "string", "description": "File path." } }), &["path"])
}

fn s_opt_path() -> Value {
    obj(json!({ "path": { "type": "string", "description": "File path (optional)." } }), &[])
}

fn s_library() -> Value {
    obj(json!({ "path": { "type": "string", "description": "A tool library file (.camtools)." } }), &["path"])
}

fn s_import() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "An .stl, .step/.stp or .obj file." },
            "name": { "type": "string", "description": "Model name; defaults to the file stem." }
        }),
        &["path"],
    )
}

fn s_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "Operation name (key or label)." } }), &["name"])
}

fn s_opt_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "Operation name; omit for all." } }), &[])
}

fn s_add_tool() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Tool name in the document; defaults to the catalog name." },
            "catalog": { "type": "string", "description": "A list_tool_catalog name to copy." },
            "definition": { "type": "object", "description": "A full tool definition (get_job's tool shape)." }
        }),
        &[],
    )
}

fn s_stock() -> Value {
    obj(
        json!({
            "box": { "type": "object", "description": "{dx, dy, dz} in mm; centered in XY, z from 0 to dz.",
                "properties": { "dx": {"type": "number"}, "dy": {"type": "number"}, "dz": {"type": "number"} },
                "required": ["dx", "dy", "dz"] },
            "cylinder": { "type": "object", "description": "{diameter, height} in mm.",
                "properties": { "diameter": {"type": "number"}, "height": {"type": "number"} },
                "required": ["diameter", "height"] },
            "from_model": { "type": "string", "description": "A model name whose mesh becomes the stock." }
        }),
        &[],
    )
}

fn s_add_op() -> Value {
    obj(
        json!({
            "kind": { "type": "string", "description": "Operation kind: label or key from list_operation_types." },
            "tool": { "type": "string", "description": "A document tool name (add_tool first)." },
            "name": { "type": "string", "description": "Operation name; defaults to the kind's label plus a number." },
            "source": { "type": "string", "description": "\"stock\" (default) or a model name." },
            "params": { "type": "object", "description": "Fields to change from the kind's defaults." }
        }),
        &["kind", "tool"],
    )
}

fn s_edit_op() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Operation name (key or label)." },
            "params": { "type": "object", "description": "Fields to change (see get_job for the current ones)." }
        }),
        &["name", "params"],
    )
}

fn s_set_tool() -> Value {
    obj(
        json!({
            "operation": { "type": "string", "description": "Operation name (key or label)." },
            "tool": { "type": "string", "description": "A document tool name." }
        }),
        &["operation", "tool"],
    )
}

fn s_post() -> Value {
    obj(
        json!({
            "output": { "type": "string", "description": "The .nc file to write." },
            "post": { "type": "string", "description": "A built-in post name (Fanuc, Haas, Siemens 840D, Heidenhain, Mazak) or a .cps path." }
        }),
        &["output"],
    )
}
