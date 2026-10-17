# M1.4.1 spike — Dear ImGui → egui: go/no-go report

Date: 2026-10-17. Branch `ui-egui-spike` @ `644713a`; restore point: tag
`pre-egui-spike` → main @ `b279780`. Verdict: **GO**.

Four steps, each one commit with the full gate ladder green at every step
(app/ui tests, `clippy -D warnings`, imgui headless rc=0, spike headless
rc=0, `graphify update` + label continuity, windowed smoke):

| step | sha | what it proved |
|---|---|---|
| S0 pins | `8219f62` | the egui set (0.36.2 line, egui_dock 0.21.1) coexists with wgpu 30.0.1 / winit 0.30.13: `cargo tree -d` shows one egui, one wgpu, one winit — zero forced upgrades |
| S1 shell | `482a54b` | egui-winit + egui-wgpu Renderer over our own winit 0.30 loop; View3D's offscreen texture docked as an egui native texture, rebound on resize; orbit/pan/zoom through egui pointer responses; headless readback ≥5% changed px |
| S2 chrome+page | `1930f76` | the Contour property page bound to the same commands as imgui; painted ribbon + browser (the draw_list port); Fusion-caliber dark theme module; PNG dumps at 100%/150% put the text question to human eyes |
| S3 harness | `644713a` | egui_kittest (0.36.2, features wgpu+snapshot — 0.36's default set is empty) runs the SAME `shell_layout` the window runs: 8-frame run, pointer-event command round trip (checkbox → Apply → `EditOperationParams` → world), headless exit-contract port, and a committed snapshot regression — stronger than the imgui harness it replaces (which executes two frame tests, now ported) |

## Screenshots vs the hand-test references (01–04)

The spike's 1440×810 dumps (`spike-shots/egui_100.png`, `egui_150.png`,
first frame of a real windowed run, View3D readback pasted into the
viewport tab) against the HSMWorks reference shots that drove M1.4.1:

- **01 Machine Setup** — HSMWorks is a light-theme tabbed dialog. The
  spike's visible law matches its *structure*, not its colors: section
  headers with collapse triangles, right-column fields, combo + chevron,
  checkbox rows. The color call is the opposite direction and deliberate:
  M1.4's verdict asked for a modern DARK CAD shell; the theme module is
  ours (tokens in `theme.rs`), and egui draws it consistently at 100% and
  150% (the DirectWrite worry from the investigation: at 150% the dump is
  crisp at every size we tested — egui's CPU glyph atlas rasterizes
  per-pixel-ratio, no blur, no hinting artifacts).
- **02a Face operation** — the Geometry chip row ("Face Block · z=30.00 ·
  3600.0 mm² ×" + Clear) reproduces the stock-contours chip list
  idiom directly, in our own strings. The dialog's icon-tab strip is the
  M1.4 carry-over (tabbed property panels) and is planned as migration
  work on the new stack, not spike scope.
- **03 Heights settings** — the reference's stacked From-combo / Offset
  rows are exactly what the spike's `height_editor` does, and what
  M1.5.4-C restated for imgui; egui enforces the two-row law by
  construction (no `same_line` overflow class exists). The `(= Z 57.000)`
  echo is our own text, resolving through the app layer like imgui's.
- **04 Contour selection** — Geometry select + chips + Tabs/Rest-style
  checkboxes live on the spike's contour page; the viewport-side picking
  it depends on is a named divergence (below), scheduled as migration
  step M-e.

Text crispness at the OS reference scales: the open risk from the
investigation, judged from the dumps, is **cleared**.

## Memory soak

Windowed spike (`hsmworks-ui-egui`), first-frame GPU init + View3D
registration + scene upload behind it, `ControlFlow::Poll` for the whole
window, one panel per frame (the same settle + shell_layout path the
harness runs), sampled every 20 s for ~7 min: working set 240.0 → 240.6 MB
across 22 samples, 1.2 MB peak-to-peak with no trend — flat after the
warmup plateau, no per-frame growth; the process was then killed (exit
confirmed). The sampler's private-bytes counter read 0.0 (wrong property,
not a zeroed process), so working set is the only number claimed here.
Texture-delta discipline is the soak's real subject: S1's caught bug was
the `TexturesDelta` drop-assert, and the drain law (`drain_textures`,
consume-not-peek) plus the digest-gated re-upload (`scene_digest`) are the
two leak-shaped places. Both are in the frame tests now (8 frames +
snapshot), and the kittest snapshot path exercises the CPU-texture side
every CI run.

## Divergences and known gaps (honest list)

