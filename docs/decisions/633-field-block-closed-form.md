## #633 - a field block stock builds in closed form

**Profile** (release, `drill_plate.cl`, 120 mm box at the CLI's 0.375 mm auto cell, 320 × 320):

| Phase | Column | Field, before | Field, after |
|---|---|---|---|
| Build the block | 27 µs | **58 s** | **42 µs** |
| Cut (9 moves) | 23 µs | 3.4 ms | 2.6 ms |
| Read all columns | 0.2 ms | 66 ms | 53 ms |
| Cut cells | 399 | 399 | 399 |

**Cause:** `block_field` meshed the floorless block as a box **ten spans deep** (1.2 m for a 120 mm block), and `SdfField::from_mesh` breadth-first-filled every band cell along those walls: tens of millions of hash-map cells, almost all far below any cut. The cut cost (the M7.3 per-brick law, #584) was never the problem.

**Chosen:** a block's mesh-layer magnitude is the box's **exact closed-form distance** (`field_box::AnalyticBox`). `SdfField::from_box` stores no bricks, and `mag_at` answers unstored cells from the box, clamped to `cap` by the band law it already used.
- **Unchanged:** the shell is still built (a handful of facets) and still answers parity and bounds. The cut layers (`cuts`, `cut_ups`) and every reader are untouched, since all of them go through `mag_at`.
- **Why it's safe:** the closed form is the truth that the band fill's graph distance bounded. `field_block_cost::the_closed_form_reads_the_stock_the_band_fill_read` checks the same sign everywhere and agreement within two cells, and the cut-cell counts above are identical.

**Dropped:**
- **A shallower block (fewer spans deep):** the cost is linear in depth, so it would still take seconds, and it would change the floorless semantics.
- **A coarser field auto cell:** it changes verdict resolution.
- **Lazy band fill on first cut:** more machinery, for the same result the closed form gives for free.

**Not covered:** cylinder and model stocks still band-fill from their meshes. They're bounded by their real size, not by a ten-span floor, so they don't have this pathology. A closed-form cylinder can follow if profiling shows a need.

**Budget test:** `field_block_cost::a_large_field_block_runs_a_drill_program_quickly` (heavy lane, release): a 120 mm field block plus four plunges must finish under 2 s. It was 58 s.
