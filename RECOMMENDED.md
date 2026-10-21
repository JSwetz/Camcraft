# RECOMMENDED.md — decisions taken unattended, for later discussion

> _Growing file. The user stepped away on 2026-10-17 and mandated: run the
> rest of ROADMAP.md solo, every recommended decision is the way forward,
> log each one here as a point of discussion after the roadmap is cleared.
> Nothing in this file is a blocker - work shipped with each choice made -
> but any of them may be revisited._

Format per entry: **what was decided**, **why**, **what it'd cost to change
later**. Newest milestone first.

## M2.2e - Ramp entries, stage D: the certified diagonal chain (this commit)

**Ramp is a law about the space BETWEEN cuts.** Stage D adds one bool to
Finish3DParams and one branch to the runs emitter: with `ramp` on, the
link from one raster line's end to the next - the serpentine's lateral
step, or a longer diagonal at a group boundary - is certified against
the terrain, and where it certifies the hop's up-rapid-plunge triple
collapses into a single Feed. The discovery that shaped the tests: the
Parallel raster walks serpentine, so its connectors are one-stepover
laterals, not across-shelf diagonals. On flat ground the chain is the
classic no-lift raster: `45 plunges / 52 rapides` becomes `12 / 18`
while the ring phase's 43 arcs ride on untouched - the golden diff
reads exactly like the law.

- **The bool, not the enum.** Pocket's Entry3D has Ramp and Helix; the
  3D finish's between-lines motion has no honest Helix geometry (a helix
  needs XY room the stepover doesn't offer), and an arm that can only be
  refused is an arm that must be refused at the gate - validate-early
  says don't ship a parameter the generator cannot keep. `ramp: bool`
  states exactly the promise kept.
- **Certification is the support's answer, not the design's guess.**
  `diagonal_ok` samples the straight cut and asks `support_at` at each
  probe (n = d_xy / (tol/4)): center z + r must meet the support within
  tol. A convex dome sinks its chords and thereby refuses them - the
  hill test asserts the COUNT never grows and the kernel never
  complains, but NOT that any particular hop survives; that is the
  support's business. The envelope verifier remains the real gouge
  law; this certification is the generator keeping its own promise
  before the verifier ever runs.
- **A claim too wide to check is declined.** The 4096-probe cap makes
  honesty cheap: NaN spans and over-cap spans both land in `false`, and
  the declined path is BYTE-IDENTICAL to the hop (test 3: explicit
  0.5 mm spacing under tol 1e-4 puts the lateral connector at 20000
  probes - every link refuses, the two paths match move for move).
  Same budget-stated-zero law as the solid-rest work.
- **Rings and first contacts never route through the chain.** The
  branch lives in the runs emitter's `i > 0` arm alone; the all-steep
  wall test pins the law as byte-inertness of the flag where rings are
  the whole path.
- **Wire:** PROJECT_VERSION 17, pure serde-default step-up - absent
  `ramp` is `false` is the hop, what v16 ran; written always, like
  `climb`. **Stamp:** one byte appended to the Finish3D arm - every
  existing Finish3D op goes Stale exactly once, "step-up is the stamp
  itself", like every bump since v6.


## M2.2e - Around the model, stage C: the center-shadow families and the two-door spacing law

- **The shadow, not a level: one XY domain for a height-free
  family.** Contour and Project march Z-following loops over ground
  that has no single center-height - so their placement region is
  NOT `level_region`'s z-slice but the terrain's CENTER-SHADOW: every
  shallow facet's XY triangle TRANSLATED by `r·m_xy` (a plane's
  tangent centers form that plane shifted by its lean; the shadow of
  that sheet is the shifted triangle), unioned. Steep facets
  contribute nothing - the same jurisdiction every other family
  keeps - and holes of the shadow (islands, air under an overhang)
  answer through `support_at`'s own ball-clear law: a center that
  cannot rest simply breaks the piece, never a lie. Cost to change:
  the shadow is one function; making it z-aware later would just
  mean re-slicing it per level, but then it stops being one truth
  about the plan view and starts being N truths, one per rung.
- **A closed loop closed BY the march.** `march_loop` turns a Region
  loop into an arc-length parameterization (cumulative chords,
  summed in walk order) and feeds the shared kernel - so the
  validity bisection, quarter-budget chords and discontinuity break
  are not re-implemented, they are REUSED, and the ring-close falls
  out of the sampling laws: the clamp sample at `t = L` lands exactly
  on the entry point, and `push_run_pt`'s duplicate law (which
  compares against the PREVIOUS point, not the first) keeps it as
  the closing move. A test now pins it on the plateau. The other
  half of the law: a loop that broke at a shelf mouth does NOT close
  - its piece ends where the ground ended.
- **Outermost-first, entry-point-second - loop order as a fact about
  the model.** Clipper's memory is not a deterministic source, and a
  stamp that churns is a stamp that lies, so `shadow_pass` sorts by
  |area| descending, then by first point (x, then y). Both orders
  total under f64::total_cmp. Same law the rings' area-sort serves.
- **The two-door spacing law (a test-failure that reversed a claim).**
  Stage C's design note said "Project ignores the stepover" after the
  Waterline precedent. The first test run proved that claim WRONG in
  the interesting direction: `validate`'s early gate refuses a
  zero/NaN stepover from EVERY pattern - even one that never reads
  it - because AGENTS.md's "validate early" outranks "ignore
  gracefully", and Waterline already pays that gate today. So the
  honest split is TWO doors: the GATE (is the number a number) speaks
  to all patterns; the COST guard (8192-ring cap, the arithmetic
  message) speaks only where spacing marches - contour at a hairline
  stepover is hours of rings and must refuse; project's single rim
  costs no spacing budget and sails past the guard. The shipped test
  pins both doors separately.
- **The immortal-sliver belt.** The cap guard rejects a family that
  would NEED more than 8192 offsets; the while-loop additionally
  breaks at 8194 rings whatever the guard said, so numerically
  immortal geometry (a sliver that refuses to collapse) yields a
  long honest path, never a hang. A CAM kernel must terminate on its
  own arithmetic, not on its optimizer's.

## M2.2d - Polar placements, stage B: radial chords, the spiral, and the march that must end

- **The polar families are raster families by construction.** Radial
  lays `N = ceil(π·R/s)` chords through the shallow footprint's center
  - arc spacing at the rim meets the cos-law stepover, and toward the
  center they overlap ever more (the cusp law is a bound, never a
  quota); the spiral is ONE Archimedean curve in from the silhouette,
  ending a half-stepover from the pole because the next turn would
  only retrace. Both share the parallel raster's actual machinery:
  one adaptive march (`march_curve`: validity bisection, the quarter-
  budget chord refinement, the discontinuity break), one hop
  choreography (`emit_runs`), one ring close (`rings_and_tail`).
  Three ground layouts, one set of laws - and the extraction is
  byte-identical for the old paths (the float algebra that keeps it
  so: `4.0 * delta0 == delta0 * 4.0` exactly, `* 1.0` exact), every
  golden unchanged. Over a cap of 8192 ground lines the placement
  refuses with its arithmetic in the message - it is a bound on
  march-hours, not a taste. Cost to change: a fourth layout should
  join this trio, not fork another march.

- **A parameter march ends by a labeled break, not by fixture luck.**
  The spiral always reaches the END of its parameter range standing
  on valid ground (its inner limit is cuttable hill, by the apex
  law); the two-level loop - `while t <= t1` around an inner `loop` -
  took the "no progress" break at the inner level, returned to the
  outer condition, found it still TRUE at t == t1, re-clamped the
  same sample: a silent infinite spin. Twenty release minutes of it
  before the suspicion was right. The extracted raster code had the
  SAME structure, hidden only because a raster march always dies at
  the bbox edge, in the skip phase whose bare `break` does exit the
  while. Now both no-progress arms - clamp-landed and float
  stagnation (`t + delta·unit(t) == t` with t < t1: the parameter has
  grown too far for the step to move it) - `break 'march` outright.
  Cost to change: every future parameterized curve (any M2.2e
  family) must keep BOTH breaks; re-deriving the exit per family is
  how this bug survived a whole milestone of green gates.

