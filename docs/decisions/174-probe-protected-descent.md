## #174 - Probe op: stop-on-contact protected positioning moves

**Decided.**

1. **Marked in the intermediate stream, derived from the touches.** The
   dump writes a new `GUARD x y z F feed` record (`Record::Guard`) for
   every non-rapid straight move of a path that carries probing touches,
   other than the touches (`TOUCH`) and their back-offs (`RETREAT`): the
   descent to a touch's start, the bore entry to its centre, the rise
   back to the safe plane. No new field on `Toolpath`: a probe path is
   all touches, back-offs, rapids and positioning feeds, so the touches
   already say which moves these are, and the project file, the
   `.cl` dump and every other `Toolpath` literal stay as they are (a
   loaded project posts the guards with no migration). An arc or a feed
   without a rate in a probe path cannot be protected and is refused
   with a WARN - never posted as a plain move.
2. **The rapids stay rapids.** The travel between touches at the safe
   plane, and the opening rapid down to the rapid plane (approach
   clearance over the probed feature's top), are not turned into
   protected feeds: the safe plane is the op's clearance height, which
   the protected-move law already judges against the model, and feeding
   the whole clearance travel at the probe feed would make every cycle
   slower for no gain the descent does not already give. Considered:
   protecting the opening rapid too (dropped: it is the program's
   positioning block that every post writes first; the issue names the
   descent from the safe plane, which is now protected).
3. **Fanuc: G31, read the other way round.** `onProbeGuard` writes the
   G31 skip block to the point, then for each axis that moves
   `IF[ABS[#506n-<end>]GT0.001]THEN#3000=3(PROBE HIT ON POSITIONING)`:
   per Fanuc's published custom macro B documentation the skip position
   variables hold the block's end point when no skip signal fired, so a
   short stop means an unexpected contact. Alarm 3 beside the existing 1
   (out of tolerance) and 2 (no contact). One `IF` per moving axis, not
   one `IF` joining them with `OR`: each line stays the plain
   `IF[cond]THEN` form the existing touch checks already use. Every
   positioning move the cycle lays down moves one axis (Z), so this is
   one test per guard in practice.
4. **Tormach PathPilot (LinuxCNC): G38.3 + #5070.** `G38.3` probes toward
   the point and does not error without contact; `#5070` is 1 when the
   probe tripped; the test sits in `oN if [#5070 EQ 1]` / `oN endif`
   with `(MSG, PROBE HIT ON POSITIONING)` and `M2` - the same alarm
   pattern #362 (PR #376) uses for out-of-tolerance, chosen because
   `(ABORT,...)` is not in the stable LinuxCNC reference and M0 could be
   resumed past. O-word numbers run from 5001 so they never collide with
   #376's 901.. range. Sources: the LinuxCNC G-code reference (G38.n,
   numbered parameters, O-codes, messages) and Tormach's PathPilot
   machine-code reference (G38.2-G38.5, M2 listed). Placed in its own
   section of tormach.cps (before the canned cycles), away from #376's
   hunks.
5. **Refusal.** A script without `onProbeGuard` is refused by name, once
   per operation (`probing: operation "X": this post script does not
   define onProbeGuard ...`), whether or not it can probe - dropping a
   motion silently is never an option. A script without `onProbeMove`
   gets both refusals.
6. **Inverted axes.** `invert.rs` mirrors a GUARD like an absolute move
   (no refusal of its own); the touches beside it still carry the
   probing refusal on main, and #392's (PR #465) probing translation
   then covers the guards with no further change.

**Cost to change later.** Moving the mark onto `Toolpath` (an explicit
`guarded` list) would touch every `Toolpath` literal and the project
DTO; the stream record and the hooks would stay as they are.

**Follow-ups for the open probing PRs.** #376 (Tormach G38.2 touches)
gets its protected moves from this PR as is; its golden for the probe op
will now carry G38.3 positioning blocks. #360 (Haas, Mazak, Siemens
hooks) must add `onProbeGuard` to each of those scripts, or its probe
ops are refused by name once this merges: Haas and Mazak the same G31 +
`#5061..` test, Sinumerik a measuring move with an `$AC_MEA` test.
