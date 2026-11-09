## #377 - Fanuc probe result folds the expected value's sign into the operator

**Decided:** `assets/posts/fanuc.cps` writes `expr - v` through one helper,
`minusU(expr, v)`: a negative `v` becomes `expr+|v|`, a positive one
`expr-v`, and a value that formats to `0` (including `-0` and anything
under half a micron) adds no term at all (`#190=#100`). It is used by both
places that subtracted a coordinate: the deviation in `onProbeResult`
(`#190=#100--17` -> `#190=#100+17`) and the no-contact check in
`onProbeMove` (`IF[ABS[#5061--12]...` -> `IF[ABS[#5061+12]...`), which had
the same doubled sign for any G31 target below the work zero. The
expression stays **unbracketed**.

**Why:** Fanuc's published custom macro B documentation gives the
arithmetic operations in the unbracketed form `#i=#j+#k` / `#i=#j-#k`;
brackets are needed only for nesting and function arguments, both of
which the post already writes (`[[#100+#103]/2]`, `ABS[...]`). Unbracketed
keeps the committed `probe_bore_centre.nc` golden byte-identical (all its
expected values are positive), so the fix changes only the lines that were
wrong. A doubled sign is not documented as valid in any public macro B
reference, so it is removed rather than relied on. Writing nothing for a
zero term avoids both `-0` and `+0`, which are legal but noise.

**Considered and dropped:** bracketing every assignment
(`#190=[#100+17]`, as the Tormach post must for LinuxCNC, PR #376) - valid
macro B too, but it rewrites every existing Fanuc probe golden for no
control-side gain; always writing a `-0`/`+0` term for zero - harmless,
dropped for the cleaner bare variable.

**Cost to change later:** one helper; switching to bracketed output means
wrapping its return value and regenerating the three Fanuc probe goldens.
Not exercised on a real Fanuc control or simulator.
