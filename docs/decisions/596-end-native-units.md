## #596 - Tormach post: optionally end the program in the control's units

**Decided:** a document post option, "End in" - *Program's unit* (the
default), *Inch (G20)* or *mm (G21)* - held in the `PostOptions`
singleton as `end_units` (`EndUnits::Leave / Inch / Millimeters`), set by
the existing undoable `SetPostOptions`, saved as
`post_options.end_units: "inch" | "mm"` (additive key, no version step;
absent = Leave; an unknown word refuses the file by name). The script sees
it as `postOptions.EndUnits` (`"inch"` / `"mm"`, absent when left).
`tormach.cps` writes `G20` (or `G21`) on its own line after the final
`G53 G0 Z0` / `G30` and right before `M30`, only when the chosen unit
differs from the program's (the `.mch` unit). The Post page shows the row
only while Tormach PathPilot is the document's post, the way the Fanuc
Lathe threading row is shown.

**Why:** the LinuxCNC G-code reference (linuxcnc.org/docs, M-codes,
"M2, M30 Program End") lists what program end resets: offsets to the
default (G54), plane G17, G90, G94, overrides on (M48), G40, M5, motion
mode G1, M9. Length units are not on the list, so a G21 program leaves the
control in G21 after M30. The program's unit already comes from the
machine file (`machine.units` -> G20/G21 header), so on a mm-converted
copy of the 770MX file the machine file *is* the program unit. The
operator's preferred unit at the control (PathPilot's own G20/G21 setting)
is a fact about the shop, not something the `.mch` states. So "native"
cannot be read from the machine file the post already reads; it has to be
stated. A three-way choice is the simplest design that is right in every
case. It needs no machine-file schema change, and both directions (mm
program on an inch control, inch program on a mm control) work the same
way.

**Placement:** after the last move, so no position is read in the new
unit. The G30 return goes to a stored machine position, which units do not
affect. M30 then ends the program with the control in the chosen unit.

**Considered and dropped:**
- A machine-file key (`native_unit` / `control_units`). It is truer to
  "the machine is truth", but it means a parser and mirror change plus an
  edit to every Tormach file, and the stock files would state a guess
  about each shop's PathPilot setting. If wanted later, it could seed the
  option's default.
- A closing comment naming the unit the control was left in. It informs
  but does not fix: the DRO and MDI still need a typed G20.
- An on/off "end in native units" switch. With no separate native unit to
  compare against, it would be a no-op.
- Offering it on every post. Only the Tormach post reads it today. Fanuc,
  Haas and others reset or keep units by their own parameters, so the
  other posts ignore the key, as they ignore `Threading`.

**Cost to change:** a machine-file default would be one parse field and a
fallback in `endUnitsWord()`. Other posts can read `postOptions.EndUnits`
with no app change, apart from showing the row for them.
