## #173 - probing hooks for the Haas, Mazak, Siemens and Heidenhain posts (2026-11-08)

- **Chosen:** Haas, Mazak and Siemens 840D voice `onProbeMove` /
  `onProbeRetreat` / `onProbeResult`; Heidenhain keeps the engine's
  named refusal (no hook defined, a comment in `heidenhain.cps` says
  why). Each voiced post keeps the generic dialect's contract
  (`fanuc.cps`, #108): a skip/measuring move to the overtravel target,
  an immediate no-contact alarm (a probe that never touched must not
  back off and measure air), the measured trip copied to variables, an
  incremental (G91) back-off, the mean of opposite trips minus the
  expected value as the deviation, an alarm past the tolerance, then
  either the work offset written (active offset + deviation) or a stop
  for the operator. Goldens: `nc/{haas,mazak,sinumerik}/probe_bore_centre.nc`
  (the bore centre of `nc/fanuc/probe_bore_centre.nc`), pinned by
  `crates/app/tests/probe_dialects.rs`.
- **Sources, per dialect** (public documentation only; no Fusion 360 /
  HSMWorks post or output consulted):
  - **Haas** - the Haas Mill Operator's Manual (haascnc.com): G31 feed
    until skip; macro system variables #5061-#5063 (skip signal
    position), #3000 (programmable alarm), #3006 (programmable stop
    with message), #5221.. (G54..G59 work offsets, 20 per slot); G10 L2
    P1..P6; IF[..]THEN. Same words as the generic dialect, one-digit
    G1 and unpadded T/H as the Haas post already writes.
  - **Mazak** - Mazak's published EIA/ISO programming manual (the
    Fanuc-compatible G-code mode): G31 skip, #5061-#5063, #3000,
    #5221.., G10 L2. Two deliberate differences from Haas: the branch
    is `IF[..]GOTOn` with a unique `N9001..` label per test (IF..THEN is
    not documented by every Mazak EIA generation; GOTO is), and the
    no-offset stop is a comment + `M00`, not `#3006` (not clearly
    documented for Mazak). This is the weakest-sourced of the three:
    the variable numbers follow the Fanuc-compatible table the EIA mode
    states, and a maintainer with a Mazak manual at hand should confirm.
  - **Siemens 840D** - SINUMERIK 840D sl Programming Manual
    Fundamentals / Job Planning (public): `MEAS=1` on a `G1` block
    (probe 1, rising edge, distance-to-go deleted), `$AC_MEA[1]` (probe
    responded), `$AA_MW[axis]` (trip in workpiece coordinates),
    `IF..ENDIF`, `MSG`, `SETAL` (user cycle alarms 65000..), settable
    frames `$P_UIFR[n,axis,TR|FI]` taking effect when the G5x is
    programmed again (the post re-selects the active G5x after writing).
    Reading a main-run variable stops preprocessing implicitly, so no
    explicit `STOPRE` is written.
  - **Heidenhain** - refused. This post is specced from the TNC 640
    ISO User's Manual (ID 892909-25), which documents no free skip move
    and no trip-position variable; TNC probing is the touch-probe
    cycles (call form and result Q parameters in the separate
    touch-probe cycle manual). No cycle number or Q parameter is guessed.
- **Judgement calls:**
  - Sinumerik keeps trips in `R50..R61` and deviations in `R90..`,
    inside R0..R99 (more R parameters are a machine-data setting), and
    refuses by name past 13 measurements (none of today's modes exceeds 4).
  - Sinumerik refuses setting G58/G59 (axial programmable offsets on
    an 840D, not settable frames) and any active offset outside
    G54..G57 / G505..G599, by name. When the target frame differs from
    the active one, the fine part (`FI`) is copied too so the target
    equals active + deviation.
  - Haas, Mazak and Sinumerik `startPreamble` now skip the spindle-on
    block at 0 rpm (as `fanuc.cps` already did, #108): a touch probe
    starts no spindle. No existing golden carried an `S0`.
  - Arming the probe (its M-code / output on the machine) is not
    written by any post: no stream record states it and the code is
    machine-builder specific. An unarmed probe never trips; every touch
    then alarms at once at its target, after the overtravel - the same
    behaviour as the generic dialect.
- **Dropped:** voicing Heidenhain via `G55`/cycle 0 from memory
  (unsourced); IF..THEN on Mazak (generation-dependent); `R100+` on
  Siemens (not guaranteed to exist).
- **Cost to change:** each dialect's hooks are one section of its
  `.cps` and one golden; a Heidenhain voicing needs the touch-probe
  cycle manual and a fourth golden.

## #173 follow-up: protected positioning moves (#174's onProbeGuard) on Haas, Mazak, Sinumerik

- **Chosen:** each dialect voices #174's GUARD record, modelled on
  `fanuc.cps`:
  - Haas: `G31` to the target, then one `IF[ABS[#506x-target]GT0.001]
    THEN#3000=3(PROBE HIT ON POSITIONING)` per moving axis (a skip that
    did not reach the target was a trip). Haas Mill Operator's Manual
    (G31, #5061-#5063, #3000).
  - Mazak: the same test in the `IF[..]GOTOn` / `#3000=3` / `Nn` form
    the no-contact test already uses. Mazak EIA/ISO programming manual.
  - Sinumerik 840D sl: `MEAS=1 G1` to the target, then `IF
    $AC_MEA[1]==1` -> `MSG`, `SETAL(65002)`, `M0`. Job Planning manual
    (measurement, MEAS/$AC_MEA; SETAL user alarms).
  - Each alarm is distinct from no-contact (Fanuc-family 3 vs 2;
    Siemens 65002 vs 65000) and from out-of-tolerance (1 / 65001).
  - Haas/Mazak gain a `minusU` helper (as in `fanuc.cps`) so a
    negative target never writes `#5061--7`; the no-contact test now
    uses it too (no golden changes: the bore's targets are positive).
- **Considered and dropped:** a Sinumerik `$AA_MW[axis]` vs target
  test - the measured value is only written on a trip, so on a clean
  move it is stale; `$AC_MEA[1]` (probe responded) is the direct,
  documented status.
