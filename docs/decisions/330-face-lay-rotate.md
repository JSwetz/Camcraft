## #330 (reopened) - Lay on a plane: the face row's controls stay on screen

**What was decided.** The reopened report ("Lay on plane with a picked
face only moves the model; a tilted face stays tilted") was reproduced
through the real Setups page in the shell at the hand-test window size
(1280 x 720), with the seed block turned 30 degrees about X. Two UI
faults, not the turn, were behind it:

1. On main the row armed the viewport in Edge mode, so a click on the
   face - even near its middle - took an edge: one edge is refused and
   Apply stays grey. #554 (PR #573, commit 7d641c8b, "the setup page's
   plane alignment picks faces only") already fixes this on the stack.
2. The row was one long horizontal line: "Lay on a plane - pick a face",
   the pick button, the plane combo, Flip and "Face to plane level" -
   and then, once a face was picked, "Picked: Face <19-digit id> of
   '<model>'" with Apply after it. In the ~285 px properties pane, Flip,
   "Face to plane level" and Apply all ran past the pane's right edge,
   out of reach (a real click on them does nothing). The only Apply in
   reach was the Part alignment row's, which turns the part by typed
   angles about its bounds centre - the "changes the alignment by hand,
   shifted relative to the origin" in the report. And without Flip, the
   default lays the face's outside UP (part below the plane).

The face row now uses short lines: the label and "Pick a face"; the
plane combo and Flip; "Face to plane level"; the picked face on a line
of its own; and the row's own button, renamed "Apply face" (a name of its
own, like "Apply points", so it can't be confused with the angle row's
"Apply"), on its own line. The three-points row (#555) and the "Axes in
the plane" rows (#557) had the same overflow (their pick buttons and
Flip boxes ran off screen) and get the same short lines. The structural
test pins every option of the three rows inside the window.

**Why.** The command was correct: `pick::lay_on_plane` +
`AlignPartToSetup` turn a picked tilted face onto G17/G18/G19 (on main
too - proved by an app-level test and by forcing a face into the draft
in the shell). What the user could reach in the UI was what was broken.

**Considered and dropped.**
- Changing the default to Flip on (face down on the table). The existing
  face row, the points row and their tests all read "unflipped = outside
  along the plane's +axis"; changing it would be a separate behaviour
  change for the maintainer to decide. With Flip now reachable, "face on
  the floor, part above" is one click.
- `horizontal_wrapped`: the pane's content is wider than what is shown
  (other rows widen it), so wrapping did not bring the controls back on
  screen; explicit short lines do.
- Shortening the face words (dropping the id): kept as is; they now sit
  on a line of their own and don't push anything off.

**Cost to change later.** Layout only; no project-file or command change.
