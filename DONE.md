# DONE.md - the archived roadmap

Phases whose every milestone passed all green gates, moved verbatim out of
`ROADMAP.md` under the Done-ledger law (AGENTS.md). This file is history:
append-only, never rewritten except to correct a fact, and never read by
choice - open it when current work needs to know HOW something shipped (a
refactor touching old ground, a regression hunt, a "was this done?" check).
The registry table in `ROADMAP.md` stays the single status truth; `RECOMMENDED.md`
holds each phase decision, and `git log` the commits.

Archived 2026-10-31: Phases 3-6 (M3.1-M3.3, M4.1-M4.4 + M4.2b, M5.0-M5.3,
M6.1-M6.2) - releases 0.3.0 and 0.4.0 shipped inside them.

Archived 2026-11-02: Phase 8 (M8.1-M8.3) - release 0.5.0 shipped inside it.

---

## Phase 3 — Associative CAM & Production Workflow

The workflow that justified HSMWorks' name: the model changes, and the toolpaths
update. Re-MACHining, multi-setup jobs, posts for the machines people actually
own. The operations catalog completes here too - every 2D/3D name gets a
registry home before Phase 5's multi-axis and turning families arrive.

### M3.1 - Associativity & Regeneration
**Goal: a model revision no longer means rebuilding the job.**

- ✅ **Geometry association**: operations reference model features and curves, not
      stale copies - the law already held structurally (picks are live feature ids,
      re-validated by `SetOperationGeometry`/`valid_pick`, never stored geometry), and
      stage C adds the workflow on top of it: `stale_reason` answers WHY an operation
      reads Stale by asking the world the live questions (vanished tool, deleted
      source, moved pick, unreachable height plane), the properties page prints it,
      and `RegenerateAll` fixes the whole stale job in one undo unit, skip-not-fatal
      for anything whose generator refuses (M3.1 stage C)
- ✅ **Model update detection**: a same-named re-import lands as `UpdateModel` on
      the one entity the name keys (revision counter, undo restores the previous
      geometry exactly); the diff is a set difference over stable content-hashed
      feature ids - faces, welded faces, edge chains - and staleness of dependent
      operations falls out of the stamp's revision signature, so unchanged
      re-imports churn nothing and same-count-different-solid updates cannot pass
      in silence (M3.1 stage A)
- ✅ **Re-MACHining**: automatic rest against the areas previous operations
      machined - `rest::prior_state` replays every predecessor's CACHED moves
      through the stock_sim dexel grid and hands the 3D pocket's level ladder a
      mask that only ever removes cuts (a masked run is a subset of the
      unmasked one, so air-cutting shelves vanish and the islands law still
      guards the part). The predecessor's term in a dependent's stamp is the
      hash of its recorded path, not of its intentions: an edited,
      unregenerated operation moves nobody, and a program converges within two
      `RegenerateAll` rounds. A fully-cut duplicate refuses as Empty with the
      "already machined" word; program order is the post's order - names sorted,
      the same sequence `posting::program_ops` walks (M3.1 stage D)
- ✅ **Parameter propagation**: changed stock, tool, or WCS flows correctly
      through dependents - the tool and height arms already digest live world
      state, and the copy that could go stale was `FromModel` stock:
      `UpdateModel` now re-derives the stock's material inside the same undo
      unit, and the stock stamp digests the copy's facet signature, so a
      source-model update reaches stock-based operations through the metal
      and a stock whose source was deleted stays honestly fresh
      (M3.1 stage B)

### M3.2 - Setups & Documentation
**Goal: multi-setup production jobs with paperwork.**

- ✅ **Multiple setups**: per-setup stock / WCS resolved through each operation's
      declared setup (the implicit-setup law: none == the legacy globals, the first
      CreateSetup materializes and never renames, private furniture is spawned on
      first use); the flip is placement (rotation arrives with M3.3); datum transfer
      clones a setup's stock and placed WCS into its own names, `datum_from` kept as
      provenance; program order is setup rank then name, rest masks same-setup only
      (M3.2 stage A)
- ✅ **Seperate workspaces**: the workspace follows the machine's kind, inferred from
      the `.mch` axis inventory it already declares (X/Y/Z → milling, +rotary →
      mill-turning, X/Z without Y → turning, undecidable → milling - the assumption
      every other line of code already makes); a turning document darkens the milling
      `Create` cells with a tip, because generating an invented turning workspace
      would be the silent wrong answer - the real turning family lands with M5.2,
      and M3.3's "machine declares its kind" replaces this inference with a stated
      field (M3.2 stage E)
- ✅ **Fixture modeling**: a fixture is a model wearing a marker - the user loads the
      shop's real workholding as STEP and marks it for a setup; collision checking
      knows the solids (shank/holder/spindle bands + rapid-air law), the spindle band
      and the table come from the machine file, and nothing about a fixture silently
      changes toolpaths - workholding rides the post's live gate, never the stamp,
      so moved steel blocks the post by name without ever staling a fresh path
      (the M2.3 law, extended; M3.2 stage B)
- ✅ **Setup sheets**: printable text sheets generated from the CAM data itself -
      one page per setup, form feeds between them, everything resolved live (tools,
      offsets, stock, fixtures, the tool list); no timestamps, so the same document
      renders byte-identical (M3.2 stage D)
- ✅ **Tool list**: per-setup inventory in tool-change order with change counts,
      cut/rapid travel and estimated run time - a read of what the spindle will
      actually hold, never an editable list (M3.2 stage C)

### M3.3 - Post System & Machine Library (Release 0.3.0)
**Goal: real G-code for real controls, written by the user, not by us guessing.**

