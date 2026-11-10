## #722 - verify's `ok` counts the ops' own post-blocking warnings (2026-11-10)

- **Chosen: `ok` is the post's whole verdict.** `v_verify` (`cam.verify`,
  `cam.check`, the MCP `verify` tool, and through them the
  `camcraft-cli process` exit code) now also requires that no op in scope
  holds the post with its own cached warnings. The rule is the post's
  own, restated in `holds_post`: a suppressed op is not in the program,
  and a warning counts when `Warning::blocks_post()` says so, which is
  exactly when `posting::stream_records` writes it as a `WARN` record. A
  scallop or probe-search note stays a note. **Why:** `ok` read only
  generation status, so it said `true` for a program `post` refused with
  "post blocked - N warning(s)", and the CLI exited 0 ("clean") for it.
  **Dropped:** moving the per-op warnings into `posting::verification_report`
  as gate problems. That would change the gate's meaning for the GUI's
  status line and the post's own refusal text; the warnings already
  refuse the post one step later, so only the summary needed to agree.
- **Chosen: name the holders.** The report gains `held_by`, the labels of
  the ops in scope holding the post (empty when none), so a caller sees why
  `ok` is false without scanning every finding. The typings
  (`assets/scripts/cam.d.ts` and its template in `script/mod.rs`) and the
  MCP tool description say so.
- **Scope:** like `operations`, the warnings part follows the names asked
  for, while the gate and freshness stay whole-document as before.
  `verify("C1")` answers for C1 and the program; `verify()` for every op.
- **Cost to change later:** small. If the post's warning rule changes
  (a new non-blocking kind), `blocks_post()` is the one place both read.
  A caller that treated `ok: true` plus blocking findings as postable
  now sees `ok: false`: that is the fix, and the CLI's exit 1 for such a
  job is the intended change.
