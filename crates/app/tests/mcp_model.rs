//! The MCP part-model tools, driven in-process over the server's
//! JSON-RPC handler: a part moved and turned with the stock that rides
//! along (#716), an open STL's missing face filled by repair (#717), one
//! undo step each, and every bad input a tool error that names it and
//! enters no history.
//!
//! Gate map (M4.4): covers app::mcp::model_tools, app::commands::move_model,
//! app::commands::repair_model.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How far a reported coordinate may sit from the exact answer: the
/// quarter turn below goes through a normalized quaternion, so its
/// corners land within a few ulps of the round numbers, not on them.
const COORD_TOL: f64 = 1e-9;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-model-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// The box's six faces as corner indices (bit 0 = +x, bit 1 = +y,
/// bit 2 = top); `TOP_FACE` is the one at z = `z`.
const BOX_QUADS: [[usize; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
const TOP_FACE: usize = 1;

/// A `2x` by `2y` by `z` box as ASCII STL, centered in XY, z from 0.
fn write_box_stl(path: &Path, dims: (f64, f64, f64)) {
    write_box_faces(path, dims, None);
}

/// The same box with its top face left out: 10 triangles around one
/// 4-edge open loop, the hole a repair fills with 2 triangles.
fn write_open_top_box_stl(path: &Path, dims: (f64, f64, f64)) {
    write_box_faces(path, dims, Some(TOP_FACE));
}

fn write_box_faces(path: &Path, (x, y, z): (f64, f64, f64), skip: Option<usize>) {
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -x } else { x }, if i & 2 == 0 { -y } else { y }, if i & 4 == 0 { 0.0 } else { z }]
    };
    let mut s = String::from("solid box\n");
    for (face, q) in BOX_QUADS.iter().enumerate() {
        if Some(face) == skip {
            continue;
        }
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for i in tri {
                let p = v(i);
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    std::fs::write(path, s).expect("write stl");
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    /// The named model's world bounds, as get_job reports them.
    fn model_bounds(&mut self, name: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        let models = job["models"].as_array().cloned().unwrap_or_default();
        let model = models.iter().find(|m| m["name"] == json!(name));
        model.map(|m| m["bounds"].clone()).unwrap_or_else(|| panic!("no model {name}: {job}"))
    }

    /// The first setup's stock bounds, as get_job reports them.
    fn stock_bounds(&mut self) -> Value {
        self.ok("get_job", json!({}))["setups"][0]["stock_bounds"].clone()
    }

    /// How many facets the named model has, as get_job reports them.
    fn facets(&mut self, name: &str) -> Option<u64> {
        let job = self.ok("get_job", json!({}));
        let models = job["models"].as_array().cloned().unwrap_or_default();
        models.iter().find(|m| m["name"] == json!(name)).and_then(|m| m["facets"].as_u64())
    }
}

/// `v` read as `N` numbers (a bounds corner, a quaternion).
fn nums<const N: usize>(v: &Value) -> [f64; N] {
    let mut out = [f64::NAN; N];
    for (o, x) in out.iter_mut().zip(v.as_array().unwrap_or_else(|| panic!("not an array: {v}"))) {
        *o = x.as_f64().unwrap_or(f64::NAN);
    }
    out
}

fn assert_near<const N: usize>(got: [f64; N], want: [f64; N], what: &str) {
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() <= COORD_TOL, "{what}: got {got:?}, want {want:?}");
    }
}

/// A one-part document's box stock follows a part move, and one undo
/// puts both back. Guards #716's "the stock that followed" report: a
/// move that silently left the stock behind (or moved it without
/// saying so) would desync what the agent believes from what the sim
/// cuts.
#[test]
fn a_moved_part_carries_a_one_part_documents_stock_and_one_undo_puts_both_back() {
    let dir = tmp_dir("stock");
    let part = dir.join("block.stl");
    write_box_stl(&part, (10.0, 5.0, 4.0));
    let mut c = Client::new();

    let tools = c.request("tools/list", json!({}))["result"]["tools"].clone();
    let row = tools.as_array().and_then(|t| t.iter().find(|t| t["name"] == json!("move_model")).cloned());
    let row = row.unwrap_or_else(|| panic!("move_model is listed: {tools}"));
    assert_eq!(row["inputSchema"]["required"], json!(["name", "position"]), "{row}");

    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 30.0, "dy": 20.0, "dz": 10.0 } }));
    let part_before = c.model_bounds("block");
    let stock_before = c.stock_bounds();
    assert_eq!(part_before["min"], json!([-10.0, -5.0, 0.0]), "{part_before}");

    // Drop the part 4 mm, so its top sits at Z0.
    let moved = c.ok("move_model", json!({ "name": "block", "position": [0.0, 0.0, -4.0] }));
    assert_eq!(moved["model"], json!("block"), "{moved}");
    assert_eq!(moved["origin"], json!([0.0, 0.0, -4.0]), "{moved}");
    assert_eq!(moved["rotation"], json!([0.0, 0.0, 0.0, 1.0]), "an omitted rotation keeps identity: {moved}");
    let stock = moved["stock"].as_array().cloned().unwrap_or_default();
    assert_eq!(stock.len(), 1, "the one-part document's stock rides along: {moved}");
    assert!(stock[0]["name"].as_str().is_some_and(|n| !n.is_empty()), "{moved}");

    let part_after = c.model_bounds("block");
    assert_eq!(part_after["min"], json!([-10.0, -5.0, -4.0]), "{part_after}");
    assert_eq!(part_after["max"], json!([10.0, 5.0, 0.0]), "{part_after}");
    let shifted = |corner: &Value| {
        let [x, y, z] = nums::<3>(corner);
        [x, y, z - 4.0]
    };
    let stock_after = c.stock_bounds();
    assert_near(nums(&stock_after["min"]), shifted(&stock_before["min"]), "stock min follows");
    assert_near(nums(&stock_after["max"]), shifted(&stock_before["max"]), "stock max follows");

    // One History step: undo puts part and stock back, redo replays both.
    c.ok("undo", json!({}));
    assert_eq!(c.model_bounds("block"), part_before);
    assert_eq!(c.stock_bounds(), stock_before);
    c.ok("redo", json!({}));
    assert_eq!(c.model_bounds("block"), part_after);
    assert_eq!(c.stock_bounds(), stock_after);
}

