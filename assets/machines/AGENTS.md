# AGENTS.md — how to write a `.mch` machine definition

Companion to the root AGENTS.md and to `assets/posts/AGENTS.md` (the
post side of the same contract). Provenance: original JSON, authored
from public machine-tool spec sheets, RS-274/ISO 6983 axis-lettering
convention, and machine-tool handbooks — for CAPABILITY NUMBERS and
behavior only. No vendor definition file was read, adapted, or
paraphrased; the layout was specced from Autodesk's published
post-processor documentation (behavior only) — the grammar and every
key name are ours (`crates/app/src/machine.rs`, `post/machine.rs`).

## What a `.mch` is

Plain JSON, parsed twice on purpose. The CHECKER (`crates/app/src/machine.rs`)
reads the file form for limits and collision (travel bands, spindle
envelopes, toolchanger counts, traverse timing); the POST MIRROR
(`crates/app/src/post/machine.rs`) restates it, and the script's
`machine` global is the flattened form of that. Law throughout:
ABSENT IS NOT ZERO — a limit that is unstated, null, zero, or
non-numeric reads as "not stated", the mirror states nothing, and
posts' `> 0` guards keep their silence. The app never judges an
unstated capability into existence.

## The file-form grammar

    {
      "general":    { "model": "...", "vendor": "Camcraft", "kind": "mill" },
      "kinematics": { "default": { "units": {...}, "parts": [ ... ] } },
      "controller": { "synced_configuration": { "parts": { "X": {...} } } },
      "tooling":    { "number_of_tools": N }
    }

- `general.kind` ∈ `mill` | `lathe` | `mill-turn`. The DECLARATION
  outranks the axis inventory (`kind()` checks the word first; an
  unknown word REFUSES the file at the door — never a silent demotion
  to inference). The inference, when no kind is stated: Y + a TURNING
  rotary is mill-turn, Y alone is mill, X+Z without Y is lathe — note a
  turning center with a C axis and no Y is a LATHE, not a mill-turn. A
  rotary is turning unless its stated direction leans (z² < 0.99·|d|²
  — the same tilt test the RTCP judgment and `can_tilt` read, one law
  at three sites): an XYZ machine with A/B trunnions or a tilt head is
  a MILL whose axes can lean, not a mill-turn.
- `kinematics.default.units`: `"length": "mm"` (or `"inch"`/`"inches"`
  — the factor multiplies EVERY distance and distance-per-minute at
  load; the checker stays mm-only), `"angle": "degrees"`.
