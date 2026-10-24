# Phase 5 plan — multi-axis, turning, scale

The seam map below was measured against the code (2026-10-24); the stage
ladder is the work order. Law that shapes it (user directive, recorded):
**low coupling, high cohesion** — Phase 5's families live behind their own
seams and never widen the fan-out a Phase-4 change already costs.

## The coupling fact that decides the shape

Adding one operation kind today touches **14 files** (the `OpParams::Finish3D`
grep: types/kind/clearance/region_height/is_sane/for_each_height, params
stamp tag, heights, material, generate dispatch + verify-mode + VerifyContext,
posting HeightRefs, setup_sheet, project DTO + both conversions, ui retype +
quick_params + panes' five tables + pages' three dispatches, state CommandKind
+ drain arm). Phase 5 adds **~25 kinds** (multi-axis block 6 + swarf 2 +
5-axis contour/parallel 2 + 3+2 + turning ~12 + mill-turn). Same-channel
growth means ~350 edit sites; that is the silent-wrong-answer risk in
maintenance form. And the files are already over the 400-line law:
operation/types.rs 808, machining2d.rs 2299, finishing3d.rs 1579,
verify.rs 905, stock_sim.rs 1240 (drilling.rs, machining3d.rs next).

**Decision: M5.0 runs first** — a structure rung that (a) splits the six
oversized files along their section seams, (b) collapses the per-kind fan-out
into **one registration table per kind family** (a `define_ops!` style macro
in the operation module owning kind ↔ params ↔ stamp ↔ dispatch ↔ DTO ↔ UI
tables as data, so a new kind is one params struct + one generator fn + one
table row), and (c) pays M4.2b (micron-quantize before cull — it is one
sampler in finishing3d's project path; doing it now retires the two named CI
skips before any new goldens are born). No new operation kinds land in
M5.1/M5.2 until the table shape exists. Coverage law: `#[test]` count 792
before == after across the restructure; goldens byte-identical (M5.0 is
structure, not behavior — the one exception is M4.2b, which must NOT move
Windows goldens: quantization is below the cull epsilon's decision boundary).

## Crate seams (who owns what in Phase 5)

| seam | crate | owns |
|---|---|---|
| pose data | `camcraft-cl` | tool-axis vector on `Move` (workpart frame, unit, `Option`); µm lattice gains it via quantized direction |
| planning geometry | `camcraft-motion` | tilt/lead/lag solving, ruled surfaces, wrap/unwrap, turning profile math — pure, world-blind |
| machine semantics | `camcraft-app` | axis→`A/B/C` conversion against the mounted `.mch` rotaries, reachability refusal, orientation search driving sim queries |
| kinematic sim | `camcraft-sim` | axis-aware collision bands (today `bands_vs_fixtures` places cylinders on +Z only — the law: never treat a tilted tool as vertical), per-frame head/rotary pose |
| dialect | `camcraft-post` | unchanged contract: keyed `A/B/C` millidegrees, tilt refused on rtcp:false; scripts stay honest |
| surfaces | `camcraft-ui` | operation pages ride the kind tables; turning workspace darkens milling cells by declared kind (M3.3's field already outranks inference) |

Angle→vector is exact; vector→angles is machine-dependent (trunnion B/C vs
tilting head), so the conversion is ONE app function with per-machine
solution + shortest-travel unwrapping, unit-tested against the mirror's
rotary direction vectors. The stream keeps only what stage D defined (abc,
tip-positioned XYZ); `abc` gains a producer (post/mod.rs:188's hardcoded
`abc: None` goes when a tilted dump exists — the refusal law is already
tested end-to-end in the five .cps).

## M5.1 — 5-axis milling (stages)

- **A. Pose seam** ✅ (commit 7313f96) — `cl::Move.axis`, dump conversion, `MoveRec.abc` producer,
  law: a tilt the mounted machine cannot reach refuses by name at generation
  time (validate early), never mid-stream. Sim bands read the axis. Goldens:
  one tilted golden stream + its 5 .nc dialect matrix (3 refuse-by-WARN —
  already legal — fanuc/one rtcp-capable emit angles). Delivered: the dialect
  matrix rides live inline streams (`fanuc_post.rs` — the .nc goldens lock the
  3-axis generator contract and stayed byte-identical; tilt is new machinery
  with no legacy bytes), tilthead.mch registered under the inference law, and
  rapids' tilt law deferred to B where indexed position moves make it concrete.
- **B. 3+2 (tilted plane, indexed)** — `TiltedFrame{origin,u,v,axis}` in
  motion; existing planar generators run in-plane, every record carries the
  one commanded pose; position moves between cuts ride rapids. The planar
  laws (in/out, gouge, islands) transfer by rotation-invariance; tests:
  tilted disc pocket = flat pocket's path rotated — exact-structure law.
- **C. 5-axis contour & parallel with axis control** — lead/lag angles from
  the literature (tool axis = normal rotated by lead about the cut tangent
  and lag about the surface normal×tangent), tilt step clamps max tilt,
  side/step downs drive parallel patterns on the freeform. New files:
  motion/multiaxis/{tilt,contour5d,parallel5d}.rs.
- **D. Swarf & advanced swarf** — ruled-surface (blade) finishing: axis along
  the ruling, passes across it; tolerance-aware multi-pass is the
  stock_sim-masked iteration of the same path. motion/multiaxis/swarf.rs.
- **E. Rotary family** — wrap/unwrap for rotary-parallel/contour (cylinder
  map + radial axis), rotary offset roughing via the existing pocket
  machinery in wrap space; multi-axis roughing = level ladder + reachability
  filter; multi-axis finishing rides C's patterns.
- **F. Collision-aware orientation** — per contact patch: cone of admissible
  axes sampled over the shank envelope vs stock/fixture dexel queries
  (sim's API, not a copy), pick min-tilt-from-normal feasible, then
  shortest-travel smoothing across the path (no flip-flops). Refusal when
  no axis clears: named, counted, write-blocking — the prime directive.
- **G. Machine simulation** — kinematic program sim in app: A/B/C interpolated
  with the machine's geometry (pivot offsets from `.mch`), tool + holder +
  head solids vs stock + fixtures, deviation report extended with per-frame
  clearance. The viewer's sim already draws toolpath + material; this adds
  the heads.

## M5.2 — turning & mill-turn (stages)

- **A. Lathe model** — X/Z axis model + spindle-as-cutting-axis law;
  turning stock = revolved profile; `Kind::Lathe` finally offers operations
  (M3.2's darkened cells flip by declared kind).
- **B. Turning set** — facing, profile rough/finish, grooves (3 kinds),
  adaptive rough, threading (G76/G92 from RS-274 public tables), boring,
  parting, chamfer, taper, form, multi-pass — motion/turning/*.rs, each
  pattern its own file; cycles reach the post as the same `Move`/`Cycle`
  records (a lathe post dialect is a new built-in .cps, original text).
- **C. Turning holes + live tooling** — M1.6 cycles on the turned solid
  (radial = tilted C-pose reuse of M5.1's pose seam; axial = plain).
- **D. Bar feeder & mill-turn** — part-handling intents prefigure M5.3's
  block; C-axis continuous milling = M5.1-B on a Y-off lathe kinematic.

## M5.3 — shop scale (0.4.0)

- SDF stock (CPU first — GPU-resident is Phase 6's), threaded generation +
  incremental recompute (the stamp graph already knows what's stale),
  scripting API: **JS through the existing boa engine** (one embedded
  language, the post contract's sandbox, TS consumable via typed JSON —
  the open question answered, RECOMMENDED), headless `camcraft-cli` (the
  project IS the batch format), regression CI over golden models, 0.4.0.

## Execution order & gates

M4.2b → M5.0 → M5.1 (A–G) → M5.2 → M5.3. Each stage: law tests fast-lane,
verdicts `#[ignore]`d, goldens only at close-out; gate = build once +
touched binaries + (for M5.0) the FULL suite incl. heavy lane — structure
changes earn their full ladder. Milestones close with ✅ rows, RECOMMENDED
entries, `m5.x-done` tags and a release build, per the standing convention.
