## #263 - a ball nose's corner radius is its ball

- **Chosen:** A ball nose's corner is half its diameter by law
  (`Tool::tip_corner_r`). The tool editor shows that number as text on a
  ball nose, with no field to edit. Apply saves the tool with
  `corner_r = d / 2`, and the library summary reads `tip_corner_r`, so the
  document and both screens state the radius that is cut.
- **Why:** Every reader already ignored the field on a ball nose:
  `Cutter::of` sets the corner to the radius, `Tool::profile` draws the
  full dome, and the stock sim sweeps the exact sphere. An editable field
  that changes nothing is the inert-field case the editor's own law
  (show a field only where the kind reads it) exists to prevent. A draft
  retyped from an end mill also saved a ball with `corner_r = 0`, and the
  summary then said "corner r 0".
- **Considered and dropped:** Hiding the corner row on every kind that does
  not read it (end mill, drill, the lathe kinds). That follows the same law,
  but it changes the end-mill tool editor's pixel golden, which can only be
  regenerated on the CI platform (#255). It is left for that cleanup.
  Refusing a ball nose whose `corner_r` is not `d / 2` in `Tool::is_sane`
  was also dropped: it would refuse ball noses that cut correctly today
  (an MCP or CLI caller adding one with any corner) for no change to what
  is cut.
- **Cost to change later:** Small. The law is one method, and the save path
  is one line in `ToolUi::to_tool`. A future kind with a spherical tip adds
  one match arm.
