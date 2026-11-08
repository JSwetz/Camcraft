//! The MCP server (#128), driven in-process over its JSON-RPC handler
//! and over a byte pipe: the handshake, the tool list, a whole job
//! (import, stock, tool, operation, generate, verify, post, undo), and
//! the refusals - every bad input is a tool error or a JSON-RPC error,
//! never a panic.
//!
//! Gate map (M4.4): covers app::mcp, app::script, app::project::json,
//! app::cli.

use camcraft_app::mcp::{self, Server};

use serde_json::{json, Value};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 40 x 40 x 15 box as ASCII STL, z from 0: twelve triangles, wound
/// outward. Written by the test so the fixture is the code.
fn write_box_stl(path: &std::path::Path) {
    let (x, y, z) = (20.0f64, 20.0f64, 15.0f64);
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -x } else { x }, if i & 2 == 0 { -y } else { y }, if i & 4 == 0 { 0.0 } else { z }]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut s = String::from("solid box\n");
    for q in quads {
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
        Self { server: Server::new(), next: 1 }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        assert_eq!(reply["id"], json!(id));
        assert_eq!(reply["jsonrpc"], json!("2.0"));
        reply
    }

    /// A tools/call: (isError, structuredContent-or-text).
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
}

#[test]
fn handshake_and_tool_list() {
    let mut c = Client::new();
    let init = c.request("initialize", json!({
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" }
    }));
    assert_eq!(init["result"]["protocolVersion"], json!("2025-06-18"));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("camcraft"));
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // The initialized notification has no id and gets no answer.
    assert!(c.server.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());

    let list = c.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for want in [
        "open_project", "import_model", "save_project", "get_job", "list_operation_types", "add_operation",
        "edit_operation", "delete_operation", "set_tool", "undo", "redo", "generate", "verify", "post",
    ] {
        assert!(names.contains(&want), "missing tool {want}: {names:?}");
    }
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], json!("object"), "{t}");
        assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
    }
    // An unknown protocol version is answered with ours.
    let other = c.request("initialize", json!({ "protocolVersion": "1999-01-01" }));
    assert_eq!(other["result"]["protocolVersion"], json!(mcp::PROTOCOL_VERSIONS[0]));
}

