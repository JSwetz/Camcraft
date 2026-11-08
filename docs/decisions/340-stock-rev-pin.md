## #340 - a loaded stock keeps its geometry revision

- **Chosen: the loader's Stock arm pins `rev` to the file's value right
  after its `set_facets`, unconditionally, exactly like the Model arm**
  (`project/apply.rs`). `SetStock` lands facets and bumps the counter on
  every call, the save writes it (`"rev": 2` after two sets), and the load
  re-lands the regenerated mesh once, so without the pin it read back 1
  and save -> load -> save was not byte-identical.
- **Older files:** a stock with no `rev` key (pre-v20, or never counted)
  reads `dto.rev = 0` and is pinned to 0, the same default as the Model
  arm - the re-save then omits the key again (skip-if-zero), so the file
  stays a fixed point. No project-version bump: the wire grammar is
  unchanged, only the loader stops drifting.
- **Why:** byte-identical round-trips are the M1.1 law; staleness never
  reads `rev` (the stamp's signature does), so this is file churn, not a
  wrong path - but churn makes every open-and-save look like an edit.
- **Dropped:** skipping the save of a stock's `rev` (it would hide the
  counter instead of keeping it, and diverge from models); pinning only
  when the key is present (the Model arm's comment explains why the pin is
  unconditional - the counter must end where the file left it).
- **Cost to change:** one line in the Stock arm; pinned by
  `stock_set_twice_keeps_its_rev_across_save_load_save`
  (`crates/app/tests/project_roundtrip.rs`).