- **Ring strokes and march samples obey different laws, and Z tells
  them apart.** Building the structural test surfaced it: the rings
  legitimately emit long lateral Feeds - iso-z tangent loci, and the
  exact-straight collapse keeps a wall's length chord-free - while
  the march owes the stepdown budget per move. So the test bounds
  XY-span on the FEEDS THAT CHANGE Z (rings ride one tip plane; on a
  curved model every march sample climbs). Related, logged as known
  behavior rather than a phantom: a lone sheet gets edge-grazing
  rings at EVERY ladder level - the forbidden-center law is being
  faithful (tangent = zero material removed), not leaking; on a
  closed solid those same levels hug the walls.

- **Append-only ordinals slot polar words into the v16 grammar with
  no bump.** `Radial = 4`, `Spiral = 5` extend FinishPattern, digest
  into the stamp through the existing `*pattern as u8` (since
  M2.2b), and write as new words in the same `pattern` field; older
  files never see them, unknown words still refuse Corrupt, so no
  cross-version confusion is reachable. Cost to change: reordering
  the enum invalidates every digest - all finishing ops' stamps go
  stale at once; the append-only law is a cache law, not aesthetics.

## M2.2d - Hidden rest: solid truth behind the height field, stage A

- **Parity membership is decided by CLAIMS, not by half-open edge
  ownership.** The textbook ray-parity test needs each column to count
  every coincident sheet once per sheet. The clever O(1) alternative -
  per-triangle half-open (u,v >= 0, u+v < 1) ownership of shared
  edges - FAILS twice over: a closed box's center sits ON both caps'
  shared diagonal, and cancelling the pair (the natural "two claims,
  one crossing" reflex) erases a real crossing; two flush sheets
  stacked face-on-face collapse to one crossing and lie about
  material. The shipped law (solid_rest law (a)): every containing
  facet claims INTERIOR (strict barycentric beyond 1e-12) or
  BOUNDARY; at each coincident-z cluster, per plane group (normals
  equal up to sign - winding-blind, law (c)), crossings = interior
  count, else 1 for bare boundary claims (adjacent pieces of ONE
  sheet merge honestly). The knife edge - a column riding two flush
  sheets' own shared edge - cannot be told apart, and the merge
  chooses to merge: the result is ODD and reported `Crossings::Open`,
  an unknown, never asserted material. Doubt biases toward unknown
  because unknown costs a report, a lie costs a crash. Cost to
  change: any edge-ownership shortcut re-opens the diagonal hole;
  the claim lists are what a future BVH pruning feeds on, so the
  interface survives, the settles do not.
- **The union is in the FIELD only - hidden rest never touches a
  predicate.** `EnvelopeReport.rest_mm`/`at` gain the solid truth
  (material tops occluded by a ceiling, judged by the SAME distance
  law: nearest center - r - floor); the ridge pass, rest_cells,
  gouge and the tolerance math are computed exactly as before. Every
  existing golden is byte-identical (proved: release full-suite
  regression alongside this commit) because on prismatic fixtures
  the highest paired interval top IS the height-field top, so the
  occlusion filter (`hi >= t - floor: continue`) rejects everything.
  Cost to change: folding hidden rest into rest_cells would make the
  standing-mask region depend on ray parity - the region and the
  scalar would stop agreeing, and `measure`'s removal accounting
  rides the region.
- **Over budget the hidden scan answers with the STATED ZERO.** The
  per-column claim bins cost 24 B/cell plus claims; budgets are
  1.2 M cells (~30 MB of headers) and 40 M claims. Beyond them
  hidden_rest returns 0.0, i.e. "the height field's debt stays on
  the books" - the report degrades to exactly M2.2c behavior, and
  the degrade is structural (a missing number), not a guessed one.
  Discussion point: a `rest_hidden: bool`-style flag on
  EnvelopeReport saying "the solid scan skipped" - cheap to add,
  would make the zero visible instead of inferable; left off this
  stage because nothing reads it yet.
- **The hidden scan has NO XY window on the distance loop; the ridge
  law's soundness argument does not transfer.** Pruning segs to the
  column's r + band square is valid for the ridge pass ONLY because
  its near >= 2 predicate masks the case where a far-but-relevant
  seg exists; with every segment excluded the ridge law's `continue`
  fires first. A hidden column has no such mask: dmin = infinity
  would silently report "reachable" for deep material. The full
  O(columns x segs) distance loop is therefore deliberate; it is the
  honest half of the cost of the union. Cost to change: a proper
  XY BVH over segs with an exact lower-bound - the window's
  failure mode, not its shape.

## M2.2c - Scallop as the diagonal cross

- **Gate speed, measured and then halved: the kernel tests are
  profile-invariant, so the overnight ladder can run them in release.**
  The scallop dome envelope costs 1073 s single-threaded in debug (the
  O(moves × facets) sweep is the point of law (f) - brute force, not a
  formula). Rust's f64 is IEEE and nothing here fast-math-reassociates,
  so the release profile computes the same bits: `golden_cl` passes
  15/15 under `--release` against the debug-regenerated goldens,
  0.29 s wall. Discussion point: whether future milestone gates run
  the envelope-kernel binaries in release by default and reserve debug
  for the refusal/round-trip suites.

- **Scallop ships as the ±45-deg crossed raster - NOT as a
  curvature-adaptive family.** The roadmap's bullet is "constant cusp
  height", and the industry name carries two readings: (a) the
  crossed 90-deg pair CAM tools ship, and (b) the research-grade
  scallop whose step direction follows the surface's principal
  curvature at every point. We implemented (a), rotated 45-deg from
  ParallelMulti's axes so the third option genuinely adds coverage:
  on this codebase's prismatic models every linear feature runs along
  X or Y, and the diagonals are the only directions no single-family
  raster crosses head-on. Discussion point: (b) is the honest M2.6
  candidate (flow-field families, per-point sec-law); if the user
  expects scallop to run FEWER passes than parallel at the same
  cusp, that expectation is not met here, deliberately - see next.
- **No sqrt(2) stepover discount, and why - the union-of-removal
  argument.** The folklore claim "crossing halves the cusp, so step
  over sqrt(2) wider" is FALSE for a plane: every point of the grid
  lies within s/2 of a line in EACH family, and the removed height at
  a point is the MAX over passes (material falls if any ball uncovers
  it), so the crossed pattern's worst point - the cell center, s/2
  from both - is exactly the single family's worst point: s^2/8r.
  The crossing earns its keep on CURVED and prismatic terrain, where
  a feature parallel to one family is cut diagonally by the other;
  the tests measure that verdict instead of assuming either factor.
  Cost to change: one line in `scallop_stepover`'s use if a future
  measurement ever shows the discount is real for a specific
  pattern - we do not ship unearned stepovers.
- **Persistence: a new WORD is not a new GRAMMAR - v16 stands.** The
  v16 bump at M2.2b was for two new FIELDS (needed serde defaults on
  old files); `scallop` is a new spelling in the existing pattern
  field: no v16 file carries it, every v16 file still reads
  byte-identically, and an unknown word refuses as Corrupt at load
  (tested) rather than defaulting. The stamp follows automatically:
  `pattern as u8` = 3 for Scallop, so switching is Stale-green like
  every other param move.
- **The scallop golden rides the plateau, not the cone - the razor
  rule for goldens.** The cone's flank facets sit at m_z = cos(45)
  EXACTLY on the SPLIT_MZ jurisdiction line; a float wobble across
  that equality flips a facet from shallow to steep and churns the
  golden for non-law reasons. The plateau's shelf (m_z = 1) and walls
  (m_z = 0) are nowhere near the line, so `finish3d_scallop_plateau.cl`
  pins the diagonal crossing, the break law and the ring handover
  with zero razor. (Waterline's cone golden predates the raster and
  never reads `shallow`, so it stays.)
- **The scallop test flipped a bug hunt: `verify`'s Crossing law is
  blind in Envelope mode.** The first scallop battery went red with
  ~100 `Crossing` warnings on dome, plateau and tilt - while the
  envelope kernel called all three paths clean. The two verdicts
  disagree because the law predates crossed rasters: "two feeds
  cross in XY at shared depth" was written when that phrase could
  only mean a chord dragged across a pocket, and a scallop makes it
  mean "the two families did their job." Standing it down in
  `Mode::Envelope` (law comment at the gate, module doc updated) is
  the honest shape - the chord that could actually hurt there is a
  chord biting into or sailing above the surface, and law (f)
  measures exactly that, move by move. The 2D modes keep the law
  byte-identical (the verify.rs tests at `Mode::Area` still assert
  it fires). Side effect worth knowing: the law was quadratic in the
  move count, and the finish3d battery spent 1866 s - most of it in
  Envelope-mode verifies that were about to report only by-design
  crossings. Expect the suite to run dramatically faster.
- **A fixture bug, not a pattern bug: the tilted scallop's "gouge."**
  The one failure after the verify fix put 1.09 mm of gouge at a
  corner of a `tilted(30deg, half = 60)` slab - a mesh whose tilted
  top falls below its own base and 30 mm above the op's declared
  band. Tangent balls cannot gouge a plane, so the kernel was
  correctly measuring a self-intersecting model. Switched to the
  `half = 8` fixture the cos-law test actually uses (comment states
  the law). Discussion point: nothing today refuses a non-solid
  model at import or generation - a validation pass for the
  M3-era audit list, if the user wants degenerate-mesh refusals
  priced in.
- **Scallop reuses `parallel()` end to end** - same raster_pass,
  cos-law, flush/break laws, ring auto-split, refusals, stepover
  override. The variant buys its two directions in the match arm;
  every raster refusal test is a `scallop_shares_the_raster_refusals`
  call, not a copy of the parallel set.

## M2.2b - Parallel raster, the 45-deg auto-split, and two bug hunts (commit `ac7413c`)

- **The underside "gouge" was our fixture, not the generator - and the
  reversal matters.** Mid-milestone we believed `facet_polygon`'s
  symmetric forbidden-center set (a facet ⊕ a ball of zero thickness
  = valid centers on BOTH sides of the sheet) was an underside bug and
  had a candidate-offset fix (`q + r·n̂`) designed. It was never
  written: the tilted test mesh was a single open sheet, and an open
  sheet has a phantom back side a ring lawfully rests on that no
  height-field measure can tell from material. Closing the fixture
  into a wedge made the "gouge" vanish with zero generator change -
  the symmetric law is correct for closed solids, which is everything
  we machine. Discussion point: whether the verifier should REJECT
  open shells at import with a "not a solid" diagnostic, rather than
  letting a phantom-back-side path through at all; today we generate
  on them and the envelope measure honestly reports the rest they
  leave. Cost to change: an orientation/enclosure check in
  `decompose`, but it would refuse files current tests feed on
  purpose.
- **The ring arc-fit budget: `tolerance * 0.25`, not full tolerance.**
  The region boundary is already tessellated, so its vertices sit a
  chord-sag INSIDE the true offset locus; letting `fit_arcs` shortcut
  its own samples at full tolerance stacks a second sag on the first,
  and it shows at corners where the vertex-sphere radius
  ρ = √(r² − (cz−q_z)²) changes fastest (measured: a fitted R=6.3
  circle cut 2.43 from a corner whose true locus is 2.52 - a 0.144 mm
  gouge at 0.01 tolerance). The raster already fit its boundary chains
  at tol/4; the ring now matches. Law: any fitter whose input is
  already tessellated gets the quarter budget. The envelope verifier
  caught this with its own numbers, no assertion was weakened.
- **The None-gap flush law.** After a line segment resolves to air
  (occluded or unsupported), the next supported run must START a new
  piece, not extend the previous arc chain - straddling the gap let
  the blind-pocket floor gouge until the fix landed. Same family as
  the discontinuity-break law: a raster line is a promise of
  continuous contact; where the contact breaks, the path breaks.
- **Multi-direction is two raster passes with retract hops, not a
  rotated toolpath.** 0° then 90° over the same levels, each pass
  individually continuous. It doubles the plunge count; the posting
  question (hops vs full retract at M1.7) rides the same answer as
  the XY pass's hops.
- **`retype` now defaults Finish3D to Parallel, not Waterline.** A new
  3D finish op on a flat-topped plateau with Waterline alone would
  leave every flat shelf standing proud of tolerance - a silent miss,
  the one answer this app never gives. Parallel covers flats AND
  steep flanks (via the auto-split); Waterline remains one combo
  click away. Cost to change: one line, but every "fresh op" golden
  and UI test follows it.
- **Stepover override: stored ≠ derived, and Some() is legal under
  every pattern.** The params carry `Option<f64>`: None means the
  cos-law derivation, Some means the machinist's call wins even where
  it is coarser than tolerance demands (that is often the point -
  cusp aesthetics, cycle time). `is_sane` checks positivity only; the
  pattern that ignores it (Waterline) says so in its own generate, not
  in a refusal.
- **Height-field occlusion means hidden rest is invisible - deferred
  to M2.2d with eyes open.** Under a ceiling, the envelope's
  per-column "part" is the topmost surface, so material sheltered
  below it never appears in the rest measure, and the floor-outranks
  law keeps the raster on the ceiling where that is the support. The
  hidden-floor test now asserts exactly what can be KNOWN (path never
  below bottom, verifier silent, ceiling ridden) and its comment
  names M2.2d's rest-reporting job: a true hidden-rest answer needs
  the solid-truth (inside/outside) scan, not the height-field one.
- **The headless gate was racing: `drain_actions()` is one poll, and
  imports went async at `e867bea`.** smoke.rs now calls
  `State::settle()` (blocking drain, 120 s deadline). Every milestone
  since the async import ran this gate on a race it happened to win -
  found here because the tilted fixture changed the import's timing,
  not its geometry. Discussion point: whether `settle()`'s deadline
  should be shorter so a hung import fails the gate fast instead of
  after 120 s.
- **The golden churned honestly:** `finish3d_waterline_cone.cl` went
  118 → 121 moves from the ring's quarter-budget fit (two rings
  gained a corner arc - the fix working), reviewed segment by segment
  before `WRITE_GOLDEN=1`.

