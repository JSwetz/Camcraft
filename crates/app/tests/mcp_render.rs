//! The MCP stock render (#182), driven in-process over the JSON-RPC
//! handler: a generated job renders to a PNG image block that decodes,
//! has the asked size, shows stock, and is byte-identical run to run;
//! a real cut repaints the picture and moves the summary's rest volume
//! and stock top; and every refusal (no program, no stock, nothing left
//! above the floor, bad arguments) is a named tool error.
//!
//! Gate map (M4.4): covers app::mcp::render, app::raster.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-render-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 40 x 40 x 15 box as ASCII STL, z from 0, wound outward.
fn write_box_stl(path: &Path) {
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

/// One tools/call: the whole `result` object.
fn call(server: &mut Server, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    reply["result"].clone()
}

fn ok(server: &mut Server, name: &str, args: Value) -> Value {
    let r = call(server, name, args);
    assert_eq!(r["isError"], json!(false), "{name} refused: {r}");
    r
}

fn refused(server: &mut Server, name: &str, args: Value) -> String {
    let r = call(server, name, args);
    assert_eq!(r["isError"], json!(true), "{name} should have refused: {r}");
    r["content"][0]["text"].as_str().unwrap_or_default().to_owned()
}

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("not base64: {c}"),
        }
    };
    let bytes = s.as_bytes();
    assert_eq!(bytes.len() % 4, 0, "padded base64");
    let mut out = Vec::new();
    for q in bytes.chunks(4) {
        let pad = q.iter().filter(|c| **c == b'=').count();
        let n = q.iter().take(4 - pad).enumerate().fold(0u32, |n, (i, c)| n | (val(*c) << (18 - 6 * i)));
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    out
}

/// The image block of a render result, decoded: (width, height, RGB).
fn decode(result: &Value) -> (u32, u32, Vec<u8>, String) {
    let content = result["content"].as_array().expect("content array");
    assert_eq!(content[0]["type"], json!("text"));
    let img = content.iter().find(|c| c["type"] == json!("image")).expect("an image block");
    assert_eq!(img["mimeType"], json!("image/png"));
    let data = img["data"].as_str().expect("base64 data").to_owned();
    let png = b64_decode(&data);
    let dec = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = dec.read_info().expect("a PNG");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("one frame");
    assert_eq!(info.color_type, png::ColorType::Rgb);
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf, data)
}

