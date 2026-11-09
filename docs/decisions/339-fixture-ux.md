## #339 - fixture UX: mark, import, see (slice 4)

**Decided:**

- **Mark/unmark from the browser reuses the Fixtures page's commands.**
  A right-click on any model row (the Models section or a setup's
  Fixtures child) opens a small menu: "Use as fixture (<active setup>)"
  queues `TagFixture` bound to the active setup, "Stop using as fixture"
  queues `UntagFixture`. A right-click on a setup's Fixtures node
  activates that setup (the browser's existing focus law) and offers
  "Import fixture…", which queues the page's own `ImportFixtureDialog`.
  No new mark/unmark command: the page's are already one undo unit each.
- **Import-as-fixture is now ONE undo unit** (`ImportFixture`, new): the
  worker's finished `ImportModel` (or an `UpdateModel` when the name is
  taken) and the `FixtureDef` binding apply and invert together. It was
  two (import, then tag), which left "imported but no longer a fixture"
  one Ctrl+Z in. The ui suite's `import_as_fixture_lands_as_two_undo_units`
  is restated as `..._one_undo_unit` (same test count). A refused import
  now also clears the pending fixture binding, so it can never tag the
  next, unrelated import.
- **Mesh state is the mesh law's own verdict.** `fixture_view::mesh_states`
  runs `FixtureMesh::build` (#476's validation) on each fixture's world
  facets: `mesh`, or `box only: <reason>` from the `MeshError` name (open
  mesh, non-finite, collapsed, over the facet cap, no facets) or `shared
  box` (another fixture of the setup has the identical world box - the
  mesh law then keeps the box for both). A mesh refusal outranks a shared
  box: it is the one the user can fix. The Fixtures child row shows the
  short form as its suffix; the Fixtures page prints the full sentence
  ("Collision: judged by its bounding box: the fixture mesh is not closed
  ... Repair Model may close it") as a plain label, visible without logs.
- **Cached per document version.** Building validates and indexes the
  mesh, so the browser reads it through a small `VersionCache` (a
  `Mutex<Option<(key, Arc<T>)>>`, poison taken back - it is a cache,
  never truth) held on the UI `State`, keyed by `doc_version`.
- **The struck fixture is resolved, not guessed.** `FixtureHit` names op,
  move and band but not the steel (the laws judge all boxes at once). For
  the selected op, `fixture_struck::struck_by` re-runs the post gate's
  exact per-op law (`op_hits`, now shared: `posting::fixture_problems`
  calls it too, so the two cannot drift) once per fixture, each ALONE,
  and keeps the fixtures that reproduce a warned move. Warned moves no
  single fixture reproduces (the steel moved since generation - a
  workholding change never stales a path - or two boxes only convict
  together) are listed in the op banner as "Fixture not resolved for
  move N ... regenerate to re-check"; nothing is tinted for them. Cached
  per (doc version, op, path signature).
- **The tint is display only.** A struck fixture's model part carries
  vertex class `STRUCK_CLASS` (4.0; stock classes are 0..=3 and only the
  stock shader reads them), and `fs_main` mixes it toward the palette's
  gouge colour - colour-vision aware, no new uniform, no new pipeline.
  The scene digest already covers classes, so the tint re-uploads only
  when it changes. The derived machine table is not an entity: it is
  named in the banner but never drawn.

**Considered and dropped:**

- *A new `MarkFixture`/`UnmarkFixture` pair:* duplicates `TagFixture` /
  `UntagFixture` for no gain; dropped (the task's own "reuse" rule).
- *Naming the fixture inside `Warning::FixtureHit`:* changes a sim type
  every law, golden and cache carries, and the cached warning would still
  go stale when the steel moves. Resolving at view time against the live
  steel is honest about that, and cheap enough per selected op.
- *Guessing from box overlap of the move's endpoints:* wrong for swept
  bands and ramps; a guess is exactly what the brief forbids.
- *Recomputing mesh state per frame:* `FixtureMesh::build` on a 200k-facet
  vise is ~50 ms - per version, not per frame.

**Limits stated:** "post gate names a fixture" rides the same path - the
gate's problem strings name op, move and band, and the banner resolves
them the same way. Two fixtures that only convict a move together (a
shared box) show as unresolved for that move.

**Cost to change later:** the view helpers are two small app modules
(`fixture_view`, `fixture_struck`); a future `FixtureHit { fixture }`
field would make `struck_by` a lookup and leave the UI untouched.
