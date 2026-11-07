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
