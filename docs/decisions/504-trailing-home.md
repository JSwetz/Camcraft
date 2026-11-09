## #504 - a trailing Go home merges with the post's program-end home

**Decided:** each built-in post that homes at program end decides it in
its own script. `afterOp()` records `homedAt = linesWritten()` right after
a trip home; `onClose()` skips its own reference return when
`linesWritten()` still equals it - nothing was written after the trip home,
so the machine is already home. `linesWritten()` is a new one-line PRELUDE
helper (flushes the open line, returns `OUT.length`). Applied to Fanuc,
Haas, Mazak, Siemens 840D and Fanuc Lathe. A Go home last in the program
(entry or per-op choice - both fold into the last posted op's
`PARAM AfterOperation home`) now posts as `M5 / G91 G28 Z0 / G28 X0 Y0 /
G90 / M30` on Fanuc, the close's pair not repeated. Mid-program homes,
Suppress and "Skip pauses and homes" are untouched (they act on the stream
before the post).

**Why per post, not in the app/engine:** the posts differ. Siemens closes
with `G53 Z0` only - dropping a trailing XY home there in the stream would
lose a requested home. Fanuc's trip home writes `G49` first under TCPM and
its close does not, so dropping the ask in the stream would also drop that
guard. The lathe's trip home is X then Z in two blocks, the close one
`G28 U0 W0`. In every case the trip home covers at least what the close
home does, so the script keeping its own (asked, more careful) home and
skipping the close's is the safe merge, and only the script knows that.
Heidenhain refuses a trip home by name and Tormach does not read
`AfterOperation` (refused by name) - both unchanged.

**Considered and dropped:** (a) the app's `posted_afters` dropping a
trailing home - it cannot know what the post's close writes (Siemens would
lose XY). (b) An engine-level `closeHomes` manifest flag that strips the
last section's `home` ask - generic, but loses Fanuc's TCPM `G49` and the
lathe's X-then-Z order. (c) Counting with a per-script `block()` counter -
misses lines written through `writeln`/`writeComment`; `OUT.length` sees
every line.

**Cost to change later:** five small `onClose` guards and one PRELUDE
helper; a new homing post opts in by copying the two lines, and one that
does not simply homes twice as before (harmless).
