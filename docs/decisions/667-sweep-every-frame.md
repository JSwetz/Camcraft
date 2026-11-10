## #667 - The setup-pick sweep runs every frame, whatever the dock shows

**Decided:** #664's sweep moved out of `pages::properties` into the
shell's per-frame `shell_layout`, bracketing the dock. Before the dock
draws, a Properties tab that will not draw this frame (closed, behind
another tab in its leaf, or its leaf collapsed - `layout::is_shown`)
disarms every Setups page pick (`axis_pick::disarm_all`). After the dock,
`axis_pick::disarm_hidden` disarms any pick whose rows were not drawn and
sets `axis_rows_shown` from the same row marks (the plane and axes rows
always draw together).

**Why:** viewport clicks route inside the dock (`interact::pick_at` runs
in the viewport tab), and egui_dock draws leaves breadth-first, so in the
default layout the viewport draws before the properties leaf. An
after-dock sweep alone leaves one frame where a pane closed or hidden by
a click on the previous frame (its rows drawn that frame) still has an
armed pick when the viewport routes; the pre-dock visibility check shuts
that frame. A pick armed at a frame's end therefore always had its rows
on screen that frame.

**Dropped:** keeping the sweep in `pages::properties` and adding a second
one for the closed case (two places to keep in step); deferring viewport
picks until after the dock (reorders the whole pick path for one rule).
A collapsed floating window holding the pane is caught by the after-dock
sweep only (no click can land in the frame of the collapse click).

**Cost to change:** small - two calls in `shell_layout` and one helper.
