## #670 - a plasma profile operation

Sources (clean room, behaviour only): Hypertherm's published cut-quality
guidance (cut direction), the LinuxCNC Plasma Primer §13 (small holes),
Tormach's *1300PL Operator's Manual* UM10720 v1223A and *PathPilot Post
Processor Guidelines* (AutoFS, G41/G42 entry, kerf ≈ 1.5x orifice) - as
cited in the #621 research comment. No commercial CAM output was read.

**1. The torch is a tool kind; its diameter is the kerf.** `ToolKind::Torch`
(#669 decision 3): `diameter` = kerf width (the editor labels it "Kerf
width"), `corner_r` must be 0, `rpm` must be 0 - `Tool::is_sane` relaxes
rpm > 0 for the torch only, and a torch stating an rpm is refused by name
(`Tool::torch_refusal`) at `AddTool`, `ChangeTool` and the tool-library
reader. The op may override the kerf (`PlasmaOp::kerf`, blank = the
torch's) - one torch runs many materials. A starter `PT-KERF-150` (1.5 mm)
ships in the catalog. *Dropped:* a separate kerf field on the tool (the
diameter already feeds every offset and the G41 D register).

**2. A new family and section, appended.** `Family::Plasma` /
`Section::Plasma` / `OpType::PlasmaProfile` (stored id 39, appended - no
kind id moves). `Family::admits` pairs Plasma with `Kind::Plasma` only; the
mill-turn admits Mill and Turn, no longer `_` (a mill-turn strikes no arc).
The family door's wording names both ways ("plasma cutting work ... mount
a plasma table"; "a plasma table cuts only plasma operations (a Plasma
Profile)"). The machine check's `cuts_plasma` answers true for the family.

**3. The op is its own generator, like the probe.** `run_job_arms` returns
early into `operation::plasma_op` -> `motion::plasma`. The milling verify
laws (tangency, reach, depth, the stock sim) have no torch to judge, so the
row's mode is `Engrave` and the stock sim / rest skip torch paths (named in
the comments); the generator certifies its own **kerf law**: every lit move
of a closed contour, chord-sampled every 0.25 mm, keeps the kerf disc off
the part's through-silhouette (part offset by kerf/2 − 0.025 mm) or the op
is refused by name. The plasma verdicts proper (cut order, kerf crossings,
tip-ups) are #672's, from the CL records below. *Why not the mill verify
with a fake cutter:* its laws assume a spindle column and a stock model a
through-cut does not have; a pass from them would be a green that never
looked.

**4. Geometry: the picks the contour's way, else the through-silhouette.**
A selection source resolves through `selection_geometry_impl` like the 2D
Contour (closed rings AND open runs). An entity / stock source cuts every
ring of the model's XY silhouette (`slice::project`) - a through-cut can
only make through features, so a blind pocket is correctly absent. Each
closed ring's role is read from the part: material just inside it -> an
outer edge, air -> a hole (so a hole rim picked alone is still a hole);
with no part (empty silhouette) nesting parity decides, said in the notes.
Parents are the smallest enclosing closed ring. Open runs are cut on their
centre line (no side, no leads), noted and exempt from the kerf law.

**5. Kerf in the computer by default; G41/G42 as the option.** Computer:
the moves ARE the offset ring (outline grown by kerf/2, hole shrunk by
kerf/2, clipper2 round joins). Control: the moves stay the same centre
truth and a `CompSpan` per contour carries the PART contour with straight
engage / cancel leads; the door refuses a lead shorter than the kerf
radius by name (PathPilot: the entry must be longer than the tool radius -
ours is lead + kerf/2 ≥ 2 x radius). Small holes under control comp ride
the computer's offset (noted): the hole rule has no lead-out to carry G40.
*Dropped:* arc leads under G41/G42 (most controls refuse comp start / stop
on an arc).

**6. Direction: outer CW, holes CCW; `reversed` flips both and is stated**
in the path notes and the CL header (`REVERSED`). Open runs keep their
picked direction.

**7. Leads.** Arc lead-in / lead-out on the waste side, tangent to the
contour at the midpoint of the offset ring's longest edge (never a corner);
the parameter is the arc LENGTH of a quarter arc (radius 2L/π). A lead with
no room (it would bring the kerf onto the part - a narrow hole, an island)
is halved up to four times, the shortening noted; past that the contour is
refused by name. Default 5 mm in, 3 mm out.

**8. Small holes (Primer §13).** A hole is small when its area-equivalent
diameter is under `small_hole_dia` (default 32 mm) or under 1.5x the plate
thickness (top − bottom). It pierces at the hole's centroid, leads in
straight to the nearest point of the kerf ring (perpendicular to it, ≤ the
radius), takes no lead-out, runs on `overburn` mm (default 0 - optional)
and feeds at `small_hole_feed` (default 65 %, the band's middle). A
centroid outside its own cut (a C-shaped hole) is refused by name.

**9. Cut order: post-order of the nesting tree** - every contour after
everything inside it (island, then its hole, then the outline); siblings by
lowest-then-leftmost point, deterministic. Contour ids in the CL are the
cut order.

**10. Z and the controller's numbers.** The lit moves ride the plate top
(+ the cut height when stated); rapids at the clearance plane; the descent
to the cut plane is a rapid before `TORCH ON` (the controller's G15/G16 own
it). Feed, pierce height, cut height and delay are `Option`s - blank is the
controller's AutoFS; a blank feed leaves the lit moves without `F` and the
header says `AUTOFEED`.

**11. The CL records for #671 / #672** (docs/CNC-INTERMEDIATE.md, "Plasma
records"): `Toolpath::plasma: Option<PlasmaCut>` -
`; PLASMA KERF<µm> [CONTROL] [PIERCE<µm>] [CUTH<µm>] [DELAY<ms>] [AUTOFEED] [REVERSED]`,
`; TORCH ON C<id> OUTER|INNER|OPEN [PARENT<id>] [SMALL<f×1000>]` before the
first lit move, `; TORCH OFF C<id>` after the last. Comment-prefixed, so
the legacy reader and the post's stream parser skip them; `.cl` v5. They
round-trip the text dump and the project's path cache (`plasma` key,
additive). *For #671:* the `.cnc` dump does not yet emit them - the post
owns that, and posting a plasma table stays refused until it ships. A
cached path whose plasma record fails its sanity check on load keeps none;
the plasma post must refuse a Plasma Profile path that carries no record.

**12. Owed / not judged here:** material left by a slot or inlet narrower
than the kerf on an OUTER contour that the offset bridges without
enclosing (an enclosed gap or a pinched hole IS refused) - #672's
material-left law; kerf crossings between contours and tip-up risk - #672;
pierce-point placement relative to previous cuts - #672.
