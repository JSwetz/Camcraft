## #344 - probe_op stops pinning the project version

**Decided:** `every_older_kind_keeps_its_stored_id_and_digest_tag` drops its
`assert_eq!(project::PROJECT_VERSION, 51, ..)` line and keeps the kind
id / digest-tag loop as its pin.

**Why:** the loop is what proves adding the Probe kind needed no format
rung (every older kind keeps its stored id and tag, so an older file loads
with no op going Stale). The hard-coded version proves nothing about the
probe and went red on every unrelated rung - #196 moved it to 51, #329's
v52 left it red on main, #326's v53 would again.

**Considered and dropped:** updating the number to 52 (stale again at the
next rung); asserting `>= 51` (still says nothing about the probe).

**Cost to change:** none - a test-only line.
