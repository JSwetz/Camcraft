## #691 - a step too small to ladder is refused by name

**Decided:** the two shared step ladders in `motion::machining2d`, `band` (facing, zigzag pocket and slot rows) and `cut_levels` (every depth-stepping 2D op and the 3D entry ladders), now return `Result`. Before they allocate anything they check `pass_budget`: a step needing more than `MAX_PASSES` (100 000) passes, or one that cannot advance the coordinate (`span + step == span`), is refused as `GenError::Invalid("<stepover|stepdown> <v> mm over <span> mm needs more than 100000 passes - raise the <stepover|stepdown>")`. Callers propagate it with `?`.

**Why:**
- The crash was the allocation, so the guard has to sit where the count is known, which is the ladder.
- The door (`OpParams::is_sane`) sees no span, so it cannot name a pass count.
- 100 000 rows or levels is far past any real job: 0.01 mm over a metre is 100 000. The vector it bounds stays under a megabyte.

**Considered and dropped:**
- An absolute floor on stepover/stepdown at the door (e.g. 1 µm). It doesn't depend on the span, so it can't fail by name the way the issue asks. It could be added later as a belt-and-braces check.
- Clamping the step silently. That emits a toolpath other than the one asked for.

**Cost to change:** one constant (`MAX_PASSES`).

**Not covered:** generators with their own ring loops, such as offset pockets and 3D stepovers, don't go through these ladders. They aren't reported crashing, and this PR doesn't touch them.