/// A quarter turn about Z swaps the part's X and Y extent, a rotation
/// given as a non-unit quaternion is normalized, and a later move with
/// no rotation keeps the turn. Guards the "omitted keeps the current
/// one" rule: resetting to identity would silently untwist the part.
#[test]
fn a_quarter_turn_swaps_x_and_y_and_a_later_move_without_rotation_keeps_it() {
    let dir = tmp_dir("turn");
    let part = dir.join("block.stl");
    write_box_stl(&part, (10.0, 5.0, 4.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));

    let half = std::f64::consts::FRAC_1_SQRT_2;
    // [0, 0, 2, 2] is a 90-degree turn about +Z, twice too long.
    let turned = c.ok("move_model", json!({ "name": "block", "position": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 2.0, 2.0] }));
    assert_near(nums(&turned["rotation"]), [0.0, 0.0, half, half], "the rotation is stored normalized");
    let b = c.model_bounds("block");
    assert_near(nums(&b["min"]), [-5.0, -10.0, 0.0], "X and Y extents swap");
    assert_near(nums(&b["max"]), [5.0, 10.0, 4.0], "X and Y extents swap");

    let lifted = c.ok("move_model", json!({ "name": "block", "position": [0.0, 0.0, 4.0] }));
    assert_near(nums(&lifted["rotation"]), [0.0, 0.0, half, half], "no rotation given keeps the turn");
    let b = c.model_bounds("block");
    assert_near(nums(&b["min"]), [-5.0, -10.0, 4.0], "still turned, now lifted");
    assert_near(nums(&b["max"]), [5.0, 10.0, 8.0], "still turned, now lifted");
}

