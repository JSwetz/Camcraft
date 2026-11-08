## #373 - Machine file: per-axis direction

**Decided:** a linear part of a file-form `.mch` takes `"direction": 1 | -1`
(absent or null = `1`, standard). It parses to `AxisLimits::direction`
(`machine::AxisDirection::{Standard, Inverted}`), serialized `1`/`-1` in the
project file and skipped when standard, so every shipped machine, every
saved project and the post mirror stay byte-identical. Anything else (`0`,
`2`, `0.5`, `"-1"`, a vector, a bool) refuses at load naming the axis; a
garbled sign never reads as standard. The CAM frame stays right-handed
(ISO 841, #372's decision): generation, verify and the stock sim are
untouched.

**Travel:** a stated stroke is the machine's own (what its controller reads),
so an inverted axis is judged as `-v` against `[lo, hi]` in the program
check (the breach line says "(inverted axis)"), and the derived table box
maps it into the program frame as `[-hi, -lo]`
(`AxisLimits::program_travel`).

**Posting until #374:** a mounted machine with an inverted axis adds a named
problem to the verification gate - "machine <model> has an inverted X axis;
posting for inverted axes is not supported yet (#374)" - so the single
post, per-op posts and the script API's gate all refuse; the CLI `post
--machine` refuses the same way (and now refuses a file `parse_mch`
refuses, rather than posting it raw). #374 removes this clause when it lands
the translation.

**Machine page:** the machine editor window edits a draft and applies it
through the existing undoable `EditMachineApply` -> `LoadMachine` command, so
the per-axis choice ("X+ moves: right / left", Y "away / toward you", Z
"up / down", other letters "standard / reversed") rides that command; no new
command was needed. An inverted axis shows a "posting refused until #374"
note beside it.

**Considered and dropped:**
- The issue's example shape, a top-level `"axes": {"x": {"direction": -1}}`
  map: dropped for the linear part's own key, where its travel and
  resolution already live (one place states one axis); the controller
  table (`synced_configuration.parts`) was the other candidate and is about
  rates, not geometry.
- Reading the travel as program-frame numbers (no mapping): dropped - a
  builder's stroke is stated in the machine's coordinates, and the post
  (#374) will write the negated coordinate the controller reads.
- Inverting rotaries: out of scope (#372 names rotary on an inverted axis a
  refusal); a numeric `-1` on a rotary part refuses by name, other shapes
  there (a vendor's vector) are not read, as before.
- Mirroring the sign into the script's `machine` global now: left to #374,
  which decides what the scripts need; nothing posts for such a machine
  until then.

**Cost to change later:** the key is one parse site and one serde field; a
different spelling would need a reader alias, not a migration.