#[test]
fn a_whole_job_over_the_tools() {
    let dir = tmp_dir("job");
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));

    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    assert_eq!(imp["model"], json!("block"));
    assert_eq!(imp["facets"], json!(12));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));

    let kinds = c.ok("list_operation_types", json!({}));
    let face = kinds["kinds"].as_array().and_then(|k| k.iter().find(|k| k["label"] == json!("Face"))).cloned();
    let face = face.expect("Face kind listed");
    assert_eq!(face["key"], json!("face"));
    assert!(face["fields"].as_array().is_some_and(|f| f.iter().any(|f| f["name"] == json!("stepover") && f["type"] == json!("number"))));

    let add = c.ok("add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL", "params": { "stepover": 2.5 } }));
    assert_eq!(add["operation"], json!("Face1"));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["status"], json!("never"));
    assert_eq!(job["operations"][0]["params"]["face"]["stepover"], json!(2.5));
    assert_eq!(job["tools"][0]["name"], json!("EM-0400-2FL"));
    assert!(job["setups"][0]["stock"]["box"].is_object(), "{job}");
    assert!(job["models"].as_array().is_some_and(|m| m.iter().any(|m| m["name"] == json!("block"))));

    let edit = c.ok("edit_operation", json!({ "name": "Face1", "params": { "feed": 1000.0 } }));
    assert_eq!(edit["params"]["face"]["feed"], json!(1000.0));

    let generated = c.ok("generate", json!({}));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert!(ver["findings"].is_array());

    let nc = dir.join("out.nc");
    let posted = c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert_eq!(posted["files"], json!([nc.display().to_string()]));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0));
    assert!(std::fs::metadata(&nc).is_ok_and(|m| m.len() > 0), "the program was written");

    // Undo walks the History: the generation, then the edit.
    let u = c.ok("undo", json!({}));
    assert_eq!(u["undone"], json!(true));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["status"], json!("never"), "undoing the first generation drops its cache: {job}");
    c.ok("undo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["feed"], json!(1200.0));
    c.ok("redo", json!({}));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["feed"], json!(1000.0));

    // Save, delete, reopen: the saved file still holds the op.
    let camproj = dir.join("job.camproj");
    c.ok("save_project", json!({ "path": camproj.display().to_string() }));
    c.ok("delete_operation", json!({ "name": "Face1" }));
    assert_eq!(c.ok("get_job", json!({}))["operations"], json!([]));
    let opened = c.ok("open_project", json!({ "path": camproj.display().to_string() }));
    assert_eq!(opened["operations"][0]["key"], json!("Face1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_input_is_a_tool_error_never_a_panic() {
    let mut c = Client::new();
    let msg = c.refused("no_such_tool", json!({}));
    assert!(msg.contains("unknown tool"), "{msg}");
    let msg = c.refused("import_model", json!({ "path": "Z:/definitely/missing/part.stl" }));
    assert!(msg.starts_with("import_model:"), "{msg}");
    let msg = c.refused("get_job", json!({ "bogus": 1 }));
    assert!(msg.contains("unknown argument"), "{msg}");
    c.refused("undo", json!({}));
    c.refused("redo", json!({}));
    c.refused("add_operation", json!({ "kind": "Face", "tool": "no-such-tool" }));
    c.refused("add_operation", json!({ "kind": "Nonsense", "tool": "x" }));
    c.refused("edit_operation", json!({ "name": "Face1", "params": { "feed": 1.0 } }));
    c.refused("post", json!({}));

    // A real op, then patches that must not land.
    c.ok("set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c.ok("add_operation", json!({ "kind": "face", "tool": "EM-0400-2FL" }));
    let msg = c.refused("edit_operation", json!({ "name": "Face1", "params": { "stepover": "wide" } }));
    assert!(msg.contains("bad params patch"), "{msg}");
    let msg = c.refused("edit_operation", json!({ "name": "Face1", "params": { "stepovr": 2.0 } }));
    assert!(msg.contains("no field \"stepovr\""), "{msg}");
    c.refused("edit_operation", json!({ "name": "Face1", "params": { "stepover": -3.0 } }));
    c.refused("edit_operation", json!({ "name": "Face1", "params": [1, 2] }));
    c.refused("set_stock", json!({ "box": { "dx": -1.0, "dy": 5.0, "dz": 5.0 } }));
    c.refused("add_tool", json!({ "definition": { "kind": "end_mill" } }));
    // None of the refusals reached the History: one undo per real change.
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"][0]["params"]["face"]["stepover"], json!(3.0));
    for _ in 0..3 {
        c.ok("undo", json!({}));
    }
    c.refused("undo", json!({}));

    // Protocol-level junk: JSON-RPC errors, the session lives on.
    let parse = c.server.handle_line("{ not json").expect("parse errors are answered");
    assert!(parse.contains("-32700"), "{parse}");
    let unknown = c.request("no/such/method", json!({}));
    assert_eq!(unknown["error"]["code"], json!(-32601));
    let bad_call = c.request("tools/call", json!({ "arguments": {} }));
    assert_eq!(bad_call["error"]["code"], json!(-32602));
    assert!(c.server.handle(&json!([1, 2])).is_some_and(|r| r["error"]["code"] == json!(-32600)));
    assert!(c.server.handle_line("   ").is_none());
}

#[test]
fn the_stdio_loop_answers_line_for_line() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_job","arguments":{}}}"#, "\n",
    );
    let mut out = Vec::new();
    mcp::serve(input.as_bytes(), &mut out).expect("serve");
    let text = String::from_utf8(out).expect("utf8");
    let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).expect("one JSON object per line")).collect();
    assert_eq!(lines.len(), 2, "the notification is not answered: {text}");
    assert_eq!(lines[0]["result"]["protocolVersion"], json!("2025-03-26"));
    assert_eq!(lines[1]["result"]["isError"], json!(false));
    assert_eq!(lines[1]["result"]["structuredContent"]["operations"], json!([]));
}
