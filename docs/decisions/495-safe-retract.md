## #495 - Tormach post: the safe Z retract comes from the machine

**What was decided.** `G53 G0 Z0` is "straight up" only where machine Z0
is the top of Z travel (Tormach's PathPilot convention). A LinuxCNC
machine homed with Z0 at the bottom rapids down into the part on that
line (reported on #435). So:

- The `.mch` file states the height: an optional `safe_retract` on the
  linear Z part, in MACHINE coordinates and the file's length unit
  (scaled like travel). Zero is a statement (unlike the rate limits,
  where zero reads as unstated). A non-number, or the key on another
  axis, refuses the file naming the axis. The mirror carries it as
  `safeRetractZ` (mm); the script sees `machine.safeRetractZ` (or
  `null`) plus `machine.model` (the model name through either door, so
  the CLI's raw file judges the same as the app's mirror).
- Every shipped Tormach file states `0.0`; so do the generic VMCs
  (`vmc.mch`, `tilthead.mch`, and the default `3axis-vmc.mch` as
  top-level `"safeRetractZ": 0.0`) - their own Z travel already tops
  out at 0, and the default VMC is what a document with no machine
  posts against.
- `tormach.cps`: stated -> `G53 G0 Z<that>`; unstated and no machine
  (empty name) or a model whose name starts with "Tormach" -> 0 (today);
  unstated on any other machine -> one counted `message()` naming the
  machine and the `safe_retract` key, and no retract line is guessed
  (the in-app gate blocks the write).
- `G30` follows the PathPilot convention: written after the retract only
  when the safe Z is 0. A machine stating another height is not on that
  convention, and an unset G30 position (LinuxCNC's default is the
  machine origin) would drive Z back down - the reporter removed those
  lines too. There the retract stands alone and M6 / the control's own
  tool-change position does the rest.
- A header comment `(SAFE RETRACT G53 Z<v> - STATED BY THE MACHINE)` is
  written only when the safe Z is non-zero. Writing it always would
  have changed every Tormach golden for no safety gain, so the Z0 case
  stays silent and byte-identical.

**Why.** Prime Directive: the post must not guess which way is up. The
machine file is already the place the posts read machine truth from;
a Tormach (the post's own target) keeps its output byte-for-byte.

**"Is a Tormach" signal.** The only clean signal is the model name
(`general.model` / mirror `name`); `vendor` is "Camcraft" on every
shipped file by the naming law, so it says nothing. Since every shipped
Tormach file now states the key, the name rule only covers a user's own
Tormach file written before #495. A case-blind leading "Tormach" is the
least surprising reading of that.

**Considered and dropped.**
- Infer the direction from the Z travel band (max 0 = top). The travel
  is stated in PROGRAM coordinates (machines AGENTS.md), so it says
  nothing about where machine Z0 is; the Tormach files' symmetric bands
  prove it.
- A post property (user-settable "safe Z") instead of a machine key:
  it is a fact of the machine's homing, not of a program, and a
  per-post setting would not reach the other posts that need it later.
- Refuse every unstated machine (Tormach or not): would refuse old
  documents carrying a mounted Tormach machine saved before #495.
- Keep G30 after a non-zero safe Z: keeps the reported hazard on a
  machine whose G30 position was never set.
- `G28` / `G91 G28 Z0` as the Tormach retract: PathPilot programs use
  the G53 form, and G28 has the same stored-position problem as G30.

**Behaviour change for existing documents.** A document that mounted
the library "VMC 3-axis" (or another non-Tormach file) before #495 has
the parsed machine stored without the key, so posting it through the
Tormach post is now refused by name until the machine is re-mounted or
the key added. Tormach machines, no-machine documents and the default
VMC post exactly as before.

**Other posts, same assumption (reported, nothing filed).**
- `sinumerik.cps`: `G0 G53 Z0` before a tool change and `G53 Z0` at the
  end - the same machine-Z0-is-top assumption (usual on Siemens VMCs,
  but an assumption).
- `fanuc.cps`, `haas.cps`, `mazak.cps`: `G91 G28 Z0` - a return to the
  builder-set reference point, not to machine Z0; safe by that control
  family's definition of the reference point, not the same hazard.
- `fanuc-lathe.cps`: `G28 U0` / `G28 W0` - reference return on a lathe;
  not affected.
- `heidenhain.cps`: writes no home move (G53 is a datum shift there).

**Cost to change later.** The key is additive and optional; a post
reading it is opt-in. Moving the "Tormach by name" rule to a stated key
only means dropping one branch in `safeZ()`.
