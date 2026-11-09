## #261 - Mark a warned path's moves in the viewport (part 1)

**Decided.** A generated path's verify warnings now show where they are:
every move a warning names draws in the colour-vision palette's gouge
colour instead of its feed / rapid / plunge colour, the first move of
each folded warning line gets a small 3-axis cross in the same colour,
and each folded line on the op page's status banner is a link - a click
centres the viewport camera on that move's midpoint (orientation and
zoom kept, the Deviate bar's `frame_at` jump). The "which moves" logic is
a pure app function (`app::warned_moves`: `moves_named`, `warned_mask`,
`move_focus`), the painting is `ui::path_lines::cache_lines`, split out of
`interact::toolpath_lines`. Part 2 of the issue (a path for a refused
generation) is not in this PR.

**Why.** The indices were already cached with the path; the only surface
was text ("shank/holder at move 84"), and counting moves in the viewport
is not a workflow. The gouge colour is the issue's own suggestion and is
already colour-vision gated against clean / rest / unjudged stock.

**Details chosen.**
- `RestUnjudged` marks and frames nothing: its `at` is "the first cutting
  move" by convention, not a location. Every other kind (Scallop
  included - it is located at the nearest cutting move) marks its move;
  `Crossing` marks both moves and frames the first.
- Indices are untrusted (a cache can come from a project file): an index
  past the path's end is dropped, never indexed with.
- The cross marker exists because the house plunge red
  (0.92, 0.30, 0.25) sits close to the gouge red (0.90, 0.18, 0.14): a
  warned plunge would not read by colour alone. One cross per folded line
  (not per warned move) keeps a 1000-move shank run from carpeting the
  view.
- Click-to-frame is view state (`State::cam.target`), never a command,
  like every camera move. It reuses `actions::frame_at` and keeps the
  zoom rather than fitting the move, so a click never jumps the scale.
- A path with no warnings draws vertex for vertex what it drew before
  (no default fixture or pixel snapshot moves).

**Considered and dropped.**
- Adding "warned/plunge" to `Semantic::must_distinguish`: the Off palette
  would fail the 20 dE00 gate and changing the house gouge or plunge
  colour moves every snapshot; the marker carries the distinction instead.
- Zoom-to-fit the warned move: a short move would zoom the camera into a
  pinhole; centring keeps the user's scale.
- Clickable warning text in the browser row (it shows a count only):
  the op page is where the folded lines are.

**Cost to change later.** Small: the colour choice is one function
(`path_lines::move_colour`), the marker one loop; swapping to a dedicated
"warned" palette slot means adding it to all three `Semantic` tables and
the gate.
