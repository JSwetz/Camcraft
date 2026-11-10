## #641 - grooving and parting blades post their touched-off corner

**Verified first.** The blade kernels (`crates/motion/src/turning/blade.rs`,
`parting.rs`) emit the middle of the blade's tip edge: every plunge stands
at a band centre `s` and the floor is read over `[s - w/2, s + w/2]`; the
part-off stands at the parting plane itself. The Fanuc Lathe golden posted
those centres raw (`groove_part.nc`: a 3.2 groove between Z-3.6 and Z-6.8
with a 2 mm blade posted Z-4.6/Z-5.8; the 3 mm part-off at plane Z-10
posted Z-10). A blade touched off on a corner therefore cut width/2 off in
Z - a real bug, not a not-a-bug.

**Decided.**
- The blade's reference corner reuses #630's `orientation` field (no new
  field, no new file key, same editor row, same `ChangeTool` undo), read
  through the LinuxCNC chart: a blade plunges toward the axis, so its tip
  is the chart's -X side and only the Z sign is left - **1** = the +Z
  (right, free-end) corner, **2** = the -Z (left, chuck-side) corner,
  **6** = the width centre (the old posting), **9** read as 6.
- **Default 2, the left corner**, for every blade that states none (old
  projects and libraries included): LinuxCNC Lathe User Information 4.2
  works its parting-tool touch-off example from the left side.
- The conversion is the shared `post::nose_tip::control_point_path`, shift
  `(0, side * w/2)`; the CL radius is already the tip line, so X does not
  move. A rigid shift, like #630's - the clearance rapids move too.
- **No corner-radius term.** The imaginary tip of a radiused corner is the
  intersection of its tip line and its flank line - the sharp rectangle's
  corner, w/2 from the edge's middle - so `corner_r` changes nothing.
- Codes 3, 4, 5, 7, 8 state an ID or a face blade, which no OD groove or
  part-off rides; posting one is refused by name, listing `[1, 2, 6]`.
- The cache, verifier, stock sim and gouge laws keep the band centre
  (unchanged); the parting plane still names the kerf's centre.
- The tool note writes `ORIENTATION q` for a blade, as for an insert.

**Considered and dropped.** A dedicated `ref_corner` enum field: a second
key for the same "which point was touched off" fact, a second editor row
and a second round-trip law, for no added expressiveness. Defaulting to the
right corner (the face-touch-off habit): the cited source works from the
left. Re-reading `parting_z` as the part's back face: that changes the
generator and the sim's truth, out of scope for a post fix.

**Cost to change later.** The default is one match arm
(`orientation::default_for`); a stated corner in a saved project is kept.
