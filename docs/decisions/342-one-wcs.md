## #342 - One WCS per setup; G54-G59 is only the setup's post label

**What was decided.** A setup owns exactly one work coordinate system:
`SetupDef.wcs` is now one entity name (was a six-entry slot list), and
`SetupDef.work_offset` is only the word (`G54`..`G59`) the post writes for
it - changing the word moves no origin and no height. `setup::wcs_entity` /
`wcs_entity_name` take no slot; `setup::wcs_label` reads "WCS (G55)".
`SetWcsOrigin::new(world, location)` places the active setup's WCS (no
slot argument); the UI's `CommandKind::SetWcs` lost its `slot`, `WcsUi`
lost `slot`, and `UiTarget::WcsSlot(i)` became `UiTarget::Wcs`. The browser
shows one row labelled plain "WCS" with the posted word as its faint
suffix; the WCS page heading, setup sheet, rotation refusal, MCP
`get_job` and viewport height chips say "WCS (G55)" / "WCS G55".

**Entity names.** A document without setups keeps the legacy `G54`
entity as its WCS and the materialized `Setup1` points at it (the
implicit-setup law: nothing renamed). A new setup's WCS is `<setup>/WCS`.
A migrated setup keeps the name of the slot entity it posted
(`Setup2/G55`), so its origin and every cached stamp carry over byte for
byte; the name is internal - the word is the label.

**Migration (project v54, `project::wcs_one`).** Each v53 setup takes the
entity of the slot it posted (its pick, else G54) as its one WCS. Placed
slots no setup posted are dropped, each with a load note naming the entity
and its placement (`project::open_noted` / `load_noted` return them, the
shell puts them in the status line, and they are logged). A document
without setups keeps `G54`, drops the rest with notes. An operation naming
a setup the file does not hold posted at `<setup>/G54`; that entity is
renamed `<setup>/WCS` so it keeps its zero. A v54 file still listing slots
is refused as corrupt. The "setup whose operations reached two different
origins" case can only arise from a v51-or-older file (since v52 every
operation of a setup posts its one slot); the existing v52 rung refuses
those by name before this rung runs, so the migration never has to choose
an origin. The two-setups-same-word-different-origin post refusal (#196)
stands unchanged.

**Breaking.** v54 files are unreadable by older builds (the `wcs` key is a
string), and `SetWcsOrigin::new` lost its slot parameter. No previously
loadable file is refused: every v53 file loads (with notes where slots are
dropped). Marked `fix(setup)!` with a BREAKING CHANGE footer for the
project-format bump and the dropped-slot data.

**Considered and dropped.** Keeping the slot list and only hiding rows in
the browser (the model would still allow several origins per setup -
exactly what the maintainer ruled out). Renaming every migrated WCS to
`<setup>/WCS` (cleaner names, but rewrites entity names stamps and hidden
lists refer to, for no user-visible gain). Refusing files with unposted
placed slots (they cut nothing; a note is honest and keeps old files
loadable). Showing one WCS row per setup for every setup at once (the
browser's Setup section follows the active setup, as Stock already does).

**Cost to change later.** The label/entity split is local to
`crate::setup`; renaming entities later would need one more rung.

**Stamp.** An operation's stamp still digests the word its work-offset
heights post under (the #329 shape), so every migrated stamp stays
byte-identical and a migrated document needs no regenerate. The price: a
relabel marks such operations stale once, and the regenerate lands the
same path.
