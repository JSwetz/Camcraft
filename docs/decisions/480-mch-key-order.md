## #480 - a saved .mch keeps its key order

**Decided:** the machine draft records the file's key order beside its
value (`machine::order::KeyOrder`, built by a serde visitor in one pass
over the source text) and `MachineDraft::text` writes through its own
pretty writer in that order; keys the file did not have go last in
their object. A blank machine has no recorded order and writes exactly
what `serde_json::to_string_pretty` would. Every save of the Machines
pane (#478: Save, Save As, New from template) goes through
`MachineDraft::text` into `store::save_text`, so all of them keep the
order; validation (`validate_text`, with #478's M-word checks) reads the
same text.

**Why:** the workspace `serde_json` has no `preserve_order`, so a `Value`
keeps keys sorted and a hand-written file came back reordered - noise in
every diff of a machine file the user edits.

**Considered and dropped:** `preserve_order` workspace-wide (changes every
JSON writer, project files included); depending on `indexmap` directly
(already in Cargo.lock transitively, but still a dependency change for
the maintainer). The visitor is ~60 lines and needs neither.

**Cost to change later:** none outside `machine::order` and
`MachineDraft::text`; the pins are `crates/app/tests/machine_key_order.rs`.
