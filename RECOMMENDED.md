# RECOMMENDED.md — decisions taken unattended, for later discussion

> _Growing file. The user stepped away on 2026-10-17 and mandated: run the
> rest of ROADMAP.md solo, every recommended decision is the way forward,
> log each one here as a point of discussion after the roadmap is cleared.
> Nothing in this file is a blocker - work shipped with each choice made -
> but any of them may be revisited._

Format per entry: **what was decided**, **why**, **what it'd cost to change
later**. Newest milestone first.

## M4.1 - The crate split, rung by rung (commits 43745cb..415dd85 + this docs)

**New crates are `camcraft-*`; existing binary crates keep their names for
now.** `hsmworks-app`/`hsmworks-ui` ride until the M4.3 debrand renames
them; the six new crates had no legacy brand debt, so they shipped as
camcraft-geometry/cl/tools/post/sim/motion directly. Cost to change: one
Cargo.toml pass + import sweep, same work M4.3 owes anyway.

**post.exe lives in camcraft-post.** It is the post engine's binary; R3
moved it there with its bin name unchanged (`post`), so the packaging
script keeps finding it without a hardcoded app path (re-verified at
close-out: `post 0.3.0 (boa_engine)`).

**R0 was a fifth relocation, not a no-op rung.** Type homes moved inside
app first (doc note listed four; a fifth - sim's Traverse - emerged when
stock_sim split). Logged so the graph and the docs agree.

**The release-offset subtask ran at close-out, not per rung.** ROADMAP's
"with every gate passed where a feature has been set" was read as the
milestone (a feature ship-unit), not each refactor commit: a
`cargo build --release` after every rung would have re-compiled the same
tree six times. It ran once at M4.1 close-out and passed (one transient
flecs_ecs_sys gcc build-script failure under the backgrounded run -
rerun clean; worth noting as CI flake if it recurs).

**Deviation register (R1-R4), all deliberate:**
- R4 left `rest.rs` in app - it is re-machining orchestration (walks the
  world, reads stamps), not a generator; the motion crate owns the
  physics, app keeps the choreography.
- R4 sank the islands law `material_above` to
  `geometry::slice::material_above` - it reads a mesh, nothing above the
  geometry layer; three readers (machining3d wrapper, sim verify,
  app stock-sim path) share one law at the lowest honest home.
- R4 put solid-rest's engine in camcraft-sim, not motion - rest queries
  are material accounting (dexel grid), motion only consumes them; the
  motion→sim dependency is exactly that edge.
- R5's 400-line rule landed as "split along section seams"; six files
  still exceed 400 (see ledger) - the alternative was mid-function cuts
  that break the "one concern per file" honesty.

**R5 400-line backlog ledger** (line counts at the split; each gets cut
again the next time it is touched, no dedicated sweep):
operation/generate.rs 481, project/params_impl.rs 499, project/params.rs
463, operation/selection.rs 416, operation/stitch.rs 413,
operation/types.rs 808 (pre-dates R5: the component/type registry -
splitting it means splitting schema.rs's twin, hold for M4.3's naming
pass). machining2d.rs 2299, finishing3d.rs 1579, feature.rs 1545,
stock_sim.rs (world half) 1240, drilling.rs 1024, machining3d.rs 915,
verify.rs 905 live in crates now and carry the same debt per crate.

**Graphify:** R5's file moves were many enough for a full rebuild. The
plain rebuild tripped both export guards - the #479 shrink guard (the
cache re-mints doc nodes sparsely, 3477 raw < 4318 saved) and the
semantic-loss guard (849 saved doc ids). Resolved per the
doc-node-guard recipe: fold saved ∪ new with every saved-origin doc
node restored (saved wins the id on label duplicates) - no `--force`,
nothing content-lost. Residual: the restored edges that pointed at
legacy `_rs_`-format sourceless ghosts are dangling in the raw sidecar
(310) and dropped by the builder - zero in the final graph. Final:
4397 nodes / 11501 edges / 221 communities, old names carried by
member-overlap vote.

## M4.4 - Test cost shape

**The verdict lane is `#[ignore]`, not a second binary.** ROADMAP offered
either; ignore-in-place keeps every fixture, helper and golden assert in
one file (finish3d's law tests share `dome()`/`cone()` builders with its
verdicts — a split binary would duplicate them or reach across `mod`).
The reason string names the lane and the command, so `cargo test
-- --ignored --list` is self-documenting. *Cost to change:* a separate
`--test` binary later is a cut-paste of the marked blocks; nothing
structural prefers one over the other.

**Six LAW-shaped tests joined the heavy lane.** `a_hop_plane...`,
`climb_and_conventional...`, `the_generator_is_deterministic`,
`levels_snap...`, `rings_close...` and `a_chain_too_long...` assert
generation STRUCTURE (ring counts, link classes, determinism) but ride
the 12×96 dome because their expected counts are that fixture's numbers -
10 to 52 s each at debug speed with no envelope scan at all. Shrinking
their fixture would mean re-deriving every hardcoded count - a rewrite
with silent-weakening risk, which the Prime Directive outranks. They run
in the release lane instead; M4.1's suite move is the honest moment to
give them small-fixture twins. *Cost to change:* the twin work, whenever
the geometry module lands in its own crate.