- `kinematics.default.parts` is the physical tree — `table` and `head`
  parts frame it; `linear` and `rotary` parts are the driven axes
  (`collect_axes` walks it depth-first: the nest chain is the
  kinematics, the limit table is flat). Each axis part:
  - `name` (or `id`) — the RS-274 LETTER for driven parts (see
    convention below); `X`/`Y`/`Z` map to their direction, other
    linear names are dropped by the mirror.
  - `travel`: `{ "min": a, "max": b }` — a stroke in PROGRAM
    coordinates, absolute world, not joint-local (the checker's
    traverse bands and collision sweep read these directly). Linear:
    mm (unit-scaled); rotary: DEGREES, never unit-scaled. Must be
    finite with `max > min` or the field is dropped. The file cannot
    know the work offset, so a linear slide of stroke L is written
    `-L..L` - the FULL stroke each way, never the machine-frame band
    (`-500..0` for a 500 mm column refuses every retract above the
    work origin, #457; the Tormach files follow the same rule).
  - linear only: `direction` `1` | `-1` (#373) - which way the slide
    moves for a + command; absent or null is `1`, and anything else
    refuses the file NAMING the axis. The program frame stays standard
    (ISO 841); the stated `travel` is the machine's own stroke, so an
    inverted axis is judged as `-v` against it. The post (#374) writes
    an inverted X / Y mirrored (sign, I/J/K, G2/G3, G41/G42) through a
    script that declares `axisInversion = true` (probing too only when it
    also declares `probeAxisInversion = true`, #392); an inverted Z or any
    other letter refuses posting by name. A rotary part's numeric `-1` refuses (a reversed
    rotary is not supported).
  - linear Z only: `safe_retract` (#495) - the Z, in MACHINE
    coordinates (the file's length unit, scaled like travel), a post
    rapids to with `G53 G0 Z..` before a tool change and at program
    end. Zero is a statement here, not "unstated": `0.0` says machine
    Z0 is the top of travel (every shipped Tormach file, `vmc.mch`,
    `tilthead.mch`; the mirror-form default `3axis-vmc.mch` states
    `"safeRetractZ": 0.0`). Absent or null is unstated, and the
    Tormach post refuses a non-Tormach machine that states none (the
    Siemens 840D post, #503, any machine that states none); a
    non-number, or the key on any other axis, refuses the file naming
    the axis. The mirror carries it as `safeRetractZ` (mm).
  - rotary only: `axis` `[x,y,z]` — the rotation direction; three
    finite numbers or the file refuses NAMING the axis (a garbled
    tilt vector must never read as "no tilt": the RTCP judgment rides
    on it). A stated zero vector refuses too. `resolution` is the
    positioning step (same scaling law as travel).
  - `type: "head"` parts carry the spindle and station, first match
    wins: `spindle { min_speed, max_speed }` rpm, `power` kW (stated,
    not judged — the sheet renders it, no model consumes it),
    `body_diameter`/`body_length` for the envelope checks; and
    `tool_station { max_tool_length, max_tool_diameter, coolants }`.
    Separately from first-match, the checker also COUNTS (M5.3 C):
    `Machine.spindles` is how many head parts anywhere in the tree
    state a spindle — the handoff gate reads it (`PART TRANSFER` to a
    second station on a one-spindle machine is a named refusal at the
    program gate). A file stating no spindle keeps `None`: the
    unstated-limit law again, never a guessed `Some(1)`.
  - `live_tool` (issue #33), on the tool station: `{ "c_axis_on",
    "c_axis_off", "spindle_on", "spindle_off" }` - the builder's words
    that engage the spindle as a positioned C axis, release it, start
    the live tool (the post appends `S<rpm>`) and stop it. No public
    standard fixes them, so a post never guesses: all four non-empty,
    or the machine states none and a C-axis live-tool section is
    refused by name. The mirror carries them as
    `head.liveTool { cAxisOn, cAxisOff, spindleOn, spindleOff }`.
  - `coolants`: entries are `{ "name": "Flood", "mCode": "M8" }` (the
    M-code is the machine's own word — string `"M8"` or bare number
    `8`, both normalized to `M8`; `mcode` spelling reads too) or a
    bare NAME string: a named option with no code rides CODELESS, and
    a post that cannot find the code stays silent rather than invent
    `M8`. The stop pair is the fixed M9, not a per-machine number.
    The names the posts read are `Flood`, `Mist`, `Through-spindle`
    and `Through-tool` (`machine::COOLANT_KINDS`, the machine editor's
    pick list; #150) - compared case-blind, spaces and underscores
    reading as hyphens, so `Through spindle` reads. A Flood op takes
    the first Flood entry, else the first coded entry of no other known
    kind (a shop's own `Brine`); a list stating only other kinds has no
    flood code and a Flood op on it is refused by name. A file that
    keeps its list at `tooling.coolants` instead of on the station also
    reads.
- `controller.synced_configuration.parts` is keyed BY LETTER:
  `{ "coordinate": i, "max_normal_speed": r, "max_rapid_speed": r }`.
  The kinematics tree decides WHICH axes exist; this table says how
  fast they go (mm/min for linear — deg/min for a rotary, and those
  deg/min NEVER mirror into a prismatic field: the unit-lie law) and
  `coordinate` is the display/sort order (0=X, 1=Y, 2=Z, rotaries
  from 3 up). A letter absent here simply has unstated rates.
- `tooling.number_of_tools`: DISTINCT tool positions the program may
  call (a double-tier of the same 40 stations is 40, not 80); a
  program asking for a tool number above it is a checker refusal.

## What the script sees

`mirror()` restates the checker form in the static grammar:
`kinematics.static.axes[] { letter, direction, speeds{rapid,normal} }`
(linear only), `kinematics.static.rotaries[] { letter, type,
direction, travel[min,max] }` (geometry only — no speeds), and
`kinematics.static.head.spindle.max` + `head.toolStation.coolants`.
`script_props()` then flattens it into the `machine` global:
`name`, `capabilities`, `rapidRate` and `feedRateMax` (the MAXIMA over
stated LINEAR rates — absent everywhere → 0 → silence, never a fake
ceiling), `spindleMax` rpm, `coolants[]`, `rotaries[]
{name, direction, travel}`, and `rtcp`.

`rtcp` is DERIVED GEOMETRICALLY, never a hand-typed flag: true iff
some rotary states a direction that is not the Z column
(z² < 0.99·(x²+y²+z²)). A C table ([0,0,1]) is not RTCP; a trunnion
or tilted axis is. A post may output the tool-center-point promise
only when this reads true — `machine.rtcp` says so.

## The axis-lettering convention

RS-274: rotation about X is `A`, about Y is `B`, about Z is `C`; the
letters in a `rotaries` list are these, not shop names. A slide is a
slide even if the shop calls it a fourth axis; the stream's `A/B/C`
PARAM words (docs/CNC-INTERMEDIATE.md) address the same letters — a
machine with no `C` part must not make a post emit one; the tilted
record is refused instead.

## Registering a library machine

1. Write the `.mch` here with its provenance header comment
   (JSON `//`-style notes ride the loader's tolerance; the shipped
   files carry theirs in `vendor`/`model` and this style section).
2. One `("<Name>", include_str!(...))` line in `MACHINES`
   (`crates/app/src/posting.rs`; `vmc.mch` stays DEFAULT_MCH — entry
   order is not law, the names are what the picker reads).
3. The library test parses every entry BOTH WAYS (file form and
   mirror) and asserts declared kind == inferred kind, so a
   half-registered or self-contradictory definition fails the suite,
   not the shop.
4. Gate: `cargo test -p camcraft-app --test machine --test posting
   --test machine_library` (the last runs the Machines pane's validator,
   `app::machine::validate`, over every library entry - #287),
   plus a CLI smoke — `post.exe --machine <name>.mch <post>.cps
   <file>.cnc out.nc` over a committed golden stream (a 5-axis
   definition should drive the refusal path in fanuc.cps's `tilt()`
   on a tilted golden, and the G43.4 path with `--trace`).

## Style

2-space JSON, keys in the order above, numbers plain with a decimal
point (`30.0`, `0.001`). Model names are shop-uppercase words ("VMC
3-AXIS", "MULTITASKING 5-AXIS", "TURNING CENTER 2-AXIS") —
`vendor = "Camcraft"`, per the M4.3 naming law. Nest as the machine
moves: the standard chain nests Y in X in Z (or off the table for a
trunnion); `base`/`table` roots, `head` beside the axes.
