## #312 - MCP detects holes through the app's own rule

- **Chosen:** The Detect holes rule moves from the UI into the app as
  `operation::detect_holes(world, name, params, setup, src)`:
  - the operation's source read for holes now (`holes_of`);
  - written into whichever hole list the kind has (Drill, Bore, Thread);
  - the drill's radial list filled when the machine holds a turret
    (`cross_holes_of`);
  - refusing by name when the kind has no list or the source has no hole.

  The UI's button (`State::detect_holes`) and a new MCP tool
  `detect_holes {name}` both call it and commit the result as one
  undoable `EditOperationParams`.
- **Why:** Drill, Bore and Thread machine only the holes stored in their
  parameters. Over MCP nothing could fill them, so an agent's drill on a
  model with holes always refused "no holes to drill". Copying the rule
  into the MCP module would let a click's holes and an agent's drift.
- **Unchanged:** the UI's refusal texts ("not a drilling operation", "no
  drilled holes found in the source model - Z or radial", "no Z-axis holes
  found ... this ride reads only those") and its draft rule (a pane that
  targets the operation reads its draft). The UI's own test
  `detect_holes_refuses_recognizes_and_undo_empties_again` passes
  unchanged.
- **Considered and dropped:** having MCP's `add_operation` detect holes
  automatically for the drilling kinds. That would change what
  `add_operation` means for one family, and an agent that wants a subset
  of holes still needs the explicit list. Detecting is its own step, as
  in the app.
- **Cost to change later:** Small. One function, one tool row.