fn faced_job(dir: &Path) -> Server {
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut s = Server::new();
    ok(&mut s, "import_model", json!({ "path": stl.display().to_string() }));
    ok(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    ok(&mut s, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    ok(&mut s, "add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    ok(&mut s, "generate", json!({}));
    s
}

#[test]
fn render_stock_is_a_deterministic_png_of_the_stock() {
    let dir = tmp_dir("png");
    let mut s = faced_job(&dir);
    let r = ok(&mut s, "render_stock", json!({ "width": 320, "height": 240 }));
    let (w, h, rgb, data) = decode(&r);
    assert_eq!((w, h), (320, 240));
    assert_eq!(rgb.len(), 320 * 240 * 3);
    let bg = [236u8, 238, 241];
    let stock = rgb.chunks(3).filter(|p| *p != bg).count();
    assert!(stock > 320 * 240 / 10, "the stock covers the frame: {stock} pixels");

    // The JSON copies never carry the picture; the summary does carry
    // the numbers.
    let sc = &r["structuredContent"];
    assert!(sc.get("_png_base64").is_none(), "{sc}");
    assert!(sc["rest"]["volume"].as_f64().is_some_and(|v| v >= 0.0), "{sc}");
    // The faced block: 60 x 60 x 20 stock over a 40 x 40 x 15 part, the
    // face pass at the stock top - 40 x 40 x 5 of rest stands.
    assert_eq!(sc["stock_bounds"], json!({ "min": [-30.0, -30.0, 0.0], "max": [30.0, 30.0, 20.0] }), "{sc}");
    assert!(sc["rest"]["volume"].as_f64().is_some_and(|v| (v - 8000.0).abs() < 1.0), "{sc}");
    assert!(sc["rest"]["top_bounds"]["max"].is_array(), "{sc}");
    assert!(sc["image"]["stock_pixels"].as_u64().is_some_and(|n| n > 0), "{sc}");
    assert!(sc["judged_cells"].as_u64().is_some_and(|n| n > 0), "{sc}");

    // Same document, same bytes - and a second server over the same job
    // agrees too.
    let again = ok(&mut s, "render_stock", json!({ "width": 320, "height": 240 }));
    assert_eq!(decode(&again).3, data, "the render is deterministic");
    let dir2 = tmp_dir("png2");
    let mut s2 = faced_job(&dir2);
    let other = ok(&mut s2, "render_stock", json!({ "width": 320, "height": 240 }));
    assert_eq!(decode(&other).3, data, "a fresh session renders the same bytes");

    // The top view and the toolpath overlay: the feed moves paint blue.
    let top = ok(&mut s, "render_stock", json!({ "width": 200, "height": 200, "view": "top", "toolpath": true }));
    let (tw, th, trgb, tdata) = decode(&top);
    assert_eq!((tw, th), (200, 200));
    assert_ne!(tdata, data);
    assert!(trgb.chunks(3).any(|p| p == [20, 110, 230]), "feed moves are drawn");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

/// Change the Face op's finished height (offset from the stock top) and
/// regenerate it.
fn face_to(s: &mut Server, offset: f64) {
    ok(s, "edit_operation", json!({ "name": "Face1", "params": { "cut_z": { "offset": offset, "reference": "top_of_stock" } } }));
    ok(s, "generate", json!({ "name": "Face1" }));
}

#[test]
fn render_stock_shows_what_the_program_cut() {
    let dir = tmp_dir("cut");
    let mut s = faced_job(&dir);
    // At the stock top the Face pass removes nothing: the baseline.
    let uncut = ok(&mut s, "render_stock", json!({ "width": 320, "height": 240 }));
    let (_, _, before, before_data) = decode(&uncut);
    // Two millimetres off the top: a real layer comes away.
    face_to(&mut s, -2.0);
    let cut = ok(&mut s, "render_stock", json!({ "width": 320, "height": 240 }));
    let after_data = decode(&cut).3;
    let sc = &cut["structuredContent"];
    assert_ne!(after_data, before_data, "the cut changes the picture");
    assert_eq!(uncut["structuredContent"]["stock_bounds"]["max"][2], json!(20.0));
    assert_eq!(sc["stock_bounds"]["max"][2], json!(18.0), "{sc}");
    assert!(sc["rest"]["volume"].as_f64().is_some_and(|v| (v - 4800.0).abs() < 1.0), "40 x 40 x 3 of rest: {sc}");
    assert_eq!(sc["rest"]["top_bounds"]["max"][2], json!(18.0), "{sc}");

    // Down to the part's top: the rest is gone, and the region over the
    // part repaints from rest amber to clean.
    face_to(&mut s, -5.0);
    let done = ok(&mut s, "render_stock", json!({ "width": 320, "height": 240 }));
    let (_, _, after, _) = decode(&done);
    let sc = &done["structuredContent"];
    assert_eq!(sc["stock_bounds"]["max"][2], json!(15.0), "{sc}");
    assert_eq!(sc["rest"]["cells"], json!(0), "{sc}");
    assert_eq!(sc["rest"]["volume"], json!(0.0), "{sc}");
    assert!(sc["rest"]["top_bounds"].is_null(), "{sc}");
    let amber = |rgb: &[u8]| rgb.chunks(3).filter(|p| p[0] > 150 && p[2] < 80).count();
    assert!(amber(&before) > 320 * 240 / 10, "the uncut part top reads as rest: {}", amber(&before));
    assert_eq!(amber(&after), 0, "nothing reads as rest once the part top is faced");
    let changed = before.chunks(3).zip(after.chunks(3)).filter(|(a, b)| a != b).count();
    assert!(changed > 320 * 240 / 10, "the faced region repaints: {changed} pixels");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Facing below the stock's floor leaves no material: a named refusal,
/// never a picture of a sheet hanging under the block.
#[test]
fn render_stock_refuses_when_no_material_stands() {
    let dir = tmp_dir("through");
    let mut s = faced_job(&dir);
    face_to(&mut s, -25.0);
    let msg = refused(&mut s, "render_stock", json!({}));
    assert!(msg.starts_with("render_stock: the simulated stock has no surface to render"), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn render_stock_refuses_by_name() {
    let mut s = Server::new();
    // No stock, no program.
    let msg = refused(&mut s, "render_stock", json!({}));
    assert!(msg.starts_with("render_stock: no stock simulation to render"), "{msg}");

    // A stock and an operation that never generated.
    ok(&mut s, "set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 10.0 } }));
    ok(&mut s, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    ok(&mut s, "add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    let msg = refused(&mut s, "render_stock", json!({}));
    assert!(msg.starts_with("render_stock: no stock simulation to render"), "{msg}");

    // Arguments out of range or of the wrong type.
    for bad in [
        json!({ "width": 5 }),
        json!({ "height": 100000 }),
        json!({ "width": 64.5 }),
        json!({ "width": "big" }),
        json!({ "view": "side" }),
        json!({ "toolpath": "yes" }),
        json!({ "zoom": 2 }),
    ] {
        let msg = refused(&mut s, "render_stock", bad.clone());
        assert!(msg.starts_with("render_stock:"), "{bad}: {msg}");
    }
}

#[test]
fn render_stock_is_listed() {
    let mut s = Server::new();
    let list = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).expect("answer");
    let tools = list["result"]["tools"].as_array().expect("tools");
    let t = tools.iter().find(|t| t["name"] == json!("render_stock")).expect("render_stock listed");
    assert_eq!(t["inputSchema"]["type"], json!("object"));
    assert!(t["inputSchema"]["properties"]["view"].is_object());
}
