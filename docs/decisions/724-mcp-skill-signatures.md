## #724 - camcraft-mcp skill: tool signatures match the server, and a test keeps them so (2026-11-10)

- **Chosen: correct the skill to the server, not the server to the skill.**
  The 8 drifted entries (`post`, `add_operation`, `set_stock`,
  `set_operation_area`, `set_operation_geometry`, `set_work_offset`,
  `load_machine`, `add_tool`) now use the names `tools/list` publishes.
  **Why:** the server's names are what every in-repo MCP test, the live
  GUI session and existing agent scripts already use; renaming arguments
  would break them all to match a document. **Dropped:** accepting both
  spellings server-side (two vocabularies for one argument, and the
  skill's old shapes for `set_stock` and `set_operation_area` were not
  just renames).
- **Chosen: a drift test, `crates/app/tests/mcp_skill_signatures.rs`.**
  It reads every `` - `name { ... }` `` signature in the skill and checks
  it against the live `tools/list`: the tool exists, each documented
  top-level key is an `inputSchema` property, and each required argument
  is documented. A floor on the number of signatures found stops a format
  change from making the check pass on nothing, and a unit test pins the
  key reader itself. **Why:** the skill is how agents learn the tools,
  and it drifted silently once; `cam.d.ts` has the same kind of alarm.
  **Not checked:** optional-vs-required markers (`?`) and value types,
  and completeness (a tool the skill doesn't mention). Those would need
  a stricter entry format than prose bullets.
- **Also:** the "Known Remaining Gaps" list is brought current. Pocket
  recognition (#35), in-plane part axes (#557) and the live GUI session
  (#183, #567) have shipped. `MoveModel` and `RepairModel` are open PRs
  (#719, #721). Face-to-plane and three-point alignment (#330, #555)
  shipped in the GUI only.
- **Process:** the skill is a process file, so `.claude/PROCESS_VERSION`
  goes 4 -> 5. The merging session adds the #681 comment.
- **Cost to change later:** small. If the skill's entries move to a
  table or a stricter format, the reader in the test changes with them;
  the `MIN_SIGNATURES` floor says when it no longer sees them.
