## #398 - Isolate setup deletion without changing linked staleness

**Decided.** Keep `DeleteSetup` in its own `commands::setup_delete` module,
extracting only the command and its deletion-specific guards from
`commands::setup`. Preserve both existing access paths,
`commands::DeleteSetup` and `commands::setup::DeleteSetup`. Keep `StockSnap`
private and share it internally only if both create and delete still need it.
This is a structural refactor: deletion rules, linked-transform behavior and
operation-stamp behavior do not change.

**Why.** `commands::setup` currently combines creation, deletion, activation
and operation reassignment in a 445-line file, beyond the repository's
~400-line source-file guideline. Deletion has a distinct boundary: construction
refuses when an operation resolves to the setup (including the implicit first
setup) or a linked setup depends on it, snapshots only furniture it owns, and
`apply`/`invert` remove and restore that state.
Isolating that concern avoids broadening the refactor while keeping those
existing guards together.

Linked setups already follow a parent's later part-alignment changes (#316,
#326); the resulting placement is what the operation's existing stamp logic
observes. The regression contract is therefore: generate a fresh operation in
linked child `Setup2`; change parent `Setup1`'s alignment and observe the child
placement move and the operation become `Stale`; undo and observe the prior
placement and `Ok`; redo and observe the moved placement and `Stale` again.
This pins existing propagation and undo/redo behavior, not a new invalidation
rule or a special case in `DeleteSetup`.

**Considered and dropped.** Leaving deletion in `commands::setup` keeps the
mixed concern and oversized module. Moving or reorganizing all setup commands
would add unrelated churn. Removing either existing `DeleteSetup` access path
would force callers to migrate for no behavioral benefit. Changing transforms
or stamps, or forcibly marking operations stale when alignment changes, is
unnecessary: linked alignment propagation and stamp comparison already own
those effects.

**Cost to change later.** Moving the command again requires updating the
module declaration, both compatibility re-exports and callers, while keeping
its deletion guards and any shared snapshot type internal. Changing linked
placement or staleness semantics later is a separate behavior change: it must
revisit the existing alignment/stamp contract and the regression's apply, undo
and redo expectations.