/// Every refusal names what is wrong and leaves the history alone, so
/// the next undo undoes the last real step. Guards the "refusals are
/// data" rule: a refused move that still pushed a no-op step would make
/// the agent's next undo silently do nothing.
#[test]
fn move_model_refuses_by_name_and_a_refusal_enters_no_history() {
    let dir = tmp_dir("refuse");
    let part = dir.join("block.stl");
    let vise = dir.join("vise.stl");
    write_box_stl(&part, (10.0, 5.0, 4.0));
    write_box_stl(&vise, (20.0, 20.0, 15.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    c.ok("import_model", json!({ "path": vise.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 30.0, "dy": 20.0, "dz": 10.0 } }));
    let wcs = c.ok("set_wcs", json!({ "origin": [0.0, 0.0, 10.0] }));
    let wcs_name = c.ok("get_job", json!({}))["setups"][0]["wcs"]["entity"].as_str().map(str::to_owned);
    let wcs_name = wcs_name.unwrap_or_else(|| panic!("the placed work offset has an entity name: {wcs}"));
    c.ok("add_fixture", json!({ "model": "vise" }));

    let at = json!([0.0, 0.0, 0.0]);
    let why = c.refused("move_model", json!({ "name": "vise", "position": at }));
    assert!(why.contains("fixture"), "a fixture is placed by move_fixture: {why}");
    let why = c.refused("move_model", json!({ "name": wcs_name, "position": at }));
    assert!(why.contains("work offset"), "a work offset is placed by set_wcs: {why}");
    let why = c.refused("move_model", json!({ "name": "nothing", "position": at }));
    assert!(why.contains("nothing"), "an unknown name is quoted: {why}");
    let why = c.refused("move_model", json!({ "name": "block", "position": at, "rotation": [0.0, 0.0, 0.0, 0.0] }));
    assert!(why.contains("quaternion"), "a zero rotation is not a quaternion: {why}");
    let why = c.refused("move_model", json!({ "name": "block" }));
    assert!(why.contains("\"position\" is required"), "{why}");
    let why = c.refused("move_model", json!({ "name": "block", "position": [0.0, 0.0] }));
    assert!(why.contains("\"position\""), "a short position is named: {why}");

    // Nothing moved, and the next undo is still the fixture mark.
    assert_eq!(c.model_bounds("block")["min"], json!([-10.0, -5.0, 0.0]));
    let fixture_of = |c: &mut Client| {
        let job = c.ok("get_job", json!({}));
        job["models"].as_array().and_then(|m| m.iter().find(|m| m["name"] == json!("vise")).map(|m| m["fixture"].clone()))
    };
    assert_eq!(fixture_of(&mut c), Some(json!(true)));
    c.ok("undo", json!({}));
    assert_eq!(fixture_of(&mut c), Some(json!(false)), "undo took back add_fixture, not a refused move");
}

/// An STL with its top face missing gets the hole filled, the counts
/// come back as data, and one undo restores the open mesh exactly.
/// Guards #717: without the tool an agent could see an open edge but
/// never close it headlessly.
#[test]
fn repair_model_fills_an_open_boxs_missing_face_in_one_undo_step() {
    let dir = tmp_dir("repair");
    let part = dir.join("open.stl");
    write_open_top_box_stl(&part, (10.0, 5.0, 4.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": part.display().to_string() }));
    assert_eq!(c.facets("open"), Some(10), "five faces of two triangles");

    let r = c.ok("repair_model", json!({ "name": "open" }));
    assert_eq!(r["model"], json!("open"), "{r}");
    assert_eq!(r["weld_tol"], json!(0.001), "the Model page's default tolerance: {r}");
    assert_eq!(r["max_hole_edges"], json!(256), "the Model page's default budget: {r}");
    assert_eq!((r["holes_filled"].as_u64(), r["triangles_added"].as_u64()), (Some(1), Some(2)), "{r}");
    assert_eq!((r["triangles_before"].as_u64(), r["triangles_after"].as_u64()), (Some(10), Some(12)), "{r}");
    assert!(r["summary"].as_str().is_some_and(|s| s.contains("filled 1 hole")), "{r}");
    assert_eq!(c.facets("open"), Some(12), "the repaired mesh is the document's");

    // One History step: undo brings the hole back, redo closes it again.
    c.ok("undo", json!({}));
    assert_eq!(c.facets("open"), Some(10));
    c.ok("redo", json!({}));
    assert_eq!(c.facets("open"), Some(12));
    // A closed mesh has nothing left to repair, and says so.
    let why = c.refused("repair_model", json!({ "name": "open" }));
    assert!(why.contains("nothing to repair"), "{why}");
}

/// Every refusal names what is wrong and leaves the history alone, so
/// the next undo undoes the import. Guards the "refusals are data" rule
/// and the "a repair with nothing to do is a refusal, not a no-op step"
/// rule `RepairModel` keeps.
#[test]
fn repair_model_refuses_by_name_and_a_refusal_enters_no_history() {
    let dir = tmp_dir("repair-refuse");
    let part = dir.join("open.stl");
    let closed = dir.join("closed.stl");
    write_open_top_box_stl(&part, (10.0, 5.0, 4.0));
    write_box_stl(&closed, (10.0, 5.0, 4.0));
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": closed.display().to_string() }));
    c.ok("import_model", json!({ "path": part.display().to_string() }));

    let why = c.refused("repair_model", json!({ "name": "closed" }));
    assert!(why.contains("nothing to repair"), "a closed box is a refusal, not a no-op step: {why}");
    let why = c.refused("repair_model", json!({ "name": "ghost" }));
    assert!(why.contains("ghost"), "an unknown name is quoted: {why}");
    let why = c.refused("repair_model", json!({ "name": "open", "weld_tol": -0.1 }));
    assert!(why.contains("weld tolerance"), "{why}");
    let why = c.refused("repair_model", json!({ "name": "open", "max_hole_edges": 2 }));
    assert!(why.contains("at least 3 edges"), "{why}");
    let why = c.refused("repair_model", json!({ "name": "open", "max_hole_edges": 2.5 }));
    assert!(why.contains("\"max_hole_edges\" must be a whole number"), "{why}");
    let why = c.refused("repair_model", json!({ "name": "open", "weld_tol": "fine" }));
    assert!(why.contains("\"weld_tol\" must be a number"), "{why}");
    let why = c.refused("repair_model", json!({}));
    assert!(why.contains("\"name\" is required"), "{why}");

    // Nothing changed, and the next undo is still the second import.
    assert_eq!(c.facets("open"), Some(10));
    c.ok("undo", json!({}));
    assert_eq!(c.facets("open"), None, "undo took back the import, not a refused repair");
    assert_eq!(c.facets("closed"), Some(12));
}
