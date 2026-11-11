## #585 (M7.3 2/6) - law (d)'s prior readers read through `Columns`

- **Chosen:** `verify::prior_surface` and `verify::acquit_by_prior` take `&Columns<'_>` instead of `&Dexel`. `Columns` gains `Columns::of_grid(&Dexel)` (a bare rest grid read through the seam) and three lattice accessors (`cell`, `center`, `origin`).
  - Its source is now an internal `Source::{Grid, Field}`, so a reader no longer needs a whole `Stock`.
  - The operation layer still holds the program-ahead rest grid as a `Dexel` (`drill_prior`, `generate`), so it wraps it with `Columns::of_grid`. When #586 moves that holder to `Stock`, the call becomes `stock.columns()?` and the law is untouched.
- **Why:** this is behaviour-neutral. `Columns` already maps a point to its column by the column engine's own formula, so a grid read through it answers exactly as before. The four existing `verify::prior` unit tests pass unchanged, wrapped. A new suite (`prior_columns`) proves that a column stock, a field stock and a bare grid read the same prior surface and acquit the same dives.
- **Scope corrected from the #26 research** (recorded on #585):
  - `solid_rest` names no `Dexel`, so it needs no port.
  - `carry::Body` uses `Dexel` as the **storage** of carried two-sided stock (top and floor per column), not as a reader. Moving that storage onto the field is a data-model change that belongs with #586 (app carried stock) and #589.
- **Dropped:** taking `&Stock` in the readers. It would force the app to clone its rest grid into a `Stock::Column` per call. `Columns::of_grid` borrows instead.
