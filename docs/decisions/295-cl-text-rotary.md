## #295 - the CL text dump, `is_prismatic` and `TiltedFrame` keep the stated spindle C

- **Chosen:**
  - `cl::text::to_text` writes a move's stated C (`Move::rotary`) as the
    golden writer's ` SC<millidegrees>` word, after any pose letters, and
    `from_text` reads it back (`Move::with_rotary`). A path with no C
    dumps exactly as before.
  - `Toolpath::is_prismatic` is false when any move states a C: a C-live
    cut turns the part, so it is not a 3-axis program. The dump's version
    (and the golden writer's, which reads the same method) is then v4.
  - `TiltedFrame::world_path` copies `rotary` onto every move it maps,
    chords included, instead of `None`.
- **Why:** The headless `camcraft-cl` / `camcraft-motion` binaries (#279)
  read and write this dump. A C-live path written through it lost its C
  word, and read back as a 3-axis path cutting somewhere else.
- **Considered and dropped:**
  - Refusing C-live moves in `world_path` by name: that function can't
    fail, and no caller can reach it with one today. The rotary kinds take
    no tilt (`OpParams::tilt` is `None` for them), so a frame never wraps a
    C-stamped path. Keeping the angle is the lossless answer. A tilted
    C-live setup, if one is ever added, has to decide what C means under
    the tilt at its own door.
  - A new word name: `SC` is what `crates/app/tests/common`'s golden
    writer already spells, and it can't be read as the arc sense `S`
    because the reader matches the two-letter prefix first.
- **Cost to change later:** Small. One word in the dump, one condition in
  `is_prismatic`, six struct fields in `world_path`. No golden holds an
  `SC` word, so none moved.
