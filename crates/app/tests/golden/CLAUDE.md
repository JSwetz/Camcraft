# Golden CL files

Committed toolpath dumps for every 2D generator (AGENTS.md: "golden
CLFile/G-code outputs committed under `tests/golden/`"). The behavioral
assertions in `tests/machining2d.rs` say a generator is *reasonable*; a
golden says the exact output did not move. Both are required, and every
new generator test that produces a path must also dump it into a golden -
a generator change that only checks numeric properties can still reorder
half a path unnoticed.

## Format

`common::gen_to_cltext(&Toolpath) -> String` - ours, not a vendor CL
format. One file, one generated path:

```
; HSMW-CL v1 scale=0.001mm moves=412
START 0 0 5000
Rapid ...
Plunge 12000 5000 -2000 F100000
Feed ...
```

Coordinates are integers at `CL_SCALE` (0.001 mm = 1 um, the same
fixed-point resolution the geo2d layer bridges to). Every move line shows
its kind, its destination, and its feed in mm/min (x1000) when the move
has one. The dump carries no timestamps and no iteration-order-sensitive
data, so it is byte-stable across machines and runs - generator code is
pure and ordered by construction.

## Naming

By operation and shape: `face_<shape>.cl`, `contour_<shape>.cl`,
`pocket_<strategy>_<shape>.cl`, `slot_<shape>.cl`, `trace_<shape>.cl`.
(G-code goldens `post_<op>_<machine>.ngc` join the directory when the
post arrives, M1.5.)

## Workflow

Tests read the file and compare (`common::write_golden`): a mismatch
fails with a "stale; regenerate with WRITE_GOLDEN=1" hint, and a missing
file fails the same way - never silently generating it. To regenerate:

```
WRITE_GOLDEN=1 cargo test -p hsmworks-app --test golden_cl
git diff crates/app/tests/golden/
```

Read the diff. A golden changes only when the generator changed on
purpose; the diff *is* the review of that change. Commit generator code
and its goldens together.