- ✅ **Post template engine**: generalizes M1.7's CPS engine so users own
      their posts in the app (same clean-room line: our format, our
      text, no adaptation of any vendor's post files) - the document's
      mounted `.mch` now shapes the output, a `SelectPost` command picks
      a user `.cps` through the undo history, and a missing or broken
      script refuses by name instead of falling back (M3.3 stage A)
- ✅ **Built-in posts**: Fanuc, Haas, Siemens 840D, Heidenhain, Mazak - each
      with conformance tests against a golden G-code suite: five original
      `.cps` scripts, a 5x13 matrix over the committed streams plus a
      synthesized one that reaches the tool/spindle/modal-cycle words, the
      registry addressed by name so projects survive it growing
      (M3.3 stage B)
- ✅ **Machine definitions**: a machine group in the Setup pane - select or edit a
      machine, its name shown in the setup, driving simulation limits and post
      output. A machine declares its kind - mill, lathe, or mill-turn - and
      the operation families a setup offers follow the kind; today every
      assumption in the app is a mill. The definition carries the machine's
      numbers: X/Y/Z travel and rapid rates, spindle speed and power limits
      (the ceilings the tool library's per-material speeds and feeds are
      checked against), rotary axes where present
      (A/B/C travel, rapid speed, positioning resolution), tool changer
      capacity, and coolant options (none / mist / flood / high-pressure /
      through-spindle) that reach the post as M-codes (M1.7's posts read
      the coolant set from the `.mch` machine file). The user-facing workflow (pick a machine,
      edit it, see it on the setup) is specced from published CAM user
      documentation only; no vendor strings, templates, or post files are
      copied or adapted - the declared word now outranks the axis
      inventory (inference kept as the unstated-file fallback), rotary
      parts join the table (fixing the law M3.2 E stated but no file
      could reach), the checker counts distinct tools against the
      changer, a mounted coolant list speaks its own M-codes through
      the mirror, and the machine editor drafts over a copy whose Apply
      is a plain `LoadMachine` at the mount's path, so undo restores the
      exact previous numbers; three original built-in machines mount by
      name from the pane (M3.3 stage C)
- ✅ **RTCP output**: G43.4 / A-angles correctly emitted for trunnion and tilting-head
      machines - the stream carries absolute commanded angles as keyed `A/B/C` words
      (millidegrees, tool-tip frame; a tilted record states all three letters, every
      prismatic dump omits them), a rotary's direction vector rides the machine file
      and mirrors as geometry (letter, axis, degree travel - its deg/min rates join
      nothing, closing stage C's ceiling leak), `machine.rtcp` is a geometric judgment
      (some rotary off the Z column: an A tilts, a spindle-column C turns), the Fanuc
      post arms `G43.4 H_n` once per tool, re-arms at the change, and commands the
      angle words modally in both directions - and a tilted move on any machine the
      script says cannot tilt is a counted, named refusal that writes no angle words
      (M3.3 stage D)
- ✅ **Post debugging**: block-by-block trace from toolpath to line of G-code - the
      engine records which stream record wrote which program lines (`TraceBlock`,
      half-open; header and footer own no record, a swallowed record rides the trace
      labeled "(no output)" so loss stays visible), `post --trace <file>` prints the
      program as its own provenance, and the viewer grows a "trace" pane whose
      selection is DERIVED from the scroll cursor the window already publishes -
      clicking a block centers its first line, the pane stores no second truth
      (M3.3 stage D)

---

## Phase 4 - Refactoring and reuseability.

The rust codebase is to complicated and convoluted. All motion types, operations, tool management,
heights and offsets, movement, simulation. Should be isolated as smaller subcreates. Code
files should be no larger than 400 lines of code to keep it maintainable. All tests should be isolated
to each subcrate to make testing cleaner. Each crate should produce a standalone executable that can be
headless tested.

### M4.1 - Seperating the code into crates
**Goal: Sepearting all components into smaller crates.**

- ✅ **Identify code blocks**: the seam map in `docs/M4.1-CRATE-SPLIT.md` -
  geometry/cl/tools/post/sim/motion as crate futures, plus the R0 type
  relocations that pre-positioned ownership inside app (M4.1 R0)
- ✅ **Chunk the parts up**: six crates landed, one rung each -
  camcraft-geometry (R1), camcraft-cl + camcraft-tools (R2),
  camcraft-post (R3), camcraft-sim + camcraft-motion with the
  stock_sim/fixture knot dissolved (R4); app's operation and project
  monoliths split along section seams (R5). Tree-mirrored re-exports
  keep every `crate::x::y` path compiling across boundaries
- ✅ **Move tests**: each crate carries its own suites (geometry 29 lib +
  24, sim 16 + 17, motion 27 + 108, post 17, cl 4, tools 5); coverage
  law held at every rung - 785 `#[test]` before == after
- ✅ **Run tests**: every crate's tests run individually (`cargo test -p
  <crate>`); the CI split lane runs six `-p` flags in parallel, the
  heavy lane the release-profile verdicts (finish3d 27 ignored in
  motion, app verify 2 envelope verdicts)
- ✅ **Clean up**: workspace check green, clippy `-D warnings` green over
  all eight packages, release build on stable green (the 0.3.0-milestone
  vehicle), user hand-test deferred per the standing workflow

### M4.2 - Multi plattform build targets.
**Goal: Building the same application, for multiple OS environments.**

- **Windows**: First usecase, its what we got.
- **Linux**: Prioritized build case, where we will have most adopters.
- **MacOSX**: Secondary prioritsation.
- **WASM**: A web version for online use.
- **Docker**: Using docker builds for every target.
- **GPU backends**: the shell ran egui through a plain wgpu instance
  with auto adapter selection (`Backends::PRIMARY`, first hit) — the
  Metal/Vulkan/DX12/GL-ES(ANGLE) options are compiled in but never
  chosen, forced, or reported. Add a backend pick (app flag or env),
  an honest fallback chain, and a startup log line naming the adapter
  the viewport actually got — a driver blacklist workaround needs
  something to work around *with*. The 3D viewport is the GPU-heavy
  consumer (toolpath meshes, stock sim surfaces); the WASM target
  reaches the same code through wgpu's WebGPU/WebGL shim, so the
  GL-ES tier is what makes the web build honest rather than lucky.

- ✅ **Windows**: unchanged home turf — full suite green (189 UI tests
  incl. 7 new gpu-pick tests; the gpu_render goldens byte-stable).
- ✅ **Linux**: workspace `cargo check --all-targets` green natively
  (WSL2 rustc 1.99, clang for the flecs C build); golden suites run
  18/20 + posting + fanuc_post + roundtrip green — the two dome goldens
  drift 2 moves on Linux's libm (named CI skip, fix = M4.2b, see
  RECOMMENDED); CI carries an ubuntu-latest job.
- ✅ **macOS**: CI build job on macos-latest (check + goldens + split
  crate tests); the first green runner run is the proof (RECOMMENDED).
- ✅ **WASM**: geometry/cl/post compile for `wasm32-unknown-unknown`
  (post needed only getrandom's `wasm_js` target dep); pinned by a CI
  wasm job. The app/UI-on-wasm story is the web-sys-mvp rung.
- ✅ **GPU backends**: `--gpu-backend <name>` + `CAMCRAFT_GPU_BACKEND`
  (flag wins; bad name refuses by name, exit 2, before any window), the
  honest preferred→auto→all fallback chain with each miss logged, and a
  startup line naming the adapter and answering backend.

### M4.3 - Defer HSMWorks label thru out the code.
**Goal: Removing the HSMWorks branding as its conflicting**

- ✅ **Debrand**: Find every instance of where "hsmworks" been used.
      Every crate, string, comment and identifier now reads camcraft -
      packages `camcraft-*`, bin `camcraft-ui.exe`, project files
      `.camproj`, window title "CamCraft", the cps vendor line, the zip
      name, the import thread; the `.cl`/`.cnc` **wire magics stay**
      (`HSMW-CL`/`HSMW-CNC` - a format signature is a compatibility
      contract, the parser accepts both histories, renaming one would
      silently orphan every file on a user's disk), the hash **salts
      moved** (`camcraft-op-stamp-v2`, `camcraft-rest-*` - one-time
      Stale-read for saved documents, the same churn a version bump
      causes, by design); trademark sentences that NAME Autodesk's
      product ("a clean-room implementation of HSMWorks", "no HSMWorks
      source was consulted") are true statements and stay (M4.3)
- ✅ **Rename**: New name is "camcraft" - `cargo check/test/clippy
      --workspace` green on the new names (595 passed / 0 failed / 29
      ignored, 792 `#[test]` before == after), goldens byte-stable,
      post `--version` intact, dist artifacts `camcraft-0.x.0-*`

### M4.4 - Test cost shape: fast, targeted gates
**Goal: a gate run costs what the diff cost — no 30-minute suite tail**

The dev-time tax, measured: `crates/app/tests/finish3d.rs` alone runs
~30 min single-threaded in the debug gate, and it holds up every ladder
regardless of what changed. The cost is concentrated in the
envelope-VERDICT tests (`scallop_crosses_the_diagonals_and_measures_clean`,
`the_hill_under_radial_and_spiral_is_envelope_clean`,
`the_hill_under_contour_and_project_is_envelope_clean`) — each one is a
grid `envelope_scan` over a 12×96 dome plus a full `verify()` in
Envelope mode with its own `slice::project(facets, 64)`, at debug speed.

- ✅ **Split LAW from VERDICT.** Law tests assert generation shape —
  bucket membership, refusal strings, move and link counts — and must
  run in milliseconds: generate one small path, assert the structure,
  done. Verdict tests assert the geometry *machines clean* (scan,
  project, envelope-verify) and are minutes each. Every suite gets this
  cut, finish3d first: the law half stays in the default binary, the
  verdict half moves to its own `#[test] #[ignore]` lane or a separate
  `--test` binary so `cargo test -p hsmworks-app --test finish3d` is
  seconds, not the tail that decides the milestone.
- ✅ **The heavy lane runs on release, opt-in.** `#[ignore]`d verdicts
  execute under `cargo test --release -- --ignored` — an hour of debug
  grind is minutes optimized — scheduled at milestone closes and
  pre-push, never per-edit. The split must not lose coverage: the count
  of law + verdict tests before and after must match, and the verdict
  lane keeps its own golden asserts.
- ✅ **Targeted gate selection.** The gate ladder learns to run only the
  test binaries the diff touches (`cargo test -p <crate> --test <name>`
  per changed file; suites are already one binary each, so selection is
  free), with the full ladder reserved for milestone commits. Document
  the map in the test file headers: which binary covers which module.
- ✅ **Budget.** Default debug ladder target: under ~5 min wall on this
  32-core box — the finish3d split is the single biggest lever (it is
  the current longest pole), so it lands first; then sweep the other
  suites for the same verdict-in-the-law-lane shape (`machining3d`,
  `rest`, `verify` envelope tests carry the same `project`+`verify`
  pattern in smaller doses).

---

## Phase 5 — Power: Multi-Axis, Turning, Scale

The Ultimate-edition feature set on top of a mature product: continuous
multi-axis, mill-turn, and the performance and automation to run a shop
programming department.

The stage ladder and crate seams are specced in `docs/M5-PHASE5-PLAN.md`
(measured against the code, 2026-10-24). Its governing decision: **M5.0
runs first** - the operation-kind fan-out (14 files per kind today) and the
six oversized files are collapsed before ~25 new kinds arrive, so each is
one-table-row-cheap; the M4.2b dome-quantize fix rides along so no new
golden inherits the platform-libm drift. Low coupling, high cohesion is the
Phase-5 shape law.

### M5.0 - Structure prelude
**Goal: make adding an operation kind cheap before Phase 5 adds twenty-five.**

- ✅ **Split the oversized files** along their section seams to the ~400-line
      law: operation/types.rs, machining2d.rs, finishing3d.rs, verify.rs,
      stock_sim.rs, drilling.rs, machining3d.rs - structure only, goldens
      byte-identical. Done in M5.0.1 (commits 4463808..4bf5e4d): seven
      flat modules became seven directory modules, pub surfaces
      re-exported from each mod.rs, 793 `#[test]` before and after,
      29 `#[ignore]` kept (the 31 in the run-up notes was a miscount;
      git grep says 29 at HEAD, then and now), goldens porcelain-clean
      at every rung. Two files sit at the cap's edge by recorded choice
      (drilling/tests.rs 408, verify/engine.rs ~415) - RECOMMENDED.md,
      M5.0.1.
- ✅ **Collapse the kind fan-out** into one registration table per family:
      kind <-> params <-> stamp <-> generate-dispatch <-> verify-mode <->
      DTO <-> UI tables, so a new operation is a params struct + a generator
      fn + a table row, not a 14-file hunt — done in M5.0.2 (commit
      c465558): `KindInfo::ALL` in `operation/kinds.rs` carries label,
      caption, digest tag, verify mode and seed fn per kind; the retype
      carry folds moved from the UI with it; the UI's three silent
      `_ =>` catch-alls are gone; registry order is compiler-forced
      (exhaustive `ordinal()`) and pinned by assert. The glue that
      must be per-kind (region builders, generator dispatch, sanity,
      DTO mirrors) stays exhaustive matches - RECOMMENDED.md, M5.0.2.
- ✅ **Pay M4.2b**: quantize the dome sampler to microns before the cull,
      drop the two named CI skips — done in M5.0's run-up: scan points on
      the lattice, hull in exact integers, arc splits lattice-biased; all
      20 goldens pass on Linux and Windows from one committed set

### M5.1 - 5-Axis Milling
**Goal: simultaneous-axis and tilted machining done safely.**

- ✅ **Pose seam** (plan stage A, commit 7313f96): `Move.axis` - the tool
      orientation rides every move, None stays prismatic byte-for-byte,
      the dump converts leaned axes to the canonical A/B/C the post and
      machine already speak, generation refuses an unreachable pose by
      name, and the swept-band collision laws enclose the lean. Every
      bullet below rides this seam - RECOMMENDED.md, M5.1 A.
- ✅ **Swarf and advanced swarf** (stage D, commit 371cd3e): the
      bladed finish - the mesh tells the pass which way to run: a
      per-facet curvature fit claims a ruling direction, the raster
      lines ride it, and on ruled contact the posed axis lies ALONG
      the ruling (lead, lag and the cone bend nothing that would cut
      a groove the profile doesn't have; unruled facets fall back to
      stage C's lean law, honestly); advanced swarf adds the scallop
      ladder - pass i at 4x tolerance, 2x stepover, centers tangent
      to the model every pass - so the last rung IS the finish path;
      refusals name their remedy (dome, Z-wall ruling, coarse fit);
      positions stay bit-identical to parallel5d under a vertical
      axis - RECOMMENDED.md, M5.1 D.
- ✅ **5-axis contour and parallel** (stage C, commit b668048): lead/lag,
      max tilt over the waterline and raster ladders - a sphere's
      envelope is rotation-invariant, so the paths ARE stage B's, plus
      the stamp: every move ending in tangency poses the surface normal
      leaned forward about the binormal (lead) and sideways about the
      tangent (lag), folded onto the max-tilt cone, never past it;
      rapids and air moves hold no pose, drill cycles refuse the stamp
      by name; the axis trio always rides the project file (v24), so a
      re-post can never silently drop a lean - RECOMMENDED.md, M5.1 C.
- ✅ **Rotary contour and parallel** (stage E, this commit): the turned
      part's two native ladders, each riding an axis FITTED to the
      model - the circumcenter-concurrency law reads the rotation axis
      off the normals alone (a revolve facet's normal line meets the
      true axis at any tessellation; direction from the normals'
      area-weighted second moment, position by least squares, both
      eigenvector candidates tried, a sagitta-budget coarseness gate
      refusing pretending polygons); contour = the waterline rings in
      the axis frame with heights read as axial stations, parallel =
      the swarf raster in a frame that lays the axis down - and on
      every cut move the posed axis is the RADIAL, the line from the
      fitted axis to the tool (lead/lag/cone measured off that seed,
      the same trio as stage C); wrong stances refuse each other by
      name, domed ground points at the 5-axis kinds, freeforms name
      the taxonomy - RECOMMENDED.md, M5.1 E.
- ✅ **Rotary offset roughing** (stage E2, this commit): the pocket
      ladder on a turned part - pocket3d's staircase law run in
      developed wrap space (s, v, rho) over one full period, the
      stepdown RADIAL, each column's stock read off its own part
      surface so a part step wider than the infeed is left whole to
      the finishing kinds (honest, not padded); facets with on-axis
      vertices sweep their wedge, seam walls close the bands at the
      station table, and world exits wrap back radially via
      wrap_exit - kernel `crates/motion/src/multiaxis/rotary_offset.rs`
      + `wrap_exit.rs`; app kind RotaryOffset (ordinal/tag 14,
      project v27), Envelope verify (law (f)), ball-nose refused by
      name; the ribbon glyph assets/ui/rotaryoffset.png is drawn
      original - RECOMMENDED.md, M5.1 E2.
- ✅ **Multi-axis roughing** (stage E3, this commit): the pocket
      ladder with the spindle posed - the positions ride the 3-D
      pocket's ladder bit for bit through the `multiaxis_rough_prev`
      mask seam, and stage C's stamp writes the axis over the
      finished path: a move ending in wall or corner tangency poses
      the cone about the contact normal (lead/lag folded onto the
      max-tilt cone as in finishing), floors and air stand honest
      vertical; the ball-nose gate refuses a flat end at generate,
      the editor carries the pose trio - kernel
      `crates/motion/src/multiaxis/rough.rs`; app kind
      MultiAxisRough (ordinal/tag 15, project v28, tag
      `multi_axis_rough`), Envelope verify (law (f) restated for the
      rough: a sphere's envelope is pose-invariant, so the verify
      sees the pocket's volume) - RECOMMENDED.md, M5.1 E3.
- ✅ **The rotary family**: multi-axis contour, multi-axis finishing
      (the parallel and contour twins landed with stage E, rotary
      offset roughing with stage E2, multi-axis roughing with stage
      E3, the flowline finish with stage I, the drive-curve contour
      with stage J) - the catalog's multi-axis block lands here,
      each riding the orientation and machine-sim laws below
- ✅ **5-axis flowline finishing** (stage I, commit 378184a): the
      finishing pass that CURVES with the part - the per-facet
      shape-operator fit claims each facet's least-curving direction
      (`curv.rs`, split out of `ruling.rs` so swarf and flow read
      ONE curvature kernel and the field the two kinds see cannot
      drift apart); the direction field answers a column with the
      nearest facet centre's claim, on a grid two mean facet edges
      wide and within a two-cell reach, and with the region's
      dominant fallback where the column's facet claims nothing -
      the double-angle (2theta) area-weighted vote, since theta and
      theta+PI are one ruling, +x when the vote is silent: flat
      ground degrades to the parallel raster's answer and CUTS,
      never silently unfinished; the geometry is the raster's laws
      on turning lines (`finishing3d::flow` - the curved-line
      family on the shallow ground, the waterline rings the flanks'
      jurisdiction, half-step seeding keeping every point within
      half a stepover of a traced line); stage C's lean law and
      stage F's shank dodge pose every cut move; app kind Flow5D
      (ordinal/tag 16, wire tag `flow5d`, project v31 a pure
      step-up; `stepover` and `retract_z` are `Option` - an absent
      stepover is the cos-law scallop derived from the tolerance,
      an absent retract the kernel's own rapids plane - and
      `shank_avoid` is always stated in the file), Envelope verify,
      the refusal trio inherited from the 5-axis twins (the FlowF
      case), and the ribbon glyph `assets/ui/flow5d.png` drawn
      original - RECOMMENDED.md, M5.1 I.
- ✅ **3+2 tilted-plane indexing** (stage B, this commit): the five
      plane kinds carry a `Tilt`; generation runs them in the frame's
      LOCAL coordinates and leaves through `TiltedFrame::world_path` -
      the tilted path is the rotated flat path, move for move, with the
      pose stamped on every move; planar laws verify local, steel laws
      world; picked selections refuse a tilt by name; the program posts
      only to a machine with the letters. Position commands between
      cuts ride A; 3+2 ROUGHING strategies (the helical/adaptive
      families on the plane) continue below
- ✅ **Collision-aware orientation** (stage F, this commit): auto-tilt
      to avoid shank contact, with shortest-travel smoothing - the
      spindle dodges the wall WITHOUT moving the path: the three
      axis-law kinds (5-axis contour and parallel, multi-axis
      roughing) take an opt-in `shank_avoid`, hand the stamp the
      tool's real stack (flute, shank, holder), and where the
      surface-law pose would drive the shank into material the axis
      tilts to the nearest clear angle inside the max-tilt cone -
      the pose-only freedom is law (f)'s, so positions stay
      bit-for-bit the flag-off path's; where no cone angle clears,
      generation REFUSES by name rather than post a program known to
      rub. Kernel `crates/motion/src/multiaxis/avoid.rs` (`Shank`,
      `shank_hits` mirroring verify's law (c), `escape_axis`),
      project v29 - RECOMMENDED.md, M5.1 F.
- ✅ **Multi-axis contour on drive curves** (stage J, commit caaab7a):
      machine to a curve on a surface with orientation rules, said
      literally - the 18th kind machines the picked edge chains WHERE
      THEY STAND: no pattern is asked of the model, the curve IS the
      path, so there is no stepover and no ladder to author spacing
      with, and `tolerance` is only the verify budget (the envelope
      scan's chord allowance, law (f)); kernel
      `crates/motion/src/multiaxis/curve.rs` makes every chain
      vertex a station - the fan of planes meeting there (smooth
      fans merge, an edge is two planes, a corner three), the ball
      resting on the bisector at h = r/max|cos| with the pass side
      resolved by winding (edge-pairing closure + signed volume,
      air/sink bids as fallback), the pose stage C's lead/lag-then-
      max-tilt law; the window law refuses any vertex standing
      outside [bottom, top] BY VERTEX, never clips, and an open run
      or a chain standing off the model refuses by name; app kind
      MultiAxisContour (ordinal/tag 17, wire tag `multi_axis_contour`
      auto-derived, project v32 a pure step-up), the picks ride the
      existing `SrcRef::Selection(Chain)` field so the digest's
      stamp law covers model edits for free, and the dome-and-chain
      glyph `assets/ui/multiaxiscontour.png` is drawn original -
      RECOMMENDED.md, M5.1 J.
- ✅ **Machine simulation** (stage G, this commit): full kinematic
      simulation of the post's A/B/C motion model, not just the
      toolpath - the head turns on its own clock and a tilt that
      sweeps into steel is named: every move splits into a linear
      leg and a rotary leg, secs = max(the two), each family paced
      by its own rate and holding at arrival - XY frozen at its
      spot while the head swings, equal legs bit-exact prismatic -
      rotary rates under the rotaries' own roof (`Traverse.rot`,
      fed from the .mch mirror), the post's modal pose law (rapids
      hold, unposed cut = home) threading each segment's
      [from,to] letters; and verify's law (c3), kernel
      `crates/sim/src/verify/sweep.rs`: the band's law-(c) column
      tested on a grid of 4 path x 5 pose fractions for every cut
      move whose commanded pose CHANGES - pose endpoints in, path
      endpoints out, plunges never skipped, moves (c) convicted
      left to (c) - a graze only the swing reaches is now
      `Warning::SweepHit`, forcing project v30 (a pure step-up) -
      RECOMMENDED.md, M5.1 G.
- ✅ **Close-out - the family's envelope verdicts, measured** (this
      commit): the dome flow read 0.088 mm of "gouge" where the path
      provably never penetrates past 0.012 and the ring tail's arcs
      stand tangential with clearance - the fault was the yardstick:
      `envelope_scan` cut arcs to chords with a sagitta budget of
      cell/4, larger than the tol+floor gate it feeds, so chords
      corner-cut every tangential ring into a phantom trench. The
      measurement now keeps the generator's own chord law - every XY
      shortcut keeps tolerance/4 - one budget for the Dexel truth and
      the distance truth; it only ever TIGHTENS the verdict (0.094 to
      0.0125), because the machine follows the commanded arc: dome
      flow 0.08238 to 0.01656 mm (gate 0.07344), rest unchanged, all
      goldens byte-free, the verify law (f) shares the honest
      yardstick - RECOMMENDED.md, M5.1 close-out.

### M5.2 - Turning & Mill-Turn
**Goal: lathe parts and C-axis parts from the same system.**

- **Lathe machine model**: the turning side of M3.3's machine kinds - a
      lathe-kind machine offers turning operations on its own axis model
      (X/Z, spindle as the cutting axis), so the mill-centric plumbing
      finally branches; mill-turn machines offer both families
- **The turning operation set** (the catalog's list, made checkable):
      facing, profile roughing and finishing, roughing/finishing groove,
      turning groove and single groove, adaptive roughing, threading,
      boring, parting, chamfering, taper turning, form turning,
      multi-pass turning - inserts from the tool library, passes judged
      against the M2.3-style material sim on the revolved axis model
- **Turning holes**: radial and axial drilling through live tooling -
      M1.6's cycles and hole recognition reused on the turned solid
- **Bar feeder patterns**: catch/transfer support for production turning
- **Mill-turn**: live tooling operations indexed and continuous (C-axis)

- ✅ **Lathe branch substrate + Facing** (stage A, commit 1648ce9): the
      door moved from the ribbon into the command - `CreateOperation`
      now refuses a kind whose family the document's machine cannot
      run, BY NAME (kind, family, and the mount that opens it), on one
      new data field `KindInfo.family` with the admits law
      `(_, MillTurn) | (Mill, Mill) | (Turn, Lathe)` and the
      unstated-machine-is-a-mill default; new motion domain
      `crates/motion/src/turning/`: `profile.rs` fits the turned
      silhouette about the M5.1-E axis and answers band radii of
      TRIANGLES, not vertices (the stubble law - a rung whose window
      holds no vertex would else leave a full-radius sheet standing
      above the plane), `face.rs` holds the CL point at
      `station + nose_radius` so the insert ball is tangent to the
      facing plane by construction, rungs at `stepdown <= 2r` (refused
      at the door past that), passes inward to the axis, rapids at
      `r_max + stepdown + nose`; new kind Facing (ordinal 18, wire tag
      `facing`, project v33 - a pure step-up: one params tag, one tool
      kind word, one serde-defaulted `nose_radius` riding `Tool` as
      the inert-field pattern's third member), no stepover authored
      because one full-radius pass per station misses none -
      RECOMMENDED.md, M5.2 A.
- ✅ **Profile roughing and finishing** (stage B, commit d6f10ad): the
      kinds that machine the silhouette itself. `offset.rs` rides the
      MAX OF CLAIMS - every boundary piece of the profile states a
      radius its distance-d neighbors must stand outside (parallel
      line raised by `d*sqrt(1+b^2)`, circle of radius d about every
      vertex within d of its station) and the offset curve is the
      maximum of those claims, so no-gouge is a theorem of the
      construction rather than a corner-case table, and step shadows
      and narrow grooves bridge exactly as a real nose rolls over
      them; `profile.rs` samples the turned silhouette itself - the
      envelope asked OF EDGES (a vertex-only sweep leaves a step
      face's wider endpoint invisible), bisection on the convex
      sagitta law, shoulder stations kept as their descending
      vertical pair in walk order; `prof.rs` is both rides: the
      finish ONE pass at d = nose (radial plunge at the window's
      foot, ascending station, radial slide-off), the rough the
      concentric ladder d_j = nose + leave + j*stepdown eaten
      OUTERMOST FIRST - facing's ridge law read radially, a stepdown
      past 2*nose leaves a shell nothing touches and is refused at
      the door; new kinds ProfileRough (ordinal 19) and
      ProfileFinish (20), wire v34 (pure step-up: two params tags),
      the envelope verdict judging both with `stock_to_leave` as its
      own threshold, so the wall the ladder leaves is the wall the
      verdict lets stand - RECOMMENDED.md, M5.2 B.
- ✅ **Grooving, parting and the revolved blade verdict** (stage C,
      commit e880d56): the kinds where a full-width blade, not a nose
      circle, owns the cut. `revolve.rs` judges them in the MERIDIAN -
      no swept sphere can speak a groove (the part is undercut BY
      DESIGN and the height-field law reads every correct floor as a
      phantom trench) - with the boxcar MIN of the covering tips as
      the cut C(s) against the silhouette's own chord polyline E(s):
      gouge where C < E, rest where C > E, silence inside a cutoff
      kerf, and a floor no band reached resting by the whole window
      shell so a missed pass never buys silence. The tangency promise
      rides the lattice's INTERVALS by a spelled-once HAIR (1e-9): a
      band edge ON a wall station is the flush groove wall, a graze
      is noise and cuts nothing - and the same hair snaps a
      noise-pushed rung to its floor, so feed counts cannot flip on a
      float's whim. `blade.rs` holds the three kernels: rough steps
      centers one width apart and ladders each from the window shell
      to `floor + leave`, finish plunges each center once to its
      band's floor, part drops one stepped ladder through the axis at
      the parting plane (the last feed standing ON the axis IS the
      severance); rapids ride the whole bar's widest radius because a
      mid-bar window still traverses standing shoulders. New kinds
      GrooveRough/GrooveFinish/Part (ordinals 21-23, wire v35 - a
      pure step-up carrying a serde-defaulted `width` on every tool
      row, inert except on `ToolKind::Groove`; the kerf's width IS
      the tool and rides the TOOL digest, so re-grinding the steel
      marks every op that rides it Stale through undo both ways), the
      catalog's GT blades, the sim's `Mode::Revolve` standing its
      XY-plane laws down to the meridian scan, and a wrong tool on a
      blade kind refused at generation by three names - op, tool, and
      what the tool is - RECOMMENDED.md, M5.2 C.
- ✅ **Threading on the revolved wave verdict** (stage D, commit 2647832):
      the kind where the insert's V, not a boxcar, is the footprint.
      `thread.rs` ladders one helical pass per rung from the blank's
      crest down to the modeled root, and the MODEL states the phase:
      the kernel scans the silhouette for the first contiguous valley
      inside the window and centers the run's first V on it, so a
      threading op is a promise over a MODELED thread (a revolved V
      section) - a smooth bar's silhouette is a flat crest with
      nothing to aim at, and is refused by name. Doors at the command:
      width fit (`insert_width < pitch - 2*tol` - a V wider than its
      spacing stands its flanks in the neighbouring turn), full-form
      reach (`half*cot + tol >= depth` - a short form leaves crests no
      pass can touch), depth against tolerance, through-axis depth
      ("that is a hole, and holes are boring work"), the run's turn
      count, and plumb axis (`LEAN_MAX` - one turn per pitch is a
      CIRCLE law; a tilted spindle is mill-turn work, M5.2 G). Each
      rung rides: radial plunge at the first center, half-turn arcs
      advancing `pitch/2` of station each, alternating point and
      antipode (even ends stand on the frame's meridian, so the wave's
      V lands where the geometry says it cut), radial retract at the
      last - clearance radius `r_max + tol` throughout, asserted by
      construction. The verdict is the `revolve` scan with the new
      WAVE clause: the floor at distance d along the axis from a
      center stands deeper by `cot(A/2) * d` - the flank slope itself
      - on the helix's own lattice, with the crest law on (a form
      floor above the blank measures nothing; the shell owns the
      crest) and zero leave. New kind Threading (ordinal 24, wire v36
      - a pure step-up: one params tag, one tool kind word; the V's
      width and included angle ride the TOOL digest as
      `ToolKind::Thread` - byte 8 - so re-ground steel marks every op
      that rides it Stale both ways, while the lead is WORK intent and
      rides the params), and the sim's rapid law (d) joined (b) and
      (c) in the Revolve stand-down - a lathe insert is a point in XY,
      so the mill's column law convicts every threading pull-out
      (which slides along the flanks of its own cut V); the lathe's
      rapid promise rides the kernel's clearance radius and the
      meridian scan, fixture steel still speaks every mode -
      RECOMMENDED.md, M5.2 D.
- ✅ **Chamfering, taper turning and the multi-pass ladder - the
      promised-face rides** (stage E, commit 9f38881): the contour
      kinds - ops that state a FACE and swear the model already
      has it. `contour.rs` rides the profile finish's own
      machinery: the nose's offset walk along the MODEL's skin
      inside the window, because the verdict measures against the
      model and a pass that chased an authored cone BELOW it would
      cut past the drawing and call it output. The authored
      numbers spend themselves at the door instead: `worst_dev`
      walks the silhouette's POLYLINE SEGMENTS (a straight wall
      carries no vertex sample in a short window and is no less a
      lie for it) strictly inside the window against the authored
      line - radius vs. station, the meridian's own reading - and
      a vertical segment (a face, the wall of a shoulder) deviates
      only as far as the line stands outside the radius interval it
      sweeps. The chamfer's angle states the slope's SIZE only:
      both readings (falling toward either window end) are tried
      and the door judges the honest one - the sign comes from the
      model, never the parameters; a 45-degree face's offset CL
      stands `nose*sqrt(2)` out radially (the intercept shift), and
      the max-of-claims law keeps the corner's radial plunge
      tangent to cone, cylinder and end disk at once. The taper's
      two diameters state the cone outright - equal ones are a
      legal straight skim of a cylinder. Multi-pass states no new
      surface at all: the rung ladder down to `nose + leave`, then
      the spring-pass rides, ride k of n at
      `d = nose + stock*(n-k)/n` - the handbook's half-the-spring
      law as a distance schedule, the last ride the profile itself,
      one ride the classic rough+finish pair; the cap (64) refuses
      a polishing sequence by name. New kinds Chamfering /
      TaperTurning / MultiPassTurning (ordinals 25-27, wire v37 - a
      pure step-up: three params tags, no new tool kind and no new
      key on any row: the rides stand on the nose circle every
      `turning` row already files, and the general tool digest
      marks them Stale on a regrind exactly as the profile kinds
      do). They are judged by `Mode::Envelope` because the scan's
      own law is the contour's - gouge is now facet-to-path
      distance truth (the column floor that answered it before
      cannot read a tilt, a wall or an under-pass honestly), rest
      is only ridges the pass promised to roll over, untouched
      columns are law and not this number - and the ride stands
      nose-center-still for it, so the windowed band the door
      measured is exactly the skin the verdict measures; "adaptive
      roughing (turning)" ships ABSORBED by the ladder (constant
      radial steps are constant load when the work spins; the
      engagement variability adaptive clearing exists to fix is a
      milling accident - RECOMMENDED.md, M5.2 E), form turning
      stayed open for the gauge verdict and landed with E2 below.

- ✅ **Form turning** (stage E2, commit 2d38617): the gauge whose
      edge is a curve - one radial plunge per MODELED valley, the
      crown centered on the model's own low-floor run and sunk to
      `tip = max(chord - sag)` over exactly the events the scan
      judges, so the floor `tip + R - sqrt(R^2 - d^2)` (new
      `Floor::Crown`, the wave's curved sibling) never dips under
      the model BY CONSTRUCTION, and a mismatched gauge stands
      shallow and convicts through `rest_mm`, never a silent Ok.
      The blank-crest clip (a lone crown sits INSIDE the bar; its
      own window's span_max is no blank) makes the rest speak.
      `ToolKind::Form` (byte 9 - crown rides `nose_radius`, band
      `width`, so the global nose row and the kind-scoped width row
      convict a re-grind from the tool side alone), new kind
      FormTurning (ordinal 28, wire v38 - the arrival law a seventh
      time: one kind word, one params tag, no new keys), doors by
      name: no-crown "is no arc at all", band past reach, smooth
      bar "nothing to form", on-axis floor "that is a hole", and
      the app's family door - RECOMMENDED.md, M5.2 E2. The crown
      verdict's own judge, the sim envelope, gains the fourth
      configuration of its facet-distance law (segment pierces
      face): a path that buries its centers now reads the
      saturation `r` instead of the crossed wall's edge distance -
      E1 fallout found because E2 ran the binary E1's Gate map
      never named; RECOMMENDED.md records the law and the gate-map
      lesson.

- ✅ **Turning boring** (stage F, commit edf562c): the inside twin
      of the profile ride - the nose walking a hole's wall, judged
      by the same envelope law stage E gave the OD rides (`Mode::
      Envelope`, `ToolGeom::nose_radius` speaking for `Bore`): a
      center curve at distance >= d BELOW every piece of the hole's
      inner wall (the min-of-claims twin `profile_offset_in` -
      stage B's theorem read through the mirror) is a bar whose
      swept ball never enters the steel. What is new is what the
      wall IS - the silhouette's edge sweep folded with MIN - and
      the doors that ask the MESH (three-angle Möller–Trumbore
      parity vote from the axis point, Ericson ch. 5) whether a
      hole stands open at all, at every window station and at the
      ball's own axial reach: a plain bar rides tangent IN steel
      and the facet verdict measures nothing wrong there, so the
      silent-tangency failure this stage could ship is refused by
      name before a path exists. The axis-vertex filter's vacuity
      proof, the obstacle-named reach suggestions, and BR-1000's
      catalog holder (a lathe bar's steel lies radial - the turret
      IS the holder, like every tn row) are RECOMMENDED.md, M5.2 F.
      New kind BoringTurning (ordinal 29, wire v39 - the arrival
      law an eighth time; the global nose row already convicts a
      re-ground bar through the TOOL digest), one ascending ride in
      v1, the kernel shipping split at its seam as `turning/bore/
      {mod,doors,wall}.rs`.

- ✅ **Radial (cross) drilling + hole recognition on the turned
      solid** (stage F2, commit 32d3f27): the live tooling's cycle -
      drill, peck, deep-retract march - plunged from a turret station
      along X or Y at the spindle axis, its poses riding `cl`'s
      axis machinery (tag = tip->shank = the negated plunge, so the
      station hop exits the way the tool entered), its rapids proven
      outside the silhouette by inflation algebra (the 90-degree
      traverse chord keeps `hop_s * sqrt(2)` of clearance, never a
      sampled guess). Verification speaks a new mode: `Mode::Radial`
      stands laws (a),(b),(c),(c3),(d) down - a height field cannot
      see a tunnel, and the kernel's construction is the proof the
      Revolve precedent already owns - while the fixture laws keep
      their 3-D voice. Recognition grows the cylinder primitive:
      three candidate axes, dihedral-gated edge weld, the Kasa fit,
      and the Z pipeline's every law read through the candidate's
      own coordinates (a hole is a closed round wall wherever its
      axis points); doors vote three-angle ray parity. Mixed
      lists, a mill asked to drive live tooling, and a Bottom plane
      over radial holes all refuse by name; the wire grows an
      optional `cross` key (v40) with pure-Z files byte-identical.
      The slicer-versus-ray closed-mesh law and the port-verbatim
      lesson: RECOMMENDED.md, M5.2 F2.

### M5.3 - Performance & Automation (Release 0.4.0)
**Goal: big models, headless jobs, scripting.**

- ✅ **Large models**: streaming mesh import and a tiled sparse stock grid (stage E); the GPU-resident SDF road was dropped for CPU determinism (see RECOMMENDED.md, M5.3 E)
- **Generation performance**: threaded toolpath generation, incremental recompute after model changes
- **Scripting API**: Typescript bindings for batch ops - generate, verify, post, report
- **Part handling**: loading, unloading, and transfer choreography as
      post-emittable motion intents (the catalog's part-handling block;
      orientation and clamping are already M3.2's setups and fixtures) -
      M5.2's bar feeders get their general form here
- **In-process inspection**: probe-style measurement hooks after ops,
      feeding the deviation verdict a re-run decision, not a report tail
- ✅ **CLI**: headless project processing and regression runs (stage G)
- ✅ **Regression suite**: golden-model CI (import → generate → verify → post) with numerical tolerances (stage G)

- ✅ **Incremental recompute after model changes** (stage A, commit
      d11b1e1): the per-frame staleness wall (a facets clone, a full
      content hash and a face walk per visible operation) becomes an
      entity-scoped `GeoSig {rev, sig}` memo keyed by M3.1's revision
      counter - and the counter is now enforceable where it matters,
      because `set_facets` (the one `Facets` writer) moves it with every
      landing; six sites used to land geometry without counting, which
      is why the counter had been demoted to bookkeeping. Restore
      commands pin a past revision paired with the exact bytes it
      carried (the command snapshots), the loader's pin became
      unconditional (a pre-v20 file still reads rev 0), and a hit is
      gated on the facets still being mounted - a stored fresh, never a
      stored stale: the stamp's law (compare against live facets) is
      untouched, the memo only says when re-deriving is owed. The last
      M4.3 residues went with it (`hsmw-signature-v1` ->
      `camcraft-signature-v2`, one-time honest Stale per precedent; six
      test temp names); every .nc/.cl golden byte-identical.
      RECOMMENDED.md, M5.3 A.

- ✅ **Threaded toolpath generation** (stage B, commit ab3809d):
      generation gets its world-free seam - `collect_job` reads the
      world once exactly as `generate` always has and packs the answers
      into a `Send` `GenJob` of plain owned data (`&World` is `!Send`,
      so the type system is the law: a job that compiles cannot
      smuggle a live read onto a worker thread); `run_job` builds the
      verified path without ever naming a `World`, and
      `generate = run_job(collect_job(..)?)` keeps the one-op road
      byte-for-byte the old behavior. `RegenerateAll` stages stale ops
      serially - error precedence rides positionally, stored `Result`s
      taken at each arm's exact statement (`GenError` is not `Clone`) -
      then finishes on rayon with an ordered `collect`, parts in tree
      order. Constructors only read, so a batch's world is frozen and
      parallelism cannot change what "before" means: tested as a law,
      `threaded_batch_matches_the_serial_loop_bit_for_bit` (paths,
      warnings, stamps equal both roads). Re-machining masks ride as
      `Arc<PriorState>` (a dexel field borrowed, never cloned);
      `region_for_src` retires into the seam. Kernels untouched - every
      .nc and .cl golden byte-identical. RECOMMENDED.md, M5.3 B.

- ✅ **Part handling: the transfer choreography** (stage C, commit
      7e81920): M5.2 G.2's feeder signals get their general form -
      `Record::Transfer { from, to }` joins the lifecycle family as a
      SIBLING arm (every existing Part site byte-unchanged), the
      stream word `PART TRANSFER <from> <to>` carrying 1-based station
      ordinals in the order the machine file states its spindles, a
      missing or zero operand refused by name (an ordinal is not a
      length - the micron lattice law is not its domain). The engine
      voices it through `onPartTransfer` with `From`/`To` properties -
      `onCommand` never sees a lifecycle record - and the PRELUDE
      default is the comment `(PART TRANSFER 1->2)`: the engine
      invents no M-code it was not handed. `Dump.part_transfer` puts
      the handoff after the last cut and before the unload -
      LOAD..cut..TRANSFER..UNLOAD - and the flag off writes nothing,
      the 13 .nc goldens byte-identical again. Capability is the
      program gate's question, never the post's: the checker counts
      spindle-stating head parts of the `.mch` tree into
      `Machine.spindles` (unstated is `None`, never a guessed 1), it
      mirrors to scripts as `head.spindleCount` under the same name
      through all three machine doors, and `check_document` refuses a
      transfer on a machine stating fewer than two spindles - naming
      the op, the machine and the count. The flag rides the stamp like
      every post-visible signal. RECOMMENDED.md, M5.3 C.

- ✅ **In-process inspection: the ask on the stream, the verdict at the
      post** (stage D, commit 4cb29c6): `inspect_tol` joins the Part op's
      signal family - a `PROBE <um>` intent on the stream (the number IS
      the switch, silence is free, the protocol the probe speaks staying
      the script's dialect through `onProbe` and its `Tolerance`
      property) - and the re-run decision lives where the program's
      truth is: the post's verification gate. The part is what the
      program's blades cut TOGETHER, so `operation::probe_problems`
      composes every blade leg of the asked part (Part, GrooveRough,
      GrooveFinish, the stock-sourced ones pinned through their setup)
      into one meridian scan - `revolve::blade_program_warnings`, the
      M5.2 C boxcar-min law run across the PROGRAM - leaves zero BY
      CONSTRUCTION (a roughing promise is what the PROMISE forgives,
      never the part), and measures under the strictest authored ask. A
      rough that posts silently on its promise refuses by named asker,
      named leg, named move and millimetres the moment its answering
      finish never comes; the same leave posts once the finish answers
      for it - the tests' strongest pair. A composed part the judge
      cannot frame, or an ask split across two parts, refuses rather
      than reporting silence; the door floors a nonzero ask at 0.1 um,
      where the world's best machines measure. `BandMove.half` carries
      each move's blade width so the single-tool scans keep their exact
      verdicts and the composed one lets every leg keep its own. No
      cache field, no schema change: the verdict is a derived read, like
      the gouge and fixture lines. RECOMMENDED.md, M5.3 D.

- ✅ **Large models: the shared mesh and the sparse grid** (stage E):
      the triangle loses its dead stored
      normal (96 -> 72 bytes, the `normal()` method recomputing what
      every consumer already read from the winding), facets ride
      `Arc<[Triangle]>` so a model is stored once and every system
      borrows the count, and the STL door streams - an 8 MiB window,
      the binary/ASCII sniff a file-size law (`84 + 50 * count`), both
      flavors through one per-facet acceptance machine, so a 10M-facet
      file never coexists with its triangles: measured 517 ms to write
      500 MB and 3.98 s to stream it back IN DEBUG. The stock grid
      gets tiles: a 64x64 tile that was never cut is not stored at all
      (its value IS `stock_z`), a never-varying tile is one number,
      and only tiles a cut reached pay a box - a fresh 2000-squared
      grid costs zero allocations where the dense `Vec` cost 32 MiB,
      model stock collapses its air and full-height runs the same
      way, and every verdict (`top_at`, the equality, the masks) reads
      values, never bytes, so the representation is private and the
      answers are bit-identical. The GPU-resident SDF reading of the
      roadmap line died on the determinism law (RECOMMENDED.md, M5.3 E
      - the color map re-reads this field every frame and a driver is
      not a reproducible witness). 30M-facet import ceiling, 4 GiB
      file door; all 13 .nc goldens byte-identical.

- ✅ **Scripting API: the job-script host** (stage F, commit eec07c2):
      the batch lane's door - `app::script` runs a JS job against a
      fresh world, and every `cam` verb (load, save, import,
      operations, generate, verify, post, report, check, result, log)
      rides a real command through a real History, so the Prime
      Directive keeps one shape everywhere and the law is MEASURED: a
      script's post is the app's post byte for byte. One engine - the
      workspace-pinned boa the post already runs, nothing adapted from
      any vendor's scripting surface - with one host door
      (`__cam_call(name, argsJson)`) crossing as JSON text both ways,
      because boa's closures cannot soundly capture an `Env`: the
      bridge carries one raw pointer, valid by ownership order. The
      `cam` object AND `cam.d.ts` are generated from the same VERBS
      table, and a drift test refuses a typed surface the runtime
      does not back. What a refusal looks like: a catchable Error
      whose text names the reason, recorded in the run's errors even
      when swallowed - a job that caught its failures is not a clean
      job. `verify` is a question, never a refusal: while any
      operation is stale the program gate SKIPS and says why, because
      a stale verdict about metal is worse than none (the sandbox
      shows no console, no fs, no fetch). RECOMMENDED.md, M5.3 F.

- ✅ **CLI + golden-model regression suite** (stage G, this commit):
      the batch lane gets its door - `camcraft-cli` is thin over the
      scripting host, so every run is a `script::run` someone could
      have typed: `script <job.js>` runs a job, `process <project.camproj>
      [-o out.nc] [--cps name|file]` runs a generated one (load,
      generate, post, verify) and speaks the verdict as the exit code -
      0 stood, 1 a verb or the verify report refused, 2 the command
      line was wrong; logs and the parked answer on stdout, refusals on
      stderr, judged by real process spawns (`tests/cli.rs`, 7). The
      regression lane (`tests/regression_models.rs`) JUDGES NUMBERS
      where the .nc goldens judge bytes: import a model, stock it, face
      it, generate, run the program gate, post - and measure facets,
      moves, millimetres, minutes and lines against goldens with a
      micron floor on the floats and exact equality on the counts. Two
      fixtures speak: a wavy slab generated in-code as BINARY STL (its
      f32 read is the only quantizer, so every platform sees the same
      vertices; 386 facets, 1342 cut mm, 1.4677 min, 54 lines) and the
      committed `cube.stl` M1.1's acceptance has read since day one
      (12 facets, 12 cut mm, 16 lines). Both lanes ride the CI Golden
      step, Windows and the parity job alike. The gouge gate earned
      its keep on the first draft: a face plane below the model's own
      top refuses, and the fixtures went up to skim, not the gate down.
      RECOMMENDED.md, M5.3 G.

---

## Phase 6 — UI refactoring.

The UI is based on EGUI. And it has a dark theme that is hard coded into the application. That is a not something users want
2 defualt themes should exist, light and dark for one. But also the ability to change ribbon size, input fields, button size
should all be exposed thru the theme properly. The 2 hard codded themes can for sure just exist in code, but the "alteration"
should be read and loaded din realtime from a a "themes" folder.

### M6.1 - Adopt the sibling repo's theme architecture
**Goal: bring our egui UI up to the theme setup proven out in solidcraft
(`~/Projects/solidcraft`, `crates/ui-egui`) — one token table that drives both
widgets and painted chrome, loadable from JSON at runtime. Same engine as ours
(egui + egui-wgpu over wgpu), so the shape of the code ports; the values and
assets are ours.**

Solidcraft's reference design, read from `crates/ui-egui/src/theme.rs` (1030
LOC) and `theme_files.rs` (898 LOC): a single `Tokens` struct is the truth for
colour, radii, spacing, fonts and control sizes; `Visuals` are *derived* from
it, not authored; a theme file names a base palette and overrides individual
tokens by name, so a custom theme is a handful of lines. Their shipped set is
~20 JSON themes (gruvbox, nord, solarized, tokyo-night, high-contrast, an
`amber-phosphor` CRT, `solidcraft-95`) under `assets/styles/ui/themes/`,
`include_str!`-compiled for defaults and read from disk for user themes.

- **One Tokens struct, two render paths**: replace the 14-field `Palette` in
      `crates/ui/src/egui_app/theme.rs` with a named-token table (ink/well/
      accent/text tiers, borders, field states, selection — then extended with
      the geometric tokens below). `visuals()` becomes `fn visuals(&Tokens) ->
      Visuals` derived from the table; `painted.rs` and `shell.rs` read the
      *same* tokens by name. Their structural win is the exhaustiveness
      pattern: their apply walks every token field in one destructuring arm, so
      adding a colour without wiring it fails the build instead of silently
      rendering default-egui grey. We have the identical drift risk in our
      painted path — adopt their guard.
- **Themes as files, not code**: a versioned format (`camcraft-theme/1`) with a
      `descriptor` (name, creator, description, base `dark`/`light`), a `colors`
      map keyed by token name (`#rrggbb`/`#rrggbbaa`, or an array for ramps),
      and the size/shape knobs. No file paths inside a theme — icons stay in
      `assets/ui/` by convention (M6.1's rule, kept). Hostile-input caps from
      day one: byte budget, colour count, name length, custom-theme count —
      corrupt or unknown-token themes refuse with a named diagnostic, never
      crash, never half-apply (the Prime Directive applies to themes too).
      Unknown keys are reported, not ignored silently.
- **Compiled-in defaults + disk overlay**: `include_str!` the light and dark
      defaults so a binary with no assets directory still themes correctly
      (M6.1), and read `assets/themes/*.json` at runtime for the rest, listing
      them for the switcher. Cache parsed themes per path so the realtime swap
      costs nothing per frame.
- **Sizes exposed through the theme**: the Phase-6 complaint was that ribbon
      size, input fields and button size weren't themeable. Make them tokens —
      corner radii (`radius_sm`…`xl`, `control_radius`), item/window/button
      spacing, `control_height`, and the ribbon/toolbar glyph size — each
      overridable in the theme file, with per-widget clamping to their own
      bounds so a 400-point radius can't blow out a checkbox. Their `radii`
      block is the model; we extend it with spacing and heights.
- **Theme switcher in the top-right**: dropdown listing defaults + discovered
      files, applying live on the next frame. Selection rides `ActivateSetup`-
      style command law (AGENTS.md: everything is a command) — it must be
      undoable and persist in the project, not be an ad-hoc field poke.
- **Derived contrast, kept honest**: they generate hover/active steps and
      text-on-fill choices from the accent rather than trusting hand-typed
      values. Adopt the derivation for the states we don't spell out, and pair
      it with an accessible-contrast check in the light theme's golden — a
      theme that can't be read is a visible refusal, not a style preference.
      Their `ColorVision` layer (protan/deutan/tritan remaps applied on top of
      any theme) is a cheap follow-on once the token table exists.
- **Both themes land in this milestone, not as an experiment**: the dark values
      carry over from today's `Palette::DARK` so existing snapshots stay
      meaningful; the light theme ships alongside as a second golden set.

Provenance: architecture studied from a sibling project's own source, at the
user's direction; no HSMWorks material involved. All numbers, colours, icons,
and asset names are ours, drawn from public machining-UI convention.

### M6.2 - The structured ribbon, redux - a home for every operation
**Goal: M1.4.3's promise, kept late - every kind the registry holds has a
ribbon home, and the ribbon's shape says which family it belongs to.** — ✅

The honest state, verified at the M5.1 close-out (commit caaab7a): the
registry carries 18 milling kinds, but the ribbon's Create row is eight
cells (panes.rs, `Cmd::Create(0..=7)` - the imgui-era Face-through-3D-Finish
set). The ten M5.x kinds (5-axis contour/parallel, swarf and advanced
swarf, rotary contour/parallel/offset, multi-axis rough, flowline, the
drive-curve contour) have no cell at all: they are reachable only by
creating some operation and switching its Type in the page's flat
18-row combo. Nothing is grouped - no family labels anywhere in the
UI, and `KindInfo` carries no family field, though the Operation
Catalog at the head of this roadmap has always known the taxonomy.
M1.4.3's home law ("an operation with no ribbon home is a bug, not a
backlog item", landing before the M5.1 ops) was overtaken: M5.1 landed
first. This milestone is that debt, paid with the M5.2 turning kinds
arriving behind it.

- **Sections by family**: the ribbon divides into labeled sections -
      Milling 2D, Milling 3D, Drilling, Multi-axis, Turning - mirroring
      the catalog; a `Family` field on `KindInfo` is the registry-side
      seed (the same table the tree, sheet and combo quote - no second
      truth).
- **Machine-kind gating, made real**: M3.2 stage E darkens every Create
      cell on a lathe document and M5.2 stage A moves the refusal into
      the command; the ribbon then HIDES what the machine cannot run
      (a lathe setup never shows waterline; a mill-turn shows both
      families) instead of darkening eighteen cells.
- **Overflow dropdowns**: kinds beyond a section's width fold into a
      dropdown carrying the section's label - never an iconless
      chevron. The 5.x set will not fit one row; the law is reach, not
      row count. (Rebuilt at the user's hand-test verdict: the
      width-driven fold became the structural fold - every family
      behind one labeled door at any width; see the M6.2 rebuild entry
      in RECOMMENDED.md.)
- **Pin to ribbon**: any catalog item can be pinned/unpinned by the
      user; pins are a document setting persisted with the project
      (like panel docking), not an undoable command - undo replays
      cuts, not layout. (Budgeted by the rebuild: at most four pins
      show, and a pinned kind leaves its section door - one home per
      kind on the bar.)
- **The machine door** (added by the rebuild): one cell in the Setup
      group switching Milling / Turning / Mill-turning by mounting
      that kind's reference machine as a `LoadBuiltinMachine` command,
      so the switch rides the undo history and the ribbon's family
      filter follows the mounted machine - the mode IS the machine.
- **The combo earns the same shape**: the operations page's type picker
      groups by family the way the ribbon does - one taxonomy, two
      surfaces.

No new glyphs are needed (every shipped kind carries its original icon
already); the work is the registry field, the ribbon's group model, the
overflow control, and the gates M1.4.3 specified. Test law: the existing
tips/enablement tables (`tips_match_the_imgui_table` and kin) move with
the new shape deliberately, with the verbatim-imgui promise retired in
RECOMMENDED.md the day this lands.

---

## Phase 8 - Cutting order: the generator review & refactor.

**Start order: this phase runs as soon as Phase 6's UI milestones are out
of the way - before Phase 7.** The SDF engine changes what the sim
*knows*; it changes nothing about the order in which a tool reaches
metal. An entry that dives straight into a finished wall is wrong in
both stock engines, and fixing it must not wait on either.

The founding findings, verified at the phase's opening in the shipped
code (`crates/motion`):

- **The chosen entry family never meets the first level.** Both pocket
  ladders apply `Plunge`/`Ramp`/`Helix` only at LEVEL CROSSINGS:
  `machining2d/levels.rs` gates the arms on `crossing = !first &&
  last_cut_z != Some(z)`, and `machining3d/ladder.rs` gates them on
  `li > 0`. The first dive into uncut stock always takes the fallback
  arm - rapid over `passes[0]`'s start and a straight axial plunge at
  `plunge_feed` - whatever the user selected. A "helix entry" pocket
  enters material with a plunge.
- **And the plunge lands on the wall.** A Waterline/`Adaptive` plan's
  `passes[0]` is the OUTERMOST ring (`pocket.rs::waterline_passes`:
  ring k is the area eroded by `k * stepover`), so an adaptive pocket
  of a round hole starts by diving full-depth at the finished wall -
  100 % wall engagement on the surface the operation exists to leave
  clean - then marches rings toward the center, leaving the core
  attached until last. Backwards order for roughing.
- **The helix that does run is in-plane at the places it matters.**
  `entry::helix_down` is a true Z-descending dive (it spirals one
  stepdown between planes), but the ladders place it with
  `entry::dive_point`, which picks the plan's longest chord - any
  consecutive point pair, line or arc segment alike - whose midpoint
  sits inside BOTH leg regions eroded by the helix radius. There is no
  straightness test; containment is what bites. An all-arcs ring plan -
  exactly what a round pocket tiles to - has its chords ON the ring
  boundary, so every midpoint lies inside the cut, never in the eroded
  clear regions; the dive resolves `None` and the arm falls back to
  climb-and-plunge. On round geometry the helix entry rarely engages at
  all, silently.
- **`Adaptive` is rings, not trochoids.** The strategy spaces
  waterline rings at the engagement-law step (M2.1c's sagitta); the
  bound holds only each ring's own wall bite. The name promises HEM;
  the pass order delivers offset rings.

The phase is a VISIT, not a rewrite: every generator in the registry,
one by one, asked the same three questions - (1) how does the tool
FIRST engage uncut material, and does that answer honor the user's
entry choice? (2) in what ORDER does the plan traverse the material,
and is that the right order for the op's job (rough: air and floor
before walls, engagement bounded everywhere; finish: entry at the
least-harmful tangent, never a dive on the finished face)? (3) what do
the LINK moves between passes do, and can any of them cross live
material or a planned pass? Each visit ends in a RECOMMENDED.md entry
stating the order of operations (rapid → approach → entry → cut →
link → retract) as found, verdict right/wrong, and if wrong the fix
with its goldens regenerated only for the genuinely-changed order and
the heavy envelope/verify lanes green as the referee.

Provenance: entry motions from the public G-code literature (linear
ramp and helical interpolation, RS-274 canned cycles - their exact
controller-side semantics per NISTIR 6556, "The NIST RS274NGC
Interpreter - Version 3" (Kramer/Proctor/Messina 2000), ingested at
`raw/nist_823374.pdf`: the reference both for what posts and for which
pass orders a controller will execute as planned); pass-order and
engagement claims from published trochoidal/HEM computations - the
same sources `entry.rs` already names. No HSMWorks material.

### M8.1 - The roughing ladders: entry into metal first, then pass order
**Goal: the entry family the user chose is what first engages the
stock, and a roughing plan reaches the middle without biting the
wall it must leave for the finisher.** — ✅ (M8.1 code cc5d89d;
`dive_room` erosion-by-distance, inside-out roughing rings,
substitution notes on the status line; level-0 family obedience in
both ladders became TRUE only at the fix 1edb0c5 — the gate round
verified counts, not shapes; ledger in RECOMMENDED.md)

- **Level 0 obeys the family**: `Helix` dives in from the safe plane
  along a true Z-descending helix whose landing spot has room for the
  widened disc across the WHOLE depth it crosses (a center-eroded
  plan's own floor, not a straight-run search), `Ramp` takes its
  diagonal from above; only `Plunge` may dive axially, and then at a
  legal interior point, not on `passes[0]` on the wall. The `li > 0`
  gates become proofs, not excuses: legality at the first level is
  "the column the disc sweeps is inside the cutter-center area", the
  crossing arms' superset law generalized to the surface dive.
- **Roughing order, stated per strategy**: wall-ring-first stays for
  FINISH waterline (it is the honest shape there); the roughing
  branches get interior-out or a continuous inward spiral with an
  earned wall pass last. Zigzag keeps its rows; its first dive obeys
  the same entry law.
- **`Adaptive` earns its name or says so**: either trochoidal wall
  following per the engagement law's own literature, or the label and
  the panel's promise amended to "constant-engagement rings" - the
  RECOMMENDED entry picks one and the UI quotes it.
- Silent fallbacks (dive `None` → plunge) become diagnostics: the
  status line names the substitution - Prime Directive, honest-output
  edition.

### M8.2 - The finishing and planar families
**Goal: the same three questions answered for every finishing and 2.5D
kind, with M1.3's open debt (spiral/helical entries) swept at the
contour side.** — ✅ (M8.2 code 95d50b3 bda85c1 6d2c424; contour's Z
entry rides the family certified against the destination's air, the
wall refuses by name (A); tangent-ring entries re-proved vertical,
ramped first contact opt-in with refusal notes (B); slot certified on
the closed-form convex stadium, arms stood down where a tab is metal,
the hop rides a tall tab's top, trace's plunge theorem stated (C);
heavy lane green at 6d2c424; ledger in RECOMMENDED.md)

- finishing3d waterline/parallel/polar/flow: the tangent-ring entry
  ("a vertical plunge at a tangent-ring entry never cuts above the
  ring's level", `finishing3d/mod.rs`) gets re-proved on steep parts
  and gains a ramped first-contact option where the tangent is not
  gentle; ring-to-ring links keep their floor-hop law.
- 2D contour/slot/trace/chamfer/face: in-plane tangential leads exist
  (`contour.rs` `Lead::Arc` law); the Z entry they ride - plunge at
  the lead's wall point - is the M1.3 open item: a helical or ramped
  entry to depth at a legal spot replaces the dive where the tool
  enters on a wall, and `Lead::None` stays the machine-simple default.
- Tabs/bridges and hop planes: order checked so no link crosses a
  standing tab's air; the verifier's rapid-inside-footprint rule
  quoted per family.

### M8.3 - Drilling, multi-axis, turning
**Goal: the canned, posed and revolved families answer the same
questions; cycle order and approach order get stated, not assumed.** — 📋

- drilling cycles: spot → peck → bottom → retract orders audited
  against the G73/G83 semantics already posted (`drilling/cycle.rs`;
  the cycle word semantics are NISTIR 6556's, `raw/nist_823374.pdf`),
  the R-plane law per cycle, and hole-recognition defaults checked
  against the entry family question (a drilled hole needs no helix; a
  bored one may - helical bore entries join this visit).
- multi-axis: the posed ladder's first engagement is a dive AT THE
  POSED AXIS - what the cone dodge leaves standing at entry, and the
  wrap/rotary families' radial entries, get the same verdict lines;
  lead/lag law restated as this family's entry answer.
- turning: profile approaches from the OD obey promised-face; boring
  enters through the pre-existing hole or a bored pilot, never
  through the core face - the inside-out law (M5.2 F) re-asked as an
  ORDER question: rough OD → face → drill/bore → finish, per op,
  stated in the catalog rows.
- Close-out: the whole registry table annotated with its order-of-
  operations verdict; the heavy lane runs every family's goldens;
  tag with the next release (0.5.0, with the Phase 6 UI items in it).
