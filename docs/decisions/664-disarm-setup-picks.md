## #664 - Setups page picks disarm when their page is left

**Decided:** every Setups page row that can collect viewport picks (the
lay-on-a-plane face, +Y/+X by Edge or Two points) records its setup in
`State::setup_pick_rows` each frame it is drawn. `pages::properties`
clears that list before drawing the pane and, after it, calls
`axis_pick::disarm_hidden`: any armed pick whose setup's rows were not
drawn this frame stops collecting. One place covers every way off the
page - another page or tab, an operation opened, the section closed, a
different setup's page. The picks themselves stay: the axis draft keeps
its edge/points (as its own Stop did already); the plane draft gains a
`picking` flag and is paused rather than dropped, so returning shows the
picked face with Apply, and "Pick a face" resumes the same draft.

**Why:** the issue asks for the drafts' picked items to survive a page
change; a frame-level "rows not drawn -> disarm" sweep matches the
existing `axis_rows_shown` / `boundary_view.shown` pattern and cannot be
missed by a new navigation path the way per-action resets could. Arming
is ephemeral view state, so no command is involved.

**Dropped:** resetting the picks in each navigation action
(`actions::target`, Select, close, ActivateSetup) - many sites, easy to
miss one. Dropping the plane draft on leave (the old `disarm`) - loses the
picked face the issue says to keep. The plane row's own "Stop picking"
button still drops its draft as before (unchanged behaviour).

**Cost to change:** small - the sweep is one function and the list one
field; per-row flags could replace the list if more pick rows appear.
