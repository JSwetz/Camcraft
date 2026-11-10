## #362 - probing hooks for the Tormach PathPilot post (2026-11-08)

- **Chosen:** `assets/posts/tormach.cps` voices `onProbeMove` /
  `onProbeRetreat` / `onProbeResult` in LinuxCNC words, keeping the
  generic dialect's contract (`fanuc.cps`, #108; the #173 siblings): a
  probing move to the overtravel target, the measured trip copied to
  `#(100+3k)..#(102+3k)`, an incremental back-off, the mean of opposite
  trips minus the expected value as the deviation in `#190..`, a stop past
  the tolerance, then the work offset written or a stop for the operator.
  The preamble no longer starts a spindle, writes a belt note or turns on
  coolant for a section stating 0 rpm (a touch probe). Goldens:
  `nc/tormach/probe_{face,edge,corner,bore_centre,boss_centre}.nc` on the
  770MX (inch) and `probe_bore_centre_mm.nc` (metric), pinned by
  `crates/app/tests/probe_tormach.rs`; `probe_op.rs`
  `the_tormach_post_probes_the_op_with_g38_2` posts a Probe op through
  `posting::post_script` (it was refused by name before).
- **Sources** (public documentation only; no Fusion 360 / HSMWorks post or
  output consulted): the LinuxCNC G-code reference (linuxcnc.org/docs,
  `gcode/g-code.html` G38.n and G10 L2/L20; `gcode/overview.html`
  numbered parameters #5061-#5070, #5220-#5390, #5420-#5428, expressions
  ABS / GT, comments `(MSG,...)`; `gcode/o-code.html` if/endif); Tormach's
  PathPilot post-processor guidelines (knowledgebase.tormach.com) and its
  machine-code page for G38.x (tormach.com/straight-probe-g38-x).
- **Touch = G38.2, not G38.3 + #5070.** Both references say G38.2 stops on
  contact and the control signals an error and halts when no contact
  comes (and refuses a start with the probe already tripped). The control
  itself guarantees a probe that never touched never backs off and
  measures air, so no post-side no-contact test is written. Dropped:
  G38.3 + `o<n> if [#5070 EQ 0]` - documented, but it moves that safety
  from the control into our own branch for no gain. The positioning feeds
  around the touches are #174's protected moves (main's `onProbeGuard`:
  G38.3 + `o5001.. if [#5070 EQ 1]` / M2) - there G38.3 is right, as no
  contact is the expected outcome; the touches' own `o901..` result
  tests use a separate O-word range, so the two never collide.
- **Trip frame.** #5061-#5063 are "in the coordinate system in which the
  G38 took place", in program units - the section's own WCS, the frame and
  unit every coordinate this post writes is in, so the expected values
  (origin-relative, `fu()`-scaled) subtract directly. If the probe does
  not trip, the parameters hold the end point - moot, G38.2 has halted.
- **Tolerance alarm = `o<n> if [ABS[#19x] GT tol]` / `(MSG, ...)` / `M2` /
  `o<n> endif`,** one unique O number per test (o901..). Dropped:
  `(ABORT, ...)` - not in the stable LinuxCNC reference nor Tormach's
  guidelines (forum reports it erroring on some builds); `M0` - a pause
  the operator could resume onto a wrong offset. M2 (program end, listed
  by Tormach) is the closest to Fanuc's `#3000` alarm. The no-offset case
  keeps the resumable stop: `(MSG, CHECK PROBE RESULT #190..)` + `M0`
  (Fanuc's `#3006`).
- **Work offset = `G10 L20 P<n> X[#5420-#190] ...`, not `G10 L2` from
  `#5221 + 20 x slot`.** L20 makes "the current coordinates become the
  given values"; #5420-#5422 are the current position in the active WCS in
  program units, so the new slot's offset is the active one plus the
  deviation, all in one unit. L2 with #5221.. needs the stored offset
  parameters in program units, which the LinuxCNC reference does not
  state - a G21 program on an inch-configured PathPilot would mix units.
  Assumes no G92 offset and no XY rotation (R) on the active WCS (G52 is
  unsupported on PathPilot). P1..P6 = G54..G59 (FOUND's range).
- **Arming:** neither reference documents an M-code to arm the probe; the
  probe input is live and its type (passive / active) is a PathPilot
  Settings-tab choice. Nothing is written. Tool 99 is Tormach's convention
  for its built-in probing screens only; G38.x runs with any tool number,
  so the post does not force one.
- **Cost to change:** contained in the probing section of `tormach.cps`
  and its goldens. Swapping M2 for `(ABORT,...)` or L20 for L2 is a few
  lines plus a golden regeneration.
- **Owed (PathPilot Hub check, the contributor's):** whether PathPilot's
  preview stops at the first `M2` (preview probes report the end point, so
  every test may read "out of tolerance" there); that the back-off G1
  after a trip raises no "probe tripped during non-probe move"; that
  `o9xx if` blocks and `(MSG, ...)` display as expected.
