## #24 - the hidden-rest scan answers past its budget (M13.1)

**Decided.** The 1.2 M-cell / 40 M-claim numbers now bound what the
hidden-rest scan HOLDS at once, not what it may judge. A lattice whose
claims do not fit is counted row by row, cut into row bands that fit both
numbers, and judged band by band through the same judging body
(`stock_sim/scan_hidden.rs`), so the banded verdict equals the
whole-lattice verdict bit for bit (same claims, pairing and distance law;
rows ascend, ties keep the first worst). A third budget, `work` (1 G
facet-column visits plus ball-center distances, ~10 s at worst), bounds the
whole walk; past it, or when one row's claims alone cannot be held, the
scan still answers `OverBudget` / `RestUnjudged` ("NOT JUDGED") and blocks
the post. The nearest-ball-center query goes through a uniform XY grid
(`stock_sim/scan_near.rs`), exact: rings outward, stopped by the ring's XY
bound together with the path's Z-range gap, with the previous query's
segment tried first and an early exit only when the top provably cannot
raise the worst rest.

**Why.** A false clean is a gouge-class bug, so the over-budget answer has
to be the real verdict, not an estimate. The old budget existed for memory
(40 M claims of 40 B is ~1.6 GB of bins); streaming removes the memory
reason without changing a single column's verdict. The work cap keeps the
Prime Directive's "no hangs": the cost of a verdict is now stated, and a
refusal past it is still by name.

**Readings.** A 21-plate stack over 100 x 100 mm with a 0.5 mm ball
(1.07 M columns, 44.0 M claims, past the shipped 40 M): before, NOT JUDGED;
now Judged, rest 41.206 mm at the deepest plate's top between two passes
(hypot(10, 40.5) - 0.5, as expected). Release: 2.2 s for the verdict
without the hidden half, 14.5 s with it (the heavy-lane test
`a_stack_past_the_shipped_budget_gets_a_real_verdict`).

**Considered and dropped.**
- *A coarser lattice over budget* (sample every k-th column): cheap, but
  a hidden ledge narrower than k cells is skipped and reads clean - not
  conservative, so not a verdict.
- *Asking the SDF field (`SdfField`) per column*: its sign is the same ray
  parity, but `shell_crossings` is linear in the facets per column -
  O(columns x facets), far worse than the binned scan, and building the
  field for a verdict costs its own band memory.
- *Keeping the refusal and raising the budget*: moves the cliff, does not
  remove it, and the memory grows with it.

**Cost to change.** The `ScanBudget` struct gained a `work` field (a
breaking change for struct literals, all in tests). Tightening `work`
turns more huge models back into NOT JUDGED, never into a wrong clean.