1. **Painted strips** (ribbon, status, tree rows) — by design: the draw
   list port is the spike's question. Verdict from the dumps: alignment
   and hit-targets hold at 100% and 150%. One cosmetic defect found in
   the final shot: on the Contour1 tree row the trailing eye button
   overlaps the "40 warn" text. imgui's browser column model (eye BEFORE
   the glyph+name) doesn't have this class; migration step M-b ports that
   column model and the defect goes with it.
2. **DragValue replaces InputDouble** (D5) — egui has no InputDouble.
   Values, commit-on-Enter/defocus semantics and step speed all map;
   typed-entry parity is a migration-step test item, not an unknown.
3. **No viewport picking in the spike** — feature pick, the WCS anchor
   grid and the overlay handles stay imgui-side until M-e moves them. The
   raycast, hit-test and camera code is app-layer (imgui-free), and the
   spike proved the pointer plumbing it needs (`viewport_points`, egui
   pointer responses → orbit/pan/zoom).
4. **Dialogs** — `rfd` is toolkit-agnostic (its own thread/COM path); no
   spike proof needed, none claimed. It rides M-d.
5. **egui_dock is single-maintainer** and trails egui by a minor (0.21 ↔
   0.36 — verified again at S3 when `egui_kittest`'s empty default-features
   set was found the hard way). Our layout is code-built and ini
   persistence is off, so a lag means holding pins or vendoring ~7.8k
   LOC (measured) — a real cost, but bounded and MIT, and the same
   docs-and-comments-heavy size as our own ui crate.
6. **Focus/IME**: Latin-first is fine on egui 0.36's winit IME path; a
   CJK pass is future hand-test, flagged not settled.

## Dependency cost

egui + egui-winit + egui-wgpu + egui_dock (all `=0.36.2`/`=0.21.1`,
MIT OR Apache-2.0, notices in `assets/ui/LICENSES.md`), dev-only
`egui_kittest`. Zero upgrades to the existing pins. Spike code: 3,805
lines in 7 files (`crates/ui/src/bin/egui_spike*`), all additive — the
imgui app was never touched and both headless gates stayed green at every
step.

## Migration order (GO plan)

Both stacks coexist as entries until the last panel moves — the spike bin
graduates into `crates/ui`'s `egui` module and the imgui binary stays the
default `cargo run` until M-g retires it. Every step: whole panels only
(no half-ported page), parity tests + snapshot, the golden CLFile/G-code
gate untouched, full ladder green, one commit.

- **M-a shell graduation** (~1 step): the egui loop + GPU init + dock +
  View3D-texture law moves from spike bin into the lib as the `--ui egui`
  entry; `test_harness` learns to spawn it; undo/redo keys, FrameAll,
  shortcuts live. Stub panes (empty ribbon/browser/properties) — the app
  is usable for viewport work immediately.
- **M-b ribbon + browser**: the painted strips graduate (they're written
  already); tree column model per browser.rs (kills divergence-1's
  overlap); eye/leaf toggles, ops rows, warnings chips.
- **M-c property pages**: operations.rs's five 2D pages + heights +
  warning fold (the spike proved the Contour one and the height law);
  then setup.rs (machine/stock/WCS/origin pages). Pages arrive already
  tabbed (geometry / heights / passes / leads and links / feeds) — that
  is the M1.4 "panel UX overhaul" carry-over riding this step, per the
  reference dialogs (02a/04 icon-tab strips).
- **M-d dialogs + file ops**: rfd wiring, new/open/save, save-as,
  status/error surfaces (state.rs `error_for` display side).
- **M-e viewport interaction**: pick + hover chips + anchor grid +
  overlay handles through egui pointer events against the app-layer
  raycast (the imgui `viewport_rect` law restated); simulation playback
  scrub if still app-side.
- **M-f actions + paths panels**: actions.rs port; post/report surfaces.
- **M-g retire imgui**: drop the three dear-imgui deps + dockbuilder
  config, LICENSES.md notes, icons module folds into egui textures,
  full `/graphify .` rebuild, ROADMAP M1.4.1 ✅. Carry-overs land after:
  tabbed property panels and the graphical tool editor window ride the
  egui stack as M1.4.2+ (AGENTS layering unchanged throughout: ui → app
  → flecs, commands, schema, persistence and every generator untouched —
  crates/app is 100% imgui-free, re-verified at each step).

## Decision

**GO.** Merge `ui-egui-spike` into main with a `--no-ff` merge commit so
the spike's history (S0–S3 + report) lands intact and the branch stays
bisectable; `pre-egui-spike` remains the restore point. main SHA before
the merge: `b279780` (= the `pre-egui-spike` restore point). The migration is behavior-preserving framework
work with two real risks left (typed-entry parity → M-c test; dock crate
lag → hold-or-vendor), each with a named owner step.