## M2.2a - Waterline contouring and the envelope kernel (commit `c755b12`)

- **The outer-loop-only law: cut what the region encloses from outside.**
  The forbidden-center region's CW loops are interior wavefronts - the
  ball cannot reach a hole's forbidden disk by rolling on the model's
  outside, so those loops are unreachable geometry, not missed rings.
  Islands are separate CCW components and get walked. Discussion point:
  a model whose interior cavity opens only through a smaller neck would
  need a reachability test to be contourable from the inside; we do not
  attempt inside-out waterlines at all yet (and no test asserts we
  would notice). Cost to change: a component-reachability pass before
  the outer filter, ~30 lines, but it re-opens the nesting question
  with the sketch-parity law that union_all(NONZERO) closed.
- **Two truths, each by its own law.** Gouge is dexel truth (what the
  ball's lowest points removed, column by column); rest is distance
  truth at RIDGES - surface points with at least two band contacts.
  Why two laws, not one grid: "this pass rolled a ladder over you" is
  the pass's promise, and the promise is exactly two near-chords - a
  once-grazed point (a lone annulus) or untouched point (a flat shelf,
  the crown, a vertical wall) is rest machining's or a later op's
  jurisdiction, law there, not a silent zero here. The tempting gate
  "rest only where this pass cut the column" was PROVED dead: a ball
  reaching a column's surface is the same event as a center standing
  inside `r + floor` (it rearranges algebraically), so such a gate can
  never report the scallop it exists to measure. Cost to change: the
  ridge count is one comparison in `envelope_scan`; a different
  promise-band law touches nothing else.
- **The exact-arc collapse - and what it broke in our own tests.** The
  region fixes (union_all + outer filter) replaced clipper-boolean
  polygons with exact analytic circles, so `fit_arcs` legitimately
  collapses each ring to two or four exact arcs (180-degree
  semicircles on a true circle have zero sagitta - the arc IS the
  ring). Paths became ~5x shorter (the cone golden: 118 moves, 32
  arcs; the suite: 54-180 s where it ran 930 s). Two consequences
  logged: (1) an endpoint shoelace over such a ring is a sliver
  between antipodal vertices, not its footprint - the mirror-walk
  test now samples arcs <= 15 degrees before taking the area (and the
  box test now asserts the measured truth, `gouge == 0, rest <=
  gate`, where the corner columns honestly read a sub-gate cusp from
  the two wall rings grazing at right angles - silence was always the
  VERIFIER's word, not the scan's); (2) semicircular 180-degree arcs
  post fine under RS-274 single-block IJ format, but the M3.x posts
  should note multi-block arc splits are not needed for waterlines.
- **Hop law, finish flavor:** the inter-ring rapid rides
  `min(retract_z, clearance)` - a level change still climbs to
  clearance; `retract_z` non-finite refuses at the params check. Same
  shape the 2D ops established, tested on twin domes (one dome has
  nothing to hop between - the fixture says so).
- **The probe-before-assumption lesson, filed:** the "13/13 green"
  log we trusted predated the region fixes; the first real full-suite
  run after them failed two tests that had never actually been re-run.
  The geometry had gotten BETTER (exact circles) and two test metrics
  had quietly become invalid. A stale green is the same failure class
  as a silent gouge - re-run the binaries after any law change, even
  when the theory says "this cannot change the answers".
- **Walls are column-invisible; shelves are parallel's.** The dexel
  model sees one z per XY, so a box's vertical faces carry no surface
  sample: waterline rings graze their rims tangentially and the box
  finish is structurally left to the parallel strategy (M2.2b) - the
  test documents the jurisdiction rather than pretending the envelope
  watched the walls. The module's entry law stands: overhangs (under-
  cuts the ring can't see coming) refuse LOUD, verified by the same
  scan.

## M2.1d - Engagement-based feed modulation (commit `b164c69`)

- **The median-anchor law: the path's own median loaded engagement sets
  what runs at the programmed feed.** f = F0 · clamp(ae_median/ae_move,
  FFO_FLOOR, 1). Why: the textbook alternative — derive ae from the
  nominal stepover (ae = step/2r, or the M2.1c sagitta relation) —
  under-reads whole strategy families: an adaptive ring engages the full
  core on its inside, a zigzag's links engage the fresh shelf ahead, and
  neither is a "stepover number". Measuring against the dexel field that
  already exists costs one dry sim run and is strategy-blind by
  construction (the dexel-as-verification-layer law again: measure, never
  derive, from the sim). Cost to change: the anchor lives in one block of
  `optimize_feed`; a nominal-formula variant would replace ~10 lines.
- **Never faster, floor 0.25: the op's feed word IS the modeled tool
  limit.** Modulation only slows toward it; light bites never bank speed.
  Why: speeding up needs a spindle/torque/power curve this build does not
  carry, and a silent speed-up is exactly the class of surprise the Prime
  Directive forbids. FFO_FLOOR = 0.25 is the handbook's roughing
  slow-down band's safe end and keeps a near-tangent read (ae→0) from
  dividing out toward a stopped tool. Discussion point: with a real
  machine file (M3.3's) a >1 band could open later, ceilinged by it.
- **The two-station chord, found by a median collapse.** v1 sampled one
  perpendicular chord at each move's midpoint; on the flat slab the
  modulation was inert — every F word stayed at the programmed feed.
  The histogram led to the two pathologies: stepover links read ae=0 (the
  chord across their travel sits fully in the trough just cut — their
  fresh material is *ahead*, along travel), and the level-1 skin moves
  read ae=0 honestly (cut plane == stock top: `top > z` is false
  everywhere). Fix: average the perpendicular chord at mid-move and at
  3/4 along the move — the trapezoid of the swept side width; a link
  now reads ~0.5, which is what it cuts, and rows are unmoved. Same
  lineage as M2.1b's phantom-cutter lesson: the measurement window must
  match what the move sweeps, and a law test now pins that the first cut
  of a level is its slowest kind.
- **Loaded-only median; air keeps the programmed feed.** Zero-engagement
  readings hold no vote in the anchor and modulate by ratio 1 (no load,
  no reason to slow — and never below the floor's division by ~0). If
  *nothing* in a path is loaded, the pass returns the path byte-identical
  — the rapid-only identity law test.
- **Planar moves only.** Plunges, ramps and helix feeds keep their
  assigned plunge_feed/feed: axial engagement is a different curve
  (chip thinning vs depth, not width) and modulating it needs a model we
  did not write. Honest scope note, not an oversight — the doc comment
  says so at the function.
- **Placement: `feed_opt: bool` on the params (the rest_rough pattern),
  stamped, v14 step-up a serde-default no-op.** It changes every F word
  it touches, so flipping it changes the stamp — old paths go Stale
  exactly when the answer changes. Absent in a v13 file means off: those
  files post byte-identical to before.
- **The post needs no change** — M1.7a's per-move F-word representation
  pays off: modulated feeds flow through `.cnc` and the Fanuc post with
  zero new machinery. One sim run per generate when the toggle is on is
  the only cost (the rest pass already runs one; discussion point if
  users find generation slow on big pockets).

## M2.1c - Constant-engagement adaptive clearing (commit `70c356b`)

- **The engagement sagitta law is the strategy: step = r·(1 − cos θ).**
  Chord geometry, published HEM literature (the 7–15 % radial-engagement
  band roughers are specced in). Why: it makes the ring family a function
  of tool radius and a single angle the user can reason about — the
  scallop on flats then follows from the same geometry — instead of a
  stepover number that silently changes load with wall curvature. Cost to
  change: the law lives in ONE function,
  `machining3d::engagement_step(tool_r, deg)`, read only through
  `Pocket3DParams::tiling_step()`; the sim cell (`min(r, step/2)`) and the
  offset machinery already key off `tiling_step()`, so any restatement of
  the law is one line.
- **Adaptive ≡ Waterline at the law step — the *tiling law*.** Both the
  3D `PocketStrategy::Adaptive` and the 2D pocket's `Adaptive` alias feed
  the same ring engine (`plan_passes`/waterline) that Waterline uses, only
  with the step computed by the engagement law instead of typed in;
  "Adaptive rings are Waterline's geometry; who chose the step is the
  caller's business" is the comment that keeps them from diverging. Why:
  one engine, one set of correctness laws; a second ring generator would
  double every join bug we could ship. Cost to change: low (a strategy
  enum arm), but then the join laws below must be proved twice.
- **Ring closure is a property of the point DATA, never a motion the
  walker adds (`close_rings` removed).** `waterline_passes` emits every
  loop with its start point repeated; `walk_pass` walks and adds nothing.
  Why: the old flag's per-level closing diagonal dragged one giant
  last→first feed across chained multi-ring passes — the crossing a verify
  test caught — and its predecessor had been *keeping the old balance law
  green* with a phantom edge; a tour that needs a synthetic move to close
  was never closed, it was papered over. Cost to change: none — moving
  closure back into the walker re-imports the phantom.
- **Containment is not non-crossing (`chord_clears` +
  `chord_crosses_path`).** A join chord proven inside the cutter-center
  area can still cut across another planned ring of the same level, so
  phase 2 now plans every ring of the level first and chains only along
  chords that properly cross none of them; the test is verify's Crossing
  criterion restricted to one chord, so an accepted chord is verify-clean
  *by construction*. Why: the Prime Directive — silent crossings are the
  worst failure, and a plan that verify would refuse must not be made.
  Cost to change: none; it is a correctness gate, not a preference.
- **Chords from different chains can cross without touching any loop —
  the cut sweep.** A shattered ring family (the boss fixture's k=3 level:
  the hole offset pokes past the box offset, four 31-pt corner slivers
  remain) links its sliver entries with chords running along the broken
  wall, and those cut across a spiral's closing chords; acceptance-time
  tests are blind to chords of not-yet-built chains. Fix: build chains,
  test every cross-chain chord pair, cut the later-listed one into an
  honest rapid hop, iterate — cutting only shrinks chord sets, so the
  sweep converges. Why: a hop is a machine movement that is TRUE; a
  crossing feed is a crash. Cost to change: none — it's the same
  non-crossing law, applied where the first version couldn't see.
- **The per-level tour-closure law, restated where close_rings died:**
  on each cutting plane every XY is feed-entered exactly as often as it is
  feed-left, except the level's two honest open ends (the plunge landing
  point, the level handoff point). Why: the old global-balance test only
  passed because of the phantom closing edge; this version says what the
  tours actually do (an open zigzag still fails it, which is the point).
  Cost to change: none — it is the test, not the policy.
- **`max_engagement` legality: (0°, 90°], default 45°, only read when
  `strategy` is Adaptive** (`is_sane` guards; 90° = half-radius step,
  beyond that the sagitta law's chord model is meaningless for a
  round tool). The 2D pocket's Adaptive needs no new field — its alias
  shares the stepover gate. Persistence v13 is a pure `#[serde(default =
  "default_engagement_deg")]` no-op step-up: old files open on
  Waterline-shaped Adaptive defaults; stamp strategy index
  `Adaptive = 3`. Cost to change: a wider range rewrites the is_sane arm
  and the page tooltip, nothing else.
- **Corner-contact geometry and engagement-based feed modulation stay in
  Feed-rate optimization, not here.** This milestone makes lateral load
  constant by SHAPE; trochoidal slots, corner-aware step handling and
  feed-rate along the path are load handled by MOTION/SPEED. Why: they
  are separately testable laws and the ROADMAP already lists them as a
  bullet. Cost to change: the enum arm for trochoidal exists in spirit
  only — no stub was added to tempt a half-law.
- **Freedom-to-operate review WAIVED by the user, 2026-10-19** ("Ignore
  any patent claims for now. Clean room or not, this is an opensource
  project with no monetary gain."), against ROADMAP's standing pre-M2.1
  note. The clean-room protocol is unchanged: HEM here is derived from
  published technical literature (sagitta/chord geometry, the
  engagement-band rule), no vendor implementation inspected. Cost to
  change: revisit before any distribution beyond open-source; the
  derivation comments in each module are the evidence trail.

## M2.1b - Dexel material simulation and rest roughing (commit `4e077ae`)

- **The dexel grid is the VERIFICATION layer, never a second source of
  truth.** One number per column - the Z of the top of standing material -
  and every verdict (rest, gouge, removed) is measured against the part
  surface read off the MESH's own facet planes, never a re-entered
  approximation. Why: the ROADMAP's standing rule for any discretized stock;
  a grid that generates while a mesh verifies is two worlds that drift.
  Cost to change: none structurally - M2.3's color map shades this same
  field; only resolution policy could move.
- **The segment-window cut law, learned the honest way: a test caught a
  phantom cutter.** `cut_chord`'s reach parameter window must be
  intersected with `[0, 1]`, and an empty window must cut NOTHING - the
  first version clamped the window's ends instead, which turned "the disc
  never reached this column on this move" into "the disc cut at the
  segment's endpoint anyway": a silent wrong answer exactly in the Prime
  Directive's forbidden class, and it had been *hiding* the corner-rest
  residue the fixed sim then honestly reports (15 false gouge cells became
  0). Why noted here: it is the strongest evidence yet that the sim exists
  to be tested against, not trusted. Cost to change: none - the clamp is
  the bug, the intersection is the law, pinned by
  `an_oblique_chord_cuts_the_reach_interval_not_the_endpoints`.
- **Chords under-sweep, so the sim over-reports rest and never invents a
  gouge.** Arcs ride `arc_chords` at `CHORD_TOL = 0.01` mm (the same
  sagitta law as everywhere else), Z interpolated at each chord endpoint;
  rapids touch nothing (G0 practice). Why: the one asymmetry worth having -
  a rest pass that chases ghosts is waste, a sim that flatters a gouge is
  danger. Cost to change: tightening the tol is a parameter; reversing the
  direction is not ever.
- **Corner rest is reported, never hidden.** A round disc has no legal
  center that reaches a square pocket's corner columns (distance > r to
  every point of the eroded center region), and the honest fixtures say
  so: 4 cells on the staircase grid, 8 on the flat slab (3+2+2+1 - grid
  phase puts the lattice 0.75 mm nearer one pair of walls), four boss
  wedges + four pocket wedges on the slab-with-boss, ≈2.9 % of removed
  volume. The pre-fix sim "passed" those tests by phantom-cutting them.
  Why: this is standard CAM reality (corner radius of a round tool), and
  the fillet / point-plunge strategies (M2.1c+ rest work) are what close
  it - not a rounding in the report. Cost to change: a fillet pass would
  make the numbers smaller, never the law different.
- **Rest entries are always plain plunges, by the nesting law.** Deeper
  plane ⇒ larger island ⇒ smaller target, so any standing column is
  superset-clear above and a vertical dive is provably legal; Ramp/Helix
  arms are guarded OFF when a rest ladder is running (`li > 0 &&
  rest.is_none()`). Why: a diagonal entry through standing stair material
  would be a lie the level law never sanctioned. Cost to change: low, once
  per-level side-clear proofs exist.
- **The rest ladder's all-clear is SUCCESS, not an error.** A fully-cut
  pocket has empty targets at every level; `GenError::Empty` from the rest
  run is caught, the path byte-identical to the plain ladder (prefix
  identity is asserted in-test as the determinism witness). The rest
  ladder runs at HALF the stepdown (`cut_levels` at `stepdown * 0.5`) - the
  convergence knob; deeper ladders are the same call with a different
  constant. `target ∩ dex.above(z)` makes each level cut only where
  material stands, so the staircase floors close without touching the
  finished columns. Cost to change: the fraction is one line.
- **`rest_rough` rides `OpParams::Pocket3D` only** - stamped into the
  digest (a flip marks the path Stale), undoable via the existing
  EditOperationParams flow, and the generator struct
  `machining3d::Pocket3DParams` stayed untouched (ten test literals' worth
  of churn saved). v11→v12 persistence is a pure `#[serde(default)]`
  no-op step-up - old files open with the checkbox off. Cost to change:
  moving it into the generator params means touching every direct
  generator call site.
- **Grid law (`sim_of`): grid = footprint bbox, cell = min(r, stepover/2),
  each column starts at `max(stock_z, part top)`.** The raise is the
  "stock contains its model" invariant - a model standing above the
  entered stock surface is still material the sim must show standing.
  `measure` counts removal from `stock_z` (gross, the tool's verdict) and
  judges rest/gouge against the part at `tol = EPS`. Cost to change: cell
  policy is the performance knob for big models (M4), structure is sound.

## M2.1a - Mesh-driven 3D pocket (commit `7672c06`)

- **The level law is the whole design: the model is the FINAL part, and
  each level cuts `footprint ∖ material_above(z)`.** A level's target
  region is the silhouette minus the columns of solid still standing at
  that plane, so islands are computed from the mesh at every Z, never
  picked; a boss floors its own columns at its own surface (at most one
  stepdown of stair), and the last level snaps to `bottom_z` so a flat
  floor comes out exact. The slice is probed at z+ε with a 3-try nudge
  because the half-open chord law would otherwise count a plane exactly
  through a vertex as filled. Why: any other rule (section-at-z,
  picked islands, fixed ZMax ladder) either re-derives geometry the
  mesh already states or cuts into what the part still needs. Cost to
  change: it is the generator's backbone; rest-material passes (M2.1b)
  compose with it rather than replace it.
- **Descent legality is asked of the PREVIOUS cut level's center, not
  the current one** (the superset/nesting law: deeper plane ⇒ larger
  island ⇒ smaller target, so the tightest region a dive can traverse
  is the one above it). Skipped (empty) levels keep the older, deeper
  state. Why: a ramp/helix chord from prev_z down crosses only regions
  the ladder already proved open; asking the looser current level would
  dive through live wall. Cost to change: none — it's one `Option<(f64,
  Region)>`; loosening it would be the bug.
- **The ring-close is strategy-given, not inferred from the points.**
  A waterline pass is one SPIRAL of concentric rings (first point is
  the outer ring's start, last is the inner ring's), so it must be
  closed by a feed back to its first — that chord runs the 45° corner
  line and touches every ring only at corners, which no law counts as a
  crossing. A zigzag pass is an open scan: closing it would drag a
  diagonal across every row. Nothing about the point list tells the two
  apart (both end first≠last), so `walk_pass` takes `close_rings` from
  the strategy itself. Why: the original `if a != last` inference was
  wrong in both directions — zigzag diagonals spammed Crossing, and
  deleting it broke waterline's XY start/end multiset law. Cost to
  change: none; keep it a law of the strategy.
- **Verify's crossing law now separates planar from non-planar chord
  pairs**: two flat chords cross only at a shared plane (bit-identical
  to the old test, so 2.5-D goldens don't move); a pair where one
  descends flags only when their Z-intervals share positive length —
  a single-plane touch is a GRAZE of already-cut floor at the dive's
  own start/end depth, which the superset law above is what makes
  safe. Why: every 3-D entry crosses the previous level's rows at
  exactly that endpoint plane; the old closed-interval test flagged
  all 67 of them. Cost to change: the graze's safety proof would have
  to move into the generator; keep the two laws paired.
- **Helix dive point: midpoint of the longest straight run of the plan,
  membership-proved in both the previous and this level's centers
  eroded by (helix_radius + tol)**, and the swept-disc link checked to
  the entry point; any failure falls back to a plunge ("a slower dive,
  never a gouge"). The first level can only ever plunge — there is no
  shallower plane to start the descent from. Waterline + Helix
  effectively always falls back (the spiral's runs are short arcs):
  honest but worth a UI note someday. Why: "first zigzag midpoint" was
  the old shape; longest-run picks the spot the widest disc fits.
- **`material_above` is pub in machining3d and verify caches it
  (`islands_at`)**: one law, three readers (generator, gouge/shank
  island walls, M2.1b's simulation) can never drift. Cost to change:
  if the law ever parameterizes, the cache key is the (mesh, z) bits —
  revisit then.
- **An ε-grazing shelf cuts tangentially and returns `Ok`, not a
  refusal** — a block exactly filling the footprint gets its top face
  as one real path; the levels under it are all island and skip. The
  degenerate test pins it. Why: refusing would be honest-but-noisy for
  a shape that is a legal skim.
- **Ribbon went from fixed imgui pitch to a FLOW row: cells share the
  strip width, `CELL_W = 56` stands as the cap.** The 7th Create cell
  silently pushed the Simulate button off-screen at the 1280 default —
  a real product bug the kittest caught as a failed click, not a
  coordinate-math miss. Floor is 40 px, below which the row overflows
  on purpose rather than render illegible. Why: M2 adds more operation
  kinds; a fixed pitch re-discovers this cliff at 22 cells. Cost to
  change: both snapshots churn (they already churned once for the new
  cell); the layout law is `ribbon_cells` alone.
- **`OpType::Pocket3D` reuses `Icon::Pocket`** — no new asset; the 3-D
  kind is the pocket idea with computed floors, and inventing a
  near-identical glyph costs a PNG and a mental distinction nobody
  asked for. Cost to change: one asset + one `icon_of` arm when M2.1's
  siblings (adaptive, rest-rough) need their own row anyway.
- **Persistence v11 with a pure-stamp step-up** (the `pocket3d`
  ParamsDto tag rides the new enum arm; no field migration), and the
  law made explicit in `wrong_version_is_refused`: the too-new probe is
  always PROJECT_VERSION+1, so the test never rots on a bump. The v10→
  v11 round-trip test covers the grammar.
- **Test-fixture law discovered the hard way: the box's ±x walls are
  `quad(1,3,7,5)`/`quad(2,0,4,6)`** (bit0→x, bit1→y, bit2→z corner
  convention) — the diagonal-looking alternatives are diagonal PLANES
  through the cube, and a non-watertight shell slices into two lens
  chords with hollow centers, which poisoned three tests as a phantom
  "island emptiness" bug. Watertightness of hand-built facets is now
  worth checking edge-by-edge when a slice result looks wrong.
- **FTO note** (the open-questions section asks for it before M2.1):
  M2.1a ships multi-level Z machining with mesh-derived islands —
  published-literature general knowledge, nothing HEM/trochoidal in
  it. The patent-sensitive bullet is adaptive clearing (M2.1c); the
  formal review belongs there, before that code lands, not retro-fitted
  onto the pocket.

## M1.7e - CI matrix, release packaging, v0.1.0 (commit `3305468`)

- **The CI matrix is one axis: `windows-latest`, structured to widen.**
  The job runs on a `strategy.matrix.os` list so adding Linux is one
  line, but the list holds only the platform this project has proven:
  the flecs alpha bindings, the wgpu fallback adapter the GPU tests
  request, and the headless `run()` path have never been executed on a
  Linux runner. Why: "all tests green" is the bullet — a second axis
  that flakes or fails makes the CI a lie, and green over breadth is
  the Prime Directive's CI-side reading. Cost to change: run the ladder
  once on an `ubuntu-latest` box (or a WSL check locally), fix the
  fallout, add the entry.
- **0.1.0 ships as a portable zip, unsigned, no installer.** The bullet
  said "signed installer + portable zip"; we ship the zip and defer the
  rest: no Authenticode certificate is owned, and an MSI layer (WiX or
  similar) would be an unvalidated moving part in the first public
  build. The zip carries `hsmworks-ui.exe`, our `post.exe`, and the
  `assets/posts` + `assets/machines` files the CLI reads from disk —
  the whole product surface. SmartScreen may warn on first run. Cost to
  change: get a cert, add a sign step before `Compress-Archive`; if an
  installer is ever wanted it is additive, not a rework.
- **The release version comes from the binary, not from parsing
  metadata.** `post.exe --version` stamps `CARGO_PKG_VERSION` itself, so
  the packaging script splits it from the built artifact. This came out
  of running the workflow script locally before committing it: the
  first draft read `workspace_version` out of `cargo metadata` via
  `ConvertFrom-Json`, which came back *empty* under PowerShell 5.1,
  silently producing `hsmworks--win-x64.zip`. Why: the artifact we ship
  is the version's single truth — and the episode is the argument for
  smoke-testing release scripts locally, not just on the runner. Cost
  to change: none worth taking.
- **`v0.1.0` is a local annotated tag for now; `release.yml` is inert
  until the repo has a remote.** `git remote -v` shows none — the
  workflows are committed so the moment the repo is pushed the CI and
  the tag-triggered release publish themselves, but today the honest
  release artifacts are this zip under `dist/` plus the tag as the
  milestone restore point (the standing "make a restore check-in / tag"
  instruction, satisfied at each phase boundary; `pre-egui-spike` was
  the other). Discussion point: where does this repo actually get
  published?
- **Version 0.1.0 does not pre-empt the clean-room audit.** AGENTS.md:
  "Version is 0.x.0 until the clean-room audit and release" — 0.1.0
  keeps the 0.x.0 shape, and RELEASE_NOTES.md states plainly that the
  audit is still ahead. If the audit finds anything, it lands in 0.1.x
  or the next minor, not a forced renumber.

## M1.7d - G-code viewer with scroll-synced toolpath highlight (commit `98bf320`)

- **The viewer shows what the post kept, not what the file says.**
  `posting::Posted` now carries the exact program text plus a per-line
  position map out of the same engine run; the UI never re-reads the
  `.nc` file. Why: text and positions must be one artifact - if
  something edits the file afterward (or the dialog target moves),
  highlighting a line must not silently disagree with what the viewer
  printed. Cost to change: a file re-read + parse pass, and the sync
  law weakens to "probably the same file".
- **Line positions resolved app-side from the stream's own laws**
  (`viewer_map`): one Move record per path move in chain order (the
  dump's law), so the k-th Move record is the k-th global move - the
  same counter M1.7b used to seat cycles' `first_move`; a canned-cycle
  line resolves to its first expanded move. Coordinates stay
  pre-origin-shift world mm (the script applies the shift), i.e. the
  numbers the viewport already draws - a highlighted line and a
  highlighted move cannot drift apart by construction. Cost to change:
  none worth naming; this is the seam the viewer was designed around.
- **`State.post_view` is frozen evidence, unlike the sim clock.** The
  M1.7c law ("rebuild from the world every frame, no resets") applies
  to the timeline; it deliberately does NOT apply here: a program is
  the record of one specific post, so New/Open retire it, but
  in-document edits and undo leave it (it truthfully says what was
  posted, and re-posting replaces it wholesale - a refused post
  touches nothing, same gate law). Cost to change: pick which edits
  mark a viewer stale; none felt honest short of a diff view.
- **The scroll cursor is measured, not stored.** Uniform monospace
  rows (spacing zeroed, `add_sized`) let the ScrollArea clip rect's
  center line divide back to the middle row each frame - no offset
  state to drift, no per-row responses. The viewport highlight is then
  one pure resolver (`shell::gcode_segment`): previous stored position
  to current, comment lines rest the cursor, closed window shows
  nothing (hidden is display, M1.7c's law reused). Cost to change: a
  virtualized list would need the stored offset again; 2^3 lines is
  fine un-virtualized.
- **ROADMAP honesty sweep while tick-listing M1.7d**: the M1.7a/b
  bullets (validation gate, post engine + CPS, Fanuc post) shipped in
  `13eff0d`/`d30f39a` but were never ticked - done here, ✅ with
  nothing added to their text but the gate note. The table row stays
  📋 until CI and the release build (M1.7e) land.

## Regression fix — the origin anchor picker returns to the Origin page (commit `fix(ui): restore the origin anchor picker on the Origin page`)

- **What was wrong**: the 3x3 compass grid, the stock-vs-model source
  choice, the Z-level radios and the "Pick markers in viewport" toggle all
  lived in the imgui setup pane, and the egui port never carried them over
  (retired un-ported at M-g, `6071415`). `interact.rs` kept its half of the
  law intact - the marker overlay, the screen-space hit test, the
  pick-beats-entity routing - so the symptom was specifically: crosses
  never render (nothing could set `pick_anchor`) and no grid to click.
  Found by `git log -S pick_anchor`, not by code archaeology in the live
  tree.
- **Restored shape, deliberately literal**: the imgui layout ported
  one-for-one - ComboBox for the source (imgui combo), three radios for the
  level, the checkbox, and the grid with the chosen cell bracketed
  (`[SE]`) as the imgui highlight law. Alternative considered: an egui-
  idiomatic segmented control or colored highlight; kept the bracket
  because the hand-eyeballs know that look, and the parity law for
  ported panes is "same fields, same order, same echoes" (pages.rs header
  comment). Cost to change: one widget block.
- **One-law preservation**: the pane grid calls `interact::anchor_points`
  - the same function the viewport markers and the pick hit-test read - and
  a click commits through the existing `SetWcs` draft law (write drafts,
  queue the command), so grid click, marker click and *Set Origin* cannot
  disagree. A viewport pick sets `anchor_xy` and queues `SetWcs` with the
  marker's position but leaves the pane's numeric drafts to
  `sync_setup_ui`'s next pass - the imgui-era behavior, kept, not
  "fixed".
- **Pinned by** `origin_page_shows_the_anchor_grid_and_arms_pick_mode`
  (ui lib tests): grid present, click places the slot on exactly the point
  `anchor_points[8]` says, the chosen cell brackets, and the checkbox arms
  the overlay - the last one being the exact half that regressed away.

## M1.7c — move simulation: transport bar, timeline laws, tool marker (commit `feat(app,ui): move simulation - transport bar, timeline laws, and tool marker (M1.7c)`)

- **The chain is truth; cycles contribute only dwell seconds.** `sim::script`
  walks the *expanded* move list the generators already produced - the same
  geometry verify and the `.cl` goldens speak - and lets each `Cycle` add its
  dwell to the move that reaches its bottom. No second, parallel move chain
  exists to disagree with. The agreement law between the `Toolpath.cycles`
  record (the post's source of truth, M1.6) and the walk is pinned in the
  app tests: per cycle, the deepest plunge of its group equals its
  `bottom_z`, and the dwell sum rides `total` exactly. Why: a rehearsal that
  drifts from the posted program is worse than no rehearsal. Cost to change:
  the whole dwell pass lives in one function.
- **Rapids are simultaneous-axis, paced by the machine file.** Time for a
  rapid is 60 * max(|dx|/rate_x, |dy|/rate_y, |dz|/rate_z) with the rates
  read from the `.mch` kinematics tree (the X/Y pair shares the XY rapid;
  Z owns its traverse rate) - never axis-traveled and summed, never a
  hardcoded speed. The post and the timeline answer to the same setup file.
  Cost to change: one `rapid_secs`; a 4-axis machine model (M3+) would add
  feed-per-axis table lookup there.
- **The simulation rehearses the machine's program, not the picture.** The
  sim op walk mirrors the post walk exactly - every operation, hidden ones
  included; the eye-off toggle is display, not suppression. (Revises an
  earlier draft note that would have excluded hidden paths; the display
  overlay keeps its own hidden-path skip - the two lists are allowed to
  differ *because* they mean different things.) Cost to change: `sim_script`
  in `shell.rs` gains a filter the post gate would then have to duplicate.
- **The timeline is rebuilt from the world every frame, and the playhead is
  immediate-mode view state.** `Script` construction is a pure world read,
  so a document switch, a regrow or an op delete changes the program under a
  playhead that never resets - same `t`, next frame. Arming, pausing and
  scrubbing queue no command (AGENTS.md's explicit exemption for playback
  position); undo is untouched by anything the transport does. Cost to
  change: caching the `Script` by doc digest would need invalidation
  plumbing for exactly the churn this design deletes.
- **Two honesty guards, drawn not phrased**: a stalled frame's dt is clamped
  to 0.1 s so the tool never teleports through material, and past 2^20
  segments the timeline truncates with a flag the bar shows - the same
  budget law the overlay uses. Arc timing interpolates the exact sweep
  (display and verify may flatten; the clock should not).
- **Transport UI law**: the bar draws only while the ribbon cell is armed
  (snapshot churn confined to the new Inspect cell itself, both images
  updated once with that diff as the whole story); Play at the end rewinds,
  Stop leaves simulation rather than pausing at a parked end; speeds are
  1/2/4/8x multipliers on dt, never a fake clock rate; the readout verb
  (RAPID / F-value through the units lens / DWELL / END / STANDBY) is a
  pure function pinned by test. The glyph (`assets/ui/sim.png`) is original
  artwork - no Tabler motion-path glyph fit the 20 px ribbon - and the
  LICENSES.md originals section was fixed to cover `post.png` (listed as
  original in code, missing from the notice file) alongside `drill.png` and
  the new `sim.png`.

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


## M2.2e stage E — the two surface-finish numbers (code `bf92a64` + this docs)

- **The staircase halts AT the floor, not past it.** The refinement
  guard is `delta > delta_min`, with halving `(delta·0.5).max(delta_min)`
  and growth `(delta·2).min(delta0)`. No chord shorter than the floor
  is ever emitted; before the clamp, one halving rung could dip under
  the floor and grow back, making the number a suggestion.
- **A floor past cruise is inert, and we keep it that way rather than
  refuse it.** Growth caps at `delta0 = scallop_stepdown(r, tol)`, so
  every floor past the cruise step plateaus: 1.2 mm and 4.0 mm produce
  byte-identical paths on the witness. Discussion item: `validate()`
  could warn that the number is above the march's own coarsest step —
  it contradicts nothing, so today it just stands unused.
- **The edges are floored by nobody.** The bisections that find where
  a run starts and stops answer to the material, never to a spacing
  parameter — the witness pins the entries equal across every floor.
  A minimum step is the machinist's number about cutting; piece
  boundaries are the model's.
- **Waterline ignores both numbers, byte for byte.** min_step lives in
  the march (the raster families), ramp_angle in the serpentine
  connector; the rings never route through either.
- **ramp_angle default = 45°, v17's gift spelled.** Shallow ground
  was already exactly the ground whose support climbs no steeper than
  one in one, so writing the law down changes no behavior — every
  pre-v18 path is byte-identical. Links that barely traverse
  (`d_xy ≤ tol`) are the vertical case the ramp law already settles;
  the gate stays inert there by design.
- **The corrugated-fold story the drafts told was wrong in
  mechanism.** A crease on faceted ground is a validity GAP (the
  tangent centers of the two facets are separated by the fold), not a
  straddle the chord law bites on: the march refuses past the gap and
  edge-bisects to the exact crease. What a floor truncates is the
  staircase of approaches INTO those gap edges — real, measured (136
  short chords → 70 → 48 as the floor rises), but not the "fold sinks
  a chord by sag(h)" picture the first drafts asserted. The dome is
  the other disqualified witness: every center is self-shadowing
  there (mid-valid, dev≈0), so min_step is provably inert on domes —
  a property of the law, worth a manual paragraph someday.
- **Float law for CAM asserts:** the test compares run entries at
  1e-4 mm, not byte-wise — bisection brackets are seeded from the
  sample grid, so a floor perturbs an entry's LAST BITS (2e-8 mm)
  while the boundary it converges on never moves. `assert_eq` on raw
  floats would have been testing IEEE, not CAM.
- **Wire v18, both halves honest:** `min_step` is skip-if-None (an
  option like stepover — None must not mint a key); `ramp_angle` is
  always written (like the hop plane — a default is a fact about the
  machine). A v17 file migrates to exactly what v17 ran: no floor,
  45°.

## M2.3 stage A — the program material sim and its two truths (code `8199322` + this docs)

- **The program sim chains cached ops in order; it never re-plans them.**
  The truth of what the machine will do is the posted toolpath, not a fresh
  re-derivation from parameters - a sim that re-planned could disagree with
  the .cl that ships. One judgment, at the end, over the ops as cached: that
  is the post gate's input shape (stage D) and the viewer's (stage B).
- **Deviation is located, not just counted.** The worst rest cell and the
  worst gouge cell carry an XY *and the sim's Z at that column* - the
  warning a user jumps to must name the place, and the place includes the
  depth the sim actually reached. Counting-only would send the viewer to a
  cell that averages nothing.
- **NaN is the no-verdict column, never a zero.** Where the stock never
  stood (outside a cylinder's round) or where the model has no surface (air
  in the part), the deviation field reads NaN and carries no verdict; a
  dressed-up zero would color-map as a clean bill of health on columns that
  were never in play. Determinism includes the NaN pattern - the color map
  redraws this field every frame and two runs must agree to the bit.
- **One atom law per tool shape.** Ball ops take the exact sphere sweep;
  flat ops take the exact disc at every station, which is exact for end
  mills and for drills and taps - the final hole IS the cylinder. The
  bull-nose corner is the one known-biased atom: the disc reads at the
  corner's lowest point, so beside a fillet wall the sim over-reports
  removal (under-reports rest). It is stated at the `SimOp` type, not
  hidden; when it matters the viewer says so. Discussion item: whether
  stage B paints fillet-adjacent rest as "unknown" rather than trusting
  the optimistic zero.
- **`simulate` validates everything before touching anything.** Two
  passes: op radii finite-positive and every move coordinate finite first,
  subtraction second. A half-run grid that looks like a sim is the silent
  wrong answer wearing a lab coat - the Prime Directive in this kernel's
  own clothes. A refusal leaves the stock exactly as it stood; op 2's
  poison cannot run the program.
- **Subtraction is monotonic, so rest has one meaning: "never reached."**
  Nothing leaves a plateau by cutting HIGH; rest can only be the program
  not arriving. This law nearly went untested: the first witness tried to
  show rest with a disc plunge that "left a plateau" - physically
  impossible, the plunge ties the untouched slab top and the located max
  landed on a far corner instead. The honest witness is a face traverse
  that stops short of its own margin; the fixer op then erases the rest
  while the gouge stands - rest and gouge have opposite time-arrow
  semantics and the test says so.
- **`auto_cell` IS envelope_scan's lattice, one law in one place** (an
  eighth of the smallest tool radius, a 100 um floor, grown to keep any
  span under 1024 columns) - a second "reasonable voxel size" function
  would drift by the next refactor.
- **`from_shape(FromModel)` refuses at build with the right answer in its
  words** ("model stock builds from its facets: Dexel::from_facets") - the
  facets live in the world, not in the enum's numbers; quietly boxing the
  model would add stock the user never chose. Degenerate meshes (empty,
  XY-flat, non-finite vertex) refuse too, and `from_facets` carries one
  cell of margin so the model's own bbox corners read air, never mystery
  material.
- **Caught on the ladder, not by the bump:** the v17→v18 wire bump left
  `wrong_version_is_refused` asserting v18 as a FUTURE format - a lying
  fixture HEAD carried unnoticed (the stage E suite log truncated before
  its binary). Moved to v19; the migration list in its comment now names
  v17→v18.

## How this file ends

When the roadmap's open items are exhausted, this file gets its final
entry summarizing what "done" meant, and the discussion happens over it.
