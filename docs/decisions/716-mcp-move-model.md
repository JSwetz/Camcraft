## #716 - MCP: move and rotate an imported part (2026-11-10)

- **Chosen: one tool over the existing command.** `move_model { name,
  position, rotation? }` builds `MoveModel::new` and runs it through
  `env.history`, so a headless move is the same single undo step the GUI's
  model move makes, and every refusal is the command's own text
  (`'<name>' is a fixture - the Fixtures page places it`, `is a work
  offset`, the machine table, `no entity named`, `rotation is not a
  quaternion`). **Why:** "Everything Is a Command" - the MCP layer only
  translates arguments. **Dropped:** placing the part by writing its
  transform directly (no undo, and it would skip the stock that follows).
- **Chosen: the same argument shape as `move_fixture`.** Position
  `[x, y, z]` in mm and a unit quaternion `[x, y, z, w]`; an omitted
  rotation keeps the part's current one, so "lift it 4 mm" doesn't untwist
  a part turned earlier. A non-unit quaternion is normalized by the
  command's `check_location`, as for fixtures. **Dropped:** Euler angles
  (a second rotation vocabulary beside `set_wcs` and `move_fixture`, which
  both speak quaternions).
- **Chosen: report the stock that rode along.** `MoveModel` already moves
  the stock made from the part, or the stock of a one-part document, with
  it. The reply lists each by name with its new placement, read from
  `MoveModel::following_stock()` before `execute` takes the command.
  **Why:** an agent that moves a part and keeps planning against stock
  bounds it read earlier would be wrong without noticing.
- **Chosen: its own module, `mcp/model_tools.rs`.** `fixture_tools.rs` is
  about workholding; part placement is a sibling, and the module has room
  for the other part verbs the `camcraft-mcp` skill lists as gaps (mesh
  repair).
- **Cost to change later:** small. The tool is a thin reader over
  `MoveModel`; adding Euler angles or a relative ("by") move later is a
  new optional argument, and the reply's `stock` list can grow fields
  without breaking callers that read `name` and `origin`.
