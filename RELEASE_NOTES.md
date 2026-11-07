# CamCraft 0.13.0 (clean-room implementation)

A minor release: CamCraft now ships for Linux and macOS as well as Windows,
and four collision checks are tighter (issues #84, #91, #96, #71).

## New
- **Packages for Linux and macOS** (contributed by linuxguy123): macOS ZIP
  and DMG, AppImage, Snap, Flatpak, and Linux x86_64 TAR.GZ, DEB and RPM,
  next to the Windows ZIP and installer; an Arch Linux source package
  (PKGBUILD) is in `aur/`. The macOS build is not notarized yet - Gatekeeper
  may call a downloaded DMG damaged until its quarantine flag is cleared.

## Fixed
- **Drilling checks the tool can reach** (#84): a hole deeper from the stock
  surface than the drill's flute is refused by name (a bore or tap in an
  existing hole only needs a shank that fits it), and the holder is proved
  clear of the stock on every move.
- **A drill R plane below the stock or part top is refused** (#91): the rapid
  down to R used to cut through material and post silently.
- **A rapid straight down into uncut stock blocks the post** (#96), for every
  operation - unless an earlier operation of the setup already removed that
  stock.
- **Keyed and flatted shafts turn** (#71): the lathe axis is read on the
  turned surface, so a correctly placed keyed shaft is no longer refused; a
  note names the share of the surface (keyways, flats, holes) left for a later
  setup.
- **The AppImage starts** (linuxguy123).

Known limits: with stock declared, a hole on an already-machined pocket floor,
or face-then-drill, may be refused by the drilling checks until they learn what
earlier operations removed (#89); a large keyway is still refused as "not
revolved" (#110).

---

# CamCraft 0.12.1 (clean-room implementation)

A patch release: four fixes, one of them a collision (issues #77, #79, #67,
#74).

## Fixed
- **Cross drilling no longer rapids through uncut stock** (#77). On raw stock
  wider than the finished part, a mill-turn cross drill used to hop between
  stations inside the stock at rapid feed, and nothing caught it. Hops and
  approaches now stand off the raw stock (and a shank wider than the drill),
  hole mouths start at the raw surface, every rapid is proved clear or the job
  is refused by name, and resizing the stock marks the drilling stale.
- **The post check now covers cross-drill rapids** (#79). The rapid-in-stock
  check that blocks a post was switched off for radial programs; it now
  judges them against the raw stock (retracts up a drilled hole stay clean).
- **Cross drilling measures about the machine spindle** (#67). On a bar off
  the world origin under a moved work offset, clearance and hops were measured
  about the wrong axis. A mill-turn bar off its work offset, a hole plunging
  away from the spindle, and on a lathe a part that cannot be read as turned
  are refused by name.
- **GPU startup on dual-GPU laptops** (#74, in progress). Every adapter is
  listed and tried in turn (your `--gpu <name>` choice, then the
  high-performance GPU, discrete before integrated, DX12 before Vulkan on
  Windows); each must draw a test frame, and a failure moves on to the next.
  If none works, a dialog names every adapter tried. Frames over 100 ms are
  logged by phase - please send those log lines if you see lag.

---

# CamCraft 0.12.0 (clean-room implementation)

A minor release, breaking first: Heidenhain NC output and the project file
format change (issues #50, #45, #62, #9, #19).

## Breaking
- **Heidenhain DIN/ISO programs take the TNC's own block form** (#50):
  numbered blocks ending `*`, signed words, the `%NAME G71` frame; drilling
  posts as plain moves with the dwell at the bottom, because G73/G80/G54/G55
  mean something else on a TNC. Tapping and any work offset but the first are
  refused by name on this post until the native cycles land (#51).
- **Project files are v50** (#45 part, contributed by linuxguy123): older
  builds refuse them; v49 files load and migrate.

## New
- **Project and operation workflow** (#45 part, PR #58): Save As and kept
  filenames; rename, reorder, copy and paste operations, with playback in
  their order; browser collapse and viewport visibility controls.
- **Adaptive on the rotary wrap** (#9): each developed level clears on the
  adaptive planner under the load and kink laws; a level it refuses is named
  by its radius and cut by Offset rings.
- **Turning programs in the stock simulation** (#19): facing, profile,
  contour and boring simulate about the part's spindle; blade, thread and form
  tools are refused by name until their sweep lands (#21).

## Fixed
- **A turning part whose axis is not the machine spindle is refused** (#62):
  a part leaning even slightly, or off the work offset, used to post a taper
  or an eccentric with no warning. It is now refused by name at generation
  and again at posting (so moving the work offset afterwards is caught), to
  within 0.0005 mm over the whole part.

---

# CamCraft 0.11.0 (clean-room implementation)

A minor release, breaking first: Heidenhain NC output changes for every XY
arc (issues #49, #17, #18).

## Breaking
- **Heidenhain DIN/ISO arcs post their true centre** (#49). The TNC reads
  I/J as the absolute circle centre, entered in its own block before the
  G02/G03 (TNC 640 ISO programming manual). The post wrote the centre
  relative to the arc start, so a real control would cut around the wrong
  centre. Every Heidenhain XY arc's output changes; an arc without a centre
  is now refused by name.

## Fixed
- **Swarf links half as often and never tilts under level** (#17): about
  half the air links on the blade, the lowest pose level instead of 7-9
  degrees under, and a ball tool's re-entry into the cut it just left is no
  longer refused as a plunge.

## Decided
- **Heidenhain vertical arcs and oblique entries stay chord fans** (#18):
  the manuals give no rotation sense for off-plane arcs, so a native arc
  would be a guess; the fan is exact to the output resolution.

---

# CamCraft 0.10.0 (clean-room implementation)

A minor release: one new capability and two fixes (issues #15, #14, #16).

## New
- **Rotary Parallel and Swarf ramp in on their first entry** (#15). Where a
  straight plunge is refused, the first infeed now comes in on a sloped ramp,
  each one proved clear of the part and the stock before it is kept.

## Fixed
- **A lead-in outside a round bore is seated inside it** (#14): one descent
  per level instead of a plunge into material beside the bore.
- **Rotary Offset is judged against the real round bar** (#16). Its plunge
  check now uses the true stock cylinder about the rotary axis instead of a
  box around it, so moves in the air beside the bar are no longer refused.

---

# CamCraft 0.9.0 (clean-room implementation)

A minor release: one new capability and one measurement fix (issues #11, #10).

## New
- **Adaptive arc output covers the growing spiral** (#11). Where a single
  arc could not follow the spiral's changing curvature, the path is now
  fitted with pairs of tangent arcs (biarcs), each proved inside the area,
  crossing-free and kink-free - about a third fewer moves: rectangle
  18,372 → 12,730, L-pocket 51,788 → 34,582, slot 23,382 → 14,748.

## Fixed
- **The load reader reads chord paths at their true engagement** (#10): it
  faces the path's real direction at each corner, not the lagging chord, so
  readings on straight-segment paths dropped (e.g. the L-pocket from 22.2 %
  to 21.4 % of the tool diameter at the coarse grid).

---

# CamCraft 0.8.4 (clean-room implementation)

A patch release: three fixes (issues #13, #36, #46).

## Fixed
- **2D Adaptive no longer hangs on a long thin band** (#13). A channel too
  long and narrow for the adaptive spiral is refused in about a second with
  its size named, and Offset rings cut it - instead of planning for minutes.
- **Finishing passes ride over mesh folds** (#36). On coarse meshes the
  rows no longer skip a strip at every fold, so the scallop stays within
  tolerance on curved parts.
- **Raster passes reach the rim** (#46). Each pass ends with a row where its
  ground ends, so tilted and flat shelves are finished to the edge.

## Also in this build
- The 2D Adaptive trochoid fallback is wired but still refused by name in
  every case (its landings are #47); output is unchanged.

---

# CamCraft 0.8.3 (clean-room implementation)

A patch release: four fixes, three of them safety (issues #12, #42, #43, #40).

## Fixed
- **A whole-part Pocket or 2D Adaptive no longer cuts into the part** (#12).
  With the whole model as the source, it now clears only the stock around
  the part; if the part fills the stock outline, it refuses and says so.
- **The verifier catches a straight cut across a part's corner** (#42).
  Pocket-type operations are now checked along every move, not only at its
  end - a 0.3 mm corner intrusion reads as a gouge and blocks the post.
- **G41/G42 compensation refuses contours the controller cannot offset**
  (#40): inside fillets tighter than the tool, edges too short for the
  offset, and necks narrower than the tool - each named with its position.

- **Contour moves are checked against the part's walls along their whole
  length** (#43), so a straight cut across a convex corner of the wall is a
  gouge; a Face's rows crossing its open rim stay legal.
- The pocket check from #42 now judges only the stretch of a move below the
  stock top: a ramp crossing the wall line in the air is not a gouge.

---

# CamCraft 0.8.2 (clean-room implementation)

A bug-fix release from hand-testing 0.8.0/0.8.1.

## Fixed
- **Closing an edge loop.** In Edges mode, Shift+click takes the whole loop
  an edge lies on, and Ctrl+click adds or removes one edge. The Selection
  group shows the hint, and the "open run" error says how to close the loop.
- **The Bottom height after picking a face.** Switching Bottom's *From* to
  Selection kept the previous reference's offset (e.g. +18.7), which put the
  floor above the picked face. It now starts at offset 0, so the floor is the
  face.
- **Containment Outside on 2D Adaptive/Pocket.** It no longer machines a
  recess inside the part: only the outer edge is grown, holes stay protected,
  and pockets the band past the outline does not reach are left alone. With
  Outside alone the band is exactly one tool wide, so 2D Adaptive refuses it
  by name ("no room for an entry") - add a Boundary extension.
- **Circular follows the selection.** Pick a bore rim, a round pocket floor,
  or a face around one round hole, and the rings are centred and sized from
  it (shown as "From selection: Ø… at (x, y)"). A selection that is not round
  is refused with where it deviates; with no selection the typed centre and
  radius work as before.

## Known
- 2D Adaptive on a wide Outside band can be slow to plan (minutes); 2D
  Offset plans the same area quickly.

---

# CamCraft 0.8.1 (clean-room implementation)

A packaging release on top of 0.8.0 - the program is unchanged.

- The repository and the release now carry the project's MIT `LICENSE`.
- New issue templates on GitHub: **Toolpath / path planning bug** and
  **UI / UX bug**.
- The repository no longer carries local tooling, agent settings or
  research material.

The 0.8.0 notes below still describe everything in this build. Fixes for
the bugs reported against 0.8.0 (edge-loop picking, face heights and
Outside containment in 2D Adaptive, Circular following the selection) are
in progress for the next release.

---

# CamCraft 0.8.0 (clean-room implementation)

Eighth public release, cut early (2026-11-06) to park a stable build. It
carries what was planned for three trains (0.8.0, 0.9.0 and most of 0.10.0),
every piece through the full gate, heavy lanes included. Everything a
generator cannot prove safe is still refused by name, never posted.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining knowledge.
"HSMWorks" is a trademark of Autodesk; this project is unaffiliated (see
AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

Please start with `TESTING.md`: a short hand-test list for what changed.

## What's new since 0.7.0

### Machining
- **G41/G42 cutter compensation** on Contour: *Compensation: In control* emits
  the part contour with `G41/G42 D<tool>` / `G40` through the Fanuc post. It
  needs a straight lead of at least the tool radius; anything else is refused
  by name, and posts that cannot emit it refuse instead of writing a centre path.
- **Arc output for adaptive clearing**: G2/G3 over the HSM passes, each arc
  proven inside the area, crossing-free and kink-free (a bore went from 6,650
  to 1,200 moves).
- **3D Pocket uses the HSM planner** per level (*Strategy: Adaptive*); a level
  it cannot plan is cut with offset rings and the summary names its Z.
- **Facing goes around holes** (*Holes: Lift*, the default); *Cut across air*
  and *Through* (the old behaviour) are choices.
- **Contours across two models**: picked edges that meet end to end across
  bodies become one ring, and the depth ladder cuts every level of the union.
- **Waterline stepdown reads the slope**, and the finishing march no longer
  drops rows across mesh fold gaps - most scallop readings are now inside the
  tolerance.
- **Boundaries everywhere they apply**: on 3+2-indexed operations (read in the
  tilted plane), on every 3D finish, and on Trace/Engrave; a clipped finish
  ramps back in.
- **Lathe**: axial drilling on the spindle axis, and the first lathe post
  (`Fanuc Lathe`: diameter X, G96/G97/G50, G99, G83/G84/G85, G32). The Setup
  pane offers the matching post when you mount a lathe; it never switches it
  for you. C-axis live tooling is not in this release (refused by name).

### Safety
- **5-axis swarf** no longer drags the tool's flank through the part on its
  in-air links (it retracts along the surface normal) and never falls back to
  an upright pose inside a wall.
- **Fixtures are checked along the whole move**, not just at its ends, for
  every pose change.
- **Scallop is reported** (a non-blocking note) where a finish leaves more
  than its tolerance.

### Simulation and display
- **The machined stock** is drawn in the viewport and can be picked; rotary
  programs now show their stock too.
- **Playback jumps** no longer stall, and the CPU fallback renders off the UI
  thread.

### Models and output
- **Repair mesh** (Model page): weld vertices and fill planar holes, one undo
  step, never automatic.
- **The setup sheet** says whether each fixture is fixed or turns with the part.
- **Windows installer** (preview) beside the zip, and an optional signing step;
  this build is unsigned.

### Known limits
- C-axis live tooling, G41/G42 wear mode, and comp on posts other than Fanuc.
- Lathe operations are not shown in the field stock simulation yet (refused
  by name; the turning verdict at generation still applies).
- The adaptive planner still falls back to offset rings on island pockets.

---

# CamCraft 0.7.0 (clean-room implementation)

Seventh public release: the user-review release. Most of what is new came
straight from hand-testing 0.6.0:
- the operation dialog now reads like a CAM dialog;
- simulation playback no longer locks up;
- lead in / lead out is complete;
- adaptive clearing is a real HSM spiral;
- every boundary says what it does.

Under it, a run of safety work: wherever the verifier had a blind spot, it was
closed. Where a plan could not be made safe, it is refused by name instead of
posted.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining knowledge.
"HSMWorks" is a trademark of Autodesk; this project is unaffiliated (see
AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

Please start with `TESTING.md`: a short hand-test list for what changed.

## What's new since 0.6.0

### From your review
- **The operation dialog, Fusion-style.**
  - Label and value share a row, units sit beside the value, and fields are
    grouped in collapsible sections.
  - Only the fields the current choice uses are shown.
  - "0 = auto" fields became explicit Auto toggles, and help moved to tooltips.
  - A one-line status banner sits at the top; a fixed action bar at the bottom
    has Generate as the primary action and Delete set apart.
  - The tabs stay.
- **The boundary says what it does.**
  - One plain sentence per setting, a "Machine the other side" button, and a
    notice when a setting has no effect on the kind.
  - The viewport fills the area actually swept; open edges are dashed.
  - Contour now rides an open boundary as one open pass.
- **Simulation that keeps up.** Playback runs on a worker thread: a UI frame
  went from 182 ms to 0.13 ms, and the stock follows the cutter. Each picture
  uploads only the changed bricks (2 MB to 57 KB).
- **Source is the stock or a part.** G54, setups, tools and fixtures are no
  longer offered, and are refused if a file names them.
- **Pick a vertex, set the origin there.** A new "Vertices" pick mode; "Set
  origin" on a picked vertex or face (centroid, or Z only on a horizontal
  face). It is undoable.
- **Panels always come back.** There is a permanent viewport, a Panels… menu
  and Reset layout. Operation-page errors clear, and Clear toolpath removes the
  path, not the operation.

### Machining
- **HSM adaptive.**
  - A constant-load spiral with no kinks past 5° and one entry per level.
  - The optimal load is set in % of D.
  - Narrow slots join their pieces with lifted returns (floor air 11 m to
    0.9 m).
  - A plan that can't be made sane falls back to offset rings by name.
- **Lead in / lead out, complete.** Ramp angle and length, a continuous helix,
  and arc radius and sweep, all in one tab.
  - Every entry has extensions, entry feeds and an engagement limit.
  - Heights can be measured from the selection.
  - A plunge height of 2 mm applies.
- **Plane-aware transitions.** Arcs carry their plane: G17/G18/G19 in the CL
  and every post. Arc entries post as true vertical arcs.
- **Keep-down links** and plunge, ramp, helix or arc level transitions on every
  ladder operation; how levels link is your choice.
- **New entries.**
  - A horizontal (G17) arc entry for pockets.
  - Face entries run out to air for tools that are not centre-cutting.
  - 3D ladders slow only the first diameter of cut.
- **Area control.** Inside, on or outside containment, boundary offset and
  extension, extra include/exclude boundaries, and open boundaries that start
  outside the stock.
- **Smoother paths.** Tangent arc blends at corners and links (line-arc and
  arc-arc too), and facing reversals become single arcs.
- **More finishing.** Flowline, steep-and-shallow, flat area, pencil and corner,
  ramp, morphed spiral, geodesic, blend, ruled loft, project drape, deburr, 3D
  offset shells.
- **2D additions.** Engrave, chamfer, circular, standalone bore, helical thread.
- **Rotary and 5-axis.**
  - Posed ball tools post the tip, so they land on plan (they were 4.2 mm off
    on 5-axis).
  - Rotary programs open on the clearance pose, and Rotary Parallel links run
    radially.
- **Machine limits and cutting data.** Spindle speed and power are checked, and
  speeds and feeds are suggested per material.

### Safety (the Prime Directive)
- The verifier judges the tool against the part's real solid, with the tool's
  real tip. A program it can't judge says so.
- Closed blind spots:
  - stock under overhangs;
  - a positive stock-to-leave that hid a gouge of that depth;
  - cuts that only happen later in the program;
  - tilted holders sweeping through a chuck;
  - the spindle on tilted rapids.
- Tools that are not centre-cutting never plunge into solid, anywhere: 2D, 3D,
  drilling, Face and every multiaxis kind. They ramp, helix, or are refused by
  name.
- No level under a blind floor; 2D pockets and contours see what hangs above
  them.

## Projects
- The project format goes from v42 to v49. Older projects load. New fields are
  written only when used, so a re-saved 0.6.0 project only grows where you
  change something.
- The persistence of picks, open boundaries, vertex picks, fixed fixtures and
  the pocket arc entry is round-trip tested.

## Known limits (tracked in ROADMAP.md)
- Adaptive island pockets fall back to offset rings, by name.
- Rotary Parallel's radial infeeds and Swarf's first raster entry always plunge.
  With a tool that is not centre-cutting they are refused rather than ramped.
- The fixed-fixture flag has no UI toggle yet, and the generate-time fixture
  check still uses the stricter older law; the post gate applies the new one.
- Building is Windows-first. Linux and macOS build from the same source (M4.2);
  code signing and an installer are still owed (M13.3).
