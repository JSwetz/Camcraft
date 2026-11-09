## #181 - MCP: set work offsets, the machine and fixtures

**What was decided.** Nine new MCP tools, each exactly one existing
command through the session History (no new commands were needed):
`add_setup` (CreateSetup), `activate_setup` (ActivateSetup), `set_wcs`
(SetWcsOrigin), `set_work_offset` (SetSetupWorkOffset), `load_machine`
(LoadMachine, or SetSetupMachine with `setup`), `add_fixture`
(TagFixture), `remove_fixture` (UntagFixture), `move_fixture`
(MoveFixture), and the read-only `get_fixture_findings`. They live in
two new modules, `mcp/setup_tools.rs` and `mcp/fixture_tools.rs`, each
exporting its own `TOOLS` slice; `tools.rs` chains the slices instead of
growing its table, so the open PRs that append rows to that table do not
conflict with this one.

**`set_wcs` takes a stock point, not only coordinates.** "G54 off the
stock corner" is the common ask, so `stock_point: {x, y, z}` picks
min/center/max of the active setup's stock world bounding box per axis,
plus an optional `offset`; `origin` is the absolute form. Exactly one is
required. An omitted rotation keeps the WCS's current one (identity
when unplaced), so moving the origin never silently un-rotates a setup.
The rotation is the same `[x, y, z, w]` quaternion `get_job` prints.

**Fixture findings are the post gate's own walk.** `posting::fixture_problems`
was split into `fixture_findings` (the hits as `(op key, Warning)` data)
and the string formatter the gate keeps using - behaviour unchanged.
`get_fixture_findings` reads the data form, so the tool and the post can
never disagree. With a stale/never-generated op it answers
`judged: false` with the reason, never an empty "clear" list. The
`verify` tool is untouched (other PRs edit `job.rs`); its gate already
carries the same hits as strings.

**Defaults.** `add_fixture` and `set_work_offset` act on the ACTIVE
setup when `setup` is omitted (TagFixture's own `None` means the first
setup, which would surprise an agent working in Setup2).
`set_work_offset` on a document without setups refuses and points at
`add_setup`, rather than materializing a setup as a side effect.
`load_machine` pre-checks a built-in name so the refusal lists the
valid names; its schema carries them as an enum.

**Considered and dropped.** One compound tool (WCS + word + machine in
one call): several History steps per call, or a new compound command,
for little gain. Importing a fixture from a path in `add_fixture`
(AddFixtureModel): `import_model` then `add_fixture` already does it,
and binding to a setup would have taken two commands per call. A
`fixed` flag (SetFixtureFixed) and `delete_fixture`: not asked for;
each is a one-row addition later.

**Cost to change later.** Low: tool rows and handlers only; no file
format, command or post output changed.
