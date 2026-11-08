## #39 - G41/G42 cutter compensation on the other mill dialects (2026-11-08)

- **Chosen:** Haas, Mazak (EIA/ISO) and Siemens 840D write their own comp
  words for both COMP variants (control = the part contour, wear = the
  centre path), with the Fanuc post's laws carried over unchanged: the
  pending G41/G42 (or G40) rides the NEXT straight feed block; a comp word
  that would land on a rapid, an arc, or survive to the section end is a
  named, counted refusal that blocks the gate; a comp record inside an open
  canned-cycle group is refused.
  - **Haas:** `G1 G41|G42 X.. Y.. Z.. D<tool> F..`, `G1 G40 ...` - the D
    word is the tool's offset row (the H word's row; the library states no
    register; the Haas mill manual: G41/G42 "must have a Dnnn value",
    the offset being that row's geometry plus wear), one-digit words as the
    dialect already posts. The control's cutter-comp style (Setting 58,
    Fanuc or Yasnac) changes only how outside corners are joined - Yasnac
    with an arc (the verified centre path), Fanuc with a sharp or extended
    corner, both on the air side - and the manual states the same entry and
    exit for both, so the post writes no setting note. Dropped: a
    `SETTING 58 MUST BE FANUC` comment (first draft; the manual does not
    tie engage/cancel to the setting).
  - **Mazak EIA/ISO:** the Fanuc-family words as the dialect posts them,
    `G01 G41|G42 ... D<tool>`, `G01 G40`.
  - **Siemens 840D:** `G1 G41|G42 NORM ...` with no D word. The radius is the
    active cutting edge's, and the tool call already selects it (`T n D n`,
    the post's existing house form). NORM (the 840D Fundamentals manual,
    "Contour approach and retraction": straight to the contour, the tool
    perpendicular to the path tangent at the starting point) is the same
    start-up as the Fanuc engage the generator's straight lead of at least
    r is built for; it is the reset default, but the engage block states it
    so a modal KONT left by hand code can never change the engage point.
  - **Wear** (#38) on all three writes one operator comment per section
    naming the register (`(WEAR COMP - D1 HOLDS THE WEAR DELTA ONLY, NOT THE
    TOOL RADIUS)`, `; WEAR COMP - D1 ...` on 840D), exactly as Fanuc does.
- **Still refused, by name: Heidenhain** (the DIN/ISO post, which is the
  TNC dialect this repo ships - not the conversational RL/RR/R0 form).
  Two reasons. (1) The engage point: the TNC manuals state that the block
  activating G41/G42 ends with the tool perpendicular to the programmed
  point, compensation taking effect only at its end; they do not state it
  against the NEXT element the way the Fanuc start-up is. For a collinear
  linear lead both readings agree, but for the perpendicular lead the
  generator also allows they give different engage points, and the post
  sees one record at a time - writing it would be a guess. (2) Wear: the
  TNC always offsets by the tool table's R plus DR (table and TOOL CALL),
  so "the register holds only the wear delta" has no TNC form short of a
  zero R in the tool table, which breaks the tool's other uses. The
  refusal message now names the posts that do write the words.
- **Tormach** keeps its refusal: not in #39's scope (PathPilot/LinuxCNC
  comp has its own rules, e.g. the D word and the entry move), no source
  read for it here.
- **Considered and dropped:** a D word on the 840D engage block (D is the
  edge number of the active tool, already selected - writing it again
  would re-select an edge, not name a register); the native Heidenhain
  conversational post with RL/RR/R0 (no such post ships; a KLAR-style post
  is its own candidate); buffering the Heidenhain engage block to accept
  it only when collinear with the next element (possible later, but a
  partial support that refuses a lead the generator allows is a worse
  user story than one clean refusal; revisit with the TNC's APPR/DEP
  functions, which state the engage geometry explicitly).
- **Cost to change later:** the goldens under
  `crates/app/tests/golden/nc/{haas,mazak,sinumerik}/contour_{control,wear}_comp.nc`
  lock the words; a shop-configurable D register would change the D word
  in one place per script.
