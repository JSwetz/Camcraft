# Changelog

One row per behaviour-changing commit: `commit | date | change | version`.
`Unreleased` rows take the version name at the next cut (AGENTS.md "Issue
Law"). Issue references are `(#N)` at the end of the change. Docs-only and
release-mechanics commits are not listed; history before 0.7.0 is in
`RECOMMENDED.md` and git history (the retired `DONE.md` is in every tag up to
v0.8.1).

| Commit | Date | Change | Version |
|---|---|---|---|
| 619e479 | 2026-11-07 | fix(sim): Contour moves answer to the part walls along their length; pocket chord law only below the stock top (#43) | 0.8.3 |
| 5083771 | 2026-11-07 | fix(motion): control compensation refuses a contour the controller cannot offset (#40) | 0.8.3 |
| 3c0e2d8 | 2026-11-07 | fix(sim): the gouge law measures every chord of an Area move along its length (#42) | 0.8.3 |
| 0605698 | 2026-11-07 | fix(app): a whole-part Pocket/Adaptive clears the stock around the part, never its own section (#12) | 0.8.3 |
| 3271457 | 2026-11-07 | build(scripts): scripts/gate.ps1 - quick / batch / release tiers and single-test runs | 0.8.2 |
| b6b584b | 2026-11-07 | fix(app,ui): Circular fits its circle to the selection (#4) | 0.8.2 |
| 9318355 | 2026-11-07 | fix(app,ui): 2D Adaptive - close an edge loop, the Selection height offset, Outside containment (#1, #2, #3) | 0.8.2 |
| 64a5708 | 2026-11-07 | fix(ci): the release workflow may write the repo's contents - create the GitHub Release | 0.8.2 |
| c47e2b3 | 2026-11-06 | feat(geometry,motion,sim,app): contour depth reads touching bodies as their union (M13.11) | 0.8.0 |
| 2841e93 | 2026-11-06 | feat(geometry,app,ui): mesh repair - weld by spatial hash, fill planar holes, one undoable command (M13.11) | 0.8.0 |
| ba6e17b | 2026-11-06 | feat(motion,app,ui): facing rows go around holes - Face.holes Lift by default (M13.11) | 0.8.0 |
| f7e96e1 | 2026-11-06 | feat(app): contours picked across two models stitch into one ring (M13.11) | 0.8.0 |
| 6c7abf3 | 2026-11-06 | feat(cl,motion,post,app,ui): G41/G42 cutter compensation for Contour - the part contour under control comp (M13.11) | 0.8.0 |
| abddbaa | 2026-11-06 | fix(motion,sim): the finishing march seeks on a ladder past fold gaps; a closed ring never flanks its own inside (M13.9) | 0.8.0 |
| 6caab50 | 2026-11-06 | feat(app,ui): the area-control leftovers close - ramped clipped re-entries, Trace/Engrave containment, the tilted Face band (M13.10) | 0.8.0 |
| 8c9dfaf | 2026-11-06 | feat(app,sim): the rotary lift - the field eye chords every rotary kind, the column verdict refuses by name (M13.1) | 0.8.0 |
| 66c1860 | 2026-11-06 | fix(sim): the fixture sweep walks the mid-move pose law with a proven swing (M13.8) | 0.8.0 |
| 7205935 | 2026-11-06 | feat(app,ui): the ribbon reads the command door's word; the Setup pane offers the matching post on mount (M13.5) | 0.8.0 |
| e006e49 | 2026-11-06 | feat(motion,app,ui): the Pocket3D port of the HSM planner - the 2D plan inside the 3D level law (M11.4) | 0.8.0 |
| 00c5c99 | 2026-11-06 | fix(motion): late leads re-marked on the re-ordered stock; the island's hops are a leading graze (M11.4 round 6) | 0.8.0 |
| d352e37 | 2026-11-06 | feat(motion): arc output for the HSM adaptive plan under three proofs; the load reader faces an arc's tangent (M11.4) | 0.8.0 |
| 6bef486 | 2026-11-06 | feat(post,app): the first lathe post, fanuc-lathe.cps - diameter programming on the kernel's meridian (M13.5) | 0.8.0 |
| 055d1e0 | 2026-11-06 | feat(app,motion): axial drilling on the lathe, and a rotary letter gate against the machine's inventory (M13.5) | 0.8.0 |
| 259d021 | 2026-11-06 | perf(app,sim,ui): a playback jump cuts checkpoint to checkpoint with the ledger off; the CPU fallback eye traces on a worker (M13.1) | 0.8.0 |
| eaf88bc | 2026-11-06 | feat(app,ui): boundaries read in the tilted plane, and the boundary group on the whole 3D finish family (M11.5) | 0.8.0 |
| 1bad4af | 2026-11-06 | feat(motion,sim): the swarf pose laws - normal retract, a solid reading for leaning rapids, no upright pose under the equator; the mid-move pose law lands (M13.7) | 0.8.0 |
| ee2ebef | 2026-11-06 | feat(ui,app): the machined mesh reaches the viewport's stock slot and the pick (M13.1) | 0.8.0 |
| 191b9aa | 2026-11-06 | feat(motion): the waterline ladder reads the part's slope, with the 45-degree law as its ceiling (M13.9) | 0.8.0 |
| 485e5f2 | 2026-11-06 | feat(app): the setup sheet names each fixture's workholding frame; setup_sheet.rs split along its seams (M13.8) | 0.8.0 |
| 33242f3 | 2026-11-06 | feat(sim): the lathe sweep and the rotary chord pre-pass on the field stock (M13.1) | 0.8.0 |
| fefe8ae | 2026-11-06 | feat(sim,app,ui): the scallop law lives as a non-blocking, ratcheted verdict (M13.9) | 0.8.0 |
| 2e678dd | 2026-11-06 | fix(motion): HSM growth revolutions land on their ring tangentially; a plunge landing clears the full rim (M11.4) | 0.8.0 |
| df18fb7 | 2026-11-06 | refactor(app): operation/material.rs split along its seams - a pure move, 749 -> 216 + 262 + 300 (400-line law) | 0.8.0 |
| 5927926 | 2026-11-06 | fix(motion): one descent per level on the 3D ladder and the bore contour (M11.3 owed) | 0.8.0 |
| c5e002a | 2026-11-06 | fix(ui): themes are found beside the exe at runtime, not through the build machine's source tree (M13.3, 0.8.0 must) | 0.8.0 |
| 9b51275 | 2026-11-06 | feat(app,motion,ui): the open Contour's swept band in the viewport, approach extensions on open runs, and a summary that names the chain's ends (M11.5) | 0.8.0 |
| 76f8d27 | 2026-11-06 | fix(app,ui): the generate-time fixture check runs the workholding laws with the spindle, and fixtures get a Fixed toggle (M13.8) | 0.8.0 |