**Coverage arithmetic is the law, not a nicety:** 39 `#[test]`s before,
39 after (27 ignored); verify.rs 17 → 17 (2 ignored, each a 150 s
`cone_facets(96)` envelope - the whole suite's 155 s was those two). The
default debug ladder is finish3d 0.8 s + verify 0.9 s + the ~5 s field,
against the ROADMAP's 5-min budget. *Cost to change:* none.

**Measured, this run:** heavy lane in release - finish3d 27 verdicts in
202 s (the same set ground ~30 min in debug; "an hour of debug grind is
minutes optimized" holds at 9x), verify's two envelope verdicts in 22 s.
Default debug ladder: finish3d 0.8 s, verify 0.9 s, the rest of the
ladder unchanged - comfortably under the 5-min budget.

**CI grew the release-lane step rather than losing the verdicts.** A
plain `cargo test` silently skips `#[ignore]`d tests forever - the split
would have DELETED coverage on every non-local gate. The one
`cargo test --release -p hsmworks-app -- --ignored` step (after App
tests, before UI) keeps every verdict enforced pre-push. It pays a
release compile of the app crate; that's the price of an honest opt-in
lane, and rust-cache amortizes it. *Cost to change:* dropping the step
trades measured-clean guarantees for CI minutes - a bad trade, flag it
if CI time ever forces it.

**No release build is owed for M4.4.** The standing order offsets a
release at each gated feature milestone; M4.4 moved no `src` line (test
annotations, docs, CI only), so the v0.3.0 binary is still the current
hand-test vehicle and shipping a byte-identical 0.3.x would be ceremony.
The next release offset lands on the Phase-4 milestone that first moves
src - which the ordering below makes M4.1. *Cost to change:* none; the
ladder simply picks up at the next milestone tag.

## M3.3 close-out - the 0.3.0 release and the Phase-4 ordering

**Shipped 0.3.0 on the M3.3 D commit.** Cargo version + RELEASE_NOTES
rewritten "what's new since 0.2.0", persistence quoted v19→v23, annotated
tag `v0.3.0` on the lock-sync commit, docs tag `m3.3-done`. PROJECT_VERSION
23 stayed the file-format number - the two "versions" are different clocks
and the notes say so in different words. *Cost to change:* none - the
release train is the same one 0.1.0 and 0.2.0 rode.

**Phase 4 runs M4.4 FIRST, before the M4.1 subcrate split.** The roadmap
lists them in numeric order, but M4.4 (test cost shape) is the one that
pays every later milestone: finish3d's ~30-min tail taxes each close-out
gate, and M4.1's split would move those tests twice - once to split them,
once to fix their paths. The law/verdict split + `#[ignore]` heavy lane +
diff-targeted gating lands first, then the crates move under tests that
already run in seconds. *Cost to change:* resequencing is free until
either one lands; after M4.1 it costs a re-split.

**Gate note for the close-out itself:** all 30 app suites green in two
parallel waves (32-core box, no cargo lock on direct binary runs), all 8
UI suites green serially from `crates/ui` (the snapshot goldens resolve
CWD-relative - a "Missing snapshot" panic is the cwd, not a regression),
clippy clean, the 13+57 `.nc` goldens byte-identical. The full-suite pass
here is the milestone close-out AGENTS.md allows, not a new habit.

**graphify --update at close-out hit the semantic-loss guard and was
resolved by hand** (fold saved∪new per file, `dedup=False` - the fuzzy
fold retires guard-required ids by collapsing pre-existing twin nodes).
A `--update` on this repo should keep `dedup=False` until the guard and
dedup agree; the TODO.md re-queue from the unverified-shrink flag is
expected and will force-clean on its own next run.

## M3.3 D - RTCP output and the block trace (this commit)

**The post delegates the pivot math to the controller.** The stream's
A/B/C words are absolute commanded angles in the tool-tip frame; the
Fanuc post emits them verbatim under `G43.4` and never rotates a
vector itself. Why: RTCP is precisely the function the control
implements — a post doing its own kinematics would double-transform
the moment the control's setting disagrees with ours. The refusal
that blocks a tilted stream on a non-RTCP machine is the same law
seen from the other side: without the mode, the same XYZ cuts
different metal, so reinterpretation is the silent gouge. *Cost to
change:* a future offline-RTCP mode (posts that compute tip paths for
controls WITHOUT G43.4) would add a geometry helper to the driver
side and a new machine capability word; nothing in the format blocks
it.

**`machine.rtcp` is a geometric judgment, not a naming one.** The
script's boolean is true when some rotary states a direction whose Z
component is not dominant - an A about X tilts, a C about Z spins in
plane. A rotary that states no direction tilts nothing a post may
promise (silence stays silence through the mirror AND the raw file
door). This is stage C's promise kept ("stage D adds the rotary
mirror honestly"): rotaries ride their own `kinematics.static.rotaries`
section - letter, direction, travel - and their deg/min rates enter
NO prismatic field; the ceiling folds now filter rotaries out,
closing the latent leak where a 9000 deg/min table out-rated a 7000
mm/min slide into `rapidRate`. *Cost to change:* none - the shape is
what the posts read.

**The modal angle law runs in both directions, and the arm is the
memory reset.** After `G43.4`, the first tilted block states all three
letters (the control's modal memory starts blank); afterwards only
the letters whose commanded value CHANGED ride the block - including
on the home return, where a zero that others already carry stays
silent. A tool change (G49 cancels TCPM) re-arms: the next tilted
move re-commands all three even when the angles repeat. *Cost to
change:* the dedup table is three scalars in the script's preamble.

**Angles ride the stream, never the CL goldens.** `MoveRec.abc` is
`Option`; every in-app dump of a prismatic toolpath writes `None`
(zero bytes of change to the 13 committed `.nc` goldens - they were
byte-compared after this whole seam). The `.cl` legacy dialect has no
angle words by law; only the v1 `A <i> B <i> C <i>` keyed form speaks
tilt. *Cost to change:* none; the parser's all-three-or-none law
means a half-stated tilt never enters silently.

**The trace derives, it does not store.** The viewer's trace pane has
no selection field: the highlighted block is the one owning the
scroll cursor the window already publishes, and clicking a block
centers its first line through the row's own `scroll_to_me` - the
next frame's cursor law reads the selection back. A swallowed record
appears in `blocks` with an empty range and "(no output)"; a record
that legitimately writes nothing (a stored PARAM, a TOOL whose words
the post prints in the section preamble) gets no block - the trace
reads as the program, not as the stream echoed back. *Cost to
change:* storing a selection would need a second cursor law to agree
with the first; the derived one cannot disagree.

**`sim.rs` did not change, and that is the tested claim.**
`Traverse::from_machine` only ever receives the mirror (rotary-free
by construction, the filter above is the guarantee) or the raw
default VMC (no rotaries stated). A tilted-motion TIMING model would
need the sim to understand TCP - M5's machine-sim territory, not
output. The machine/operations tests pin that the walk stays the
post's walk. *Cost to change:* none pending.

## M3.3 C - Declared kind, full numbers, coolant codes, changer capacity, machine editor

**The kind is stored as the file's WORD, and declared outranks
inferred.** `Machine.kind` is `Option<String>`, not the enum: a project
file round-trips exactly what was written, and `is_sane` is the single
place that decides what is statable (an unreadable word refuses at
`parse_mch`, so no mounted machine can carry one). When the declaration
and the axis inventory disagree, the declaration wins - "the file
knows parts the kinematics tree hides" - which is the discussion-worthy
half: a mistyped `kind: "lathe"` on a real VMC darkens the mill cells
with nothing to contradict it except the axis table a row below.
*Cost to change:* the word is the persistence shape; normalizing to
the enum at parse is a one-line switch that would start rewriting
files.

**Stage C fixed a law M3.2 E could never reach.** The mill-turn
inference tested for A/B/C names, but the parser collected only
`"type": "linear"` parts - a real file's rotary never entered the
inventory, so MillTurn was unreachable outside hand-built structs.
Rotary parts now join, with a stated `rotary: bool` (a linear axis
named A is a slide; a rotary named Q still turns) and the letter test
kept as the legacy fallback. Rotary numbers never ride the inch→mm
factor - a deg/min scaled as mm/min is a unit lie, so scaling is
per-part. The same unit law is why rotary axes stay out of the post
mirror's prismatic axis list: the CPS contract has no rotary word, and
staging D/RTCP will add one honestly rather than slip degrees into a
millimeters field. *Cost to change:* none pending; stage D owns the
rotary mirror section.

**No version bump, on the skip-if-unstated promise.** Every new field
(`kind`, `spindle_power`, `tool_count`, `coolants`, per-axis
`rotary`/`resolution`) is `skip_serializing_if`-gated: a pre-stage-C
project saves byte-identically and loads unchanged, and the roundtrip
test pins that an unstated machine MINTS none of the new keys. Coolants
reaching the post completes stage A's sentence through the same mirror
(a `toolStation.coolants` the mounted struct grew), so the editor's
Apply and the file mount take one road - `machine_value` re-mirrors
the live struct and there is no stale-raw seam to rot. A name-only
entry rides codeless and posts nothing: visible on the sheet, silent
in the program. *Cost to change:* a bump only if a field ever needs a
different spelling than its Rust name.

**The machine editor is the tool editor's draft law, and Apply is
plain `LoadMachine`.** The window edits a COPY (ephemeral,
never-saved, never-undoable); Apply mounts through the SAME command a
file-load uses, at the SAME path (`built-in: {name}` for the library,
via one shared format function so "loaded from" never quotes a file
that does not exist), and undo restores the exact previous numbers. No
new command type. A refused Apply keeps the window open - the draft is
someone's work, and discarding it silently is the silent wrong answer
wearing a dialog. The built-in machines are named in the pane but are
NOT a stored reference: mounting stores the parsed machine itself, so
editing a VMC never edits the shipped file. *Cost to change:* an
`EditMachine` command type would only exist to remember what the
History already knows.

**Two numbers stated, deliberately not judged.** Spindle power checks
nothing yet - the consumption model that could violate it belongs to
the tool library's per-material speeds/feeds work, and checking
against nothing is a false green. Changer capacity counts DISTINCT
tool NAMES across the program, not tool changes: a 12-slot drum running
13 changes of 1 tool is fine, and the checker says so by silence. The
three library files (VMC, Mill-turn, Lathe - all original, checker
grammar) are pinned by a test that reads each both ways: declared kind
equals inferred kind. *Cost to change:* power's verdict lands with the
speeds/feeds model; nothing to unwind first.

## M3.3 B - Five built-in posts, by-name registry, conformance goldens (this commit)

**Built-ins are addressed by name; the stage-A flag is closed.**
`PostRef` gained `Named(String)` exactly where stage A said stage B
would: a persisted *index* into an array that grows would make every
existing file's meaning shift under a re-order, a name cannot. Entry 0
remains Fanuc forever (`Builtin` still means it — every pre-B project
posts Fanuc, unchanged), and the Setup-pane combo resolves names to
rows at draw time; the row index is screen furniture, the document
stores the name. An unknown name (a project from a future build with
more posts) refuses at `SelectPost` creation and displays as
"(built-in, missing)" — never a silent swap to Fanuc. *Cost to
change:* none — the name is the persistence contract.

**`PostDto` is a tagged enum, not one ambiguous string.** v23's `post`
key now serializes `{"named": "Haas"}` or `{"file": "C:\\...x.cps"}`
(serde externally-tagged; `Builtin` = absent = None). A single string
field could not honestly distinguish `Haas` the name from a file
called `Haas`, and the value domain of v23 widened mid-version because
v23 has never shipped to a user — the bump already belongs to M3.3,
and pre-release formats get to firm up inside their own version. *Cost
to change:* a v24 bump if a released build ever needs a different shape.

**Where public convention is silent, house style decided, and the
goldens lock it.** `%` fence: Fanuc/Haas/Mazak yes, Sinumerik/Heidenhain
no. Line numbers: Sinumerik only (N10 steps). Motion words: 2-digit
(Fanuc, Mazak) vs 1-digit (Haas, Sinumerik DIN, Heidenhain DIN). Tool
word: `T02 M6` + `G43 H02` (Fanuc), plain `T2 M6` + `G43 H2` (Haas,
Mazak), `T2 D2 M6` no G43 (Sinumerik — DIN tool offsets own length),
`T2 M6` no length word (Heidenhain DIN). Retract mode: `G98` once +
modal bare-X Y continuation (Fanuc, Haas, Mazak) vs every hole fully
restated, no G98 word (Sinumerik, Heidenhain). Spindle: `S.. M3` vs
Mazak's `MOTON S..`/`MOTOFF`. Peck: G73 (fast) / G83 (full-retract);
Mazak G73 rides its own dialect. Each `.cps` header cites the public
standard its choices follow and names the house-style spots as ours.

**Heidenhain ships as DIN/ISO, not native KL.** The KL/TCL cycle
language (`CALL CYCLE(…)`) is a proprietary-grammar dialect we can
only reimplement from documentation fragments; DIN/ISO is the same
control's RS-244-class mode, fully public. The conformance law needs
scripts we can defend line by line. A KL-mode post is a legitimate
future registry entry — the by-name registry grows without disturbing
any saved project. *Cost to change:* one more BUILTINS row.

**The synthesized stream exists because the legacy goldens were
motion-only.** Every `.cl` golden predates params and tool blocks, so
no committed program ever contained an `S.. M3`, a `T.. M6` or a
dwelled `G82` — the exact words the five dialects disagree on.
`conformance.rs` posts one hand-built v1 stream (params, tool, contour,
five hole cycles) through all five scripts into `nc/synth_<stem>.nc`.
Its cycles anchor (`AT n`) on the opening rapid of their own hole
chain — the shape a real generator dump has. Anchor them at move 0
and `normalize_program_order` re-seats the group ahead of the
contour, whose moves then die in the chain-suppression guard
(`if (inCycle) return`) — 0 swallowed, silently absent. A synthetic
stream must be shaped like a real one; the law is now in the test's
own doc comment. *Cost to change:* regenerating `WRITE_GOLDEN=1`
reads the diff like every other golden.

**One latent, repo-wide, pre-existing eol hazard (not introduced
here, found by the commit warnings).** The goldens are LF in the
object store and `core.autocrlf` converts them to CRLF at checkout —
but the engine emits LF and the tests compare byte-exact, so a
*fresh clone* would fail every golden until someone normalizes. This
tree has never re-checked-out those files, so it has never bitten.
The one-line fix is a `.gitattributes` (`*.nc *.cl *.cps text eol=lf`
or a global `* text=auto eol=lf` decision); it touches every file
stat, so it belongs to a discussion, not a feature commit.

## M3.3 A - Post selection, mounted-machine output, user .cps posts

**The mounted machine reaches the post as a mirror, not as raw JSON.**
The plan carried a "raw rides the mount" design (store the `.mch` file's
JSON verbatim beside the parsed `Machine` and hand it to the post). What
shipped instead is `post::machine::mirror(&Machine) -> Value`: the
checker grammar (`parts`, `controller`, travel `ext`) is bridged to the
static post grammar (`kinematics.static.axes[].speeds`,
`head.spindle.max`, `coolants`) at the one call site. *Why:* one source
of truth — the parsed machine every other consumer (collision, traverse
timing) already reads, so the post can never disagree with the sim; and
mirror states only what the file stated — an unstated limit is absent,
never a zero that lies (a mounted machine naming no coolants posts no
M8, proven in `posting.rs`). Raw storage would have doubled the machine
representation and the persistence surface for no behavior the mirror
doesn't give. *Cost to change:* additive — a `raw` field on `Machine`
(serde-skipped) plus a DTO key, if a future post ever needs a grammar
the checker drops.

**`PostRef::Builtin` carries no index; built-ins are named, chosen by
position.** The enum is `Builtin | File(path)` — Builtin means
`BUILTINS[0]`. *Why:* stage A ships one built-in, and persisting an
index into an array that stage B grows to five would make every v23
file's meaning shift under a re-order. Stage B must either keep index
0 == Fanuc forever or add the index variant at the same version bump —
flagging this now is the cheap part. *Cost to change:* one migration
arm if the variant comes late.

**Project format v23: one additive top-level `post` key.** No machine
DTO change (the mirror decision above made the planned `raw` key
unnecessary), so the only new fact is the post choice: `None` =
built-in, `Some(path)` = user file; a v22 file steps up unchanged and
means the built-in. `open()` lands the singleton either way — the
leak-guard law (every field is reset on open, so a stale choice can't
ride into a fresh document). *Cost to change:* none, it's the pattern.

**The picker lives in the Setup pane, not the toolbar.** The toolbar is
a static command enum (no dynamic menus by design), so the post row
sits beside the machine row: label + "Load post (.cps)" + "Built-in".
The toolbar's single Post button still posts with whatever the document
names — one truth, no hidden choice.

**onComment is honored only where the stream carries data.** `Record::Warn`
now calls the hook; `onRadiusCompensation` stays silent because no CL
record carries comp — that waits for G41/G42 path support (M1.3's open
flag). The Warn echo can only ever appear in a REFUSED program: the
same warning count that grows the list blocks the write, so the 13 .nc
goldens are byte-identical (gate-verified) and no released program
gained a comment.

**Rotary axes are excluded from the mirror in stage A.** The checker's
`AxisLimits` carries no direction vector to map, so a rotary axis would
mirror as a lie; stage D (RTCP) brings the real geometry and the mirror
grows then. Trunnion/tilting definitions arrive with M3.3 C's machine
kinds.

## M3.2 B-E - Fixtures, tool lists, sheets, workspaces (this commit)

**A fixture is a model wearing a marker, and the marker is the whole
state.** Stage B's first draft grew a parametric `FixtureKind` enum with
synthetic vise/chuck geometry. The re-spec during the stage settled it
the other way: the shop's real workholding is a solid - a STEP file the
user loads - so a fixture is any model entity wearing
`FixtureDef { setup: Option<String> }`, positioned by the same
`Location` a part uses, and the only geometry law is the one the
collision check always had: facets in world space.

- **The user brings the fixture; the machine file brings the machine.**
  The preloaded "fixtures" are the spindle band (from the `.mch`'s
  `spindle body_diameter`/`body_length`, a cylinder past the holder) and
  the table (`Machine/Table` - a derived prose row, never an entity: it
  exists while the `.mch` says travel does, and its underside is
  unbounded by MAX_FIXTURE_DIM). No synthetic vises ship with the app;
  vises and clamps are the user's solids. Cost to revisit: parametric
  presets would be a content decision (a bundled fixture library), not
  a data-model one - the marker already carries the binding.
- **The collision gate is cheap-by-construction, and workholding never
  rides the stamp.** Verify takes `fixtures: &[FixtureBox]` +
  `spindle: Option<(r, len)>` and runs its stack test (shank → holder →
  spindle, then the flute column itself, first-hit-wins per move, every
  cylinder SWEPT along the move's xy path rather than sampled at its
  endpoints, rapids against every box with no
  vertical exemption) only when the list is non-empty - a document
  without workholding runs byte-identical pre-M3.2B checks. The laws
  were first wired into `GenStamp` as a digest term; the UI's own M2.3
  tests caught the contradiction ("load the roomy machine and the same
  program posts without re-generating" cannot coexist with a stamp that
  churns on machine load), and the Prime Directive's other half is
  served by the fix, not weakened by it: the post's LIVE `verification_
  gate` re-runs the region-free band laws (`verify::bands_vs_fixtures`,
  `verify::rapids_vs_fixtures`) against the cached paths and the steel
  as it stands now. A moved vise cannot silently post - the gate names
  the op, move, and struck element - and no fixture or machine action
  can stale a fresh toolpath. Evidence is computed here and now, never
  a cached verdict: the M2.3 machine-limits law, extended to clamping.
- **Two laws stage B's own acceptance test fixed, not review.** The
  moved-vise gate test went red the week the code went in, and it was
  right twice over - the Prime Directive working as designed, a silent
  wrong answer caught by a test rather than on the shop floor. (1) The
  workpiece sim counted fixture-tagged models as PART geometry: a vise
  box read as material the paths promised to leave standing, so a face
  near the jaws reported phantom gouging. Fixtures joined the stock
  exclusion clause - setup furniture, never design intent. (2) The band
  law sampled each cylinder at a move's law points, so an 8 mm jaw
  standing between two far endpoints could be read straight past, clean
  and silent. Bands now sweep the cylinder along the move's xy trace -
  one segment for a straight, chords for an arc (`fixture::
  band_hits_box`, z rule from the stack law, xy rule from the rapid
  law) - a strict superset of the point test, so every verdict made
  before it stands. Both halves keep the module's err-toward-HIT law,
  with its one stated err toward silence (flush z contact within
  FIXTURE_HIT_TOL is contact, not crash) untouched.
- **The struck element is named for what is at that z.** The flute
  column fallback reported its hits as "shank" - a shop-facing lie,
  since the shank stands above the flutes and a flute-in-vise means the
  endmill itself is milling the clamping steel. `FixturePart::Flute` is
  a new variant (additive: the existing names stay the file-format
  contract) and the fallback reports the flute as itself.
- **The tree marks workholding, and the mark is marker-driven.** A
  fixture model's browser row carries the vise glyph (original drawing,
  `assets/ui/fixture.png`, in the icons module's own hand-drawn family)
  and " (fixture)" in the suffix column ahead of " (hidden)"; it keeps
  the eye, because a parked vise is still hideable. `panes::
  model_row_marks` is the pure mapping - no geometry is relabeled by
  name matching anywhere in the shell.

- **A fixture-bound `DeleteFixture` undoes whole.** Marker, placement,
  and facets are snapshotted at construction and replayed verbatim,
  like every other deletion command; `DeleteEntity` learned to keep the
  marker through undo/redo the same way.
- **Import-as-fixture is two undo units.** The STEP parse stays on the
  worker (no command constructor reads disk in the UI); when the model
  lands, a `TagFixture` bound to the then-active setup executes behind
  it. Undo trades the marker back first, the solid second. A single
  fused command would have meant either a world-reading constructor or
  a headless-only path - the seam is cheaper than either.

**Stage C's tool list is a pure read, in change order, with two stated
constants.** `tool_list::for_setup` walks the program order of one setup
context and emits one `ToolRow` per tool the spindle actually holds:
operations, cut/rapid travel, minutes, change count. `RAPID_MM_S = 100`
and `TOOL_CHANGE_S = 20` are the shop's defaults, documented in the
module - not yet machine-file numbers, because the `.mch` rapids are
per-axis ceilings and converting them to a travel-time law would invent
acceleration data the file does not carry. Cost to revisit: the machine
file growing an explicit rapid-rate-for-time (G0 feed) field, then this
reads it; until then one honest constant.

**Stage D's sheet is a view, not a document.** `setup_sheet::render`
resolves everything live (heights, stock, offsets, fixtures, the stage-C
tool list), prints NO timestamps (the same file renders byte-identical
- the determinism law the goldens already keep), and one setup per
call; the UI's export writes all setups as one .txt with form feeds
(`\u{0c}`) between pages - one file per job instead of one per setup,
because a job's paperwork travels together. The dialog runs no command:
export changes nothing about the document, so the history must not
grow a "undo the export" absurdity.

**Stage E gates the ribbon on an axis-inventory inference, not a new
file format.** `machine::kind` reads the axes the `.mch` already
declares: Y → mill, Y+rotary → mill-turn, X/Z-without-Y → lathe,
anything undecided → mill (what every assumption in this codebase
already is). A lathe document darkens every mill `Create` cell with a
tip that says why - the honest workspace, not a lathe operation set we
cannot generate until M4.2. M3.3's "machine declares its kind" can
replace this inference with a stated field; the ribbon already reads
through the one function.

**Verification found the stage-A commit's one red test - logged
honestly.** `stock_sim_document::missing_pieces_refuse_by_name` asserted
the literal `"no stock defined"`, but 33f5658's own `setup::
stock_entity_name` rewording had made the refusal name WHICH stock
(`no stock 'Stock' - ...`). The refusal was correct; only the test's
earlier literal was stale, and stage A shipped with it red because the
serial 30-40-minute ladder's aggregate RC buried which check moved. The
assert now tests the LAW phrase (`no stock` + `judges material`), not
the setup's name - the name is a fixture, the law is the claim. The
sim≡post gate equality assert in the same test still proves both voices
come from one construction.

**The gate method changed mid-milestone, at the user's call: targeted
parallel binaries, `test result:` lines as truth - not re-ladders.**
Build once (`cargo test -p ... --no-run`), run the individual
`target\debug\deps\<name>-<hash>.exe` in parallel (32 cores, direct
exes take no cargo lock), parse their summary lines. Three hard-won
rules: (a) only the hashes from the LATEST `--no-run` list are current -
deps/ keeps stale old-hash binaries that test old code (a false FAILED
from a pre-edit binary cost a detour); (b) Start-Process ExitCode under
redirection is unreliable in .NET (WaitForExit(ms) returns before the
stream closes) - the printed result line is the verdict, never the RC;
(c) windowed/GPU suites (the ui lib snapshot tests, egui_app, shell,
gpu_render) must run SERIALLY or they spuriously panic in shared
window/GPU state - run concurrent, they "failed"; run alone, 108
passed. Cost to change: none - it is a working method, not a product
decision; AGENTS.md's testing section now carries it.

## M3.2 A - Multiple setups (code commit `33f5658`)

**A setup is a name that owns a world frame, and the frame is the flip.**
Stage A gives a document a list of setups - `SetupDef {stock, wcs,
datum_from}` components plus one `ActiveSetup` singleton - and routes
every stock/WCS/height read through the *operation's declared* setup,
not through a cursor. The decisions inside that:

- **The implicit-setup law: zero setups is the old world, verbatim.** An
  operation's `setup` field is `Option<String>`; `None` and a setup named
  by no entity both resolve - through `setup::effective` - to the legacy
  globals `STOCK_NAME`/`WCS_SLOTS`. A pre-M3.2 document round-trips
  through v21 unchanged and every old test passes untouched (the one
  legitimate churn: calls that now take a setup argument).
- **The first `CreateSetup` MATERIALIZES; it never renames.** The new
  setup points its stock/WCS fields at the entities that already exist.
  Renaming them would move geometry under the toolpath's feet and make
  every stamp of every existing op read Stale for a bookkeeping event.
  SECOND and later setups own private furniture (`"Setup2/Stock"`,
  `"Setup2/G54"`) that is spawned LAZILY on first use - so a setup that
  is only ever a label for a flipped second face costs zero entities.
  Cost to revisit: eager furniture makes "does this setup own a block
  yet?" a schema question instead of a world read; nothing else changes.
- **Membership is declared, not cursor-derived.** `CreateOperation`
  writes the then-active setup into the new op; moving an op later is
  `SetOperationSetup`, which KEEPS the stamp - "Ok when the two setups
  place their stock identically, Stale when they do not" falls out of
  the digest machinery (stamps digest RESOLVED values - per-setup
  height Zs, the setup-resolved stock's loc+shape - not entity names),
  so no membership term in the stamp is needed and activation stays
  pure inert data. The UI cursor is a display context only.
- **One order law, rest masks same-setup-only.** `posting::program_ops`
  calls `setup::operation_order` (setup rank by name, then name), and
  `rest::prior_digest`/`prior_state` skip operations of other setups -
  the program-ahead is what THIS frame machines against. Multi-setup
  ordering is therefore rank-then-name, not browser drag order.
- **Placement is the flip; the transform is M3.3.** A new op's world
  frame comes from where its setup's stock block and WCS origins sit.
  `datum_from` is inert provenance (a string); datum TRANSFER clones
  the source's stock snap (shape+loc+facets) and placed WCS Locations
  into the new setup's OWN names - undo removes only the clones.
  Rotation/flip matrices deliberately do not exist yet: the roadmap
  files "Part orientation" under M3.2's setups/datums row, and stage E
  or M3.3's post work will attach a rotation Location to the clone.
  Cost to revisit: none - the fields are already Locations.
- **The program sim CLIPS between setups (documented conservative
  law).** `stock_sim::simulate_document` walks ops in program order and
  at each setup change folds in the new block by lowering columns to at
  most the new stock's top, never moving what the previous setup cut -
  a re-clamped face reads as leftover (rest, visible) rather than as
  air (a gouge). A rotated or from-model stock in a LATER setup refuses
  the sim outright: the frame cannot be judged, so it is not guessed.
- **G-code words never followed the setup; entities did.** Posting
  still writes the `WCS_SLOTS[slot]` literals - the machine's offset
  register is the machine's, and the setup's private G54 entity is a
  *value* posted into that word. Furniture names are RESERVED
  (`AddTool` refuses `is_furniture_shape` names; setup-name validation
  refuses `Stock`/`G54..G59`/`Setup1/G54`/blank).
- **Delete refuses while any op belongs, naming it**; `ActivateSetup`
  of a deleted setup is legal but dangling (reads fall to the first
  existing setup), and undo replays furniture restores BY NAME, which
  is how the lazily-spawned entities stay consistent across the
  history walk.

v20 -> v21 step-up is pure serde-default: `setup` kind, per-entity
`stock`/`wcs`/`datum_from`, op-level `setup`, top-level `active_setup`.
A v21 save of a setup-free document is byte-equivalent in meaning to
its v20 (the step-up test pins it).

## M3.1 D - Re-machining (this commit)

**The program ahead of an operation is an input, and the honest digest
of it is the PATH ON RECORD.** Stage D made `Pocket3D` read the rest
material its predecessors left: `rest::prior_state` replays every
earlier operation's cached moves through the `stock_sim` dexel grid and
hands the generator a mask that only ever REMOVES cuts (a masked run is
always a subset of the unmasked one - skipping columns whose material
already stands low cuts air, never gouges). The decisions inside that
one sentence:

- **The predecessor's digest term is a hash of its cached moves, not
  its recorded stamp.** The first design hashed stamps; the survey of
  the command layer showed why that cannot work: `EditOperationParams`
  drops an operation's stamp to `None` (the "staleness on the copy"
  convention from M3.1 B/C), so a mere parameter edit - which changes
  NOTHING the machine would post today, because the cache still holds
  the old path - would have marked every dependent operation Stale.
  The path-hash term keeps the law the rest module documents: a
  dependent regenerates when what would post moves, and a program of
  edited-but-unregenerated predecessors converges in two
  `RegenerateAll` rounds (round 1 regenerates the edited predecessors
  - the dependent was Ok when the batch was built, so it is not in it;
  the new caches then mark the dependent Stale; round 2 re-machines
  it). Cost to revisit: the digest walk clones and hashes every
  predecessor's cached moves on every `status_of` - the same order of
  work the world-read (`toolpath_of` returns an owned clone) already
  forces, but O(path) per frame per dependent is more than stage C's
  cheap-live-questions budget intends. A memoized `path_digest` u64 on
  the cached-Toolpath component, written at generation, would make it
  O(1); it needs a `#[serde(default)]` field addition (0 means
  "compute once"), not a version bump - a good M3.x tune-up.
- **`prior_stale` is the LAST question in `stale_reason`, and reaching
  it forces the answer.** Every own-input question (tool, source,
  picks, heights) came back clean while the stamp still mismatches -
  so the rest digest is what moved, and only a cutting predecessor can
  move it: the reason names the first cutting predecessor and says
  "regenerate the program in order". In the compound case (this
  operation's parameters WERE edited, and a predecessor also cuts) the
  advice is still exactly right - regenerating the program fixes both
  - but the attribution may name the predecessor when part of the
  story is its own edit. The single-digest stamp cannot split that;
  saying so would need the per-term baselines that stage C declined.
- **The mask applies to every `Pocket3D`, not only `rest_rough`
  ones.** The flag's job is the closing rest pass; the level ladder's
  "don't recut what stands lower" law is independent of it. This makes
  a redundant duplicate operation refuse as Empty ("the operations
  ahead in the program already machined this area") instead of
  silently air-cutting - the Prime Directive at the program level.
- **A never-generated predecessor contributes only its name to the
  digest**; the first operation of a program has no rest term at all,
  so existing stamps are bit-for-bit what they were before stage D
  (the inert-data law), and nothing in persistence changed (v20
  stands).
- **Rotated stock refuses by name, not by guesswork.** The program
  sim needs an axis-aligned grid; a job whose stock carries a rotation
  gets `rest from previous operations: the program sim models an
  axis-aligned stock; this stock carries a rotation` out of generate -
  honest, and fixable by re-seating the stock.
- **Program order is the post's order - the name-sort wart included.**
  `rest::ahead_of` walks `named_entities` in the same order
  `posting::program_ops` does, so an operation named `Operation10`
  sorts ahead of `Operation2` for re-machining exactly as it does for
  the machine. Fixing the sort (creation-order or explicit sequence
  numbers) is a roadmap question; forking it here would make
  re-machining disagree with the post, which is the one disagreement
  this feature must never have.
- **The fixture law the tests encode:** a `Pocket3D` footprint is the
  model's projected silhouette minus the columns the solid still fills
  - so a through-cavity model has its void OUTSIDE the region a pocket
  will ever enter, and re-machining tests must model a SOLID plate
  with a BLIND pocket. (Found the hard way: a through-hole plate made
  every masked level read empty. The law itself is M2.1's and correct;
  the test geometry was lying.)

## M3.1 C - Geometry association workflow

**"References, not copies" was already true; stage C made it felt.** The
survey's finding: picks store feature IDS (content hashes), resolved
against the live world at every read, re-validated at pick time by
`valid_pick` - no operation holds a frozen geometry copy. Drill holes
stay params-data by the M1.6 decision (they are the pattern's geometry,
and the model-change → Stale path already catches drift). What a user
could not do was find out WHY a row reads STALE or fix a whole revision
at once. Two additions:

- **`stale_reason` asks the world live questions, not a stored
  baseline.** The stamp is one digest and cannot itself say what moved.
  The chosen design asks, in order: is the tool still there and still a
  tool; does the stock/source entity still exist; does every picked
  feature id still exist in its entity's live geometry; can every height
  still resolve. When all pass, the honest answer is the general one.
  The alternative design - storing a per-operation features baseline
  (parts inventory) in the doc - would give exact set-differences ("two
  faces moved, one added") at the price of a v21 persistence bump and a
  new digest per generation. The live-questions version is ~80% as
  precise, needs no persistence, and can never disagree with the world.
  Cost to revisit: the baseline design layers ON this one - the question
  order stays, the membership check just compares a stored set instead
  of re-deriving `features_of_local`.
- **`RegenerateAll` is skip-not-fatal, and one undo unit.** A stale
  operation whose generator refuses (its tool vanished) must not block
  the rest of the job - it keeps its old cache and its STALE tag
  honestly, and the batch lists it by name. This follows validate-early:
  construction runs every generator, so `apply`/`invert` are pure writes
  and the compound inverts in reverse. Deliberate scope: NEVER and OK
  operations are untouched - this is the revision-fix, not a build-all
  (first-time generation stays a per-op choice).
- **The affordance lives in the status strip, and only exists when
  stale > 0.** The tree marks rows individually and the properties page
  explains individually; the batch belongs to no entity, so it rides
  the bar - and retires with the staleness, so the fresh-document
  snapshot churns zero (verified: both snapshots unchanged).
- **`for_each_height` joins `stamp_with` as the second Height-soup list**
  - one fn enumerates every Height field per variant for the reason
  scan. New Height params must go in BOTH lists; the doc comment says
  so, and a missed field is a silent blind spot (the compiler cannot
  catch it - flagged for discussion, along with whether the two soups
  should later merge into one generated list).

## M3.1 B - Parameter propagation (this commit)

**The stamp should read what the path reads.** Stage B's survey found the
"stock, tool or WCS" bullet was two-thirds done by architecture: the tool
arm already digests the named tool entity's live numbers (a `ChangeTool`
edits the entity every stamp reads through), and `stamp_with` folds each
`Height`'s *resolved* Z - which asks the world for stock/model bounds and
the WCS slot's placement. `heights.rs` and `operations.rs` already lock
that behavior with tests; nothing was added, it was verified. The real gap
was one arm: `FromModel` stock, whose geometry is a materialized copy, was
digesting only the copy's NAME.

- **Propagation happens inside the update command, not lazily at read
  time.** `UpdateModel` snapshots any derived stock's facets at
  construction and writes the new ones at apply, so model and material
  trade back together on undo - one user action, one undo unit. The
  alternative (stock geometry re-derived on every read) threads a
  special-case layer through viewer, simulator, and post; the codebase's
  law is that the world holds the truth, and `SetStock` already
  materializes copies. Cost to revisit: the guard is a single
  `stock_of()` predicate; a lazy scheme would delete its two write sites
  and add a derivation to each consumer instead.
- **The stock stamp digests the copy, not the source.** The generator
  mills the stock's facets, so the stock's facets are what staleness
  tracks - the source's name stays in the digest as provenance only.
  Deliberate consequence: delete the source model and stock-based
  operations stay Ok, because the material they describe has not moved
  (a test pins it). Placement is never re-adopted either: an update
  changes the metal, not the setup - the stock's own `Location` stays as
  the user put it.
- **The Selection arm joined the ids-as-signatures law.** It digested
  `feature::digest_of` (per-facet positions), which churns a pick on a
  pure re-tessellation whose faces all survive. It now digests
  `revision::signature`, same as the Entity arm; chain and patch ids are
  derived from face content, so a surviving face set cannot hide a moved
  pick. `digest_of` stays as the Features cache key (model.rs) - there it
  is exactly right, a content address for memoization.
- **No persistence change.** Propagation is a command-time write; the
  stock's facets are already persisted in v20, so load never needs to
  re-derive anything and the version stays put.

## M3.1 A - Model revisions and stable feature identity (this commit)

**A model revision is a content event, and content already has ids.** Stage
A set out to detect "the CAD changed, which faces moved" the way the
pre-compaction plan sketched: per-revision geometric signatures plus a
greedy successor matcher (0.5 mm centers, 25 % area slack). Building it
turned the plan inside out: `feature.rs` face/patch/chain ids ARE content
hashes - exact-integer plane signatures, welded corner positions, quantized
areas - so diffing two revisions is a set difference over stable ids. No
matcher was needed, and none was written.

- **Ids-as-signatures replace the geometric matcher.** A face that survives
  a re-mesh keeps its id byte for byte; one that moved lands in
  `missing`/`added`. Pairing a missing id with an added one ("the same face,
  moved") is deliberately NOT guessed anywhere in Stage A - successor
  pairing is Stage C's decision, made visible to the user through re-pick
  affordances, never assumed by a tolerance. The cost to revisit: a matcher
  would be additive (a `candidates()` beside `IdDiff`), nothing in the diff
  law has to change first.
- **The stamp's Entity arm digests the signature, not count + AABB.** The
  old arm let a diamond prism swap in under a contour operation and keep
  the stamp - twelve facets, same box, silent gouge fuel. `revision::
  signature` is the multiset of face ids plus counts: order-free,
  winding-free, diagonal-flip-free (each invariance is a test), and it
  moves exactly when the material moves. Consequence, said plainly:
  existing saved documents re-open with their Entity-sourced operations
  Stale once - the stored stamps were computed under the old digest. Same
  one-time churn as the `b3eaebc` stamp move; the mechanism working, not
  broken.
- **Re-import by name is an update, not a conflict.** The worker thread
  stays world-blind (it cannot know the name exists); `finish_import` on
  the main thread routes: no entity - `ImportModel` (spawn, rev 1); an
  entity - `UpdateModel::new`, which refuses to overwrite setup furniture
  and refuses empty geometry at construction. Undo trades geometry back
  exactly - it never deletes a model the user only updated. `ImportModel`
  itself keeps its early-return-if-exists as the redo/idempotence guard
  only.
- **`Revision` is bookkeeping, never a staleness input.** The u64 counter
  says "geometry landed here N times" for the tree and the save file; the
  stamp rehashes the facets themselves, so a save-load round-trip can
  never inherit staleness from a counter that drifted. v20 persistence is
  the pure step-up: absent key means 0 means "pre-M3.1, nothing
  remembered"; a model first updated after loading such a file lands rev 1.
- **FromModel stock propagation is Stage B, not here.** The stock's stamp
  arm still digests the source model's NAME, so re-importing under that
  name leaves a FromModel stock silently Ok. It is the same silent-gap
  class the Entity arm just closed, and it gets the same treatment in
  Stage B (stamp inputs) - noted here so the gap is owned, not forgotten.

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

## M2.3 stage B — the viewer judges the program (code `8fb7aaa` + this docs)

- **The sim's gate IS the post's gate, by construction.**
  `simulate_document` validates through `posting::program_ops` - not a
  copy of its rules - so the error a user sees under the Deviate cell
  is byte-for-byte the error the Post would give (`missing_pieces_
  refuse_by_name` asserts `err == same`, not a paraphrase). Two gates
  that must agree is a drift bug waiting to be merged; one gate read
  twice is a law. This is also the shape stage D's blocking gate
  inherits.

- **Refusal ordering is load-bearing, and the tests encode it.** The
  planned-ops gate runs BEFORE the sim's own stock laws, so a rotated
  or moved stock first reads "inputs moved since generation -
  regenerate" (staleness, a setup truth) and only a FRESH rotated
  stock earns "the program sim models an axis-aligned stock". Writing
  these tests backwards (rotate, don't regenerate, expect the axis
  refusal) would have asserted the wrong refusal - the test suite now
  documents which law speaks first.

- **"no stock defined" is reachable only through an op that never
  reads the stock.** Any stock-sourced op goes stale at stock removal
  and the gate refuses first - so the witness is a model-sourced
  contour with fixed heights. A refusal with an unreachable trigger
  would rot unnoticed; the test keeps the code path honest.

- **Box stock is XY-centered, and the fixture law fell out of a
  failing test.** `Dexel::from_shape(Box)` spans `-dx/2..dx/2` (the
  mesh agrees), so a model box at `0..60` next to a stock at the
  origin judges HALF NaN columns - not a bug in either, a fixture
  lying about the world. `faced_doc` now centers the model on the
  stock's placement; the lattice shift moves with `Location.pos` and
  the twins test (`the_lattice_moves_with_the_stock`) pins that moves
  stay world millimeters - the same numbers the viewport draws -
  while the GRID is what translates.

- **DEV_TOL is a display law, not an authoring tolerance.** 10 um: the
  finest scallop the shipped lattice resolves visually. An op's own
  tolerance is what its generator promised; the map answers "what
  stands after the WHOLE program". Re-using op tolerances would let a
  tolerance edit recolor columns another op generated. Discussion
  item: whether the Release 0.2.0 story wants DEV_TOL surfaced as a
  user-settable "display sensitivity" or stays a constant.

- **The verdict bar is one pure string.** `actions::deviate_readout`
  builds the sentence (counts, mm³, worst depth, the located XY and
  the SIM's z there - `at.z` is the dent floor or the standing
  layer's top, never the model's nominal); the status line, the bar,
  and the tests all read the SAME function, and the ui test pins its
  shape without a GPU. A missing WHERE prints "?" - an honest gap,
  never a fabricated coordinate.

- **The sheet takes the stock's slot in draw order.** While Deviate is
  armed, the judged surface IS the stock's appearance (same translucent
  blend law, class-colored: steel / amber rest / red gouge / gray
  never-judged) - drawing both would only let the untouched solid veil
  the verdict. Retirement is the PostView law: `program_sim` is frozen
  evidence, view state, never a command, never saved, retired by any
  document switch - a verdict about the old world must not survive
  into a new one.

- **Hidden models still judge.** The deviation field compares against
  every model entity's facets; the browser eye only hides geometry
  from the viewport, not from the program's contract. Silently
  excluding hidden parts would let one click make a gouge vanish from
  the verdict. Discussion item: an explicit "include in verification"
  checkbox per model, if users ever want geometry that's reference
  only.

- **Icon reuse: Deviate draws the Stock glyph until a dedicated one
  is chosen.** No asset was invented mid-stage (original-glyph rule
  wants a design pass, not a rush job); the stock icon is exactly the
  noun the cell judges.

## M2.3 stage C — the machine speaks (code `b8114b9` + this docs)

- **The unstated-limit law, and why zeros are not limits.** Vendor
  `.mch` files carry `0` for every rate nobody configured; a zero
  rapid speed does not mean the machine cannot rapids - it means the
  file doesn't say. So 0/absent/non-finite reads `None` and checks
  nothing; only stated numbers can be violated. The alternative -
  treating a blank as a ban - would refuse every real file on earth.
  Travel bounds are the exception with a reason: a `NaN` or inverted
  stroke is nonsense, so it reads as corruption (is_sane), not as
  freedom.

- **Format reuse is what the user authorized; values stay ours.** The
  `.mch` JSON layout is parsed as HSMWorks writes it (general,
  kinematics, synced_configuration) - the CPS/MCH re-spec named the
  formats, not the numbers. Every fixture value here is invented
  (TestMill 1, Hsmworks Testbench), per the clean-room protocol; the
  module's provenance line says so.

- **Stroke lives in a `"travel"` object we add to linear parts.** The
  real format states velocities but no travel anywhere we could find
  in the public behavior notes - a machine without a stroke makes
  travel checking impossible, so the layout grows one extension
  (our own key, ignored by anyone else's reader). Axis numbers are
  PROGRAM coordinates: the check subtracts `posting::wcs_context`'s
  origin - the post's own origin law, one copy - so a G54 slide
  shifts what passes exactly as it shifts the G-code.

- **Axes are looked up BY NAME.** The table is ordered by the
  controller's `coordinate` field for display, but a positional
  index would silently check the wrong stroke the moment a file
  names a subset or leads with a U. Names make a limit mean
  something.

- **The feed bill is per-move and directional.** Only a move that
  actually drives an axis pays that axis's `max_normal_speed`; the
  ceiling is the SLOWEST stated among the driven ones, unnamed when
  none states one. Rapids: programs never ask for a rapid rate, so
  `max_rapid_speed` is carried but unbilled today - and rapids
  still pay the travel bill, which is the one that crashes
  machines. Arcs are judged at endpoints + the four XY quarter
  points; that can over-report a real stroke on a back-sweep arc,
  never hide a violation - over-reporting is the honest direction.

- **Tool length means gauge length.** The magazine's
  `max_tool_length` measures spindle nose to tip: flute + shank +
  holder, not the cutter alone - the number a tool table really
  gates. Spindle: today every op posts its tool's rpm, so the tool
  IS the program's spindle word; when per-op speeds arrive the
  checker reads them there.

- **Report, not block - yet.** Stage C's verdict joins the Deviate
  sentence ("; machine: within limits" / "N limit problem(s)") and
  refuses nothing at the post; the unstated-limit law's sentence
  half: no machine mounted, no claim printed. Stage D moves the same
  `check_document` into `post_setup`'s path, where the shared-gate
  law says it judges exactly what would post.

- **v18→v19 is the step-up pattern again**: pure serde-default on the
  `machine` key, so an old file loads byte-comfortably and a new save
  mints no key until a machine is actually loaded. A corrupt travel
  statement makes the WHOLE load a Corrupt refusal that mounts
  nothing - refuse-before-world-write, proven by test.

- **Discussion item - the scallop test's debug-profile cost.**
  `scallop_crosses_the_diagonals_and_measures_clean` runs ~1.9 s in
  release but 1853 s (31 min!) unoptimized - it passes either way,
  but a debug `cargo test` on the whole suite becomes impractical.
  Candidates: a `#[cfg(debug_assertions)]` coarser resolution for
  that one fixture, or an `#[ignore]`d slow group. Decided: ship C
  untouched (the law it pins is release-verified); fix in the 0.2.x
  maintenance window.

## M2.3 stage D — the verdict holds the door (code `e158b88` + this docs)

- **What blocks is unsafety, not incompleteness.** The gate refuses on
  GOUGE cells (the program cuts below the model's final geometry - the
  Prime Directive's "never", located) and on stated machine-limit
  violations. REST material does NOT: a roughing program posts on its
  promise of a later finish op, and a gate that blocked rest could not
  express a multi-op shop workflow at all. The intent-vs-safety split
  is the whole design: Deviate informs on both truths, the gate guards
  only the safety one. Discussion item: whether Release 0.3 wants a
  per-project "no rest at final op" authoring option, if users start
  treating the sim as a part-completeness check.

- **A refusal-to-judge is never a judgment.** When `simulate_document`
  cannot run (no stock, rotated stock), the gate contributes no line -
  absence of evidence is not evidence of danger. The net under this
  path is the per-op WARN gate, which has always been the load-bearing
  one; the stage-B law (the sim's gate IS the post's gate) plus this
  one means the program-level sim is an EXTRA net, honestly silent
  when it can't speak. The Deviate cell states its reason on demand.

- **Evidence at post time, never cached.** The gate re-runs the checks
  inside `post_setup` instead of reading `state.program_sim` - the
  PostView law ("a verdict about an older world must not survive into
  a new one") applied where it actually matters: metal. The cost is
  one sim pass per post (seconds on big programs); the alternative is
  a staleness footgun on the exact thing the release exists to
  prevent.

- **Order is law again.** `stream_records` (freshness, by name) refuses
  BEFORE the gate's sim runs - judging stale moves would print
  located-where over an old world, the one thing located verdicts must
  never do. The existing stale/post tests pin that ordering; the new
  gate could not have been appended after the engine run either, since
  the warnings check already owns "post blocked - ...".

- **The machine clause names the model** ("machine TestMill 1: 1 limit
  problem(s): ...") - in the refusal you are reading a claim about a
  specific machine, and people own more than one. The ui's sentence
  test (`the_machine_gate_refuses_the_post_by_name`) pins the whole
  status string through the real shell, so the law is checked where
  the user reads it.

- **No ui code changed in stage D** - the refusal rode the existing
  "post refused: {err}" arm untouched. A gate that needs new shell
  plumbing to be obeyed was never a gate.

## M2.3 stage E — Release 0.2.0 close-out (this docs + the tag)

- **One version, one bump.** The workspace root carries it; `hsmworks-ui`
  and `post.exe` inherit - the release.yml version-from-binary law reads
  `post.exe --version`, so the zip's name cannot drift from the tree it
  was built from. 0.1.0→0.2.0 is a Cargo.toml one-liner.

- **Notes are a document, not an archive.** RELEASE_NOTES.md leads with
  what's new since 0.1.0 and keeps a cumulative "What works" - the old
  0.1.0 prose is a pointer to the tag, not a second section to maintain.
  The goal of the rewrite: a reader can decide "can I do my work with
  this?" in one page.

- **What 0.2.0 does NOT claim.** Phase 2 is not fully green: M2.2's
  surface-finish parameters are still open (that row stays ⚠️), and the
  release story is "trust before cutting", not "3D complete". The M2.3
  row is ✅ because all five of ITS bullets shipped; fixtures-vs-collision
  was deferred to M3.2 by stage B's law, not silently dropped.
  Discussion item: a short 0.2.x maintenance pass to close the M2.2 tail
  before M3.1 starts, since associativity touches the same ops.

- **The publish path is still unproven.** There is no git remote yet
  (carried from the 0.1.0 era): the `v*`-tag trigger in release.yml has
  never run on a runner. The tag is cut locally; the first push will be
  the actual test of the packaging script. Discussion item: create the
  GitHub repo and push `main` + `v0.2.0` when the user is back.

## M4.2 — the multi-platform gate and the GPU pick (code `feat(ui): ... (M4.2)` + this docs)

- **The pick is flag-first, env-second, refuse-always.** `--gpu-backend
  dx12|metal|vulkan|gl` wins over `CAMCRAFT_GPU_BACKEND`; a bad name in
  either source exits 2 naming what it got, before any window opens —
  the `--ui` refusal shape, and the Prime Directive's "validate early"
  applied to startup. The chain then runs preferred → `PRIMARY` auto →
  `Backends::all()`, each miss logged with the backend it names, so a
  driver blacklist workaround can be read off the console line. The
  `wgpu::DownlevelFlags` machinery this bullet originally specced is
  gone from wgpu 30 — the honest tier signal now IS the backend: a `gl`
  answer logs "conservative feature set" because GLES is wgpu's own
  second tier.

- **Headless stays pinned to the fallback adapter on purpose.** The
  gates' determinism law outranks the new flag there: `--headless`
  prints one line ("pins the fallback adapter... pick ignored") rather
  than silently accepting or silently ignoring. Discussion item: whether
  the pixel-snapshot goldens should ever run against a named backend.

- **Linux: compiles green natively (WSL2, rustc 1.99) and runs the
  suites — with one honest blemish: `finish3d_project_dome` and
  `finish3d_radial_dome` emit 2 moves fewer on Linux.** Measured, not
  guessed: identical file except three near-duplicate points at
  (-2826..-2660, -8869..-8940, 8714) collapse to one when the dedupe
  cull runs on glibc's sin/cos instead of the Windows CRT's — a ULP
  wobble in the dome's angular sampling landing exactly on the cull
  threshold. Every other golden, posting, fanuc_post, roundtrip and the
  gpu tests are byte-green on both platforms. Chosen interim law: the
  CI parity job `--skip`s those two BY NAME (never silently) and
  `M4.2b` owns the fix — quantize to microns BEFORE the cull decision
  so platform libm cannot choose how many moves the shop gets.
  Considered and dropped: per-platform goldens (two truths),
  vendoring a deterministic libm for the generators (heavy until M6.1
  makes GPU math the sampling source anyway). The GUI itself is still
  unproven to RUN on Linux (needs a real display+GPU box); Docker
  Desktop on this machine died mid-milestone (its program tree
  vanished — WSL carried the Linux lane instead), so the "Dockerfiles
  for every target" bullet is M4.2's one open rung. Discussion items:
  M4.2b now vs at 0.4.0; a fresh Docker setup to retire the last one.

- **macOS is now a CI claim, not a local one.** The build job adds
  macos-latest (check + goldens + split-crate tests); nothing of ours
  ever compiled for Darwin here. First green run is the proof; expect
  one round of fixes at most (the stack — wgpu metal, flecs clang, winit
  cocoa — is all first-party cross-platform).

- **WASM: the compute core speaks it; the app does not.** geometry/cl/
  post compile clean for `wasm32-unknown-unknown` (post needed only the
  getrandom `wasm_js` target-dependency; boa_engine handles the rest).
  The app/UI half would be a wgpu-via-webgl shim + flecs-on-wasm story
  — that is the web-sys-mvp rung's work, not M4.2's; the CI wasm job
  pins what is true today so a regression in the core's portability
  fails loudly.

## M4.3 — the debrand rename: what moved, what stayed, why (code
`refactor: rename hsmworks to camcraft across crates, cps posts and
tooling (M4.3)` + this docs)

- **The law, and its one limit.** "Nothing carries the old brand" is
  enforced for everything that is *naming our product*: crate/package
  names (`camcraft-*`), the binary (`camcraft-ui.exe`), the project
  extension (`.camproj` + filter + `untitled.camproj`), the window
  title ("CamCraft"), the cps `vendor` line ("CamCraft (independent,
  unaffiliated)", `nm` fallback "CAMCRAFT"), the dist zip name, the
  import thread name, doc-comment self-references, workspace and CI
  flags. What names *Autodesk's product* in a true statement ("a
  clean-room implementation of HSMWorks", "no HSMWorks source was
  consulted", "HSMWorks-style From") is the trademark-disclaimer
  posture the repo was built on and stays verbatim - a rename that
  erased those sentences would erase the provenance law itself.

- **Wire magics stay `HSMW-CL`/`HSMW-CNC` - considered and kept.** The
  brand's initials survive in exactly two places: the first line of
  the CL dump and CNC intermediate formats. A format signature is a
  compatibility contract, not a logo: every committed golden, every
  `.camproj` with an embedded stream, and every `.cnc` file on a
  shop floor is keyed on it. Renaming would orphan all of it (the
  parser's fallback is *absence* of magic → legacy `.cl` rules, not a
  second spelling), buy zero user-visible debranding, and churn 21
  goldens the suite's stability law protects. The parser keeps one
  canonical spelling per format; if a `CAMC-*` ever ships it will be
  a versioned format decision, not a brand sweep. `docs/CNC-
  INTERMEDIATE.md` documents the tokens as what they are: names from
  the pre-rename era of our own format.

- **Hash salts moved, and the churn is the mechanism.**
  `hsmw-op-stamp-v2`/`hsmw-rest-*` → `camcraft-*`. The op stamp rides
  in saved projects, so every document saved pre-rename reads Stale
  once on open and clears on regenerate - the identical, documented
  behavior of a stamp-version bump (the `b3eaebc` precedent; stamp.rs
  now carries the note), visible in the UI, never silent. The
  alternative (parking a wrong brand in a hash because it *might*
  cause a regeneration) trades a trademark concern for a comment that
  lies about what the bytes are.

- **Test fixtures renamed too** ("CamCraft Testbench" vendor,
  `camcraft-posting-`/`camcraft-gone-` tmp names): fictional-vendor
  strings are ours to name, and a fixture is exactly where a stray
  brand re-enters the greps forever. One law test needed a real fix,
  not a rename: `fanuc_post::tilted_move_...` asserted the no-angle-
  word law with `!program.contains('A')` - harmless while the old
  brand had no A; "CAMCRAFT" has two. The assertion now checks what
  the law states: no `A` adjacent to a digit/sign, so prose in
  header comments cannot trip it (the test's intent, made exact).

- **Docs discipline: ledgers keep era names.** Completed ROADMAP rows
  and older RECOMMENDED entries keep the names they were written with
  (`cargo test -p hsmworks-app` in an M2.3-era line is history, not
  branding); operational files (AGENTS.md family, doc titles, gate
  commands) were swept. Re-writing history entries after a rename
  falsifies the decision record - the record is what the post-roadmap
  discussion reads.

## Phase 5 — the plan and its three shape decisions
(this docs + `docs/M5-PHASE5-PLAN.md`; code follows from M5.0's first commit)

- **M5.0 exists because Phase 5 is a kind-flood, and the code told us so.**
  The seam map measured one operation kind riding 14 files (52
  `OpParams::Finish3D` sites - the type enum, five semantic fns, the stamp
  tag, heights, material, generate dispatch + verify-mode + context,
  HeightRefs, setup sheet, project DTO twice, retype + quick params, five
  pane tables, three page dispatches, the command arm). M5.1/M5.2 name ~25
  kinds: same-channel growth is ~350 edit sites, and every one is a place
  the silent-wrong-answer can hide in maintenance. The user's standing
  shape law - low coupling, high cohesion - makes the prelude not
  optional. What was considered and dropped: trait-object params (fights
  the Rust-owned-component law and moves dispatch into vtables nobody can
  grep); per-family crates now (premature - the families are inside
  motion's generators and app's tables, the crate seams the plan names are
  where they already want to live).
- **The tool axis is a vector below, angles above.** `cl::Move` gains
  `axis: Option<[f64;3]>` (workpart frame); the app converts vector→`A/B/C`
  against the mounted machine's rotary directions - one function, machine-
  dependent (trunnion B/C vs tilting head have different solutions), with
  shortest-travel unwrapping; the stream/post contract stays exactly
  stage-D's (keyed millidegrees, tip-positioned XYZ, refuse-by-name on an
  unreachable tilt). Vector-up angles would bake one machine's kinematics
  into the geometry layer; angles-down from app keeps motion world-blind -
  the R4 law again. post/mod.rs's hardcoded `abc: None` gets its producer
  when M5.1-A lands, and the five-dialect refusal test already guards the
  seam end to end.
- **Scripting is JS on the engine we already ship.** The open question
  ("QuickJS / Lua / Rust-native DSL", and M5.3's "TypeScript bindings")
  resolves to boa_engine: one embedded runtime, already sandboxed under the
  post contract's rules, already in the tree; TS consumability rides the
  typed JSON the batch API speaks. A second interpreter (Lua) would be a
  second sandbox to audit for a product that must never run user code
  unsafely - considered, dropped. ROADMAP's "Lua" word on the M5.3 ledger
  row is superseded by this entry when M5.3 is planned for real.
- **M4.2b rides M5.0, not a solo commit**: the fix is one sampler touching
  cull decisions, and M5.0 regenerates nothing else - the two named CI
  skips retire in the same close-out that proves the structure split kept
  every golden byte-stable.
  *Superseded by the M4.2b entry below: it landed solo, and first.*

## M4.2b — the dome drift was a tie, not a rate (code `fix(motion,cl): quantize the 3D-finish scan, hull and arc splits off the platform's libm (M4.2b)` + this docs)

- **Root cause, measured, after two wrong hypotheses.** The M4.2 note
  said "ULP drift in the sampler's sin/cos lands near-duplicate points
  on the cull threshold" - wrong on both halves. Ring dumps (a temporary
  probe, since removed) showed the two platforms emit different VERTEX
  COUNTS for the same z-level loop: Windows 222, Linux 220, with exactly
  the three golden-adjacent vertices fused into one. The drift lives in
  `facet_polygon`'s argmax: the scan point for one facet is the best of
  at most six support candidates (3 vertices + 3 edge-stationary
  points), and on a facet with an edge perpendicular to the scan
  direction - which latitude-band facets are, everywhere - the vertex
  and edge supports are EXACTLY equal in mathematics. The winner is
  then decided by the last bits of each libm's cos/sin, and since the
  candidate's point carries `rho·u` with a different `u`, the hull's
  dedup (`==`) and cross-product (`<=0`) decisions flip: three
  near-collinear points survive on Windows, fuse on Linux. Culling is
  not the culprit; nothing reaches a "threshold" - the polygons
  themselves differ before the cull ever sees them.

- **Fix by construction: the decision surface contains no floats.**
  Scan points are snapped to the micron grid the moment they are born
  (`lattice` = the same 1e6 grid as the geo2d bridge's SCALE, so the
  bridge's round is a fixpoint and the union sees identical integers on
  every platform); the support value is compared on the grid; and
  `convex_hull` runs in exact i64/i128 - map to grid ints, sort, dedup,
  Andrew's chains with i128 cross products, map back as `x as f64 /
  1e6`. A 1000 mm part is 1e9 units, products fit i128 with room to
  spare; saturating casts keep nonsense input finite (Prime Directive).
  Tie-breaking is now candidate ORDER, identical on every target. The
  march-sampler lattice fix kept from the first pass remains - it is
  still true that every march sample lands on the grid, now asserted by
  the `every_march_sample_lands_on_the_micron_lattice` law test.

- **Law override - the goldens were regenerated, against the letter of
  the owed fix.** Both the M4.2 note and this file's Phase-5 entry said
  "regenerate nothing". That proved impossible: the committed Windows
  goldens encode Windows' tie-bits at those mathematically-exact ties,
  so any platform-invariant generator must move them. Seven files were
  rewritten, all COARSER by sub-µm slivers (radial_dome 1982→1958,
  project_dome 560→536, waterline_cone 121→113, the four plateaus −8
  each): the removed moves are near-collinear vertices the exact hull
  fuses. The tolerance proof is the heavy lane, not an argument - all
  27 ignored finish3d measurements and both app envelope verdicts pass
  in release, and fit_arcs' chord-deviation law bounds the cut
  regardless of which candidate the old tie chose. Linux parity was
  then VERIFIED, not assumed: the same 20 goldens, 20/20 on
  Ubuntu-24.04, and both named CI `--skip`s are retired in the same
  commit.

- **The two ceil sites stayed fixed anyway, orthogonal to the dome.**
  `machining2d::fit_arcs`' segment count and `cl::arc_chords`' chord
  count both divided by libm-derived quantities where a mathematically
  clean result (2π, an exact multiple) arrives as 2±1 ULP - `ceil` of
  that is a platform coin flip for EVERY closed ring and arc, waiting
  for its own drift story. The idiom is `(lattice(q) + 1e-12).ceil()`:
  the lattice swallows the accumulated noise, the epsilon biases
  UPWARD so a fix can never remove a chord Windows had - over-splitting
  is the safe direction (more, shorter arcs, same deviation bound).
  The probes showed these two did not contribute to the dome drift;
  they are kept because they remove real knife-edges, proven
  goldens-identical on Windows.

- **Solo, and before M5.0.1 - reversing the Phase-5 note.** The plan
  said M4.2b rides M5.0's close-out; it landed first instead, because
  M5.0.1's law is "goldens byte-identical across the split" - that law
  cannot be tested against a sampler whose output is a platform coin
  flip. A drifting generator under a byte-identity law means every
  split's green gate is luck, so the drift had to die before the file
  moves started. The two CI skips retire now rather than at M5.0's
  close-out - the skips were the debt's receipt; keeping them past the
  proof would hide it.

## M5.0.1 — the seven file splits: how a split stays honest (commits 4463808..4bf5e4d + this docs)

**Splits were mechanical slices, never retyped code.** A python splitter
read each flat file, found top-level item seams (an item plus its
contiguous `///`/`#[…]` lines; block end = the next head's start), and
emitted the directory module: original top-level `use` lines on every
file's head, items moved verbatim, inline `mod tests { }` de-wrapped with
its `#[cfg(test)]` attr hoisted to a `mod tests;` decl in mod.rs. Same-
named items (a struct plus its impls) were addressed by `Name@line`
when name matching alone couldn't assign them. Why: the stage law is
"structure only, goldens byte-identical" - a hand-move invites a drive-by
edit that a green suite may not catch, and a slice guarantees the moved
lines are the same bytes. Cost to change: none - the layout is normal
Rust modularity; the tooling was scaffolding.

**Private items became `pub(super)`, the pub surface re-exported from
mod.rs.** Callers, tests, and docs see exactly the paths they saw
before. Trait-impl items stayed plain (a visibility on `fn fmt`/
`fn from` is E0449), cross-file struct fields promoted only where a
sibling file touches them. `pub(crate)`-everywhere and `pub`-everywhere
were both considered and dropped: wider than the seam needs is a
coupling loan at the exact interest rate Phase 5 is here to pay down.

**Two files rode past the ~400 cap, on purpose.** `drilling/tests.rs`
sits at 408 - eight lines past the mark; cutting the fixture helpers
mid-block would have traded eight lines of slack for two more seams to
audit. `verify/engine.rs` is ~415 with the whole (a)-(f) law cascade in
one function: splitting a function body is behavior work, and this stage
forbids behavior work; its natural seam arrives with M5.1-F's law-engine
pass. Both exceptions are structural, not budgetary - re-audited at the
close ladder, goldens unchanged, suite green.

**One `tests.rs` per module, not tests scattered onto the domain files.**
The coverage law (count `#[test]` before and after, keep the ignored
lane's goldens) is a one-command check this way, and each suite's
Gate-map header stayed where its reader expects it.

**The close-out ladder got one correction worth keeping:** the
"fresh binaries" find also matches `deps/camcraft_ui.exe` - the app's
hardlink, not a test harness. Run it with test flags and it opens the
GUI and waits forever; the serial UI lane silently hung on it twice.
The lane now takes its binary list from `cargo test -p camcraft-ui
--no-run`'s `Executable` lines (seven hashed binaries, run serially with
CWD=`crates/ui` per the snapshot-golden law).

**Measured, not copied: coverage is 793, not 792.** The ROADMAP M5.0
stage row says "Coverage 792==792" - the honest count across bb0017c..
HEAD is `git grep -c "#\[test\]"` = 793 before and after all seven
splits (29 `#[ignore]` likewise - my "31" here was a miscount, corrected
in the M5.0.2 close; the law is the before==after comparison, which held
under either count), the +1 ride-along from M4.2b's probe tests. The
stage row gets corrected when M5.0 itself closes.

## M5.0.2 — the kind registry: one table, and what deliberately stays a match (commit c465558)

**A const `KindInfo::ALL` table, not a `define_ops!` macro.** The brief
said "registration table"; a declarative macro seemed the Rust-native
shape, and it was dropped. A macro hides its expansions from grep and
rust-analyzer, and the fan-out sites that the compiler must police
(`kind()`, `is_sane`, the height walks, the region builders, the
generator dispatch, the DTO mirrors) are exhaustive matches TODAY -
the match arm IS the registration forcing a new kind to be handled.
The table's job is only the parts that were data with no type-checker
behind them: the stamp tag literals, the UI's label/caption/icon/order
columns, the retype seed bodies, the verify mode. Considered and
dropped: inventory-driven Vec registration (runtime allocation for a
compile-time set), and typetag/erase (a dependency for a closed enum).

**The three silent `_ =>` arms were the actual bug class.** panes'
`text_of` answered "3D Finish" for any unknown index, `icon_of`
answered the finish3d glyph, and retype answered a Face - all three
compiled while lying. They now index the table (`ALL.get(i)`), and an
out-of-range index is a `debug_assert` plus a named `eprintln` before
the release-path fallback (Prime Directive: refuse loud, never crash
on a UI path). A new kind that forgets its row cannot compile:
`OpType::ordinal()` is exhaustive, and `info()` debug-asserts
row.ty == self so table order drift dies in the first test run.

**Digest bytes were kept sacred, not just compatible.** `stamp_with`
had `h.write_u8(N)` as the first statement of each arm; the N moved to
the table's `tag` column and the write was hoisted above the match -
the byte stream is identical by construction, which is what let the
793-test lane (and every cached-stamp path) pass untouched instead of
requiring regolden. The registry law (row order == ordinal == tag ==
combo index == ribbon `Create(i)` list) is asserted inside the
existing `stamp_follows_every_input` rather than a new `#[test]`, so
the coverage count stays byte-comparable across the refactor.

**Retype went whole.** The UI's eight `retype` arms and eight
per-kind accessors moved verbatim into app as `retype_params` + the
`Carried` folds - behavior-by-construction, per the M5.0.1 slice
discipline. `Carried::of` preserves both documented quirks (Drill's
climb is always true; Drill's bottom exists only when explicit), and
the two helpers the UI wording tests call (`top_of`/`bottom_of`)
surfaced from private to `operation::` pub - the law comments moved
with them.

**One rung was enough.** The plan's second rung (extracting named
per-kind fns from the generate dispatch) was evaluated and left
undone: those matches are the compile-forced glue the design above
protects, and renaming their bodies into functions would move code
without moving coupling. It gets revisited only when M5.1's ~8 new
kinds make a dispatch arm genuinely heavy - the table means that's a
row + an fn + arms in four honest files, not the 14-file hunt.

## M5.1 stage A — the tool axis rides the move (commit 7313f96)

**The pose lives on the Move, not beside it.** Considered: a parallel
`Vec<Option<axis>>` per path, and a `Toolpath.poses` side-table - both
dropped. A move and its orientation are one fact about one segment;
side tables drift out of step the first time a generator splits or
drops a move. `Move.axis: Option<[f64; 3]>` - unit, tip→shank - makes
the pose travel with the geometry through every existing splice, and
`None` keeps prismatic paths byte-cheap and byte-identical. The exact
vertical `[0, 0, 1]` is filtered `is_vertical`-EXACT, not epsilon: the
branch exists to keep the 3-axis contract bit-for-bit (13 .nc goldens,
untouched), and a near-vertical lean on a filtered-exact branch would
be a silent pose loss. Near-vertical leans that come back from
angles-above are genuinely near-vertical and take the tilt path.

**Angles-above is the one conversion.** axis = Rz(C)·Ry(B)·Rx(A)·ẑ is
the convention the .mch kinematics already state; its canonical
inverse A = asin(clamp(-ay)), B = atan2(ax, az), C = 0 was picked over
full ZYX Euler because C never appears in the dump (the table is
tilted about X and Y; the tool's own spin is the spindle, not an
axis), and one canonical answer keeps generator-written `Move.axis`,
hand-written keyed streams, and the sim's letters all naming THE SAME
angles for the same lean - which `a_dumped_lean_posts_the_same_program_as_a_tilted_stream`
locks end to end.

**Refusal at generation, never mid-stream.** The machine's `can_tilt`
(direction z² < 0.99·|d|² is LEANING, so a trunnion A-about-X passes
and a rotary-B-about-Z on a lathe does not) judges each commanded pose
in `check_document`, and the checks say the move index, the letter,
the degrees, and the travel in the message. Considered and dropped:
silent clamp to travel, nearest-reachable IK, and drop-the-axis - all
three are the silent wrong answer the Prime Directive forbids; a CAM
program either states what it commands or refuses loudly. The safe
floor is deliberate: stage A knows no orientation SEARCH (that's
stage F's collision-aware tilt); here the human commands a pose and
the machine says yes or no.

**The collision bands enclose, never hide.** A leaned cylinder's
swept volume is awkward to test against boxes, so each band becomes
the vertical column that CONTAINS it: center shifted to the band's
mid-height point (`mid·a[x,y]`), radius widened by the band's
half-length spread, z planes grown by the band radius - over-approximate
by construction. Considered: a true OBB swept test (dropped: more math
than a CAM refusal needs, and an approximation that can only ADD a
false graze can never hide a crash - the check's whole job is blocking
the post). The upright path computes the original expressions bit-for-bit,
which is why `stock_sim`'s prismatic goldens stand untouched. Rapids are
deliberately still upright-checked: a leaned RAPID's collision law
belongs to stage B, where indexed 3+2 position moves make the question
concrete.

**The project file stores the pose snapped once at the write.**
`MoveDto.axis` is `#[serde(default, skip_serializing_if)]` - v22 stays
v22, old files load, and a prismatic cache writes no pose key at all.
Snapping to the position lattice at write and attaching VERBATIM at
read (no re-snap) makes save-load-save byte-identical even across
platforms whose libm bits differ; re-snapping on read would let the
same file mean two poses depending on who read it. The cl crate's
section split (moves/path/pose/cycle/tests) rode the same commit
because `pose.rs` is the axis law's new home - 400-line law, and
`axis_to_angles` did not belong bolted to the end of a 700-line file.

## M5.1 stage B - 3+2 tilted-plane indexing (this commit)

**The frame lives in `cl`, not `app`.** `TiltedFrame` (origin, u, v,
axis) is pure pose mathematics next to `angles_to_axis` and
`axis_to_angles`, which it uses and reuses; `app` only owns the
component and the command door. Considered: `geometry` (dropped - the
crate must not know a tool axis exists) and `app/operation` (dropped -
`sim`'s laws and `post` both need the door, and layering forbids them
reading `app`).

**Tilt is a params field, not an Operation component.** Every planar
kind carries `tilt: Option<Tilt>` inside its own params, so the height
expressions, the staleness stamp and the project DTO all ride the
machinery EditOperationParams already has - no new component, no new
command, undo/redo and the file format stay single-track. Considered:
a sibling component on the operation (dropped: it would fork
staleness, retype and the DTO for a field that means nothing to a
3D kind, and `params.tilt()` was already the single accessor the
wrap consults).

**The angles are canonical at the command door.** `Tilt::new` stores
`a`/`b` only after `frame()` succeeds: a non-finite letter or a
zero-norm axis is refused at `CreateOperation`/`EditOperationParams`
by name, so no world state can hold a tilt that cannot command a
direction, and every deep consumer (generate, preview, post, DTO)
sees only a frameable tilt. Considered: refusing at generate time
(dropped: violates "validate early" and would let a bad tilt live in
a saved file).

**Heights are axis distance from the anchor (`h - origin.z`).** The
identity-exact law: `None` tilt and a frame at the origin shift
nothing, so every prismatic file and the 13 .nc goldens ride
untouched - and the number the operator reads is still "how far up
along the tool, from where the part is parked". The honest
asymmetry, found by the parked post fixture: the frame maps the
MATERIAL by `(q - o).axis` (full anchor dot), while L1 shifts heights
by `origin.z` only; for a leaning axis with a perpendicular anchor
offset the two differ. A tilted op's cutting plane is therefore
`h + o.axis - o.z` away from the world origin along the axis, not
`h`. Chosen anyway: any law that made the material and the heights
agree bit-wise forced a rotation of the HEIGHT definition for the
upright case (dropped), and the operator-facing convention - the
plane sits `h` above the anchor, along the tool - is L1's, not the
dot's. A rigid parked twin must shift every height by the dot law
(see `pocket_parked`'s comment). This is the entry to re-open if
stage C's tool-length compensation makes the offset matter in
practice.

**Verification splits: plane laws local, steel laws world.** The
planar laws (gouge, reach, level, rest) are rigid-invariant, so they
walk the LOCAL path against the LOCAL region - the same argument
that makes the whole wrap sound, run on the side of the door where
the numbers are prismatic. The hardware laws (fixture bands, rapids)
walk the WORLD path with stage A's stamped axes, because the machine
steel does not move with the index. One path is posted, both are
measured. Considered: running every law in world coordinates with
tilted band geometry (dropped: re-implements the enclosure
approximations stage A added, for zero extra truth - containment and
distance are the same question in any rigid frame).

**The tilted stock cap takes the frame-image of the world box
corners.** `stock_top_z` in a plane is not the model top minus
origin.z when the slab is tilted; the max over the eight corners of
the stock/model AABB through `z_local` is never below the slab over
any sampled column - over-report a graze, never hide a crash, the
bands law's exact precedent.

**A tilt refuses a picked selection, by name, everywhere.** A Feature
pick is world geometry; projecting a pick onto a tilted plane is not
implemented, and cutting a shadow of it is the silent wrong answer.
`generate` refuses (command error), `region_of` draws nothing (a
preview must not disagree with a refusal), `chains_of` yields
nothing. The law lives in one line each at the three doors, tested
from all three sides.

**The post fixture rides the cached WORLD path.** `check_document`'s
rotary/stroke laws read poses off the stored path's stamped axes
(M5.1 A's law), so a tilted program is machine-checkable without
re-deriving the frame in `machine.rs`; the tilt-only refusal for a
machine with no rotaries is the same read, no new plumbing.
`posting`'s stream carries the WARN records as always - the gate is
unchanged, which is the point: the tilted path passed the same
verdicts or the post blocked.

**Stage-B scope: the five plane kinds.** Face/Contour/Pocket/Slot/
Trace carry the `tilt` field; Drill, Pocket3D and Finish3D have none
to set - their geometry is the hole list and the world-space mesh,
and tilting those is stage C's tool-length story, not this stage's
plane indexing. The params `sane` door answers for all eight arms
one match: whatever `tilt()` returns must be frameable, so a future
kind that grows the field inherits the refusal.

## Phase 6 / M6.1 - adopting the sibling repo's theme architecture (roadmap commit 44a6006; the item merge and this ledger entry land together)

**Chosen:** one Phase 6 theme milestone, M6.1 "Adopt the sibling repo's
theme architecture" - it absorbs the two items that preceded it (the
dark/light spike and the user-theme loader) because the token-table
port delivers both their features as a side effect. Port solidcraft's
`crates/ui-egui` theme design into our egui shell: one named `Tokens` table as the single color+shape truth
(radii, spacing, fonts, control sizes as tokens), Visuals *and* painted
chrome reading the same table with a destructuring arm that turns an
unwired token into a compile error, a versioned JSON theme format
(`camcraft-theme/1` in ours, adapting their `solidcraft-theme/1` shape:
`{format, name, base, colors, radii?}`), compiled-in defaults with a
disk overlay, runtime switcher through the command law, and a
color-vision layer on top. Six work items specced in ROADMAP.md.

**Why:** our `crates/ui/src/egui_app/theme.rs` is a 122-LOC hardcoded
dark `Palette` - 14 colors, no size tokens, nothing loadable. The
sibling repo proves the token-table design on the same stack (egui
0.36 + egui-wgpu), and doing it *before* the rest of the Phase 6 UI
work means every new panel paints from tokens on day one instead of
hand-picking colors twice.

**Considered and dropped:** (a) leaving theming as-is and doing colors
per-panel - rejected, that's the drift the token table exists to kill;
(b) adopting solidcraft's files verbatim - rejected, clean-room law
covers *HSMWorks* material, but a sibling repo's code is still a
third-party dependency for us: we port the architecture, write our own
files, keep the provenance comment honest; (c) deferring to a post-6
polish phase - rejected, every later UI item would otherwise need a
theme retrofit pass.

**Also in this commit:** the Milestone Summary's stale `M6.1 GPU
acceleration` row was replaced by the three Phase 6 UI rows - the GPU
text predates the Phase 6 pivot and was already superseded.

## M5.1 stage C - 5-axis contour & parallel (this commit)

**The pose rides the path; no new geometry kernel.** Chosen: contour5d
and parallel5d are the stage-B waterline ring ladder and the
finishing3d raster line *as emitted*, plus a stamp pass that writes a
tool-axis vector onto every move that ends in contact. Why: a sphere
is rotation-invariant - the ball-center locus a 5-axis finish must
walk is exactly the ball-center locus a 3-axis finish walks - so
building a second generator would fork the scallop stepdown law, the
forbidden-center solver and the floor rule for no cut benefit. The
5-axis content is orientation, and orientation lives in the pose
field stage A added to `Move`. Considered and dropped: a native
5-axis contact solver (point/edge/face contact on a
steep-flank-derived pattern) - rejected for this stage because
without shank-collision-aware pattern planning (stage E's brief) it
buys nothing the stamp doesn't, at the cost of a second geometry
stack.

**The lead/lag axes are the industry pairing, not the plan doc's.**
The stage-C brief in docs/M5-PHASE5-PLAN.md paired lead with the
tangent rotation and lag (side tilt) with the binormal; the
implementation is the opposite: lead leans FORWARD about the binormal
`n x t` (the rotation that tips the tool ahead of the cut), lag leans
SIDEWAYS about the tangent `t`. That is the pairing every public
multi-axis manual and the ISO 10791-10 test-part tradition uses, and
parameter labels are what a machinist reads, so they ride their
standard meaning - the plan doc's swap is a typo in the doc, not a
different law. tilt.rs's module comment says the same.

**Two kinds, not one kind with a pattern field.** Contour5D and
Parallel5D are separate OpType/OpParams arms with near-identical
fields, matching how 2D Finish/3D Finish already split patterns into
kinds; a shared "5-Axis Finish + enum pattern" kind would have made
the registry, the icons and the pages switch on a sub-field, which is
the shape stage B's tilted-plane kinds deliberately avoided. The
catalog row is one bullet (this one) because the ROADMAP wrote it as
one feature.

**The stamp's laws, recorded with the numbers.** Contact is the
tangent equation `|c - q| = r` checked with a band of `0.01 + 1e-6 * r`
- lattice dust and arc-fit slack, never a fraction of the radius a
bite could hide in. Rapids carry no pose (the A/B heads should not
swing during a G0 - and stage A's post writes rapids with no axis word
anyway); a cut move whose END sits in air holds None, so an uncut hop
never pretends to be oriented. Drill cycles cannot be re-posed by a
stamp at all: a tilted spindle with a straight bit would drive the cut
off its own axis, so `stamp_axes` refuses by name (GenError::Invalid)
rather than silently dropping the cycle. A control that states no
direction (a NaN that slipped past the sane door) falls back to
vertical WITH an eprintln - log, never silence.

**max_tilt is a cone fold, never a hard stop.** An over-lean folds
the axis back onto the cone `angle(n, a) = max_tilt` in the plane the
rotation produced - the direction that was reachable stays reachable,
only the excess angle is cut. Refusing the move instead would drop
metal; clamping the angle but keeping the original tilt plane would
snap the lean sideways into the wall. A None clamp runs unclamped;
`<= 0` and `> 180` are refused at command-creation by the sane door as
machine-reach typos, and lead/lag must satisfy `|v| <= 90` - a
91-degree lead is a flipped tool, which is a different machine.

**The axis trio always rides the project file.** v24 writes
lead_deg/lag_deg/max_tilt_deg for every contour5d/parallel5d op even
at defaults, where sibling scallop fields elide at default. Why: a
re-post must never silently change a program - an elided trio in a
file written by a lean-aware build would load 0/0/90 and post a
VERTICAL toolpath under a tilted op's name. The version bump is the
pure step-up rung (v23 -> v24 adds both kinds' tags, 8 and 9, and
nothing else); the tags carry no pattern byte because the pattern IS
the tag.

**Raster end-to-end rides the hill; the dome's raster stays in the
ignored lane.** The new app test `parallel5d_generates_leaning_axes_
and_verifies_clean` tessellates a 6x24 sphere-cap hill, not the 40x80
dome: support_at marches each raster line against the facet soup, so
a full-dome raster on the app's stock/verify stack is minutes in
debug - the law M4.4 parked in the `#[ignore]`d verdict lane
(motion's own `multidirection_rasters_cross` carries the same note).
The command-chain coverage (params -> sane -> generate -> stamp ->
verify -> status) is identical on a coarse tessellation, and
contour5d stays on the big dome (12.7 s, the same cost class as
finish3d_waterline) because the waterline scan is linear-in-levels,
not march-bound.

**The glyphs are drawn original.** assets/ui/contour5d.png (a tilted
spindle over a contour ladder) and parallel5d.png (a tilted spindle
over a raster fan) join the bespoke set - Tabler has no 5-axis mark,
and the pair follows the drill/post/sim/finish3d/fixture precedent
recorded in LICENSES.md, whose note also finally names fixture.png
(an old omission the stage-C sweep caught).

## M5.1 stage D - swarf & advanced swarf (this commit)

**Swarf is the directional raster, not a new geometry kernel.** The
kind reuses finishing3d's raster line verbatim with one addition -
the pass direction `(ux, uy)` read off the mesh - so with a vertical
axis its ball-center positions are BIT-IDENTICAL to stage C's
parallel5d over the same mesh and parameters. Why: the scallop
stepover law, the seam clamp and the ramp/bridge choreography are
one body of tested arithmetic; a second swarf-specific line emitter
would fork all of it for a direction vector. Considered and dropped:
HSMWorks-style "swarf with automatic axis" as a Finish3D pattern flag
- rejected for the same two-kinds-not-one reason as stage C's pair:
the registry, the icons and the pages would switch on a sub-field.

**The ruling law: the mesh tells the pass which way to run.**
`multiaxis::ruling::analyze` fits each facet's surface operator
along its two candidate directions and claims a ruling where one
curvature is development-plain (below the threshold) and the other
is not; a coherent pass direction for the whole set comes from a
signed vote. On a ruled contact the posed axis lies ALONG THE RULING
- lead, lag and the max-tilt cone are IGNORED there (the bullet every
machinist will check: leaning off the blade would make the cylinder
facet cut a groove the profile doesn't have; the lead does no work
on a ruled surface because the cutting velocity is already along it).
Where the analysis withholds its claim - rims, doubly-curved zones,
meshes too coarse for the fit - the contact falls back to stage C's
lean law, so the kind never refuses, it just answers with the
weaker - and honest - orientation.

**Advanced swarf is a tolerance ladder, not a rest-material
strategy.** Pass i from the top cuts at `tolerance·4^i` with
stepover `·2^i` - both because the scallop law is
`stepover ~ sqrt(8r·tol)`: quadrupling tolerance doubles the
scallop-height budget AND the spacing, so each coarse rung removes
its own stock in wide, shallow passes and the final rung is exactly
the plain swarf path. The centers stay tangent to the MODEL every
pass (no z-offset), so the ladder is pure pass-count choreography -
no rest-mesh machinery. Considered and dropped: true rest-machining
(recur over the previous pass's stock) - stage-6-era cost, and the
scallop law already bounds the leftover cusp to the pass's own
tolerance. `passes` is a u32 field gated 1..=4 by the sane door
(more than four rungs puts the coarse pass's stepover past the
part's own width on any realistic tolerance), and the motion-layer
`swarf5d_advanced` carries the same door by name for CLI callers.

**The refusal taxonomy, stated at the seam that knows.** A dome
("no ruling found... this surface wants Finish3D or a 5-axis
contour"), a Z-wall ruling ("the ruling runs along Z; try Contour
or a 3-axis kind"), a mesh whose fit answers everywhere-degenerate
("too coarse for a ruling; Parallel will raster it"), an empty
selection - each refused by name at the first law that can see it,
never a silent fallback to a vertical raster. The 12-row
`stamp_follows_every_input` registry pin (was 8) now carries both
swarf arms - the swarf kinds' axes are per-move, so the same pin
that caught a dropped lead on contour5d catches one here.

**Closed solids are load-bearing at the app level - learned the
hard way.** The swarf family routes through the footprint law
(`region_for` -> `slice::project` over 16 horizontal slices), and
project refuses an OPEN mesh by name ("the mesh is open at z =
...: dangling chord ends"). The app's blade fixture was a tube with
no ends; it now carries end caps, which makes the whole chain -
footprint, selection, stock verify - behave like a real bladed
part. The caps cost something in the pose test: their junction rows
are rims, and the analysis (correctly) withholds the ruling claim
from the columns beside them, at 16 rings that is a large share of
the tube. So `blade_pose_law` counts blade-lying poses (>= 8, every
pose unit-finite, no rapid poses) instead of asserting the
every-body-move universal - the universal identity lives in
motion's `ruling.rs`, tested at the tessellation where the fit is
confident. A dropped ruled-branch would show as zero counted poses
HERE, which is what the test is for.

**stamp.rs crossed the 400-line seam - split at the test, not the
law.** The stamp's exact-angle identities (the pitched-plane ladder
that proves a 15-degree lead to 1e-9) moved to
`multiaxis/stamp_tests.rs`; the law and its documentation stayed in
`stamp.rs`. The seam is the one AGENTS.md names - a file grows past
400 lines, split along the domain edge; here the edge is "the law"
vs "the identities that pin it".

**The future-version probe must be spelled from the constant.**
`wrong_version_is_refused` hardcoded v25 as "the future"; the
stage-D bump to v25 (the rung adding the swarf tags 10, 11 and the
ladder's `passes`, nothing else) made the test ask for a refusal of
a version that was then CURRENT - red in the way that blames the
compiler. Now the probe is `project::PROJECT_VERSION + 1`: a rung
named by hand cannot survive the day the stamp moves.

**The glyphs are drawn original.** assets/ui/swarf.png (a tilted
barrel-cutter along a ruled band) and advancedswarf.png (the same,
through three widening rungs) join the bespoke set per the
contour5d/parallel5d precedent; LICENSES.md names both.

## M5.1 stage E - rotary contour & parallel (this commit)

**The axis is read by circumcenter concurrency - the one anchor law
that is tessellation-proof.** A revolve facet's circumcenter lies in
its own meridian half-plane, so the facet's NORMAL LINE meets the true
axis exactly, whatever the ring count or segment heights. Considered
and disproven on paper before code: anchoring the normal at the
centroid misses by a sixth of the arc at first order (the fan's
one-diagonal asymmetry), the edge midpoint by a quarter, plane-closest
and welded-vertex-average normals fare no better - all three refuse
well-turned parts at coarse tessellations. The normal stays the raw
per-triangle one (welding would reintroduce the fan asymmetry the
center already fixed). Direction: the axis is the EIGEN-SEPARATED
extremum of the area-weighted Σ a·n nᵗ - but BOTH the min and max
eigenvectors are tried and the better RMS wins, because a steep cone
tips the second moment to the other side of 45 degrees (motion's
`a_steep_cone_still_yields_its_axis` exists for exactly that).
Position: least squares of k̂·o = k̂·p with k̂ = n×û over every facet,
plus one pin row (ûûᵗ) through a representative point so the solution
is the axis LINE, not a plane of solutions.

**A bit-exact upright frame is snapped, not hoped for.** Eigen-solves
return (0, 0, 1±1e-16) for parts that stand on the table, and 1e-16
in the direction breaks the `TiltedFrame::is_upright` identity that
the golden law leans on (upright frames carry arcs verbatim, tilted
frames chord them - a hair-off frame would silently switch modes
per platform). So the fit snaps a candidate to the exact axis when it
lands within the noise floor nz = 32·f64::EPSILON·rho of it; anything
beyond is a real lean and rides the chord path honestly. (The same
lane caught a genuine bug on the way: `sym_eigen`'s rotation angle
was an `arctan`, which cannot see the quadrant - now `atan2`.)

**Coarseness gets its own gate: a polygon pretending to be a wall
refuses.** Circumcenter anchoring sees through flatness masquerading
as roundness only via concurrency, so the fit also sweeps the fitted
facets' bearings about the candidate and refuses when the WIDEST
EMPTY SKIP exceeds sqrt(8·REL_TOL) radians - that angle is exactly
the chord-sagitta budget written differently (a turn with skip g
stands in for a wall whose chords sag rho·g²/8). Facing flats bear
no azimuth and justly take no part - shoulders are sharp where
lathes leave them. 16 rings pass, 8 refuse; the fixture budgets are
in the tests.

**The twins refuse each other's degenerate stances, with one shared
sine.** A standing bar asked for parallel passes answers "passes
along a standing bar are its rings: Rotary Contour", and a laid bar
asked for contour rings answers the mirror sentence pointing at
Rotary Parallel; both read LEAN_MIN = sin 1° off the same constant,
so the pair cannot disagree about where the boundary sits. An
isotropic fit (sphere: normals concurrent at a POINT, every axis
equal) is refused as DOMED ground - stage C's kinds, by name - and a
model no candidate fits gets the taxonomy's generic line. Why
refuse a stance instead of computing through it: at lean → 0 the
station map s(H) = (H - o·ẑ)/(û·ẑ) divides by nothing the parameters
can speak in, and inventing a frame there would be the silent wrong
answer the Prime Directive names.

**The pose is the RADIAL, not the surface normal - and the cone
measures off that seed.** On a turned work the line the tool actually
offers the spindle is the perpendicular from the axis to the tool, so
`radial_pose` replaces stage C's facet normal as `command_axis`'s
seed n; flats (caps, floors) get radial poses too - a wall-straddling
ball cares about the axis, not which way its facet faced. Lead leans
in the (radial, travel) plane, lag rotates about the travel, and the
max-tilt cone folds toward... FROM the radial: off = angle(n, a) -
so at lead = lag = 0 the pose IS the radial exactly, cone or no cone
(an app-level law `radial_pose_law` pins: every cut move's pose meets
the axis line to the tool point). Where a point lies ON the axis no
radial exists; the move holds vertical and the fit says so on stderr
- a turned bar's rings do not go there anyway.

**No new pattern kernel, and no new heights machinery: stages A-D
composed.** Rotary contour IS the waterline rung ladder in the axis
frame with heights mapped to axial stations s(H) = (H - o.z)/(û·ẑ) -
the exact stage-B identity when the part stands on its z, and the
honest station reading when it leans; rotary parallel lays the axis
down (local z = as close to world up as the axis allows, an exact
swap for a flat-lying bar) and runs the swarf raster there, where
the rings degenerate into exactly the generatrices the kind is named
for. The fitted axis is MODEL data, not a parameter: the digest adds
the family's pattern (the twins' tags 12/13, the stepover-or-derived
split the raster kinds ride) but no key names the axis - two runs on
one mesh agree by determinism, and a re-fit after a model edit can
never go stale against a stored axis. Project v26 is the two params
tags and nothing else (a v25 file steps up with no new key).

**The fixture bowtie, caught by the footprint law.** The revolve
helper split its wall patches along a SIDE edge (a1,a2,b2)+(a2,b1,b2)
- on a cylinder the four corners are not coplanar, and that pairing
is a bowtie: two crossed diagonal slits per patch. `fit` never
noticed (normals are per-triangle truth), motion's identity tests
never noticed (both sides of an identity read the same mesh), but
the app's `slice::project` footprint law refuses an OPEN mesh by
name and said so - "the mesh is open at z = -15: 32 dangling chord
ends" - on the first honest run. Both fixtures now split along the
DIAGONAL (a1,b2,b1). The lesson rides stage D's: closed solids are
load-bearing at the app level, and a fixture bug is worth catching
even where only a stricter law can see it.

**The glyphs are drawn original** (assets/ui/rotarycontour.png: the
turned bar with its station rings, the ball riding OUT on the radial;
rotaryparallel.png: the same bar with generatrix lines along it) -
no Tabler mark says 4-axis rotary, and the ball is deliberately off
the surface on the radial line, the kind's whole claim; LICENSES.md
names both.

**Phase 7 - the SDF preview engine: what the research decided.** The
user's directive (replace the voxel preview with an implicit
signed-distance rendering of the machined volume) fixed the goal; the
shape was ours to take, specced from a Perplexity sweep of the public
literature: MERL's composite-ADF milling simulation gave the two
cornerstones - machined volume = Boolean difference of distance fields,
and the inverted-trajectory law (evaluate in the tool frame, so a
5-axis pose is one matrix inverse, not a swept geometry); the 2026
GPU-SDF + Marching-Cubes material-removal twin paper (~105 FPS, ~35x
mesh booleans) proved real-time feasibility on exactly our hardware
class; the distance-accumulating voxel twin supplied the
running-minimum accumulation that makes overlapping cuts
order-independent and seam-free. Choices taken unattended: **(1)** the
field is sparse BRICKS under a flat brick map, not an octree - the
streaming/voxel-hashing line keeps reaching this for updatable fields
(deep trees lose cache coherence; updates want shallow), reconsider if
a part's band ever outgrows the brick map's memory law; **(2)**
direct sphere-traced rendering in the viewport is the primary path and
marching cubes is a DERIVATION for triangle consumers (picking, STL
export) - the mesh-extract-only alternative was dropped because it
re-imposes a resolution choice at every frame, which is the voxel sin
Phase 7 exists to leave behind; **(3)** the pseudo-SDF pessimism of
CSG/sweeps is kept as a FEATURE, in the direction that underestimates
clearance - rest over-reports, gouges never hide, the dexel's own
honest-noise law promoted to a field - with eikonal re-normalization
owed only to the render's shading; **(4)** the sampled-pose
swept-volume fallback is display-only by law - a verdict query needs a
closed form or it refuses; **(5)** the dexel retires only after
M7.3's dual-engine verdict agreement on the heavy lane, so the phase
is additive, reversible, and the existing gouge/rest/deviation goldens
stay the refusable oracle throughout. Considered and dropped: hybrid
dexel+SDF forever (two truths to reconcile in every verdict - the
verification-layer law forbids a second source of truth, and a second
verification layer doubles the agreement surface); sparse voxel
octree as the field store (the middle option the M1.7 question offered
- strictly dominated by brick maps for dynamic narrow-band fields).
Slotting it as Phase 7 after Phase 6 (UI theming) rather than between
5 and 6 is ours: nothing in it blocks Phase 5's kinds, and the
renderer wants Phase 6's engine settled first.

## M5.1 stage E2 - rotary offset roughing (this commit)

**The pocket ladder runs in wrap space, and stepdown is the RADIAL
infeed.** A turned solid has no z-staircase worth roughing - its
levels ARE its stations, already laid along the axis - so the kernel
develops the part one full period into (s, v, rho) (circumference
arc length, axial station, radius) and runs pocket3d's staircase law
there with the infeed along rho. The consequence is a fixture
lesson, recorded rather than padded: the staircase reads each
column's stock off its own part surface, so a part step WIDER than
the infeed can't be shaved by one radial plunge - the turned bar's
6.0→5.0 step leaves that whole annular shelf to the finishing kinds
at infeeds ≥ 1.0, and the tests budget a sub-1.0 infeed to peel it
honestly instead of inventing levels the geometry doesn't have.
Considered and dropped: a z-direction ladder on the wrapped body -
it re-reads the same law but pretends a revolve has floor-to-floor
heights it does not have.

**tolerance is verify budget only; stock_to_leave folds into the
plan.** The kernel never consumes tolerance (`let _ = tolerance;`):
the wrap development is exact, there is no sampler for a tolerance
to bound. It rides outward solely as the Envelope verify's
finish_tol. stock_to_leave gets no separate offset pass either - it
folds into the plan's center offset, so the ladder peels down to
finish-plus-leave exactly once. Both still ride the params tag: a
parameter that moves no point on the path still rides the digest,
so a re-run with changed numbers is never mistaken for an
identical one.

**No retract_z parameter: the kernel names its own rapids plane.**
A round bar has one honest air plane - one infeed clear of the
biggest radius plus two tool radii, rho_max + stepdown + 2r,
computed off the wrap. The twins' user-named z retract belongs to a
prismatic stock whose top is a plane; ported here it would be a
dial with exactly one safe setting, which is a law, not a
parameter. Considered and dropped: retract_z with a default - the
safe default IS the computed plane, so the parameter adds only ways
to set it low and gouge.

**The verify posture is law (f) plus a named gate: Mode::Envelope,
finish_tol = tolerance, ball-nose refused at generate.** The swept
sphere is pose-invariant about its own axis, so stage C's envelope
argument holds verbatim for radial poses - no pose-respecting sweep
tax. An end mill's flat profile is not pose-invariant in the sense
the law needs, so the ball-nose gate refuses by NAME at generate
rather than verifying a path through an envelope it doesn't
enclose.

**The wrap mesh is one period deep, with seam walls at the station
table.** The developed body is the 2·pi·rho_max duplication, seam
walls closing every band at the station table, so islands crossing
the seam are one island, not two, and the ladder never sees a
phantom wall at the period boundary. The facet law is by vertex
count: a facet with exactly TWO off-axis vertices (its apex on the
axis) sweeps the full developed rectangle - its wedge - because its
radius spans 0..rho along the developed arc linearly; everything
else sweeps its station patch. That is what lets cone flanks rough
clean instead of leaving an uncut hub band.

**The caller's top_z/bottom_z window STATIONS when the fitted axis
leans.** pocket3d's z-window reads as world heights; on a leaned
revolve the band between two z-planes is not a band of the part, so
the kernel overrides the semantics: top/bottom bound axial stations
on the fitted axis, and the radial ladder spans rho_max..rho_min
regardless - the window selects LENGTH along the bar, never DEPTH
into it. The parameter mirror says so, so the panel speaks stations
too.

**Additive kind, no renumbering: RotaryOffset at ordinal/tag 14,
PROJECT_VERSION 27.** The variant lands after the twins (12/13);
the project file steps v26→v27 with the one new params tag and no
key renames - a v26 file rides up with nothing new to store. The
fitted axis stays MODEL data, stage E's law unchanged: the digest
names the pattern, not the axis. Ribbon glyph
assets/ui/rotaryoffset.png is drawn original - a stepped shaft with
the ladder wrapping it radially; no generic set says rotary
roughing, and LICENSES.md notes it as original.

## M5.1 stage E3 - multi-axis roughing (this commit)

**The composition law: MultiAxisRough = pocket3d_prev + stamp_axes,
and nothing else.** The kernel builds the prismatic ladder move for
move - same lattice, same mask seam, same Empty remap - and then
runs stage C's stamp over the finished path. That is the whole kind.
It buys the verify posture for free: a sphere's envelope is
rotation-invariant about its own center, so the Envelope scan of the
posed path encloses exactly the volume of the vertical one - stage
C's law (f) restated for the rough. Considered and dropped: a
roughing kernel of its own that searched poses while it cut - it
would move positions, and a ladder whose positions differ from the
3-D pocket's is a different operation, not the pocket with the
spindle posed; the identity IS the safety argument.

**Floors pose nothing: a degenerate contact normal yields pose None,
which posts as honest vertical.** The stamp reads the axis off the
contact normal at a move's end; on a floor the contact is the ball's
bottom pole and its normal is the tool axis itself - the search
returns d = 0, poses None, and the move runs vertical. Wall
tangencies are the only posed moves the ladder ever makes. The probe
agrees: on the slab+boss job all 12 poses of the stamped ladder come
out horizontal where walls ask for them and vertical everywhere else
- the floors are silent, exactly as the law says they must be.

**The ball-nose gate sits at generate, and names kind and tool.**
The stamp's tangency search reads contact off a sphere; a flat
end's flank is a cylinder the law does not own - there is no centre
to place off a wall, so a leaned end mill is a gouge waiting for its
verdict. The refusal is "Multi-Axis Rough needs a ball-nose tool,
'<name>' is a <kind>": same posture as the wrap's gate (E2), refuse
by name rather than verify a path through an envelope the tool
doesn't have.

**rest_rough and feed_opt are deferred, and their absence is stated
rather than faked.** The pocket's bridge carries two switches the
stamped ladder ships without - the kernel reads neither, so the DTO
mints neither. The honest reason: both features are specced against
a vertical-only tool, and the stamp has never been shown to commute
with either (a rest scan under a leaned ladder, a feed modulated by
an engagement the poses redistribute). A switch that changes
nothing under poses is a lie with a UI; absence is truer. Considered
and dropped: wiring the flags to the pocket arm and hoping - hoping
is not a verification.

**The wire is a pure step-up: v27 -> v28, tag `multi_axis_rough`,
"step-up is the stamp itself".** Serde snake_case mints the tag
from the variant name; the version steps with one new params tag and
no key renames, so a v27 file rides up with nothing new to store.
Unlike the wrap ladder, which computes its rapids plane and mints no
hop key, this ladder's retract_z is the pocket's own stated hop and
rides the file via `skip_serializing_if` - absent at the default,
present once named, bit-identical to the Pocket3D bridge's shape.

**The registry law re-confirmed from the inside: KindInfo::ALL grew
15 -> 16 and the UI followed without being told.** ALL_LABELS and
KIND_COUNT feed the pickers dynamically, so the new kind appeared in
every menu for free; only the four files that must spell each kind
by hand grew arms - icons.rs, panes.rs, pages.rs, interact.rs. M5.0
and M5.1 kinds since then keep the claim honest: a new operation is
a params struct, a generator fn, a table row and four hands.

**A law found by this stage's own test: GenerateOperation::new()
generates at construction, so program order is masking order.** The
identity test that wants a stamped ladder and a plain pocket side by
side in one world fails for a reason no one wrote: names sort, a
later-generated op digests the prior program (Pocket3D reads the
re-machining mask from it), and the earlier op's stamp is masked
away - the twins go Stale against each other. The test therefore
runs two lone worlds, mirroring the remachining tests' law that a
mask is a statement about the program, not about the part.

**Discovery on the way: the shipped Pocket3D with a ball tool over a
finished floor verifies CLEAN.** No app test had exercised that pair
before E3's twin needed it as the vertical baseline. The floor law -
d = 0, pose None, vertical - holds empirically, and because the
stamped kind's positions are the pocket's bit for bit, the twin
inherits the verdict rather than earning it separately.

## M5.1 stage F - collision-aware orientation (this commit)

**The dodge is pose-only, and law (f) is the license.** A sphere's
envelope is rotation-invariant about its own center, so at a fixed
CL point the cut is fixed and the axis is free: leaning the spindle
cannot gouge a micron the vertical pose did not already own. Stage F
spends exactly that freedom - it changes NOTHING but the axis
fields. The test proves the claim and not a weaker one: positions
bit-for-bit identical to the flag-off path, surface-stated poses
untouched wherever the shank stays quiet, floors posed ONLY where
the shank speaks. Considered and dropped: re-running the escape
search on the surface-stated poses themselves - F never overrides
the contact law, it only overrules it where the tool above the ball
disagrees with it.

**The collision test is verify's law (c), not a new one.** The
enclosure mirrors `camcraft_sim::verify` band for band: a vertical
band's column floor stands AT the band's lower end, a leaning band's
dips to the lower axis end minus the band radius (only its swept
rim reaches below the axis-plane end), and the annulus inside the
cutting radius is the flute's own kerf, excused - with the strict
`box_top > h0` gate so the floor being cut can never report itself.
The motion side tests the CONTINUOUS facets where verify samples
them, so a graze may be over-reported here and stay clean there:
over-report a graze, never hide a gouge, and verify remains the
post-time law. One builder (`shank_of`) feeds generator and
verifier from the same Tool stack, so they can never disagree about
what the shank is. Considered and dropped: a continuous
swept-volume collision test - cost with no verdict the endpoint
enclosure plus verify doesn't already give, and the house law is
conservative, not exact.

**The tilt amount is geometry's answer; the azimuth is what the
head pays.** Discovered by the law tests and encoded in them: every
row of a flat wall gets the SAME tilt regardless of the previous
pose - the escape candidate net (tilts {0, cap/4, cap/3, cap/2,
2cap/3, 3cap/4, cap, th0} x 8 azimuths 45 deg about the contact
normal, cap = max(cone, nominal lean) clamped to 85) is sorted by
angle from the HELD pose, so shortest axis travel decides which
clear azimuth wins, ties broken by the nominal's own lean. The held
state resets to home after an unposed feed and holds through rapids
- the smoothing is what stops an escape oscillating between
neighboring moves. The refusal contract is the flag: off, the path
generates exactly as in E3 and the verifier warns at post time; on,
generation REFUSES by name ("the shank of the tool touches the part
at move i...", naming the cone's remedies) - the Prime Directive
favors refusing to ship a known rub.

**A tool with no clear angle must refuse, never thrash.** The
narrow-slot fixture states the design law its doc-comment carries:
a stub clears only if its flute is short enough that its rim
reaches the wall's z-span AND a nonzero cone window exists - for
the 6 mm-flute fixture, theta in (9.6, 33.6] degrees. When the
geometry leaves NO clear angle inside the cone, the honest answer
is a refusal naming the shank and the cone, not a spindle flipping
between poses; that fixture pins the escape-vs-refusal boundary.
Considered and dropped: offering the dodge on the rotary kinds -
their radial axis IS the surface's answer, there is no free axis
left to spend.

**The wire is a pure step-up: v28 -> v29, `shank_avoid` inside the
three axis-law DTOs.** `#[serde(default)]` means an absent key is
exactly the v28 pose law (dodge off), so a v28 file rides up with
nothing to translate; the flag is always written on save. It rides
the params digest, so flipping it ages the cached path - staleness
stays derived, never tracked. The UI is one checkbox, "Shank
avoidance (5-axis)", shown in the params panel for the three
axis-law kinds only - the kinds the law actually reaches.

**Gate notes:** 23/23 test binaries green, the 13 goldens
byte-identical (the flag defaults off, so nothing's path changes);
`crates/motion/tests/multiaxis_avoid.rs` carries the five law tests
(never-moves-path, floors-only-where-shank-speaks,
escape-inside-cone-and-calm, slot-refuses-by-name,
quiet-shank-purity), the app seam four, `project_roundtrip` the v29
step-up.

## M5.1 stage G - machine simulation (this commit)

**The head turns on its own clock: every move is a linear leg and a
rotary leg, secs = max(the two), and a family HOLDS at arrival.**
The timeline paces XY at the feed and A/B/C at the rotary rate;
the slower leg keeps running while the faster one freezes at its
spot - XY parked in air while the head swings, the head settling
after the path has arrived. When the two legs take equal seconds
the axis is bit-exact prismatic, so the split costs nothing the
old sim did not already do, and the sim viewer shows the arc the
machine actually takes, not the straightened version of it.

**Rotary rates live under the rotaries' own roof, not in one
shared "angular feed": `Traverse.rot` is per-axis deg/min with
normal/rapid pairs, fed from the .mch mirror** (machine.rs mirrors
them), rapid falling back to normal, feed to normal-or-rapid.
Considered and dropped: a single angular rate - the .mch format
names rates per axis family, so a law invented at one family's
granularity gets refought at every call site, and one rate would
have to guess which letter it speaks for.

**The hold-at-arrival law is behavior-only provenance.** Public
G-code manuals' coordinated-motion blocks: each commanded axis
family travels at its own rate and holds when its part is done,
the program waiting on the rest. Nothing of HSMWorks was read for
this; it is the manuals' law in our own code, per the Clean-Room
Protocol.

**The sim threads the post's modal pose law, it does not re-derive
it.** sim.rs carries each segment's [from,to] letter pair from the
same law the post writes - rapids hold the pose, an unposed cut
returns home. A second copy of that law in the verifier is the
classic way for poster and sim to disagree about where the head
was; one law, threaded.

**Law (c3) tests the arc law (c) can't see: 4 path fractions x 5
pose fractions, pose endpoints in, path endpoints out.** The
to-point at the to-pose is law (c) verbatim, so path endpoints are
excluded; but "at the to-pose, not yet at the to-point" is
reachable - when the head's arc is the short leg it finishes its
swing while XY is still in transit - so the pose endpoints stay
on the grid. And a zero-XY plunge that turns while it descends is
never skipped: its lean column slides through a Z window that is
a superset of both endpoints', so neither endpoint sees what the
middle does.

**One verdict per band: (c3) speaks only where (c) is clean.** The
moves law (c) convicted are skipped via the shanked list rather
than merged into a combined verdict - (c)'s same-band warning
keeps its wording and a band never reports twice.

**The cost is paid with a sample filter, not a narrower gate: (c3)
runs where its twin (c) runs, mode != Engrave.** The obvious gate
- Area | Boundary - would have been inert on every generated
posed program: every posed 5-axis kind verifies as Mode::Envelope
(`crates/app/src/operation/kinds.rs`, rows 728-802), and a law
that never runs is decoration. The 20-state grid therefore costs
against the nearby columns only: the 40 states are collected with
a union-bounding-box filter over the boundary samples. Considered
and dropped: the Area | Boundary gate, inert - see above.

**The wire is a pure step-up: v29 -> v30, `Warning::SweepHit` the
reason.** A v29 file cannot carry the new tag, so the serde
ladder just raises the number and there is nothing to translate;
the lossy-JSON mirror fallback pattern from M3.3 A was considered
and dropped as unnecessary here - it guards a round-trip an
absent tag can never make lossy. The UI keys the tag at 200+part
in fold_warnings, clear of FixtureHit's range, and the status
line reaches it through the last_move or-pattern.

**Gate notes:** 51/51 app integration, 131/131 UI, 68/68 unit,
clippy -D warnings clean on all four crates; the 13 .nc goldens
byte-identical (stage G warns, it never edits a path);
`crates/sim/src/sim.rs` carries the 22 leg-split unit tests and
the app seam (`crates/app/tests/sim.rs`,
a_posed_move_paces_on_its_rotary_slot_through_app_sim) proves the
family clock through app::sim.

## M5.1 stage H - entry family and real engagement stepping on the planar pocket (code 1544b16 + this docs)

**One engagement law, one home: the dive/helix/engagement family
lifted into `crates/motion/src/entry.rs`, where both ladders call
it.** The laws specced at M2.1 for the 3-D ladder read only planar
geometry and the move chain, which the 2-D ladder shares, so
`machining2d` reaches `dive_point`, `helix_down`, `engagement_step`
and `chord_hits_plan` there instead of machining3d re-exporting
them for a second copy to shadow; the legality arguments that are
ladder-specific (the islands superset) stay with each ladder.
Considered: separate 2-D copies of each law (dropped: the laws are
identical, and two copies of an engagement law drift - drift is a
gouge risk).

**`run_levels` stays infallible: an entry-shaping failure degrades
to the plain vertical plunge at that level.** A failed offset or a
dive scan that finds no legal midpoint gives that level the motion
the pre-H ladder always took; the rest of the path is unaffected
and still honest. Considered: Result plumbing through the ladder
(dropped for now - the geometry itself is fine, an honest coarse
path beats a refused job, and the verify laws still catch real
collisions).

**Adaptive stops being a relabeled Zigzag: rings step by `tool_r x
(1 - cos(max_engagement))` - the 3-D side's constant-engagement
law, now on the plane - and the reach door asks the EFFECTIVE
step.** Raw stepover stays bounded even at tight engagement angles
because the door measures what the rings actually do, not what the
field says. Zigzag, Waterline and Auto emit their pre-H bytes
untouched: the 13 `.nc` goldens are byte-identical.

**The wire is three additive `#[serde(default)]` fields on the
existing Pocket DTO arm; PROJECT_VERSION stays at v30.** A file
without `entry` reads Plunge, the seed's 1.0 mm helix radius and
the law's 45 degrees - the pre-H behavior - and re-saves complete;
an unknown entry tag refuses as Corrupt rather than guessing.
Considered: a version bump plus migration (dropped: additive
defaults are a lossless step-up, the same precedent as `rest_rough`
and `feed_opt`).

**The stamp digest gains the two floats, then the strategy byte,
then the entry byte.** Fixed order so every path to the hash agrees;
pre-H operations come up Stale exactly ONCE and regenerate
byte-identically - the staleness is one-time and provably cosmetic.

**The tilted-helix test law: a tilted pocket's cached path is
already world-expressed, so axial depth is a plain dot with the
axis letter.** The tilt door ran at generate; mapping the stored
path through the frame again invented a phantom slope of stepover x
sin(belt) - the new tilt test caught it in development, and the law
is pinned there so the next reader dots instead of transforms.

**Toolchain lint drift rode this commit: rust 1.99's `manual_find`
and `collapsible_if`/let-chain lints newly fired on committed stage
F/G code.** sim's `sweep.rs` and motion's `avoid.rs` take the fixes
here because a red clippy lane blocks every gate; the walk guard
keeps NaN semantics through the NaN-honest form (`total <= 0.0 ||
!total.is_finite()`), so the lint fix changed no verdict.

**Gate note, recorded so nobody hunts it: a gate lane reported a
compile error citing a `cutter_axis` field on `MultiAxisContour` -
verified false.** No such identifier exists anywhere in the tree;
a transient mid-edit state, not chased.

## M5.1 stage I - 5-axis flowline finishing (code 378184a + this docs)

**Two kinds, not one table: Flow5D takes its own `OpType` arm and
its own params struct rather than a mode flag on
`Contour5D`/`Parallel5D`.** Its source is the whole model surface -
the direction field is a reading of the mesh, not a pattern choice
- and its geometry is a curved-line family with its own seeding
law. A mode flag would have forked both twins' signatures: every
or-pattern the twins already share (`kinds.rs` carry, `sane.rs`
door, `params.rs` digest) becomes a two-shape read and the
parallel's pattern field starts lying about which lines it picked.
Considered: `pattern: Flow` on Parallel5D (dropped: the raster
twins' DTO and digest author LINE CHOICE as a parameter; making it
a reading restamps what every v30 parallel file says about itself).

**The SEED/CLAIM cusp law: half-step seeds, half-step claims.**
Seeds walk the shallow footprint's bounding box on a lattice
`SEED_FRAC` = 0.5 of the stepover apart (`crates/motion/src/
finishing3d/flow.rs`), and a seed whose column lies within
`CLAIM_FRAC` = 0.25 stepover of an already-traced segment (or a
spent seed point) is spent WITHOUT tracing - the traced set is
maximal under the claim radius. (`CLAIM_FRAC` shipped at 0.5 with
stage I's code; commit d597201 halved it to 0.25 so the kernel
doc's law `CLAIM_FRAC + SEED_FRAC/2 <= 0.5` holds as an identity -
at 0.5 a diverging field left ground 0.56 of a step from every
traced line, past the half-step promise, and straight ground,
whose rows answer to `SEED_FRAC`, was never the law's worry.) A
ground point is never farther
from a line than its nearest seed plus the seed's claim, so
turning ground keeps the CLAIM + half-seed bound (the swirl test
pins 1.1 stepovers, where the circles' radial spacing quantizes);
straight ground, where the half-step rows are ALL traced, keeps
the straight raster's own promise exactly - every interior point
within half a stepover (plus the march's lattice quantum) of a
chord, scallop a QUARTER of tolerance. The spacing promise is the
seeding's; the envelope verdict, never the spacing, is what says
nothing gouges. Considered: seeding one line per full stepover and
claiming at half (dropped: a turning family converges as well as
diverges, and the half-step lattice is what keeps the promise
inside the grid's quantization).

**The direction field has a global fallback, so unclaimed columns
still cut.** Flat interiors make no directional claim and the
waterline rings do not machine them - a flow pass that stalled
there would be silently unfinished, the Prime Directive's own
worst shape. So the field answers an unclaimed column with the
region's dominant direction: the double-angle (2theta) vote
weighted by facet area, theta and theta+PI casting ONE vote for
the line they share, and +x when nothing claims at all. On plain
ground the whole path IS the parallel raster's answer, said in the
geometry and not in a warning - flow never fails to cut.

**`curv.rs`: one curvature kernel, two readers.** The shape-
operator fit lifted out of `ruling.rs` (stage D's machinery), so
the ruling scan (swarf) and the direction field (flow) solve the
SAME least-squares normal-difference systems per facet - the field
the two kinds see cannot drift apart, and a fit corrected fixes
both. `min_dir_xy` states the flow reader's refusals: a starved
neighbourhood (< 6 answerable facets), an isotropic facet (a
sphere's eigenpair is numerical noise, and noise must never steer
a spindle) and a claim lying along Z all answer None - no claim,
never a guessed claim.

**v31 is a PURE step-up.** Flow5D rides the existing component DTO
grammar as one new tag, `flow5d` (the serde rename states it -
the automatic snake_case of `Flow5D` would be `flow5_d`), and the
migration arm only raises the number: a v30 file cannot carry the
tag and every key v31 adds has a default that IS the old
document's behavior. The stepover-None derivation law is the
raster twins' cos law restated -
`p.stepover.unwrap_or(scallop_stepover(r, tol) * mz_min)` - and
absent `retract_z` means the kernel's own rapids plane.
`shank_avoid` is surface truth, not a free axis: absent reads as
off, but the flag is always WRITTEN - the digest and none-key
handling joined the raster twins' or-patterns so hash, file and
world agree. The loader's sane door (a 200-degree cone refuses as
Corrupt) is the twins' door read by the third sibling.

**The field's lookup: nearest facet centre over 3x3 buckets with a
two-cell reach cutoff.** Buckets are one cell per two mean facet
edges, so a 3x3 query sees the centre whose Voronoi cell the
column stands in on any near-uniform mesh; past two cells' reach
the column is over ground no facet answers for, and the fallback's
straight lines are the honest answer there. Considered:
point-in-triangle lookup (dropped: a centre's claim standing over
its cell is the same quantization the ruling scan already trusts,
and the nearest-centroid walk never races a Delaunay structure).

**The glyph is original: no Tabler flowline icon exists**, so
`assets/ui/flow5d.png` is drawn here - the dome cross-section with
the leaning ball-end, the parallel's chord bent into the blade's
own sweep ("ours to give", the house glyph law).

**GATE NOTE, recorded so nobody hunts it: the rotary-mirror law
test in `crates/app/tests/machine.rs` had rotted at HEAD since
stage G** - G moved the rotary rates under the rotaries' own roof
(`Traverse.rot`, fed from the .mch mirror), but G's gate never ran
the app machine binary lane, so the old "geometry, never speeds"
assert survived asserting a law the code no longer spoke. Stage
I's gate ran the lane and caught it; the test now states the
stage-G law truthfully: speeds ride under the rotary's own roof
for the paced head swing, and the prohibition is only deg/min
LEAKING INTO the prismatic `rapidRate` (the ceiling pin stays).
Renamed `rotaries_mirror_as_geometry_and_rates_never_as_the_ceiling`.

**GATE NOTE 2: clippy on rust 1.99 flagged `doc_lazy_continuation`
on `//! - text` dash clauses and `unnecessary_map_or` in stage I's
new code.** Fixed in-flight (`is_none_or` is the spelling; the
module docs were reflowed); the law for future module docs is an
opening parenthetical, not a dash list.

## M5.1 stage J - multi-axis contour on drive curves (code caaab7a + this docs)

**The picks ARE the source: chain picks ride the existing
`SrcRef::Selection(Chain)` field, not a params field.** The path
this kind machines is the user's selection, so it is stored where
every other selection lives, and the digest staleness machinery
follows for free - the stamp's `b'L'` arm (`stamp.rs`) already
re-stamps on model edits, so moving the model re-flags the
operation without any new plumbing. Considered: chains in params
(dropped: it duplicates the selection plumbing and breaks the
Geometry section's single source of truth - two fields would then
claim to say what the machine goes to, and only one would be
stamped).

**The region is the model's silhouette, not the picked loops** -
`region_for` answers with `slice::project` over the whole model
through the exclusion-list arm in `region_for_src`. The chains
ride the surface, so depth and preview must show what the whole
model cuts, not a hairline trace of the picks; and the 2D walk's
"is an open run" refusal never applies to this kind - the picks
were never asked to enclose anything, they asked to be walked.
Considered: region = picked loops (dropped: a preview of the
curves alone hides the model standing under the ball, and the
open-run law would refuse the kind's own honest input).

**The window law refuses BY VERTEX, never clips.** Every chain
vertex must stand within [bottom_z, top_z] or generation refuses
("picked chain N stands at z ..., outside the window"); it never
clips a chain to the window and never silently trims. Refuse-
loudly is the Prime Directive's shape: a clipped chain posts a
program that abandons the user's pick somewhere invisible, which
is exactly the silent wrong answer the law forbids. Considered:
clip-to-window (dropped: same reason, in the worst possible
dress).

**`tolerance` is the verify budget only** - the kernel's
deviation check, the envelope scan's chord allowance. It is NOT
a resample step: resampling the chain into stations is the
kernel's own arc-length law, and borrowing tolerance for it
would make a verify knob silently change where the tool goes.

**v32 is a PURE step-up.** One new tag, `multi_axis_contour`,
auto-derived by serde - unlike `Flow5D` (whose snake_case needed
the explicit `flow5d` rename to dodge `flow5_d`) the variant
name already snake-cases cleanly, so no rename was needed.
`retract_z` is an `Option`, skip-serialized when absent (the
kernel's own rapids plane answers); `shank_avoid` carries
`#[serde(default)]` and is always WRITTEN, surface truth as in
stages F/I. No migration code: a v31 file cannot carry the tag,
PROJECT_VERSION 32.

**Kernel station law** (`crates/motion/src/multiaxis/curve.rs`):
each chain vertex is a station read through the FAN of model
planes meeting there - smooth fans merge to one plane, an edge
is two planes, a corner is three. The ball rests on the
bisector at h = r/max|cos|, and the pass side (which way is
down) resolves by winding - edge-pairing closure plus signed
volume - with air/sink bids as the fallback vote; a floating
chain, one whose points find no model under them, refuses "off
the model". CLAIM: the pose law is stage C's lead/lag tilt then
max-tilt rejection, shared with flow5d's AxisControl - the same
trio, read station by station off the fan rather than off a
raster's surface normal.

**The glyph is original:** `assets/ui/multiaxiscontour.png` -
the dome with the picked chain's dots on it and a leaning
ball-end tool, drawn from scratch; no vendor material, no
HSMWorks reference (clean-room protocol, assets clause).

**UI: the two height planes ARE the machining window.** They
draw the same ladder pair as flow's, but documented as "what
may be cut" rather than cut depths - for this kind an outside
vertex is refused, not clamped, so the pair states the
refusal's bounds.

## M5.1 close-out - the dome flow verdict and the yardstick's chord (code `fix(sim): measure arcs with the generator's own chord law - the dome flow verdict goes clean (M5.1 close)` + this docs)

**The law caught a gouge that the path never cut.** `dome_flow_is_envelope_clean` (motion's verdict lane) read 0.088 mm on the 12x96 dome against a gate of 0.0734, and it rode every flow5d path the app's verify would one day block. The fix decision was made against measurements, not theories, so they are on the record: a faithful sweep of the whole path - 49 samples a move, arcs walked as true circles, exact point-to-triangle distance - never penetrated the mesh past 0.0121 mm, inside the march's own chord promise (tol/4 = 0.0125); and at the blamed rim-band column the ring tail's emitted G2 arc is TANGENTIAL, sinking 0.003 mm of clearance, not material. The rows keep the law. The rings keep the law. The number that failed was the yardstick's own discretization.

**What the scan was doing.** `envelope_scan` cut every arc to chords with a sagitta budget of `cell/4` - 0.094 mm on the r/8 lattice the ball-radius grid builds - which is larger than the `tol + floor` gate (0.0734) the same function feeds. A chord inside a circle cuts the corner; at the tangential ring, where the ball grazes the tilted flank (contact tilt ~43 deg), that corner-cut moves the ball's center 0.09 mm into the forbidden region and the sphere bottom lands 0.075 mm BELOW the facet plane - a trench no machine arc ever drags. The dome flow is all-shallow, so the ring tail repeats the waterline's ladder over ground the rows already own, and its deepest ring sits exactly on the tilt band where the phantom is worst - which is why the waterline verdict rode just under the gate with the same arcs and only flow tripped it.

**Chosen: the measurement keeps the generator's own chord law.** `chord_tol = tol * 0.25` - "every XY shortcut keeps tolerance/4", the same sentence the march and ring law docs already promise - one budget, shared by `subtract_ball` (the Dexel truth) and `center_segments` (the distance truth), so the two truths read one polyline as the law demands. Direction matters: this TIGHTENS the instrument 7.5x (0.094 -> 0.0125). A machine follows the commanded arc, chord or not, so no real gouge hides better than before; only phantoms shrink. The gate formula, every threshold, and every generator are untouched - the 13 .nc goldens and all waterline dumps are byte-free, and MAX_ARC_CHORDS (1024) never binds at this budget on the fixture radii.

**Considered and dropped.** (a) A ring JURISDICTION filter - seed `rings_and_tail` only from steep facets when the raster already owns shallow ground, matching rings.rs's own doc ("the steep flanks ride the waterline rings"): the measurements killed it - the phantom is tangency, not jurisdiction, so a steep-enough dome with one shallow shelf keeps the same false trench, and waterline-only paths carry it unfiltered. It would have fixed one fixture's symptom while leaving the yardstick wrong. (b) Emitting rings as polylines instead of G2 arcs: churns every waterline golden to repair the verifier by disarming the generator, and a polygon chord budget (~tol) is no better than the arcs' own sagitta law - the smooth arc IS the feature. (c) Raising the floor or the gate: forbidden by law, and unnecessary - the truth needs no allowance.

**Ripples, checked.** Production verify's envelope law (f) shares `envelope_scan`, so phantom refusals on tangential rings disappear machine-wide - never a real refusal with them, by the direction argument. The live display sim (`cut.rs::simulate`) keeps its cell-budget chords - a picture on the viewer's lattice, not a verdict - now said in its comment instead of citing a law it no longer mirrors. Rest is untouched where straight lines own it: the dome flow reports 0.04851 mm before and after, below the gate; the finer chords only move ring-graze columns toward their honest (lower) distance.

**The verdict, measured.** Dome flow gouge 0.08238 -> 0.01656 mm (gate 0.07344), rest unchanged; `dome_flow_is_envelope_clean`, `dome_waterline_is_envelope_clean` and the whole finish3d verdict lane (27/27) green in release; sim and app lanes run by the gate; tag m5.1-done closes the milestone.

When the roadmap's open items are exhausted, this file gets its final
entry summarizing what "done" meant, and the discussion happens over it.

## M5.2 stage A - the turning substrate and the facing ladder (code 1648ce9 + this docs)

**The door moved from the ribbon into the command.** M3.2 stage E's
machine-kind law only darkened UI cells; since stage A
`CreateOperation` itself refuses a kind whose family the document's
machine cannot run - turning work on a mill, milling work on a
lathe, nothing refused on a mill-turn - BY NAME, naming the kind,
the family and the mount that opens the door. The table is data:
one new `family: Family {Mill, Turn}` field on the KindInfo registry
row, so M5.2's ~20 remaining turning kinds land the gate for free.
Considered and dropped: refusing at GENERATE only - a created-but-
never-generatable operation is a ghost in the tree, and the law
says validate early. The ribbon's darkening stays as a courtesy
reading the same table; the command door is the law.

**An unstated machine is a mill.** The admits law
`(_, MillTurn) | (Mill, Mill) | (Turn, Lathe)` defaults the missing
document to today's world - every existing test, project file and
golden keeps its meaning, and only mounted `.mch` files can open
the turning door. Dropped: `Machine::kind` defaulting to MillTurn
("it can probably do both") - a guess about steel is exactly the
silent wrong answer the directive bans.

**The facing kernel is a construction, not a check.** The CL point
is the insert nose CIRCLE'S CENTER held at `station + nose_radius`
along the axis, so the ball is tangent to the facing plane at every
point of the pass - it cuts exactly the flat it is aimed at and can
never cross it. No gouge test is load-bearing for this kind, and
the envelope verifier still judges the path like every other. The
ridge that set the parameter law: a pass removes precisely the band
`[s, s+2r]`, so an infeed over twice the nose would leave an
untouched sheet between stations - the kernel REFUSES
`stepdown > 2r` at the door instead of trusting a later scan
(a scan that measures the path against the model, and a standing
sheet is not in the model).

**No stepover is authored because none is missed.** The Facing
params carry tolerance, three heights, stepdown and two feeds -
one pass per station runs the profile's whole width, so the
stepdown IS the spacing. HSMWorks' facing has a stepover field
too; ours drops it as noise the moment the pass is a full-radius
line (the Trace kind's no-stepover precedent).

**Passes run inward only; the descent between rungs is a feed move
in air.** The spun work presents a fresh face only on the cut move,
the retract slides the ball tangentially off the flat it just made,
and the axial stationing move at clear radius travels at
`plunge_feed` (it is motion toward the spinning part, not over a
finished face). Rapids ride `r_max + stepdown + nose` - the rotary
rough's clearance law read radially, exact from convexity of the
distance-to-axis. Considered: retract-hop-plunge per rung like the
mills - dropped, a lathe turret does not Z-hop over a chuck.

**"Facing Plane Z" is the bottom height's word.** The panel reuses
the shared height editor under a kind-scoped tag; the name says
what the plane is (the face the last pass leaves), matching the
refusal texts that name it. The heights seam resolves it exactly
like every kind's Bottom Z - one law, one label per audience.

**`nose_radius` rides `Tool` as the inert-field pattern's third
member** (after `tip_angle`, `pitch`): every kind carries it, only
`Turning` reads it, and the BOUNDS are kind-scoped in `is_sane`
(`0 < nose <= d/2` for Turning, `true` elsewhere) so a retype never
refuses a mill for geometry it ignores. Dropped: `Option<f64>` -
the drill's `tip_angle` precedent showed the Some/None dance buys
nothing over "0.0 means the field is inert here", and the file
grammar keeps its flat numbers.

**The seed breaks the fold for one number.** `seed_facing` takes
every shared value from the `Carried` fold like its siblings, EXCEPT
stepdown: the mill's folded default (5.0) would meet the house
insert's 2r = 1.6 ceiling and refuse the fresh operation on its
first Generate. The seed authors the handbook rough-facing infeed
0.5, which stands under 2r of every TN row in the catalog. The fold
stays honest for every other retype target.

**The band radius is asked of triangles, not vertices - the
stubble bug, caught by reasoning before any test ran.** The first
kernel read each rung's infeed radius off the vertices inside the
window `[s, s+2r]`. A cylinder wall's triangles span the window
with ALL their vertices outside it - both rim rings elsewhere -
and the rung then started at the nose off the axis, cut its little
disk, and left a full-radius annulus of stubble STANDING ABOVE the
facing plane. No envelope verdict catches an under-cut: the path
never touched forbidden material, the material it failed to remove
is invisible to a program-vs-model judge. Silent wrong, the worst
class. The fix is a reach sweep: a triangle whose stations span
`[lo, top]` occupies a pass's window iff the rung start lies in
`[lo - 2r, top]`; its widest VERTEX bounds the whole triangle
(convexity of distance-to-axis over the hull of its own corners -
exact, not a heuristic), so the sweep is events-by-rung into a
max-heap of (radius, life) and overshoot is safe (starting further
in air costs a shorter feed, never a gouge). `profile_of`'s doc
now states where its own convexity promise ends: exact for the
WHOLE-BODY max, silent per station. The law test pins the
signature: every pass ends on the axis and starts beyond
`R + nose` - the wall's tangency - over a drum whose wall has no
vertex in the rung's window at all.

**The ribbon tip order: the tool list speaks first.** A Create
cell's tip checks the empty tool list before the family darkening
(imgui's swap, and the deeper trouble - nothing lights until a
tool exists, whatever the machine says). The lathe-darkening proof
runs on the demo document, which carries tools, so both laws stay
visible where each is true; the command door keeps refusing for
both reasons whatever the ribbon guessed.

**The door tests declare the kind by word.** The turning suite mounts
`Machine { kind: Some("lathe"), .. }` instead of an X/Z-only axis
table: the axis-inventory inference is the .mch's own truth (pinned
in the machine suite), while the family door only cares what the
document IS - declaring it keeps the door tests honest about the
table, not about a fixture's axis list someone may later extend.

**v33 is a pure step-up.** One new params tag (`facing`), one new
tool-kind word (`turning`), one new serde-defaulted key
(`nose_radius`, 0.0 = the inert every mill already meant) - no v32
file can carry any of them, no existing shape changed, and the
sane door refuses the default for the kind that reads it: a
declared turning tool WITHOUT a nose is a mathematical point, and
`validate_kind` turns that into a named Corrupt before the world
is touched.
