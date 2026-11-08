## #322 - a new document reaches the Setups page (2026-11-08)

- **Chosen:** the browser's **Setups** row is drawn whenever the Setup
  section is open, not only once a setup exists; the ribbon's Setup group
  opens on a new **Setups** cell (`[Setups, Stock, Origin, Tools,
  Machine]`) that targets the Setups page. Both wear the existing
  workholding glyph (`Icon::Fixture`, `fixture.png`) - no new asset, no
  `LICENSES.md` change. "Create first setup" already queued the undoable
  `CreateSetup` command; it is unchanged and now reachable.
- **Why:** the only controls that create a setup live on the Setups page,
  and the only door to it was hidden until a setup existed, so a fresh
  document could never make its first one. The page already handles the
  no-setup case. A setup is a clamping, so the workholding glyph says what
  the door manages; the browser row moves from the work-offset glyph to
  the same one, so the ribbon cell and the tree row name one set of
  pixels and the Setups cell never twins Origin's glyph beside it.
- **Considered and dropped:** a keyboard shortcut for the page (no key
  is free that reads as "setups", and the ribbon cell plus the tree row
  already reach it); a new bespoke "stacked setups" glyph (an original
  drawing for a door the existing workholding glyph already explains);
  keeping the work-offset glyph on the browser row (it would then differ
  from the ribbon cell's).
- **Cost to change later:** one arm in `panes::icon_of` and one in
  `browser_rows`, plus the shell snapshots.
