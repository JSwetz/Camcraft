## #557 - Setup alignment: set the part's X and Y by an edge or two points

**What was decided.** A new in-plane turn on top of the setup's part
alignment (#326). `pick::turn_in_plane` reads a straight edge (start to
end, its walk order) or two vertex picks (first to second) for +Y and/or
+X, takes the direction into the setup frame through the setup's
CURRENT alignment, projects it onto the work plane, and turns the part
about the setup's Z until it runs along its axis. The new alignment is
`turn ∘ current`, pivoting on the parts' bounds centre as the setup
holds them, so the part stays on its spot of the table. It goes through
the existing undoable `AlignPartToSetup` (new door
`AlignPartToSetup::turn_in_plane`, plus `check_turn` for the page's
Apply), so the stock follows, linked setups (#316) follow, ops of the
setup go stale and undo un-stales them exactly as for a preset or a
face-to-plane lay. No new command, no new schema component, no project
format change (the result is the same stored `part_alignment`).

**The work plane is the setup's XY (normal +Z).** The setup frame's Z is
the spindle axis; a face-to-plane lay (#330) decides which face is up,
and this decides which way X and Y point on it. G18/G19 lays still lay a
face onto those planes, but machining happens in the setup's XY, so that
is the plane the axes are measured in. Considered: an axis mode tied to
the lay's chosen G17/G18/G19 plane - dropped, because it would turn the
part about a horizontal axis and undo the lay it is meant to complete.

**Relative, not absolute.** Unlike the face lay (absolute: re-picking
the same face gives the same answer), the axis turn composes onto what
the setup holds, because the issue asks for it to work after laying the
part flat. Re-picking the same edge after Apply is a zero turn and is
refused as "already holds the part that way" (Apply greys out with that
reason). Considered: re-computing the lay plus the turn from scratch in
one command - dropped, the two rows are independent picks and the user
may have reached the current alignment by presets or typed angles.

**Both axes: Y is primary by default, the user may pick X.** With both
given, the primary axis is laid exactly and the other is only checked:
refused by name when the two are within 1 degree of parallel, or when
they make a left-handed frame (X must equal Y x Z) - "flip one of
them". It is never silently flipped. One axis given: the other follows
right-handed from Z. Considered: averaging the two directions (a
least-squares squaring) - dropped, a datum edge is meant to be laid
exactly, as in plain fixture practice.

**Refusals, by name, at command creation:** a face or welded patch ("a
face sets no direction"), a closed loop, a curved (not straight) edge,
one point only, two coincident points (within 1 micron), a mix of an
edge and a point, a pick that is not the part, a stale pick, a direction
within 5 degrees of the plane normal ("runs (nearly) along the work
plane's normal"), parallel or left-handed X/Y, nothing given, and a
non-finite result. The landing is proved (the primary pick lies on its
axis within 1e-9 by the sine) before anything is returned. A quarter
turn within float noise is snapped to the exact quarter turn, so a
90-degree turn keeps exact quaternion components like the presets.

**UI.** A new "Axes in the plane - pick an edge or two points for +Y
and/or +X" block on the setup page (and the Setups overview), below
"Lay on a plane": per axis, "+Y by Edge" / "+Y by Two points", a pick
toggle that names what it takes ("Pick +Y edge" / "Pick +Y points"),
"Flip +Y"; then Primary (Y/X), "Apply axes" (greyed with the command's
reason) and "Clear axes", and the turn in degrees before Apply. Arming
an axis sets the viewport pick mode (Edge for an edge, Vertex for two
points), and only one draft collects at a time (arming the plane row
stops the axis pick and vice versa). The draft is ephemeral view state
(`crate::axis_pick`), never a command until Apply. Edge clicks take one
run, like the boundary and plane drafts.

**MCP.** `set_part_axes` (setup, y, x, flip_y, flip_x, primary) runs
the same command, one undo step; picks are `list_features` ids. Kept in
its own `mcp/align_tools.rs` so the shared tool table is not touched.

**Cost to change later.** Low: the turn is one function in
`pick/axis.rs` feeding the existing command; switching the work plane
or the primary rule is local to it, and nothing is persisted beyond
the alignment the command already stored.

**Hand-check fixes (PR #573, 2026-11-09).** Three UX notes from the
contributor's hand check:

- *Choosing a mode arms its pick.* Clicking "+Y by Edge" / "+Y by Two
  points" (also re-clicking the current mode) arms that axis at once,
  the toggle reads "Stop picking +Y" and the viewport pick mode follows.
  While the axes rows are on screen (`State::axis_rows_shown`, re-earned
  each frame by the rows, cleared by the properties pane like the
  Boundary group's `shown`) and nothing is armed, a Face/Edge/Vertex
  click on the part is swallowed with "no axis is being picked - click
  Pick +Y edge or Pick +X points first" (naming each axis's chosen
  mode), never "open an operation first". Model-mode clicks still
  select. Considered: only the hint, with the toggle the sole arm -
  dropped, the tester expected the mode choice to start picking.
- *The picked row reads in document units.* Its own line under the axis
  row (so it is never cut short): "1 of 2 points on 'block' (x, y, z)
  mm", "2 points on 'block' (..) -> (..) mm", or "edge on 'block',
  50.000 mm long, along (dx, dy, dz)" - no feature id. Coordinates are
  the document frame (the model as placed in the document, before any
  setup alignment), the same frame the points are read in; the
  direction is a unit vector (unitless). App helper:
  `pick::pick_coords`.
- *Flip after Apply re-applies.* Apply no longer drops the draft: it
  stops collecting and keeps the picks (`AxisDraft::applied`). The same
  picks are then a zero turn, so Apply greys out with "applied - tick
  Flip +Y / Flip +X and Apply again to turn the part the other way";
  ticking Flip makes it a 180-degree turn and Apply lands it as one more
  `AlignPartToSetup` - one undo step per Apply. Considered: Flip turning
  the part immediately as its own command - dropped, a checkbox that
  silently turns the part is surprising (the tester's own call).
