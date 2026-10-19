# RECOMMENDED.md — decisions taken unattended, for later discussion

> _Growing file. The user stepped away on 2026-10-17 and mandated: run the
> rest of ROADMAP.md solo, every recommended decision is the way forward,
> log each one here as a point of discussion after the roadmap is cleared.
> Nothing in this file is a blocker - work shipped with each choice made -
> but any of them may be revisited._

Format per entry: **what was decided**, **why**, **what it'd cost to change
later**. Newest milestone first.

## M1.7b — in-app Post, the validation gate, Fanuc CPS assets (commit `feat(app,ui): in-app Post with the validation gate + Fanuc CPS assets (M1.7b)`)

- **Span ownership decides cycle seating in a merged stream.**
  `posting::stream_records` shifts each operation's cycle `first_move`
  by the moves posted before it (anchors are stream-global); the
  engine's `normalize_program_order` then assigns each cycle to the
  SECTION span whose move-range owns that index, and where the anchor
  index doesn't exist inside the span (a truncated or hand-made
  stream) the cycle seats at *that* span's end - never re-seated past
  a boundary. Why: a hole that can't be placed must die inside its
  own section, where the reviewer sees a hole-less section, not
  silently relocate into the next operation - cross-span relocation
  is loss. Cost to change: the law lives in `span_of` (post/mod.rs)
  plus its test table.
