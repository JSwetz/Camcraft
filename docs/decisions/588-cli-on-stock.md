## #588 (M7.3 5/6) - the `camcraft-sim` CLI simulates through `Stock` (field)

- **Chosen:**
  - **Build and simulate:** the CLI builds its box with `Stock::block(Backend::Field { band: FIELD_BAND }, …)` and simulates through `Stock::simulate`.
  - **Read back:** it reads the result column by column through `Stock::columns()` (`Columns::cell`/`nx`/`ny`/`top_at`) for the removed volume, cut cells and lowest cut.
  - **Gone:** no `Dexel` remains in the binary.
  - **No-verdict columns:** a field column with no verdict (NaN) counts as uncut, the same abstention every `Columns` consumer makes.
- **Why:** M7.3 retires the column engine, and the CLI was one of its last direct users outside the app.
- **Evidence:** the CLI output was recorded on 5 golden programs × 2 tools (10 runs: contour_disc_hole, face_square_40x40, pocket_zigzag_plate_with_island, slot_fullwidth_centerline, drill_plate; 6 and 10 mm; box -60..60 × -60..60 × -25..0) before and after. Every line is **identical**, including removed volume, cut cells, lowest cut, rapids, the verdict and the exit code. `headless_bin` passes 3/3.
- **Depends on #585 / PR #595** for `Columns::cell()`, so it is stacked on it.
- **Dropped:** a `--column` flag to keep the old engine selectable. It would keep `Dexel` alive past #589's retirement for no user need.
