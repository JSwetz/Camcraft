# 374 - Posting for an inverted machine axis

Issue #374 (part of #372; builds on #373 / PR #385). Decisions taken unattended.

## Where the translation lives: the engine, once

**Chosen:** one pass over the record stream in the post engine
(`crates/post/src/post/invert.rs`, called from `engine::run_numbered_with`
before any hook). Every script then sees the controller's frame: mirrored
end points, arc centres, `Start*` chain and `Origin*` work offset, the
reversed `CircularDirection`, the swapped `COMP` side.

**Why:** every dialect already derives its words (I/J/K increments as
`centre - start`, `X = v - OriginX`, G2/G3 from `CircularDirection`,
G41/G42 from the comp side) from those properties, so mirroring the inputs
mirrors every output consistently; no script grows sign logic, and a
standard machine gets its records back untouched (byte-identical goldens).

**Dropped:** a shared JS helper in every `.cps` (seven copies of one law to
keep in step; a forgotten call is a silent mirror image); writing a
controller mirror-image word (G51.1) - #372 decided against it.

## Which scripts may post it: a manifest flag

**Chosen:** a script declares `axisInversion = true` (like `coolantModes`,
`afterModes`, `liveTooling`) once its output on the mirrored stream is
verified - goldens plus the controller-interpretation equivalence test.
Fanuc and Haas declare it. Sinumerik, Heidenhain, Mazak, Tormach and the
Fanuc lathe do not and are refused by name.

**Why:** the engine's translation is dialect-independent only as far as a
script reads nothing else that depends on axis sense (Heidenhain's arc form,
lathe diameter words); a flag makes "verified" a stated claim, not an
assumption. A user's own script opts in the same way.

## What is refused by name (hard error, nothing posted)

- **An inverted Z, or any letter but X / Y** (`untranslated_inversion`,
  shared by the engine, the app's verification gate and the CLI): tool-length
  compensation (G43), the canned cycles' R-plane retract and "up is away from
  the work" all assume the controller's +Z leaves the part. Considered:
  translating Z coordinates anyway - dropped, G43's sign is the controller's
  configuration and cannot be known from the machine file.
- **Helical arcs** (the out-of-plane coordinate changes): the issue lists them
  as unverified. The mirror math would be the same, but no golden covers it.
- **Threading cycles, tilted (A/B/C) moves, C-axis live tooling (polar /
  G12.1)**, per the issue.
- **Probing** (TOUCH / RETREAT / FOUND): the G31 skip, the measured-variable
  arithmetic and the G10 L2 write run in the controller's frame through macro
  code no equivalence reader here models. Follow-up: #392.
- **A stock-box edge without its pair** (the mirrored box's left edge is the
  old right edge, so the pair swaps; a lone edge cannot be mirrored).
- **A static-grammar axis stating `"reversal": true`**: nobody read that key
  before; now it refuses instead of silently posting standard-sign code.

Refusals are collected over the whole stream and reported once each, so one
post names everything at once. They are `PostError::Refused`, not counted
warnings, because the CLI writes its output file even with warnings.

## Canned cycles with X / Y inverted

Translated: the hole's X Y negate; the cycles drill along Z, which is never
inverted here, and no shipped cycle shifts in X / Y (G76/G87 orientation
shift is not posted). Tapping depends on spindle sense, not axis sense.

## Cutter compensation plane

The `COMP` record carries no plane; every mill generator compensates in
XY, so G41/G42 swap when exactly one of X and Y is inverted. The lathe post
(the G18 comp user) is refused by the manifest flag.

## How the machine reaches the engine

`machine::mirror` writes `"invertedAxes": ["X", ...]` only when there are
any (standard mirrors byte-identical). Through the raw door (a `.mch` file's
own JSON, the CLI's `--machine`), the engine re-reads the file with
`parse_mch` - the one parser of the `"direction"` grammar - and a file that
parser refuses posts as before unless a part states a direction, which then
refuses.

## Found on the way

The Fanuc / Haas scripts engage G41/G42 while a G18/G19 plane is still
modal when a COMP record follows an XZ/YZ arc (pre-existing, not
inversion-specific): filed as #391. The probe stream steps back into G17
with an XY arc before its COMP so the goldens do not lock that behaviour.
