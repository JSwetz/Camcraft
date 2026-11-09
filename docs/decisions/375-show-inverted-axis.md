## #375 - show an inverted machine axis (viewport, setup sheet, NC header)

- **Chosen (viewport):** the origin triad keeps its right-handed arrows (the
  CAM / program frame, ISO 841) and gets a note label just under the
  inverted axis's letter, in that axis's colour: "Machine X+ moves left
  (inverted axis)". The note rides the first triad only (the setup's own
  offset before the world axes), so two gizmos never print it twice. It
  reads the machine the shown setup mounts (#343 / #390).
- **Chosen (setup sheet):** a `MACHINE AXES` section after the datum /
  alignment block, one line per inverted linear axis of the setup's own
  machine: "Machine X+ moves left (inverted axis) - the program is mirrored
  in X for this machine". Absent for a standard machine or no machine.
- **Chosen (NC header):** the machine's inverted letters reach the script as
  `machine.invertedAxes` (`[]` for a standard machine); fanuc.cps and
  haas.cps - the only posts declaring `axisInversion = true` - write one
  `(MACHINE X+ MOVES LEFT - INVERTED AXIS)` comment per axis right after
  the program-name line. Every other post refuses an inverted machine
  before its script runs (#374, `post::invert::apply`), so it never posts
  one and needs no line.
- **One wording table:** `machine::inverted_motion` (X left, Y toward the
  front, Z down, else "the other way") and `inverted_axis_line` feed the
  triad and the sheet; the two .cps scripts carry the same table in
  upper case (a script cannot call Rust).
- **Considered and dropped:** flipping the triad's arrow (draws a
  left-handed frame the toolpaths are not in - exactly the confusion #372
  warns about); replacing the letter with a longer text (loses the big
  axis letter #195 made readable); a sheet line for standard machines
  ("all axes standard" - would change every existing sheet).
- **Machine view:** the machine editor already shows the direction choice
  per axis (#373, "X+ moves right / left"); nothing more was needed there.
- **Standard machines:** no new text anywhere - sheets, triad labels and
  every golden program are byte-identical.
