//! `detect_pockets` over MCP (#35): an imported STL with a rounded
//! pocket and one with a through slot list their features in world
//! coordinates; `create` adds the Pocket (or Slot) that machines one as
//! a single undo step, the pocket generates and verifies clean; and every
//! bad argument is a tool error, never a panic.
//!
//! Gate map (#35): covers app::mcp (pockets, tools), app::recognize,
//! app::commands (CreateOperation).

mod pocket_support;

use camcraft_app::mcp::Server;
use pocket_support::{block_with_pocket, height_field, rounded_rect, write_stl};

use serde_json::{json, Value};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-pockets-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
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
}

/// A document holding `mesh` as the model "part", a stock box with
/// margin round the 60 x 40 x 20 block, and a 6 mm end mill.
fn job(tag: &str, mesh: &camcraft_app::geometry::Mesh) -> (Client, PathBuf) {
    let dir = tmp_dir(tag);
    let stl = dir.join("part.stl");
    write_stl(mesh, &stl);
    let mut c = Client { server: Server::new(), next: 1 };
    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    assert_eq!(imp["model"], json!("part"));
    c.ok("set_stock", json!({ "box": { "dx": 80.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    (c, dir)
}

fn ops(c: &mut Client) -> usize {
    c.ok("get_job", json!({}))["operations"].as_array().map_or(0, Vec::len)
}

#[test]
fn detect_pockets_lists_a_rounded_pocket_and_creates_its_pocket() {
    let mesh = block_with_pocket(30.0, 20.0, &rounded_rect(15.0, 8.0, 4.0, 6), 8.0, 20.0);
    let (mut c, dir) = job("rounded", &mesh);
    let got = c.ok("detect_pockets", json!({ "model": "part" }));
    assert_eq!(got["count"], json!(1), "{got}");
    let f = &got["features"][0];
    assert_eq!(f["kind"], json!("closed_pocket"), "{f}");
    assert_eq!((f["floor_z"].clone(), f["top_z"].clone(), f["depth"].clone()), (json!(8.0), json!(20.0), json!(12.0)), "{f}");
    assert!(f["min_corner_radius"].as_f64().is_some_and(|r| (r - 4.0).abs() < 1e-3), "{f}");
    assert_eq!(f["operation_kind"], json!("2D Pocket"));
    assert!(f["floor"].as_str().is_some_and(|s| s.starts_with("face:0x")), "{f}");
    // The floor id is the one list_features prints for the same face.
    let faces = c.ok("list_features", json!({ "model": "part", "kinds": ["faces"] }));
    assert!(faces["faces"].as_array().is_some_and(|l| l.iter().any(|x| x["id"] == f["floor"])), "{faces}");
    assert_eq!(ops(&mut c), 0, "listing creates nothing");

    let made = c.ok("detect_pockets", json!({ "model": "part", "create": 0, "tool": "EM-0600-4FL", "name": "P1" }));
    assert_eq!(made["created"]["operation"], json!("P1"), "{made}");
    assert_eq!(made["created"]["kind"], json!("2D Pocket"));
    let g = c.ok("get_operation_geometry", json!({ "operation": "P1" }));
    assert_eq!(g["source"]["picks"][0]["id"], f["floor"], "{g}");
    let generated = c.ok("generate", json!({ "name": "P1" }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");

    // Undo the generation, then the creation: nothing is left.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert_eq!(ops(&mut c), 0);
    c.ok("redo", json!({}));
    assert_eq!(ops(&mut c), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detect_pockets_creates_a_slot_for_a_through_slot() {
    let mesh = height_field(&[-30.0, 30.0], &[-20.0, -5.0, 5.0, 20.0], |_, j| if j == 1 { 10.0 } else { 20.0 });
    let (mut c, dir) = job("slot", &mesh);
    let got = c.ok("detect_pockets", json!({ "model": "part" }));
    let f = &got["features"][0];
    assert_eq!(f["kind"], json!("slot"), "{got}");
    assert_eq!(f["slot"]["ends"], json!(["open", "open"]));
    assert_eq!(f["slot"]["width"], json!(10.0));
    assert_eq!(f["open_spans"].as_array().map(Vec::len), Some(2));
    let made = c.ok("detect_pockets", json!({ "model": "part", "create": 0, "tool": "EM-0600-4FL" }));
    assert_eq!(made["created"]["operation"], json!("Slot1"), "{made}");
    let job = c.ok("get_job", json!({}));
    let slot = &job["operations"][0]["params"]["slot"];
    assert_eq!(slot["width"], json!(10.0), "{job}");
    c.ok("undo", json!({}));
    assert_eq!(ops(&mut c), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detect_pockets_refuses_bad_arguments_by_name() {
    let mesh = height_field(&[-20.0, 20.0], &[-20.0, 20.0], |_, _| 20.0);
    let (mut c, dir) = job("refuse", &mesh);
    let none = c.ok("detect_pockets", json!({ "model": "part" }));
    assert_eq!((none["count"].clone(), none["features"].clone()), (json!(0), json!([])), "a plain block has none");
    let msg = c.refused("detect_pockets", json!({ "model": "part", "create": 0, "tool": "EM-0600-4FL" }));
    assert!(msg.contains("0 recognised"), "{msg}");
    let msg = c.refused("detect_pockets", json!({ "model": "ghost" }));
    assert!(msg.contains("no model named"), "{msg}");
    let msg = c.refused("detect_pockets", json!({ "model": "EM-0600-4FL" }));
    assert!(msg.contains("is not a model"), "{msg}");
    let msg = c.refused("detect_pockets", json!({ "model": "part", "tool": "EM-0600-4FL" }));
    assert!(msg.contains("go with \"create\""), "{msg}");
    let msg = c.refused("detect_pockets", json!({ "model": "part", "create": "first" }));
    assert!(msg.contains("feature index"), "{msg}");
    let msg = c.refused("detect_pockets", json!({ "model": "part", "creat": 0 }));
    assert!(msg.contains("unknown argument"), "{msg}");

    let pocket = block_with_pocket(30.0, 20.0, &rounded_rect(15.0, 8.0, 0.0, 1), 8.0, 20.0);
    let (mut c, dir2) = job("refuse2", &pocket);
    let msg = c.refused("detect_pockets", json!({ "model": "part", "create": 0 }));
    assert!(msg.contains("\"tool\" is required"), "{msg}");
    let msg = c.refused("detect_pockets", json!({ "model": "part", "create": 0, "tool": "no-such-tool" }));
    assert!(!msg.is_empty(), "a missing tool is the create command's refusal: {msg}");
    assert_eq!(ops(&mut c), 0);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}
