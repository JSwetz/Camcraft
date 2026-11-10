## #647 - the lathe stroke check judges the posted radius, not world X

**Decided:** on a machine of kind lathe, each operation's slide stroke is
judged on the path the post is handed (`posting::nose::posted_path`: the
control point for nose-centre turning ops since #630, the cached path for
every other op), in a module of its own (`app::machine::lathe_stroke`). X is
read the way the lathe posts read it (`fanuc-lathe.cps` `radiusOf`): the
signed offset from the spindle axis on the path's meridian (X if Y is on the
axis, else Y; half-micron tolerance), so a point past centre on the meridian
is negative and meets the band's floor. Z is the origin-relative Z. The
direction sign, the arc quarter points and the refusal wording ("<op>: move
i reaches v in X, the stroke is lo..hi") are the generic walk's. Mill and
mill-turn machines keep the generic per-axis walk unchanged.

**Two non-meridian readings:**
- An XY arc centred on the spindle axis (a thread's turn, #631) is the
  spindle turning: X is judged on its radius at every extreme.
- Any other point off every meridian is not a lathe position (the post
  refuses it). It is judged on its signed world X (the pre-#647 reading),
  its radius, and its Y against any Y band, so no single reading can acquit
  it. Over-reporting a real stroke is the honest direction of error.

**When the conversion is refused** (an orientation that cannot cut the op):
the post refuses the op by name already; the check falls back to the cached
nose centre rather than skip the op, so the path is never left unjudged, and
it adds no second message for the same refusal.

**Considered and dropped:**
- Folding the meridian reading into the generic per-axis loop: it would
  touch the same lines as PR #643 (#631) and mix lathe and mill laws in one
  loop. The lathe walk skips the generic travel block with one `continue`,
  placed above #643's hunk so the two merge cleanly (checked with
  `git merge-file`; #643's own tests pass on this branch without its code).
- Judging the cache and widening the X band by the nose radius: judges a
  point the control never drives, and lets a band-edge decision ride a
  guess about the orientation.

**Changed by it:** a lathe's Z verdict now names the control point's Z too
(orientation 2 sits r below the nose centre), so the built-in Lathe's
clearance over-travel reads 634.200, not 635.000
(`lathe_stroke::a_clearance_past_the_lathe_carriage_still_refuses_by_name`).

**Pinned:** `lathe_stroke::a_facing_control_point_below_the_x_floor_is_refused_by_name`
and `a_turning_pass_beyond_the_x_ceiling_is_refused_by_name` (both fail
before: the Y-meridian radius was never judged),
`a_facing_control_point_on_the_x_floor_passes`,
`a_facing_pass_to_centre_fits_the_built_in_lathe_x_band`, and the unit test
`machine::lathe_stroke::tests::the_meridian_radius_is_read_signed`.

**Cost to change later:** one module; the generic walk is untouched apart
from the skip.
