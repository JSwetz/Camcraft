# CamCraft 0.16.0 (clean-room implementation)

BREAKING: the project file format moved from v57 to v58 (PR #381: a tool can
carry a modelled holder body). Files saved by 0.15.0 or earlier open here and
are migrated when saved, but a v58 file saved by 0.16.0 is refused by older
builds (save a copy first if you still need 0.15.0). Two post behaviours also
tighten and can refuse a program that used to post: the Tormach posts now
retract to the machine file's `safe_retract` and refuse a non-Tormach machine
that states none, the Siemens 840D post refuses any machine that states none
(#495, #503), and a probing post script must declare
`onProbeGuard` or the Probe op is refused by name (#174). The built-in VMC,
tilt-head, Lathe and Mill-turn machines now state their strokes in program
coordinates (#457, #493), so programs they used to refuse now post.

A minor release headlined by fixtures judged against their real mesh, tools
that carry modelled holder bodies, one NC file per mount for multi-setup jobs,
a Machines pane that edits machine files, and a much larger MCP surface. It
also lands a long run of sim, gouge-gate and post fixes. Thanks to mkinney,
seelesturm, austin62jake and tbrasselle for their contributions.

## New
- **One NC file per mount** (#388): setups on different machines or posts
  generate on their own machine, and the post writes one NC file per
  machine/post, each judged against its own machine; the program sim refuses
  by name to carry stock between setups on different machines (#404).
  Pause, optional stop and go-home entries can sit between operations (#155).
- **Fixtures judged against their real mesh** (#339): the box stays the broad
  phase, so a path between vise jaws no longer refuses the post; tilted,
  rotary, spindle-C and XZ/YZ-arc moves are judged too, each arc chord padded
  by its sagitta (#491), and spindle-C moves are judged at the station pose
  (#486). An upright rapid checks the whole tool stack and a ramp each band
  over its true height span (#424, #425). The fixture UX (PR #549, replacing
  #482, mkinney): mark or import a fixture from the browser, see whether each
  is judged by mesh or box and why, and see which fixture a warned move
  strikes. The Fixtures page rows wrap and Place has labelled fields (#509).
- **Tools with holder bodies**: a tool carries a modelled holder (STEP/STL/OBJ)
  placed by axis and tip, judged by its revolved envelope (#368); import,
  place and draw it in the tool editor and sim (#371); draw a milling holder
  as a revolved outline (#93). Lathe tools carry a holder or boring-bar body
  judged against the revolved stock (#369) with an optional insert zone
  (#396). Export and import a `.camtools` library (#300, PR #487 replacing
  #466).
- **MCP server**: pick faces, edge chains and loops for an operation (#180);
  `detect_holes` (#312, PR #456 replacing #314); `render_stock` PNG with rest
  and gouge volume (#182); setups, work offsets, machines and fixtures (#181);
  tool-library export/import (#355, PR #490 replacing #467); stdio hardening
  against null ids, oversize and non-UTF-8 lines (#443).
- **Machines pane** (View > Machines): browse and validate machine files
  (#287), and create, edit and save `.mch` files with per-field validation
  and atomic writes (#383). Machine files for the Tormach PCNC 770 and 1100
  (#170).
- **Probing**: seed a Probe op from a picked face, edge, vertex or round wall
  (#171); positioning moves post as protected stop-on-contact moves (#174);
  Fanuc probing on an inverted X/Y axis (#392).
- **UI**: orthographic (parallel) projection with a Projection row and bare P
  (#434, tbrasselle); warned toolpath moves are marked in the viewport and
  framed from the op page (#261, PR #454 replacing #349); a refused post's
  reason stays in a window (#265, PR #451 replacing #296); the Post dialog
  opens where the document last posted (#258, austin62jake); the G-code
  viewer lists from the left edge (#256, austin62jake); the Stock page and
  Theme button split (#441, #442); macOS Cmd+Q asks about unsaved changes
  (#144); the browser tree reads on the light theme (#506).
- **Licence**: CamCraft is now Apache-2.0 (was MIT), copyright held by its
  contributors (#452).

## Fixed
- **Post gate false gouges and misses**: XY arcs are cut on the true circle,
  not inscribed chords, in the column engine (#402), the field engine and the
  envelope scan (#413); a column within tolerance of a wall is judged against
  the floor beside it (#299); a typed Slot's round ends no longer read as a
  gouge (#510); a 2D Chamfer or Deburr is judged against its declared edge
  break (#303); the scallop check reads to the ball diameter (#212).
- **Crash**: an intermittent SIGSEGV/SIGABRT when threads created their first
  world at once is gone (#379).
- **Alignment** picks refuse by name on an inside-out, open or non-manifold
  body instead of silently flipping the part (#366).
- **Turning**: multi-pass, chamfer and taper turning exit a longer bar safely
  (#323, seelesturm); the rotary wrap turns its seam clear of a D-flat or
  keyway (#226); rotary Adaptive no longer sorts its leave-limit level by
  float noise (#55); swarf raster lines keep their lying angle on rapids
  (#48).
- **Posts**: the Fanuc probe result no longer writes a doubled sign (#377); a
  trailing Go home is written once (#504); the Tormach and Siemens 840D
  retract from the machine file (#495, #503).

## Known issues
- **#554**: face picking on the setup page keeps snapping to an edge.
- **#330**: Align by picking a face or two edges moves the part but does not
  fully rotate it onto the chosen G17/G18/G19 plane.
- **#558**: another setup's WCS can be drawn in the wrong place when the part
  is aligned differently.
- **#523**: posts start the spindle (S0 M3) on a probe tool.
- **#74**: GPU selection can fail on hybrid Intel/NVIDIA laptops.

---
# CamCraft 0.15.0 (clean-room implementation)

BREAKING: the project file format moved from v50 to v57, and two of those
steps change what a project means. Files saved by 0.14.0 or earlier are
migrated when opened, but a file saved by 0.15.0 is refused by older builds
(save a copy first if you still need 0.14.0).
- **One WCS per setup** (#342): a setup now owns a single work coordinate
  system, and G54-G59 is only the label the post writes for it. Placed slots
  that no setup posted are dropped on load, each with a load note naming the
  entity and where it sat; single-WCS projects post byte for byte as before.
  The `SetWcsOrigin` command lost its slot argument.
- **The setup defines the work offset, not each height** (#329): a height
  measured from the work offset reads its setup's offset, and the Heights
  page no longer offers a per-height WCS choice. A project from 0.14.0 or
  earlier whose operations used different G54-G59 slots inside one setup is
  refused at load by name (open it in 0.14.0, make the slots agree, save,
  then open it here).

A minor release headlined by multi-setup machining: a setup tree in the
browser, setups placed relative to one another, the stock carried from one
setup into the next (including rest machining, verification and fixtures),
plus inverted machine axes. Thanks to austin62jake, bherbruck, linuxguy123,
mkinney and seelesturm for their contributions.

## New
- **Setups as a tree** (#343): the browser shows each setup as a node with an
  active-setup radio and its own WCS, Stock, Machine, Post, Fixtures, Tool
  list and Operations; a setup can have its own machine and post.
- **Part alignment per setup** (#326, #330): hold the part flipped or turned
  (presets or typed angles) with the stock following square to the setup
  frame, or lay it on a G17/G18/G19 plane by picking a flat face or two
  straight edges. Face, 2D Contour, 2D Pocket and 2D Adaptive generate and
  post in an aligned setup; every other kind is refused by name.
- **Place a setup relative to another** (#316): same way up, flipped, turned
  or by angles; changing the previous setup's alignment carries into the
  linked setups, and the transform between two setups is derived from their
  alignments.
- **Stock carried between setups**: the program sim carries the machined body
  through the setup transform (#317); rest machining in a later setup reads
  it, and an upstream change marks the later setup Stale (#318); rapids,
  entries and links are judged against it (#319); a setup after the first can
  take its own stock or what the setup before leaves behind, and the setup
  page shows the incoming stock (#320); fixtures read the carried stock, a
  lathe or mill-turn part flip is refused by name, and the carried body is
  cached (#321).
- **Pick the work offset a setup posts at** (#196) on the Setups page; the
  setup sheet names it and conflicting picks are refused by name.
- **Inverted machine axes**: the machine file states a per-axis direction
  (#373); the Fanuc and Haas posts write the program mirrored (#374: signs,
  I/J/K, G2/G3, G41/G42, work-offset origin; an inverted Z, helical arcs,
  threading, rotary, live tooling, probing and every other post are refused by
  name); the axis is shown on the viewport triad, the setup sheet and the NC
  header (#375).
- **Pause or home after an operation, suppress an operation, "Skip pauses
  and homes" post option and one NC file per operation** (#110, #111).
- **G41/G42 cutter compensation on Haas, Mazak and Siemens 840D** (#39).
- **Continuous C-axis live tooling** (#33): rotary kinds on a mill-turn's
  spindle state the C angle per move and post through the Fanuc lathe post.
- **Comments in posted programs** naming the setup, each operation and each
  tool, in every dialect's own syntax (#257, mkinney).
- **Machine files for the other Tormach mills** (#170, mkinney): 770M, 1100MX,
  1500MX and PCNC 440; a .mch can state spindle belt ranges and rigid-tap
  ability.
- **Lathe parts with a large keyway or flat** turn instead of being refused
  (#113, mkinney); grooving, parting and form tools are cut with their true
  blade profile in the lathe stock sim (#21).
- **Probe**: the stylus stem and body are judged against the part (#172,
  mkinney).
- **Stock view and sim**: dual contouring for the machined-stock mesh keeps a
  pocket's sharp edges (#23); the mesh paints deviation classes and lights the
  hovered facet (#25); the hidden-rest scan answers past its old budget (#24).
- **UI**: the Stock page shows stock only and the theme and colour-vision
  picks have their own Theme ribbon button (#441); build version, time and
  commit show in the toolbar and a Debug info window with a Copy button opens
  from the status strip (#352, linuxguy123; #157, bherbruck); spindle speed is
  editable as rpm or surface speed (#225, bherbruck); a refusal on the status
  strip is drawn as an alert (#192, bherbruck); ribbon section and machine
  doors wear glyphs (#207, bherbruck); a new document can reach the Setups page
  to create its first setup (#322).
- **Logging**: the launching terminal logs save, open, post, generation and
  commands (#163, bherbruck; CAMCRAFT_LOG / RUST_LOG).
- **Operation tops below the remaining stock are explained** during upright
  2D/3D generation (#104, seelesturm).
- **Packaging and tooling**: Arch and FreeBSD package targets (#72,
  linuxguy123); the Windows installer's embedded uninstaller is signed when a
  certificate is configured (#29); headless dev binaries per crate (#32); the
  kernel seam gains union/intersection and edge blends, benched only (#27).

## Fixed
- **Cutter compensation** is engaged in G17, never in a stale G18/G19, on the
  Fanuc, Haas, Mazak and Siemens 840D posts (#391).
- **Work offsets**: a rotated work offset (3-2-1 alignment) is refused by name
  instead of posting unrotated (#315); a non-finite or zero-length rotation
  or a non-finite origin is refused by name (#327, mkinney).
- **Turning**: a pass that ends inside a longer bar leaves its cut at feed
  (#229, austin62jake); a plain two-ring bar fits its true axis and wraps
  (#228, austin62jake).
- **Deburr** leaves each edge run straight up before travelling (#304,
  austin62jake).
- **STEP import** reads the file's length unit (#306, austin62jake).
- **CL text dump** keeps a C-axis path's spindle angle (#295, austin62jake).
- **Facing** tells concave returns from narrow gaps between separate holes
  (#260, seelesturm); cross-drill topology is stable under roundoff (#151,
  seelesturm).
- A fresh depth operation bottoms out at the part's floor, not the G54 plane
  (#214, mkinney).
- A loaded stock keeps its geometry revision, so a project resaves
  byte-identically (#340, mkinney).
- A ball nose's corner radius is its ball, shown and saved as half the
  diameter (#263, austin62jake); Fit to Models' cylinder stock is snug (#230,
  bherbruck).

## Known issues
- **#424 and #425 (P0, fixture law)**: an upright rapid checks only the flute
  against fixtures, so the shank, holder and spindle housing can hit a clamp
  unreported; and a ramp is judged at its lowest z, so the top of the tool
  stack at the high end can hit a clamp unreported. Both predate 0.15.0 and
  are being worked. Until they land, check clamp clearance for holder and
  spindle by hand.

---
# CamCraft 0.14.0 (clean-room implementation)

A minor release: a Probe operation, per-operation coolant, G76 threading,
wear-mode cutter compensation, a Tormach post, 3-2-1 work-offset alignment,
an MCP server for AI agents, and a long run of rapid, drilling and lathe
safety fixes. Thanks to bherbruck, mkinney and seelesturm for their
contributions.

## New
- **Probe operation** (#108): face, edge, outside-corner and bore/boss-centre
  finding with a touch probe. The Fanuc post writes G31 skip touches with a
  no-contact alarm, can set G54-G59 or stop for the operator, and stops when a
  result is out of tolerance. Posts that cannot probe refuse it by name.
- **Coolant per operation** (#112): Off, Flood (default), Mist or Through
  tool. Every post writes the machine's own code, with M9 before each tool
  change and at program end. A coolant the machine has no code for blocks the
  post by name.
- **G76 threading** (#34): the Fanuc Lathe post can write a thread as a
  two-block G76 cycle (post option "Threading", default G32 passes).
- **Wear-mode cutter compensation** (#38): the contour's tool-centre path
  posts under G41/G42 Dn / G40, with the register holding only the wear delta
  (Fanuc; other posts refuse by name).
- **Tormach PathPilot post and 770MX machine** (#131, mkinney). A machine
  file in inches now posts inch (G20) programs through posts that read the
  machine's units.
- **Controller tool numbers** (#221, mkinney): a tool can carry its own
  T/H number; two tools posting one number are refused.
- **Align a work offset 3-2-1** (#187, bherbruck) from a picked face, edge
  and vertex or bore.
- **Box stock per-side allowance** (#198, bherbruck), and the Stock page
  shows the models' size and extents (bherbruck).
- **`camcraft-cli mcp`** (#128): a stdio MCP server that lets an AI agent
  open or import a model, set stock and tools, edit operations, generate,
  verify and post - every change undoable.
- **Unsaved changes** (#107): the title shows `*`, and closing, Ctrl+Q, New
  or Open over unsaved work asks Save / Don't Save / Cancel.
- **3D finish scallop ceiling** (#37): a stepover at or past the ball
  diameter, or a tolerance at or past half the ball radius, is refused by
  name.
- **UI**: Create cells darken without stock and open the Stock page (#216);
  a chunkier, readable origin triad (#195); pinning ribbon cells is undoable
  (#143, mkinney).
- **Stock view**: the rotary simulation states (and can enforce) its
  flute-rotation bound (#20); shading reads a true distance gradient (#22).

## Fixed
- **Rapids are judged more strictly**: against the tool's whole footprint
  (#106), along the whole length of sideways rapids (#140), and for turning
  in the meridian plane against the revolved bar (#103). Any rapid into
  uncut stock blocks the post.
- **Ops go Stale when what they read changes**: a stock edit (#156) or an
  earlier op of the setup changing (#109) marks dependent ops Stale, and the
  post refuses them until they are regenerated; one Regenerate All brings a
  job current. Saved projects may show some ops Stale once after upgrading.
- **Drilling** reads the stock earlier operations left (#89). A cross drill
  is judged against the finished part (#117) and enters box stock at the face
  (#85). It no longer gets the axial-hole check on a lathe (#80), reads its
  holder per station (#119) and unwelded STL rounds at their true surface
  (#121), and hops clear between off-centre holes (bherbruck, #81).
- **Lathe**: turning ops approach from, and rough down from, a bar wider
  than the part (#130). A boring bar's body is checked on its cutting feeds
  (#146). Facing faces the stock allowance above a finished part (#122,
  bherbruck).
- **Axis fit**: it no longer leans on one cross hole (#116), reads short bars
  from their facing flats (#152), and refuses normals that only meet at one
  point (#208). Fine-meshed balls (#237) and D-flat bars (#246) read
  correctly again (bherbruck). The rotary wrap names a seam feature or skin
  hole instead of "the mesh is open" (#54).
- New operations post after their setup's existing ones, not in name order
  (#167, bherbruck).
- Project files round-trip every float exactly (#169).
- The Heidenhain DIN/ISO post declares the TNC's `.I` extension (#52,
  mkinney).
- Idle frames no longer redraw, resizing stays responsive, and rotated face
  and edge picks no longer freeze (#164).

## Changed
- With a single part (or its named source model), moving or rotating the
  part now carries a non-model stock along in the same undo step (#164).

Known limits: the probe op is not yet created from a picked face, edge or
cylinder (#171); on CPU-rendered stock previews a paused scrub may stay stale
until the mouse moves.

---

# CamCraft 0.13.0 (clean-room implementation)

A minor release: CamCraft now ships for Linux and macOS as well as Windows,
three collision checks are tighter (issues #84, #91, #96), and a keyed shaft
is no longer refused on the lathe (issue #71).

## New
- **Packages for Linux and macOS** (contributed by linuxguy123): macOS ZIP
  and DMG, AppImage, Snap, Flatpak, and Linux x86_64 TAR.GZ, DEB and RPM,
  next to the Windows ZIP and installer; an Arch Linux source package
  (PKGBUILD) is in `aur/`. This is the first release these packages are
  built for on GitHub - the macOS, Snap and Flatpak builds have not run on a
  real runner before, so report anything that does not install or start. The
  macOS build is not notarized yet - Gatekeeper may call a downloaded DMG
  damaged until its quarantine flag is cleared.

## Fixed
- **Drilling checks the tool can reach** (#84): a hole deeper from the stock
  surface than the drill's flute is refused by name (a bore or tap in an
  existing hole only needs a shank that fits it), and the holder is proved
  clear of the stock (including the cross drill's swing over the end face).
- **A drill R plane below the stock or part top is refused** (#91): the rapid
  down to R used to cut through material and post silently.
- **A rapid straight down into uncut stock blocks the post** (#96), for every
  milling and drilling operation - unless an earlier operation of the setup
  already removed that stock. Turning rapids along the spindle axis keep their
  old reading for now (#103).
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
