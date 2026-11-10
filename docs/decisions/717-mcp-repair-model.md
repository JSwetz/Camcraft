## #717 - MCP: repair an imported model's mesh (2026-11-10)

- **Chosen: one tool over the existing command.** `repair_model { name,
  weld_tol?, max_hole_edges? }` builds `RepairModel::new` and runs it
  through `env.history`. The repair itself runs at construction, so a
  refused repair never reaches the history, and the applied one is the
  same single undo step (through `UpdateModel`) the Model page's Repair
  button makes. **Why:** "Everything Is a Command"; the MCP layer only
  reads arguments and reports. **Dropped:** calling `geometry::repair`
  directly from the tool (no undo, and the `FromModel` stock would not
  follow the new facets).
- **Chosen: the GUI's defaults.** 0.001 mm weld tolerance and a 256-edge
  hole budget, the values of `RepairUi::default`, kept as named constants
  in `mcp/model_tools.rs` because the UI crate's struct isn't reachable
  from the app crate. **Why:** a no-argument headless repair should do
  what the Repair button does. A tolerance's sign and a budget under 3
  are left to the command, so the refusal text is the command's own;
  the tool only refuses a value of the wrong JSON type.
- **Chosen: the report as data.** Every `RepairReport` count (vertices and
  triangles before and after, welded, dropped, holes filled, triangles
  added) plus the command's one-line `summary()`. **Why:** an agent checks
  that the repair did what it expected (one hole, two triangles) without
  parsing prose.
- **Not changed:** "nothing to repair" stays a refusal, not a no-op
  success: `RepairModel` already refuses it so the history never records a
  change that wasn't one, and the tool reports that refusal as it is.
- **Cost to change later:** small. If the GUI's defaults move, the two
  constants in `mcp/model_tools.rs` move with them (a shared constant in the
  app crate would remove the duplication; the UI crate could then read it).
  The report's fields mirror `RepairReport`, so a new count there is one
  more line in the reply.