- **Origin-shift is the script's job: the stream speaks world
  coordinates, the program speaks machine-relative.** `dump` emits
  `OriginX/Y/Z` PARAMs (the WCS slot's Transform) and unshifted
  numbers; `fanuc.cps` subtracts the origin in `wo()` from every
  absolute word (X/Y/Z and cycle R/B) and passes the *relative*
  words through untouched - `I/J` derived as center-minus-current,
  `Q` peck steps. A legacy `.cl` has no PARAMs, so origins stay 0 and
  old goldens post byte-identical. Why: G54-relative programming is
  what a work offset *means* to the control; baking the shift in the
  engine would strip a future post of the choice to emit G10 blocks
  or take offsets from a macro variable instead. Cost to change: each
  new `.cps` must remember the subtraction - the law is a header
  comment in the shipped script.
- **The WCS origin is post context, not a generation input.** It is
  resolved at post time from the height grammar's first `WorkOffset`
  reference (else G54), and it digests into no stamp. Why: moving a
  datum changes where the machine measures from, not what is cut -
  making it stamp input would turn fixture setup into a regenerate
  storm. Cost to change: an op generated against one origin posts
  under a moved one silently, which is exactly right for 3-axis and
  worth revisiting when multi-WCS per setup arrives.
- **Tool number is library rank.** `T..`/`H..` is the tool's 1-based
  position among the *named* tools in document order - the same
  number the tool library shows. Why: one law, derived from data the
  user already sees, and the post can never disagree with the
  browser. Cost to change: reordering tools in the browser
  renumbers programs; that is what "magazine equals library order"
  means, and it's the honest version of a magazine map.
- **The gate has one body, in the app layer.** `post_setup` refuses
  stale/never-generated ops *before* building any record, refuses on
  *any* engine warning after, and writes only a clean program (and
  deletes its own half-write on an IO failure). The UI seam is two
  `CommandKind` arms and a status line - zero duplicated logic. Why:
  the Prime Directive's blocking rule - a half-posted program is the
  silent wrong answer this file exists to prevent - while the CLI
  (M1.7a) keeps its developer exit-0-with-warnings; two consumers,
  two severities, one gate computation, exactly the shape M1.7a's
  counted-warnings entry planned for.
- **Post outcomes are status-line-only, like save and open.** No pane
  error, no dialog: "posted 2 op(s), 214 lines to …" or "post
  refused - 40 warning(s): Contour1: shank/holder at move 44 …".
  Why: the file law says an effect's outcome is one line the whole
  shell agrees on; three surfaces (ribbon, Ctrl+P, tests) then read
  identically. Cost to change: a warnings pane could consume the
  same `out.warnings` vector later.
- **The seed document is honestly blocked.** The first real run of
  the gate refused `ShellUi::seeded()` with 40 ShankHit warnings -
  a full-height rim contour beside ~30 mm of uncut stock wall is
  real geometry the shank really would hit. Kept, not fixed: the
  kittest proof asserts that refusal (phase 0, nothing written),
  then makes the doc clean with a 0.5 mm top skim for the positive
  proof, then staleness, then the repost. Why: a test proving the
  gate against a fabricated document proves less. Side finding,
  commented in the test: a *stepdown* edit on a 0.5 mm skim is
  stale-by-stamp but byte-identical output - the staleness witness
  must be a field that rides the Z words (plunge feed).
- **Dialog arms are proven by queue membership.** `rfd` cannot run
  headless, so the ribbon-click test uses a no-drain frame
  (`frame_undrained`) and asserts `PostProgramDialog` was pushed;
  the `*File` arm runs the real gate in tests. Same split as
  save/open - the dialog is the only untestable millimeter.
- **gitignore scoping: vendor fixtures in, product assets out.**
  `*.cps` became `/*.cps`: the root vendor `.cps`/`.mch`/`post.exe`
  (local validator material, never committed, per the Clean-Room
  Protocol) stay ignored, while `assets/posts/fanuc.cps` and
  `assets/machines/3axis-vmc.mch` are tracked product content -
  `fanuc.cps` is an `include_str!` build dependency, and untracked
  it was a fresh-checkout build break. Why: "ours in, vendor out"
  needs the pattern anchored to the root, not the extension.
- **post.png is an original drawing.** No Tabler glyph fits "post to
  G-code", so the cell's icon is `</>` chevrons drawn in the set's
  stroke weight (40×40, round caps, alpha coverage) - the loader's
  white-repaint contract applies unchanged. Icon law kept: original
  when no generic set fits.

## M1.7a — post engine core + `.cnc` intermediate (commit `feat(app): post engine core + .cnc intermediate (M1.7a)`)

- **JS runtime: boa_engine 0.21, not SpiderMonkey** (the user raised
  SpiderMonkey on 2026-10-19 — considered, this is the answer). Why:
  boa is pure Rust — it builds in seconds on every platform our CI
  matrix will have, adds no C++ toolchain requirement, and has no FFI
  crash surface, which is a Prime Directive argument (a segfault in
  the mozjs servo crates' build or in a JS-API boundary is exactly the
  "silent wrong answer" class we refuse to ship; their Windows build
  fragility is a well-known tax). SM's real advantage is exact
  SpiderMonkey-era semantics for running *third-party vendor-style
  scripts* — which the clean-room protocol excludes by construction:
  every `.cps` in this product is ours, written against our own
  documented prelude, so engine fidelity beyond ES5 is not a
  requirement. Cost to change later: the host is one file —
  `crates/app/src/post/js.rs` is the only code that touches boa's API
  (eval-in/`to_json`-out, no captures); a SM or deno_core swap
  reimplements `Script` there and nothing else moves. Cheap insurance
  already shipped: the prelude defines `setLanguageVersion` as an
  accepted-and-ignored no-op, so a reference-era pragma line in a
  script never throws here. Revisit trigger: if we ever choose to run
  unmodified third-party posts, or posting shows up hot in simulation-
  scale batch runs.
- **The intermediate is a superset of the goldens, by parser law not
  by migration.** A stream with no `HSMW-CNC` magic parses by the
  legacy `.cl` rules, and every word accepts four spellings (bare
  positional, spaced key, joined `X1000`, separated `X:1000`), so the
  committed `.cl` goldens (all thirteen) are legal post inputs
  unchanged - the law test says so. Why: it
  makes "the real product eats its own dog food" testable in CI from
  day one - the Fanuc post must post the goldens; rewriting committed
  goldens would launder a format bug into history. Cost: the parser
  carries one classifier for all forms (~40 lines); the roundtrip law
  test pins canonical (spaced) output re-parsing to identity.
- **Legacy `; CYCLE` markers are re-seated by the engine, not the
  file.** `normalize_program_order` moves each cycle record to just
  before its `AT`-anchored move before hooks start; idempotent on
  `dump` output, which is written in program order already. Why: v3
  goldens append markers after the chain (M1.6's writer shape), and
  program order is what the hook contract means; files on disk stay
  byte-frozen while the driver sees the right order. Cost: `line_map`
  and record indices are into the *normalized* stream - documented.
- **The boa host uses a pull model: pure-JS prelude, no Rust
  callbacks.** boa's `NativeFunction::from_fn_ptr` cannot capture
  environment, so instead of shimming `writeln` in Rust the prelude
  pushes lines to a global `OUT` array and the driver evals globals
  in (`properties`, `currentPosition`, `machine`) and pulls buffers
  out (`OUT`, `MESSAGES`) as JSON. One direction per seam, and the
  whole host interface is string eval - the same shape any future
  runtime would need. Cost: one extra JSON round-trip per record;
  irrelevant at our stream sizes.
- **Nothing is silent: warnings and swallows are counted.** Stream
  `WARN` records plus every script `message()` land in
  `PostOutput.warnings`; a record swallowed by `onCommand` returning
  `false` increments `PostOutput.swallowed` (legal filtering, but the
  reviewer sees the count). The CLI exits 0 *with* warnings printed -
  it is a developer tool and the diffable program is its product; the
  in-app Post gate (M1.7b) blocks on them instead. Why: the Prime
  Directive's two consumers need two severities; one stream, one
  source of truth. Cost: the 1.7b gate must remember to fail where
  the CLI passed - planned, this is the decision it inherits.

## M1.6 — drilling: cycles, hole recognition, canned-cycle representation (commit `feat(app,ui): drilling - hole recognition, cycles, and canned-cycle representation (M1.6)`)

- **Cycles ride alongside the moves, not instead of them.** A drill
  operation generates the fully expanded chain (rapids, plunges, peck
  hops) *and* a parallel `Toolpath.cycles` list, one `Cycle` per hole
  with kind, XY, R/B/entry planes, peck, dwell and its `first_move`
  anchor. Why: the chain is the truth verify and simulation must see
  (a canned-cycle short-cut would blind the collision laws), while the
  cycle list is the truth the post must see (G81 per hole, not 40
  re-derived moves); an agreement law test pins the two views of every
  path together, and the `.cl` golden format grew `; CYCLE` comment
  lines so goldens diff the intent, not just the geometry - old
  goldens stay byte-identical because the marker is version-gated on
  "cycles exist". Cost: every drill-side change must keep both views
  consistent; the law test is the tripwire.
- **Recognition reads raw world-transformed facets, not the feature
  index.** `drilling::recognize` BFS-welds near-vertical wall facets
  (|nz| below a cosine limit) into components, circle-fits each
  (Kasa), and admits only closed rims whose XY normal span exceeds
  270°, so block sides and slot-end arcs fail the shape test by law.
  Why: features are M1.5 selection plumbing that can drift or be
  absent (mesh-only imports); holes are a geometric fact about the
  mesh and should be computed from nothing else. Cost: it re-scans the
  mesh per Detect click - fine, the button is an action, not a pass.
- **Holes are Z-axis only.** No cylindrical-axis detection. Why: the
  whole codebase is a 2.5D Z-plunge CAM; a 4/5-axis hole list would be
  unmachinable by every generator downstream. Cost: a part with side
  holes silently gets a short list - recognized as the right failure
  mode for 0.1.0, logged against the M3 axis work.
- **The hole list lives in the operation params, so it is data -
  editable, undoable, and stamped.** Detect holes queues one
  `EditOperationParams` carrying the whole `Drill` params with the new
  holes; the recognition result is snapshotted into the project digest
  so any model edit marks the op Stale rather than pretending the
  holes still match. Why: "recognition is an action, its output is
  data" is the commands-law restated; a live re-recognize per pass
  would violate it. Cost: stale ops need a re-Detect the user must
  trigger; the status line says so. An empty hole list is legal at
  creation (a fresh Drill op) and only the generator refuses (
  `GenError::Empty`), with a named reason.
- **The Auto cycle picks its shape from depth against diameter: ≤1d
  drill, ≤4d peck with a 1 mm chip-break hop, deeper a full-retract
  peck with dwell (the G83 shape); peck step 3d/1d, floored at 0.25d,
  guarded at 4096 pecks; Spot is never automatic.** Why: this is the
  textbook chip-evacuation ladder expressed in the only two numbers
  the op is guaranteed to have; auto-spotting would move the tool to a
  center-punched hole the user never asked for. Cost: an
  HSMWorks-parity "auto spot" would be a new choice arm, not a
  rewrite.
- **Constants carried in the open:** spot depth 2.0 mm, through-hole
  exit clearance 1.0 mm past the floor (the tip cone needs the
  allowance to make a full-diameter wall), dwell and peck in the
  cycle's own units, tap feed = rpm × pitch exactly. Why: each one is
  a machining convention with a real reason, not a tuned number.
  Cost: trivial to expose as params later.
- **Dwells and taps are representation-only where moves can't speak
  them.** A dwell adds no zero-length move; the chain carries no
  spindle-sync arc - the `Cycle` record holds `dwell` and the kind,
  and M1.7's post emits the P word or the rigid-tap block. Why:
  zero-length moves poison every distance-based law (feed/rapid
  length checks, kerf self-clearing). Cost: the sim timeline will
  ignore dwell time until the post exists; logged on the M1.7 list.
- **Verify learned two things for drilling:** the gouge law skips
  zero-XY-length plunges in every mode (a vertical plunge cuts
  exactly its swept disc - boundary tangency is the wrong predicate
  for it), and the shank/holder band law plus a new reach check turn
  "hole deeper than the tool" into a `ShankHit { part: true }` from
  the cycle record itself. Why: the bands already handled vertical
  moves correctly; only the two blind spots above were real. Cost:
  none - both are strictly narrower than before.
- **`tip_angle` and `pitch` are inert unless the kind reads them.**
  Tool gained both fields (`serde(default)`, drills 60<tip<180, taps
  pitch>0 in `is_sane`); profile() draws the cone tip only for
  kind=Drill. The editor seeds 118° / 1.0 mm on kind change, in an
  is_nan-guarded form so a cleared field never posts nonsense.
  Persistence stepped v9→v10 as a pure serde-default no-op - the
  step-up is the stamp itself. Cost: a future countersink kind reads
  `tip_angle` and gets the collision shape free.
- **Bore/BackBore post as their own cycles in M1.7 (G86/G89 shapes);
  thread milling stays unshipped.** Why: the milestone text asked for
  "basic cycles", and a helical thread-mill path needs circular
  interpolation the post doesn't have yet. Cost: one generator + one
  post arm.
- **Both UI snapshots churned (egui_app, egui_tool_editor) -
  intended.** The 6th Create cell shifts the Modify/Inspect group
  right by one cell width, and the tool catalog grew DR-0200/0300/
  0400/0600 + TAP-M4/M6 + BR-1000 rows. The diff images were checked to
  contain nothing else before regenerating.

## M1.5 — view cube + viewport navigation bar (commit `feat(ui): view cube + viewport navigation bar (M1.5)`)

- **The furniture is painted, not widgeted.** `viewcube::Hud` is a layout
  struct, the cube a `Painter` pass, the nav buttons painted rects with a
  hit-test - zero `egui::Window`/`Button` machinery. Why: the cube must
  be drawn through exactly the projection its pick inverts (one
  `point_screen` serves painter and pointer, so a feature that lights is
  a feature that orients), no widget graph can express that identity, and
  the kittest harness and snapshot gate can only prove in-root surfaces -
  the first floating `Window` would have been new ground twice. Cost to
  change: a real `ui.button` row for the nav bar is easy; a GPU-viewport
  cube is the expensive version nobody asked for.
- **Top-right, and it eats gestures by never arming.** The hint line owns
  the bottom-right corner, so the cube takes the top one. A press inside
  the hud acts at press time and never records into `left_press` - the
  existing release-pick and motion-promote passes then no-op on their
  own, which is how "a press on the cube must not orbit or pick" holds
  with zero new flags. Misses inside the hud are consumed too: a stray
  click between cube and buttons does nothing at all, silently.
- **Widgets outrank the raycast.** Discovered while testing: a nav button
  sits 8 px below the cube's box, but the cube's volume stands behind it
  in 3-D, so a raycast-first `click` oriented the camera when Fit was
  clicked. The law now: nav rects own their pixels first, and the cube
  raycast is gated to the cube's own box. Cost to change: none wanted;
  it's in `click()` with a comment.
- **One orientation law: `Camera::toward(d)`.** Face, edge and corner all
  classify to a unit world direction - the slab raycast's entry point,
  coordinates past 1−SNAP(0.16 cube units) counted: 1 → that face axis,
  2-3 → the sign vector. `toward` keeps `target`/`dist` (orienting never
  pans or zooms), and the poles need no special yaw because
  `basis_right`'s zero-guard pins screen-right to +X there - a law test
  holds the roll stable at six pole/yaw combos rather than trusting the
  mvp layout. Cost to change: the SNAP width is the edge/corner grab
  radius - a wider one makes faces harder to hit dead-center.
- **Fit is a command; the toggles are not.** The Fit button queues the
  same `FrameAll` the F key pushes (the nav bar's one real command);
  Iso/Wire/Paths/Axes are direct view-state writes, exactly as panes.rs
  flips the same fields - the camera/display exemption from
  everything-is-a-command, applied by the nav column's precedent. The
  command test asserts through the drain, not on a press frame: egui's
  multi-pass can run the next frame's drain before we look, so queue
  membership is pass-fragile while the executed effect is the law.
- **Vector-only glyphs.** The five buttons and the cube are drawn from
  line segments, arcs and text - no new PNGs, in step with the icon law;
  the cube's labels are the three-letter strings we already own.

## M1.3 — tangential leads for 2D contour (commit `feat(app,ui): tangential leads for 2D contour - half-arc entry parallel to the wall (M1.3)`)

- **Tangential means a half arc - a true semicircle.** `Lead::Tangential`
  `{ radius }` centers `radius` off the wall on the air normal and plunges
  at `2 * radius`: the tool starts its feed moving *parallel* to the path
  and lands on it already moving along it - G1 continuous end to end,
  which is what earns the name. The quarter arc starts its motion along
  the normal (across the air), the linear slams straight in; the three
  cover the classic lead-in families. Lead-out mirrors it: overrun
  tangentially, sweep out, stand off parallel. Cost to change: a new
  `Lead` variant - additive in the enum, on the wire (`LeadDto`, v9) and
  in the combo; removal touches every contour-params surface at once.
- **The posted sense is the quarter law's limit, not a new rule.** The
  quarter arm decides ccw from `cross((e-o), (s-o)) > 0` - the two radius
  vectors' cross, which is sin(sweep) and degenerates to exactly 0 at
  180°, where the semicircle lives. The quarter's radius vectors are
  `-R*t` and `-R*air`, so its rule factors to `-R^2 * cross(air, t) > 0`;
  the tangential posts exactly that survivor, `cross(air, t) < 0`, which
  is why the two arms agree at every sweep below π instead of inventing a
  second convention. A visible
  consequence: with `air_side`'s conventions the tangential posts CW for
  climb where the quarter posts CCW - the tests derive the expected flag
  from the law rather than pinning it, so a deliberate `air_side` change
  stays a one-place decision. Cost to change: pinning either way would
  freeze a secondary convention in a primary test.
- **Project v9, stamp-only migration.** `lead` has been a required key
  since v1, so no older file can carry the new tag - v8→v9 is the version
  step itself, like v7→v8, and every older file keeps reading with
  identical meaning. `GenStamp`'s digest gained tag byte 3 so a
  lead-kind switch marks cached toolpaths stale exactly like a radius
  edit does - file bytes are untouched by that.
- **The combo grows a fourth item sharing the arc-radius label.** Arc and
  Tangential are mutually exclusive branches and both radii mean "the
  lead arc's radius", so one `Lead arc radius` DragValue serves either -
  the label can never be ambiguous in a document. Switching to
  Tangential seeds the draft at 5.0 mm, the same shape as the Arc choice;
  the old value rides the undoable `EditOperationParams` like every other
  contour parameter.
- **The goldens say so: `contour_tangential_lead.cl` is the byte witness**
  that a half arc posts as one `S±1` arc move with the 2×radius standoff,
  and the disk-plate job is otherwise move-for-move the quarter-lead
  file - every pre-existing golden re-ran byte-identical, which is the
  law a new enum member must obey: it cannot disturb a path that never
  asks for it.

## M1.1 — document units, millimeters and inches (commit `feat(app,ui): document units - millimeters and inches everywhere including the post law`)

- **The unit is a lens, not a storage choice.** Every stored number -
  geometry, the micron lattice, toolpaths, the goldens, entity and draft
  parameters - is millimeters, always. The document's unit is one flecs
  world singleton (`schema::Units`), read at paint time through
  `units::of` and on no entity, so it never appears in a browser row or a
  saved entity list and needs no mirror in UI state that could go stale.
  This is the international inch (exactly 25.4 mm) and the
  internal-metric/display-at-the-edge pattern of CAD/CAM systems and
  G-code (`G20`/`G21`). Cost to change: storing dual representations or
  per-widget unit state - precisely what the singleton exists to prevent.
- **SetUnits is a command, and setting the unit the document already has
  is refused at construction.** The lens change is undoable like every
  other UI-visible change (its inverse writes the previous value back, so
  undo restores exactly what was on screen), and a no-op does not belong
  in the history. The control itself is a two-radio pair on the stock
  page - document-global, one place, not per-pane toggles. Cost to
  change: a silent history step, cheap to add and impossible to notice.
- **Widgets convert on a display copy with `speed ÷ 25.4`.** One physical
  pixel moves the same millimeters in either mode, so drag muscle memory
  is unit-fair; and because egui's smart-aim rounds the *shown* value
  inside a one-pixel radius, the identical ±0.1 mm aim window applies in
  both modes - the assist never favors one unit. `from_display` snaps
  inch-typed values onto the micron lattice (the same quantum as
  `cl::lattice`), so `0.5 in` stores exactly `12.7 mm` and no stored
  number ever carries float-conversion dust - the goldens keep their
  bits. mm mode stays identity: the historical DragValue, untouched.
  Cost to change: none wanted; the snap is the only rounding in the
  path.
- **Display precision keeps mm's bits and widens inches only where the
  unit demands it.** `units::len` prints the site's own millimeter
  decimals verbatim in mm mode and mm+2 decimals in inch mode (a mil is
  finer than a hundredth of a mm, so an inch readout must not lose
  information the mm one carried); feeds print bare in mm and `÷25.4` at
  two decimals in inches; areas divide twice and get +3. Every pre-existing
  string in mm mode is byte-identical - both snapshot gates passed with
  zero churn, which is the empirical proof the lens never leaked into
  storage or formatting. Cost to change: rounding in `len`, which is the
  round-trip-through-a-string bug this law exists to avoid.
- **Project file v8: an absent `units` key means millimeters.** The
  chained migration adds the key (`v7→8`), making the whole v7 corpus its
  own migration - older files reach v8 through the existing ladder, never
  skipping a step. Cost to change: a reader that defaults instead of
  migrating - rejected because the ladder is the file format's law.
- **"Correct everywhere including posts" is half specced, honestly.**
  There is no post yet (M1.7 ships the first), so nothing that exists can
  be wrong; the post's rule - convert at output time, declare the mode
  with `G20`/`G21` - is written into `app::units`' provenance header so
  the M1.7 post inherits it rather than reinventing it. Discussion point:
  confirm the Fanuc post keeps mm-internal numbers and converts per-line,
  not a whole-program mm→inch pass at the end.

## M1.4 — graphical tool editor (commit `feat(ui): graphical tool editor - its own window with a live to-scale preview`)

- **The editor is an in-app `egui::Window`, not an OS viewport.** egui's
  multi-viewport machinery (`show_viewport`) needs an OS window per
  surface - unhittable by the kittest harness, invisible to the snapshot
  gate, and unprovable by any test we can run headless. An in-app floating
  surface is the only windowing form this project can *verify*, and it
  behaves identically to the user (draggable, ✕-closeable, floats over the
  dock). The single-root-viewport law from the egui graduation stands
  untouched - this is an overlay, not a second winit window. Cost to
  change: swap `Window::show` for a deferred viewport and lose both the
  flow test and the snapshot.
- **Dock = library, window = editor - a considered revisit of this
  milestone's "standalone tool library window is gone" decision.** That
  retirement removed a *second browsing surface* duplicating the dock; the
  roadmap then asked for the editor to be "its own window". Both survive:
  the dock keeps the one library (rows, catalog, summaries), and the only
  window is the editor for one tool. The decision didn't reverse; it found
  the shape the two roadmap lines actually agree on. Cost to change:
  re-fusing editor fields back into the dock page (that's the pre-M1.4
  code - easy to resurrect, harder to keep uncramped at 280 px).
- **The preview draws `tools::Tool::profile`, and `profile` segments on
  purpose.** The side view's band breakpoints are verbatim
  `verify`'s shank/holder collision bands (cross-ref comment in
  `tools.rs`), zero-length bands skipped exactly as the checker skips
  them, and the drift test `profile_segments_match_the_collision_bands`
  fails if the picture and the checker ever disagree about where a band
  starts. One segmentation law, drawn and enforced from the same numbers.
  Cost to change: none wanted - drift here is gouge-adjacent.
- **Label ownership: the dock summary is composed text, so every field
  label resolves in exactly one surface.** `query_by_label("Diameter")`
  must never be ambiguous between dock and window - the dock shows a
  tool's numbers as three formatted `format!` lines (no widgets), the
  window owns all field labels. Buttons split the same way: "Add" is the
  catalog, "Add tool" the window's blank form, "Apply" window-only,
  "Delete" dock-only. Cost to change: any labeled widget added to the
  dock silently breaks the flow test's queries.
- **Closing is ✕-plus-ghost; no Esc binding.** The `open` mirror flips
  `State::tool_editor` to `None` on the ✕, and the ghost law mirrors
  `pane_target`'s: at the end of `drain_actions`, a window whose tool no
  longer exists (Delete, undo-of-Add, document swap) closes itself in one
  line. Esc was deliberately not wired to close - Esc must stay a no-op
  while a `TextEdit` has focus, and half-wiring it invites the bug where
  Esc-in-a-field both cancels the edit and kills the editor. Cost to
  change: one keyboard hook and a focus question.
- **`tool_editor: Option<String>` is view state on `State`, like
  `op_tab`** - never a command, never saved, never undoable; `Some("")`
  is the blank new-tool form so the "numbered past taken names" prefill
  rides `new_tool_draft` unchanged. Opening queues nothing (asserted);
  all drafting still lives in `ToolUi`, so the window reuses the
  existing sync/stage/Apply loop with zero new command surface. Cost to
  change: field relocation only.

## M1.4 — tabbed property pages (commit `feat(ui): tabbed property pages - the operation page is five named sections`)

- **Status lives above the tab bar, not inside a tab.** The stale badge and
  the folded collision warning render between the Type/Tool/Source block and
  the tabs, so a warning can never hide behind an unopened section. The
  report-visibility law outranked the tidier look of folding it into
  Geometry. Cost to change: move one block in `operation_page`.
- **The open tab is `State::op_tab`, deliberately not a field of `OpUi`.**
  `sync_ops_ui` rebuilds the drafts wholesale whenever the operation
  identity or doc version changes; a tab stored inside them would reset to
  Geometry on every Apply and every drained command - reads as a bug. On
  `State` it survives edits like the other transient view state (geo_pick,
  display mode): never a command, never saved, never undoable. Cost to
  change: one field relocation.
- **No dead tabs; an out-of-set tab falls back to Geometry.** A kind renders
  only the sections it has fields for (Trace has no Passes, non-Contour has
  no Leads & Links), and if `op_tab` names a section the kind lacks (the
  user switched the Type combo while on Leads), the page resolves to
  Geometry instead of clamping to an empty pane. The drift test asserts the
  cell is absent *and* the fallback paints. Cost to change: `has_section` +
  the one-line resolution.
- **Field-to-section placement** (the judgment call, one field per section,
  within-section order preserved from the long stack): Geometry takes the
  pick row + selection echoes, Contour's Side/Tolerance, Slot's A/B points
  and width; Heights takes every Z plane including Stepdown (a stepdown is
  a height law, not a pass count); Passes takes Stepover, Stock to leave,
  Strategy; Leads & Links takes Contour's leads and tabs only; Feeds takes
  the three feed fields. Stock-to-leave under Passes (it's how much stays
  per cut) rather than Heights. Cost to change: one `if tab ==` gate per
  group + the test table's placement list.
- **The tab bar wraps** (`horizontal_wrapped`): the properties pane is ~280
  px in the docked default, five cells do not fit on one row, and the
  M1.4-pane-width law says an overflowing widget keeps its a11y node but
  its clicks land in the wrong pane. Two tidy rows beat a clipped row.
  Cost to change: layout only.
- **The roadmap's M-c entry said "pages arrived already tabbed" - it was
  wrong and is corrected** (they arrived as one scroll; parity first). The
  redesign is this commit. Discussion point: whether the ROADMAP phrasing
  misled the hand-tests.

## M1.4 — viewport display modes (commit `feat(ui): viewport display modes - wireframe with honest hidden lines`)

- **Wireframe is a hidden-line drawing, not an x-ray.** Faces still
  rasterize - into the depth buffer only (a fourth pipeline, color writes
  off). The feature-edge wire then respects occlusion exactly as it does
  when lit: back-side lines stay hidden behind the solid. A see-through
  line soup was the cheaper build and reads as clutter on any real model.
  Cost to change: one branch in `View3D::render`.
- **The stock draws its full wire in wireframe mode** - its faces skip
  the depth pass. A stock box is a volume of material, not a surface;
  hiding its far edges behind itself would misrepresent it (and hid
  nothing real - you see through stock to plan). The two GPU tests pin
  both halves: `wireframe_paints_wire_not_faces` and
  `wireframe_hides_wire_behind_solids`.
- **Wire color is mode-aware in the shader** (`u.wire` flag, piggybacked
  in the uniform's tail padding): imgui-era dark wire is invisible over
  the dark background, so wireframe brightens it to cool gray - and lifts
  the selected-part warm tint to the hot amber the fills use. No second
  edge pipeline. Cost to change: two `select`s in `fs_edge`.
- **The toggle is browser-pane radios ("Solids: Shaded | Wireframe"),
  not a ribbon cell.** Direct view-state field flip under the same law as
  the axes/paths checkboxes - and the M-f tip table (imgui's 18-cell
  parity law) stays untouched; the ribbon has no key to promise. Radios
  over a ComboBox: the combo popup was unhittable in the kittest harness,
  and the whole choice is legible without a click - same reasoning as the
  Geometry page's pick-mode row. Cost to change: the row, the test, and
  the snapshot.
- **"Per view" read as per viewport** - this app has one viewport panel,
  so the mode lives once in `State`, like the camera. If multi-viewport
  docking ever comes, the field moves into the viewport tab's state.

## M1.4.2 M-g — imgui retired (commit `feat(ui): retire imgui - egui becomes the shell`)

- **Quit is now Ctrl+Q.** The only quit surface in imgui was its File menu;
  the egui shell has no menu. Rather than ship a migration that quietly
  loses an app-level capability, the shortcut layer gained a `Ctrl+Q →
  actions::quit` arm (same `!typing` gate as the file keys; same
  `Quit → request_quit → el.exit()` path as the window close button).
  Cost to change: one arm in `panes::shortcuts`.
- **Two actions helpers deleted, not ported:** `new_operation` (blank the
  target to show the new-op form) and `delete_operation` (menu-selected
  op delete). Both existed only for imgui menus; the ribbon's Create
  cells and Delete cell + Del key already produce identical effects
  through the same module. The `DeleteOperation` command kind itself
  stays (the Operations pane's own button pushes it, pages.rs). Cost to
  change: re-add a helper; the command law is untouched.
- **Bare `cargo run` now opens the egui window; `--ui imgui` refuses with
  rc 2** instead of silently drawing the old shell. A named refusal beats
  a fallback that no longer exists. If scripts of yours pinned `--ui
  imgui`, they now need one word changed.
- **The S0 "one wgpu" coercion test retired** (`wgpu_types_are_unified`):
  with `dear_imgui_wgpu` gone there is exactly one wgpu in the tree by
  construction - `egui_wgpu::wgpu` - and nothing left to coerce.
- **The imgui headless probe retired the same way** (`headless_frames_run`
  deleted): the egui `--headless` proof runs the same command world
  through the real `run()` entry; `tests/shell.rs` keeps the CLI-contract
  and state-law tests that were never imgui-specific.
- **Icon law: PNGs ride egui textures, vectors stay the fallback.**
  `egui_app/icons.rs` registers the same Tabler MIT glyphs via
  `ctx.load_texture` (CPU-side registration, so the kittest harness
  proves the PNG path without a GPU); every consumer checks the texture
  first and paints the vector twin when it's absent. Layout is
  GPU-independent by construction - the ribbon geometry matches imgui's
  own formula (CELL_H 35, RIBBON_H 61) rather than the spike's squeezed
  49px strip. Cost to change: none - this is how Rerun-scale apps do it.

## How this file ends

When the roadmap's open items are exhausted, this file gets its final
entry summarizing what "done" meant, and the discussion happens over it.
