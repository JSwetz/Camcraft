## #356 - Process: new user-facing commands ship with an MCP tool (or a filed follow-up)

**What was decided:**
1. Added an explicit rule to `AGENTS.md` in the "Everything Is a Command" section:
   Any PR that introduces a user-facing command must also introduce its corresponding MCP tool in `crates/app/src/mcp/` (and script verb where applicable), or explicitly file a `source:agent` follow-up issue titled `MCP: <command>` and name it in the PR body.
2. Created a dedicated agent skill and reference document at `.claude/skills/camcraft-mcp/SKILL.md` detailing the complete headless agent workflow (`open/import → set_stock → add_setup/set_wcs → add_fixture → add_tool → add_operation/list_features/set_operation_geometry → generate → verify/render_stock → post`), tool schemas, error/refusal conventions, and invariants.

**Why:**
CamCraft's headless MCP server (`camcraft-cli mcp`, #128) allows AI agents to drive complete CAM programming pipelines from import through generation, verification, and posting. When user-facing commands are added exclusively to the GUI without an accompanying MCP tool, the capability gap between GUI and headless agent driving widens. Since MCP tools dispatch through the exact same undoable commands as the GUI, the cost to expose them is low, while maintaining full undo/redo fidelity and validation consistency.

**Considered and dropped:**
- *Mandating that every command PR must implement the MCP tool immediately with zero exceptions:* Dropped in favor of allowing a filed `source:agent` follow-up issue `MCP: <command>` named in the PR description, preventing PR blocking when complex MCP DTO design or tool schema discussions are needed.
- *Keeping MCP documentation solely in code comments in `crates/app/src/mcp/`:* Dropped because Claude/AGY agents operating on the repository need a structured, loadable skill (`.claude/skills/camcraft-mcp/SKILL.md`) to understand parameter contracts, error structures, and tool calling conventions when orchestrating CAM jobs.

**Cost to change later:**
Low; this is a documentation and process policy governing future command PR contributions.
