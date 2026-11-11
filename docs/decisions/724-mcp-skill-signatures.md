## #724 - camcraft-mcp skill: tool signatures match the server (2026-11-10)

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
- **Dropped: a drift test** (`crates/app/tests/mcp_skill_signatures.rs`,
  reading the skill's signatures against `tools/list`). The maintainer
  (2026-11-10): "no need to make a drift test on process files and
  skills". Skills are process documents, kept current by review, not by
  a test suite.
- **Also:** the "Known Remaining Gaps" list is brought current. Pocket
  recognition (#35), in-plane part axes (#557) and the live GUI session
  (#183, #567) have shipped. `MoveModel` and `RepairModel` shipped as
  `move_model` / `repair_model` (#719, #721) and are listed with the tools. Three-point alignment (#555, `lay_part_on_points`) and the
  fixture locate (#561, `locate_part`) shipped while this PR was open;
  face-to-plane alignment (#330) is GUI-only.
- **Process:** the skill is a process file, so `.claude/PROCESS_VERSION`
  goes 9 -> 10 (after merging main). The merging session adds the #681 comment.
- **Cost to change later:** none; this is documentation.
