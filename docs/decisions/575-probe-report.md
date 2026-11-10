## #575 - probe programs report the measured deviation on the control

**Decided:** before a probe program writes the work offset, it shows each probed axis's measured deviation (found minus expected, the `#190..` value the offset write uses) through the dialect's own operator-message channel, and only when that channel does not stop the program. A dialect without such a channel writes nothing. A stop on every good probe is not a report.

| Dialect | Report | Source |
|---|---|---|
| Tormach PathPilot / LinuxCNC | `(DEBUG, PROBE <axis> ERROR #19n <unit>)`, one line per axis, the unit word `MM` in a G21 program and `IN` in a G20 one (the same `inch` flag that writes G20/G21), placed after that axis's `o<n> if` tolerance test and before `G10 L20`. In a measure-only op it goes before the `(MSG, CHECK PROBE RESULT ..)` / `M0` stop. | LinuxCNC G-code reference, "Messages" / "Comments": DEBUG is shown like MSG (in the status display, no stop), and each `#n` / `#<name>` in it is replaced by the parameter's value. |
| Fanuc, Haas, Mazak (custom macro B, `fanuc.cps`) | none | The published custom macro B documentation offers only `#3000` (an alarm, which stops) and `#3006` (stop with a message). DPRNT/POPEN write to an external I/O device, not the screen. A comment in `fanuc.cps` next to `onProbeResult` records this. |
| Sinumerik | none today, because `sinumerik.cps` has no probe hooks. When probing lands: `MSG("PROBE X ERROR "<<R190)`, cleared after the offset write with `MSG()`. | Siemens SINUMERIK programming manual (Fundamentals / Job planning): MSG shows a message without stopping, and `<<` joins a variable to the text. |
| Heidenhain (ISO) | none today, because `heidenhain.cps` has no probe hooks. When probing lands: still none, unless the TNC user manual confirms that FN16 F-PRINT to `SCREEN:` shows its pop-up without halting the program. | TNC user manual (FN16 F-PRINT). Not confirmed non-stopping, so not adopted. |

**Why:** the issue came from #521's run on a real 770MX. A successful probe changed G54 Z by 0.0007 in, and the offset table was the only trace of it. The operator should see what was measured before it is written. The report goes after the tolerance test, so the printed value is always one that passed the test and will be written. An out-of-tolerance value still stops with its MSG + M2.

**Unit word (follow-up from the real-machine run):** the deviation is in the program's unit, which the line did not say; on an inch 770MX `0.0007` read like a mm value. The user asked for the unit in the line, so it ends `MM` (G21) or `IN` (G20), taken from the post's own G20/G21 choice. Considered and dropped: `INCH` / `MILLIMETRE` spelled out (longer status line, no clearer), and printing the value converted to both units (LinuxCNC DEBUG substitutes a parameter, it does not evaluate an expression, so a second value would need an extra `#` assignment for no gain).

**Considered and dropped:**
- Fanuc `#3006` on every probe: it stops the program each time, which is the opposite of a report.
- A plain `( ... )` comment on Fanuc: the operator can't see it at run time.
- A report before the tolerance test, so a failing value would be shown too: the issue places it after the test. Moving it would change the order of a single line in the post only.
- Doing the report in `crates/motion` as a neutral record: each dialect owns its own message words, so it lives in the post scripts.

**Cost to change later:** one line per dialect in its `onProbeResult`, plus the Tormach probe goldens. Pinned by `crates/app/tests/probe_report.rs`.
