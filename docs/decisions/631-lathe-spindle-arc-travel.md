## #631 - a lathe's spindle-axis arc is judged by its radius

**Symptom:** the built-in `lathe.mch` (X 0..320) refused every thread: "move 2 reaches -5.600 in X, the stroke is 0.000..320.000".

**Cause:** a thread pass is a chain of arcs **about the spindle axis** (XY plane, centred on the program's X0 Y0). Their far side, and the travel check's added quarter points, read negative X. That's the CL's antipode, the spindle turning the work, never a slide position. The post writes the radius. The machine check judged the raw signed X.

**Chosen:** on a machine of kind lathe, an arc in the XY plane centred on the spindle axis (within 1 µm) is judged in X by its **radius** `hypot(x, y)` at each extreme. Every other move keeps its signed X, so a real move past centre (facing to centre and beyond, a rear tool) is still judged against the band exactly as before.

**Why this and not the other option:** the issue offered two fixes.
- **Widening `lathe.mch` to admit negative X:** dropped. It would mask a real past-centre over-travel on that machine, and every lathe file would have to repeat the workaround.
- **Judging the posted diameter:** chosen, in the radius form. The check stays in the app, on the CL, in one place, and needs no post knowledge.

**Pinned:** `lathe_thread_travel::a_thread_passes_the_built_in_lathe` (fails without the fix, with the issue's own message) and `a_real_over_travel_on_a_lathe_is_still_refused` (the same thread against an X stroke of 0..2 mm is refused "… in X").
