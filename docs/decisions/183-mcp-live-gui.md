## #183 - MCP server mode inside the GUI app (2026-11-08)

- **Chosen: transport.** A plain TCP socket on 127.0.0.1 carrying the same
  newline-delimited JSON-RPC `camcraft-cli mcp` speaks on stdio, plus a
  stdio bridge, `camcraft-cli mcp --connect 127.0.0.1:<port>`, for MCP
  clients that only launch stdio servers (`nc -w 3 127.0.0.1 <port>` works
  too). std::net and std::thread only.
  **Why:** the smallest transport that reuses the existing framing byte for
  byte; no async runtime, no HTTP stack, nothing platform-specific.
  **Dropped:** streamable HTTP (needs an HTTP server and session headers for
  no gain to a local single-user session); a named pipe / Unix socket (not
  the same code on Windows).
- **Chosen: opt-in.** Off unless `--mcp-port <n>` (or `--mcp-port=<n>`) or
  `CAMCRAFT_MCP_PORT` asks for it; the flag wins over the variable; `0`
  picks a free port. Bound to 127.0.0.1 only, by construction. A bad value
  refuses the start (exit 2, like `--gpu-backend`); a port that cannot be
  bound starts the app anyway with the reason on stderr and the status
  line. While listening, the status strip shows "MCP listening on
  127.0.0.1:<port>". No authentication: anything on the local machine that
  can reach the port can drive the document, which is why it is off by
  default. **Dropped:** a menu toggle (more UI for a developer feature; can
  be added on the same `Inbox` later).
- **Chosen: threading.** A listener thread accepts one client at a time and
  only moves text: each line goes over an mpsc channel to the UI thread
  with a per-request reply channel, and the thread wakes the idle winit loop
  with an empty user event (`EventLoopProxy`). The UI thread answers in
  `State::drain_actions` (after the user's own queued commands), so no world
  access ever happens off the UI thread. Dropping the `Inbox` (app exit; the
  shell drops it first in `shutdown`) stops and joins the thread within
  ~50 ms. Malformed JSON is a JSON-RPC parse error; a dropped client, a
  closed channel or a line over 16 MiB is logged and the listener carries on.
- **Chosen: the seam.** `app::mcp::answer_live(world, history, doc_path,
  line)` runs the existing `Server` dispatch over an `Env` built for the
  call: the world is the GUI's own (a flecs `World` clone is a refcounted
  handle to the same world, not a copy) and the History is the GUI's own,
  lent with `mem::take` and handed back after the call. So an agent's step
  is an entry on the window's History: Ctrl+Z undoes it, and the agent's
  `undo`/`redo` tools walk the user's steps too. No tool was duplicated;
  `mcp/tools.rs` and `mcp/edit.rs` are untouched. After any `tools/call` the
  UI bumps `doc_version` (views refetch) and the status line reads
  "agent: <tool>". **Dropped:** a `CommandKind::Mcp` variant on the pending
  queue (the reply channel and JSON would leak into the ui command enum for
  nothing - the drain point is the same); giving `Env` borrowed fields
  (would change every tool signature in files other PRs edit).
- **Chosen: `open_project` is refused in the live session** with a reason
  pointing at File > Open: through the session it would replace the user's
  document without the app's unsaved-changes prompt and per-document reset.
  `save_project` stays: with no path it saves over the app's open project
  (the window's path is passed in), and it does not mark the app's document
  clean, so the app may still ask to save - the safe side.
- **Cost to change later:** the transport is one file (`mcp/listen.rs`);
  an HTTP front end or a menu toggle would build the same `Inbox`.

**Security fix from review (live MCP listener):** loopback is not a trust boundary, since any web page in the user's browser can POST to 127.0.0.1. Every non-blank wire line must now open a JSON object or array; anything else (an HTTP request line or header) gets a -32600 refusal and the connection is closed before a single line reaches the app. So a page cannot smuggle a JSON-RPC body (`save_project`, `post`, `export_tool_library` with its own paths) past its HTTP headers. Reads are bounded to the 16 MiB cap as they happen, and writes time out after 5 s, so a client can neither grow memory nor stall the listener. A per-session token was considered and dropped for now: the JSON-only rule already closes the browser route, and a token would complicate `camcraft-cli mcp --connect`. It can be added if a non-browser local attacker matters.
