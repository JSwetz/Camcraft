## #170 - Machine files for the other Tormach mills (2026-11-08)

- **Chosen:** one inch `.mch` per mill whose Tormach operator's manual was
  in hand, figures from its § "Machine Specifications" and § "Performance
  Expectations": 770MX (ML) (UM10864), 770M (UM10539), 1100MX (UM10586),
  1500MX (UM10811), PCNC 440 (UM10372). The existing "Tormach 770MX" stays
  the MF-serial machine (UM10587). The ML-serial 770MX is a different
  spindle (5M toothed belt, one range 250-10000 rpm, 1.5 kW, 450/420 IPM),
  so it ships as its own entry, "Tormach 770MX (ML)", not as an edit of
  the MF file.
- **Not shipped:** the non-MX 1100 and the plain 770. No manual for
  either was in hand, and a machine file built from memory would be the
  silent wrong answer. Either can be added the same way once its manual
  is available.
- **Belt ranges in the `.mch`:** `spindle.ranges: [{min_speed,
  max_speed}, ...]`, which reaches the post as `machine.spindleRanges`.
  The Tormach post's belt note now reads the machine's ranges in place of
  the 770 family's hard-coded 175-3250 / 525-10000. A machine with fewer
  than two ranges (1500MX, 770MX ML, any non-Tormach file) gets no note.
  Consequence: `synth_tormach.nc`, posted against the generic metric VMC,
  loses its 770-family belt comment, which was wrong for that machine
  anyway. Two ranges keep the LOW / HIGH wording the 770MX's goldens and
  the user's Hub runs already use. Three or more are named by their rpm
  span.
- **Rigid tapping:** `spindle.rigid_tapping: false` on the R8 mills (770M:
  "Tension/Compression, Auto-Reversing, Thread Mill"; PCNC 440: "Thread
  Mill"), which reaches the post as `machine.rigidTapping`. The Tormach
  post refuses a tap cycle there by name (one counted warning per
  program, which blocks the post), never writing a G84 the spindle cannot
  follow. Unstated reads `true` (the unstated-limit law), so every other
  machine and post is unchanged. Considered and dropped: posting a
  floating-holder G74/G84 variant for tension/compression holders. That
  needs its own feed and dwell rules from PathPilot's documentation, so it
  is left as follow-up work if someone asks for it.
- **ATC capacity:** stated only on the 770MX (10, the user's own
  changer). The manuals treat the ATC as an accessory and state no pocket
  count, so the other files leave `tooling.number_of_tools` unstated and
  the capacity check stays off for them.
- **Travel** is the full stroke both ways in program coordinates (±X, ±Y,
  ±Z), as on the 770MX (#131's finding: the travel band is relative to
  the work zero, which can sit anywhere on the table).
- **Coolant:** every manual documents M07 / M08 / M09, so every file has
  Flood M8 and Mist M7.
