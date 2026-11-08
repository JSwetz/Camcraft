<!-- moved from RECOMMENDED.md by PR #184 (docs/decisions rule, PR #250) -->
## #128 first slice: `camcraft-cli mcp` (2026-11-07)

- **Where it lives:** `crates/app/src/mcp/` (protocol `mod.rs`, tool table
  `tools.rs`, read side `job.rs`, write side `edit.rs`), behind
  `camcraft-cli mcp`. The CLI already lives in app, and the server is a
  command executant like the script host, so it sits in the app layer.
  No new crate: a `camcraft-mcp` crate would only re-export app.
- **No MCP SDK.** The protocol surface we need (initialize, ping,
  tools/list, tools/call over newline-delimited stdio) is about 150 lines
  of serde_json. The Rust SDK (`rmcp`, MIT) pulls tokio and an async
  runtime into a synchronous, deterministic binary; dropped for that
  weight. Revisit if we need resources, prompts, sampling or the HTTP
  transport.
- **One session = one `script::Env`.** The server reuses the job-script
  host's document + History, and its load/save/import/generate/verify/
  post verbs are called directly (no JS engine involved). So the batch
  lane, the script lane and the MCP lane share one meaning of every verb.
- **Every write rides a command** (`AddTool`, `SetStock`,
  `CreateOperation`, `EditOperationParams`, `EditOperationLinks`,
  `DeleteOperation`, the verbs' `ImportModel` / `GenerateOperation`), so
  `undo` / `redo` walk the History. Refusals come from the commands'
  construction gates and never enter the history.
- **Params are the file's DTO shape** (`project::params_to_json` /
  `params_from_json`, new `project/json.rs`): `get_job` shows
  `{"face": {...}}`, `edit_operation` takes a JSON merge patch (RFC 7386)
  over the kind's field object. A patched key the kind does not have is
  refused by name (serde would drop it silently). Considered: a
  hand-written schema per kind - dropped, a second truth beside the file
  format. `list_operation_types` therefore lists fields as the default
  object's keys and JSON types; optional fields that default to off are
  absent (said in its note).
- **Unknown tool = tool error** (`isError: true`), as the issue asked,
  rather than the spec's suggested JSON-RPC -32602: an agent reads tool
  errors in-band. Malformed `tools/call` params (no name, non-object
  arguments) stay JSON-RPC -32602. Unknown arguments are refused too.
- **Panic backstop:** `tools/call` runs inside `catch_unwind` and turns a
  bug's panic into a tool error, so a generator bug cannot kill the
  agent's session. No tool path panics by law; this is defense in depth.
- **Added beyond the brief:** `add_tool`, `list_tool_catalog` and
  `set_stock` (box / cylinder / from model). A fresh document has no tool
  and no stock, so without them no op can be created or generated.
  `set_tool` assigns a document tool to an operation.
- **Owed, filed as issues:** picking faces/edges/loops; WCS, machine and
  fixtures; sim render and screenshots; a server mode inside the GUI app.

