## #503 - Siemens 840D post: the safe Z retract comes from the machine

**What was decided.** `sinumerik.cps` takes its retract height from
`machine.safeRetractZ` (#495's key), in both places it lifts Z in
machine coordinates - the after-operation home move (`G0 G53 Z..` then
`G0 G53 X0 Y0`) and the program end (`G53 Z..`):

- Stated (any number, 0 included) -> that Z, in mm (this dialect writes
  mm; an inch machine file's value arrives scaled). A non-zero height
  also writes the header comment `; SAFE RETRACT G53 Z<v> - STATED BY
  THE MACHINE`, matching the Tormach post; Z0 stays silent.
- No machine at all (null, or a machine with no name) -> `G53 Z0`, today's
  output.
- Any named machine that states none -> one counted `message()` naming
  the machine and the `safe_retract` key; no retract is written, and the
  home move's XY half is dropped with it (moving XY without the Z lift
  first would be worse). The in-app gate blocks the write.

**Why no name exemption (unlike Tormach).** #495 kept `G53 G0 Z0` for a
model named "Tormach..." because PathPilot's convention (Z homes at the
top, machine Z0 there) is the post's own target. The Siemens family's
public documentation states no such rule: where the reference point and
the machine zero sit is the machine builder's choice, set in machine
data. "Z0 is the top" is common on Siemens VMCs but it is an assumption,
so the Prime Directive rule applies: no guess about which way is up. A
model named "Siemens ..." is refused like any other unstated machine.

**Output change.** None for anything shipped: every shipped mill file
(`vmc.mch`, `tilthead.mch`, all Tormach files) and the default
`3axis-vmc.mch` (the conformance goldens' machine) state 0, and the
conformance goldens are byte-exact. The change reaches only a machine
without the key: a user's own file, `millturn.mch` (states none; a
mill-turn through the Siemens mill post is now refused by name), and a
document saved before #495 with a mounted machine (its stored copy
lacks the key - re-mount it). Two coolant test fixtures built a bare
`Machine` and now state `safe_retract_z: Some(0.0)`.

**Considered and dropped.**
- Keep `G53 Z0` for every unstated machine (the documented-convention
  door the brief left open): no public Siemens document makes machine
  Z0 the top of travel, so it would stay the unproven assumption #495
  removed from the Tormach post.
- Exempt models named "Siemens"/"Sinumerik": the name says which
  control, not where the builder homed Z.
- `G75` fixed-point approach (the family's documented tool-change
  position move) instead of `G53 Z..`: a builder-set position, safe by
  definition, but it is a different output for every Siemens program
  and needs its own decision; noted here, not done.
- `SUPA` instead of `G53`: suppresses more offsets (DRF, external), but
  changes every golden; out of scope.
- Adding `safe_retract` to `millturn.mch`: where its Z homes is not known;
  left unstated rather than guessed.

**Cost to change later.** One function (`safeZ()` in `sinumerik.cps`);
adding a name exemption or a G75 form is a branch there.
