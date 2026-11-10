## #392 - Haas probing on an inverted X/Y axis (Haas part; Fanuc merged in PR #465)

**Decided.** `haas.cps` declares `probeAxisInversion = true`. The engine's
existing mirror (`post::invert`, from #465) negates each touch's target and
nominal trip, each back-off's delta and end point, and each found axis's
expected value on the inverted axis. The Haas probing hooks from #173
(PR #522) post that mirrored stream unchanged. They hold no
inversion-specific code.

**Why this is enough.** The Haas hooks write the same words with the same
meaning as the Fanuc ones:
- G31 Feed Until Skip, with the trip read from `#5061-#5063` in work
  coordinates of the active offset. A G31 that never trips ends at its
  target.
- The work-offset registers `#5221 + 20 x slot + axis`.
- `G10 L2 P1..P6`, written in G90 after the back-off's `G90`.
- `#3000` and `#3006`, `IF[..]THEN` and `ABS[]`.

All of these are read and written in the controller's own frame. #465's
proof therefore carries over unchanged. On the inverted axis the register
holds `S·D`, a part off by `d` gives a deviation of `S·d`, and
`X[#5221+#190]` writes `S(D + d)`, the real datum in controller
coordinates. Back-offs are increments, so they flip with the axis.
`ABS[]` and the choice of travel axis do not depend on sign. The
protected positioning moves (#174, `onProbeGuard`) mirror like any
straight feed. Their target and `Start*` chain both arrive negated, so
the "axis holds still" test and the hit check read alike.

**Pinned by.** `inverted_probe` now runs every test for both posts
(`POSTS = fanuc, haas`):
- **Physical offset.** On each machine (standard, inverted X, inverted
  Y, inverted XY), these probes write G55 at the physical datum: face
  −Z, face −X, edge −Y, corners +X+Y and −X−Y, bore and boss.
- **Datum unchanged.** A part exactly where the model says writes the
  datum unchanged.
- **Alarms.** No contact alarms `#3000=2`, and out of tolerance alarms
  `#3000=1`.
- **Converse.** The untranslated program misses on an inverted machine,
  and undoing any one translated word breaks it.
- **Byte-identical.** `an_uninverted_program_is_unchanged_by_the_flag`
  shows that every case on a standard machine posts byte for byte as the
  script without the flag. The #173 Haas golden
  (`probe_bore_centre.nc`) is unchanged.

**Refusal kept.** No shipped post now has `axisInversion` without
`probeAxisInversion`, so the refusal is pinned on a shipped script with
its flag line removed. See `probing_through_an_unverified_script_still_refuses_by_name`
and `inverted_post::probing_refuses_by_name`. The message now reads
"(Fanuc and Haas do)".

**Considered and dropped.**
- **A Haas-only macro reader.** Every word the Haas hooks write is
  already modelled by the macro-B reader, including `G1` for `G01`. A
  second reader would duplicate it without adding coverage, and any word
  it does not model fails the test.
- **Haas mirror image (G101) or the controller's own axis-direction
  setting.** The machine's inversion is the machine file's
  `direction = -1`, which the post handles by mirroring the program. No
  Haas controller setting is relied on.

**Sources.** The Haas Mill Operator's Manual (public, haascnc.com),
already cited by #173's hooks: G31, the macro system variables
`#5061-#5063` and `#5221..`, `#3000` / `#3006`, and G10 L2. During this
change an automated fetch of the haascnc.com manual pages got HTTP 403,
so the pages were not re-read. The semantics used are exactly those #173
already relies on for a standard machine. The inversion adds no new
controller semantics, because every word is read in the controller's own
frame. If a reviewer finds that Haas `#5061` holds machine rather than
work coordinates, both the standard and the inverted Haas probing are
wrong together, and the fix belongs to #173's hooks.

**Cost to change later.** Remove the flag line and Haas probing on an
inverted axis is refused by name again.
