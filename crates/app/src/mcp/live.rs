//! The live seam (#183): one MCP wire line answered against a document
//! the caller already owns - the GUI's world and its command History -
//! instead of the headless session's own. Same tool table, same
//! dispatch, same commands: an agent's step lands on the app's own
//! History, so the app's Ctrl+Z undoes it like any user step.
//!
//! Provenance: our own seam over this module's JSON-RPC handler
//! (JSON-RPC 2.0 framed as the public MCP specification, 2025-06-18).
//! Clean-room, public sources only.

use super::{tool_err, Server};
use crate::commands::History;
use crate::script::Env;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};
use std::path::Path;

/// Tools the live session refuses, with the reason an agent reads. The
/// app owns these: opening a file over the user's document skips the
/// app's unsaved-changes prompt and its per-document reset.
const LIVE_REFUSED: &[(&str, &str)] = &[(
    "open_project",
    "open_project is not available in a live app session - it would replace the user's document \
without the app's unsaved-changes prompt; ask the user to open the file (File > Open)",
)];

/// What one live line did.
#[derive(Debug, Default)]
pub struct LiveAnswer {
    /// The wire answer, `None` for a notification.
    pub reply: Option<String>,
    /// The tool a `tools/call` named, when the line was one: the document
    /// may have changed (a read-only tool too - the caller refreshes its
    /// views, which is cheap and never wrong).
    pub tool: Option<String>,
}

/// Answer one wire line against `world` and `history`. `doc_path` is
/// the file the app has open, so a bare `save_project` saves over it the
/// way the app's Ctrl+S does. The world is shared by handle (a flecs
/// world clone is a reference, not a copy); the history is lent for the
/// call and handed back whatever happens - a refused or failed tool
/// included.
pub fn answer_live(world: &World, history: &mut History, doc_path: Option<&Path>, line: &str) -> LiveAnswer {
    let parsed = serde_json::from_str::<Value>(line.trim()).ok();
    let tool = parsed
        .as_ref()
        .filter(|m| m.get("method").and_then(Value::as_str) == Some("tools/call"))
        .and_then(|m| m.get("params")?.get("name")?.as_str().map(str::to_owned));
    if let (Some(msg), Some(name)) = (&parsed, &tool)
        && let Some((_, why)) = LIVE_REFUSED.iter().find(|(n, _)| n == name)
        && let Some(id) = msg.get("id")
    {
        let reply = json!({ "jsonrpc": "2.0", "id": id, "result": tool_err(why) });
        return LiveAnswer { reply: Some(reply.to_string()), tool: None };
    }
    let mut server = Server {
        env: Env {
            world: world.clone(),
            history: std::mem::take(history),
            logs: Vec::new(),
            errors: Vec::new(),
            doc_path: doc_path.map(Path::to_path_buf),
            posted: None,
            last_value: Value::Null,
        },
    };
    let reply = server.handle_line(line);
    *history = std::mem::take(&mut server.env.history);
    LiveAnswer { reply, tool }
}
