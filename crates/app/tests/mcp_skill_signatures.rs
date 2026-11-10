//! The `camcraft-mcp` skill's tool reference against the live server
//! (#724): every `` - `name { ... }` `` signature in
//! `.claude/skills/camcraft-mcp/SKILL.md` names a real tool, every
//! argument key it writes is in that tool's `inputSchema`, and every
//! argument the server requires is written. Agents learn the tools from
//! that file, so a renamed argument (`path` -> `output`, `model` ->
//! `source`) is a broken first call for every one of them - this is the
//! drift alarm, the way `scripting::the_bindings_file_is_the_verb_table`
//! guards `cam.d.ts`.
//!
//! Gate map (M4.4): covers app::mcp::tools.

use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The skill's path from this crate's manifest (not the CWD), so the
/// test reads the same file wherever it runs.
const SKILL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.claude/skills/camcraft-mcp/SKILL.md");

/// The skill documents 34 tools today. A parser that suddenly finds far
/// fewer means the entry format changed and the check went blind - fail
/// loudly instead of passing on nothing.
const MIN_SIGNATURES: usize = 30;

/// One documented signature: where it is, which tool, which keys.
struct Signature {
    line: usize,
    tool: String,
    keys: Vec<String>,
}

/// Every `` - `name { ... }` `` entry of the skill, in file order.
fn signatures(skill: &str) -> Vec<Signature> {
    let mut out = Vec::new();
    for (i, line) in skill.lines().enumerate() {
        let Some(rest) = line.strip_prefix("- `") else { continue };
        let Some((tool, after)) = rest.split_once(' ') else { continue };
        let is_name = !tool.is_empty() && tool.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        if !is_name || !after.starts_with('{') {
            continue;
        }
        // The signature ends at the first `}` the closing backtick
        // follows; a nested object's `}` is followed by a space or comma.
        let Some(end) = after.find("}`") else { continue };
        out.push(Signature { line: i + 1, tool: tool.to_owned(), keys: top_level_keys(&after[1..end]) });
    }
    out
}

/// The argument keys of one signature body: each quoted word followed
/// by `:` or `?:` at brace/bracket depth 0. Quoted values (`"G54" | ...`)
/// are skipped whole, so their text never moves the depth, and the
/// fields of a nested object argument (`"area": { "offset"?, ... }`) are
/// not top-level arguments.
fn top_level_keys(body: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < body.len() {
        match body.as_bytes()[i] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            b'"' => {
                let start = i + 1;
                let Some(len) = body[start..].find('"') else { break };
                let word = &body[start..start + len];
                i = start + len; // the closing quote
                let tail = &body[i + 1..];
                let tail = tail.strip_prefix('?').unwrap_or(tail);
                if depth == 0 && tail.starts_with(':') {
                    keys.push(word.to_owned());
                }
            }
            _ => {}
        }
        i += 1;
    }
    keys
}

/// The live tool table: name -> (argument keys, required keys).
fn live_tools() -> BTreeMap<String, (BTreeSet<String>, Vec<String>)> {
    let mut server = Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    server.handle(&init).expect("initialize answers");
    let list = server.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).expect("tools/list answers");
    let tools = list["result"]["tools"].as_array().cloned().unwrap_or_default();
    let strings = |v: &Value| -> Vec<String> {
        v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    };
    tools
        .iter()
        .filter_map(|t| {
            let name = t["name"].as_str()?.to_owned();
            let schema = &t["inputSchema"];
            let props = schema["properties"].as_object().map(|p| p.keys().cloned().collect()).unwrap_or_default();
            Some((name, (props, strings(&schema["required"]))))
        })
        .collect()
}

/// Guards the guard: the key reader takes only top-level keys and steps
/// over quoted values and nested object fields.
#[test]
fn the_key_reader_takes_top_level_keys_only() {
    assert_eq!(top_level_keys(r#" "word": "G54" | "G55" | null, "setup"?: string "#), ["word", "setup"]);
    assert_eq!(top_level_keys(r#" "operation": string, "area": { "offset"?, "corner"? } "#), ["operation", "area"]);
    assert_eq!(top_level_keys(r#" "origin"?: [x, y, z], "inner": { "deep": 1 }, "last": 2 "#), ["origin", "inner", "last"]);
    assert_eq!(top_level_keys(""), Vec::<String>::new());
    let sigs = signatures("- `post { \"output\": string, \"post\"?: string }`: posts.\n- `get_job {}`: dumps.\n- `prose` line\n");
    let read: Vec<(&str, Vec<String>)> = sigs.iter().map(|s| (s.tool.as_str(), s.keys.clone())).collect();
    assert_eq!(read, [("post", vec!["output".to_owned(), "post".to_owned()]), ("get_job", vec![])]);
}

/// #724: the skill told agents `post { path }`, `add_operation { model }`
/// and six more shapes the server refuses. Every documented signature
/// must name a tool, use only its real argument keys, and write every
/// argument the server requires.
#[test]
fn every_tool_signature_in_the_mcp_skill_matches_the_server() {
    let skill = std::fs::read_to_string(SKILL).expect("the camcraft-mcp skill").replace("\r\n", "\n");
    let tools = live_tools();
    let sigs = signatures(&skill);
    assert!(sigs.len() >= MIN_SIGNATURES, "the reader found {} signatures - did the entry format change?", sigs.len());

    let mut drift = Vec::new();
    for s in &sigs {
        let Some((props, required)) = tools.get(&s.tool) else {
            drift.push(format!("line {}: `{}` is not a tool", s.line, s.tool));
            continue;
        };
        for k in s.keys.iter().filter(|k| !props.contains(*k)) {
            drift.push(format!("line {}: `{}` has no argument \"{k}\" (it takes {props:?})", s.line, s.tool));
        }
        for r in required.iter().filter(|r| !s.keys.contains(*r)) {
            drift.push(format!("line {}: `{}` requires \"{r}\", which the skill leaves out", s.line, s.tool));
        }
    }
    assert!(drift.is_empty(), "the camcraft-mcp skill drifted from tools/list:\n{}", drift.join("\n"));
}
