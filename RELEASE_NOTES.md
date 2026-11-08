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
