## #257 - comments in the posted G-code (2026-11-08)

- **Chosen:** every built-in post writes three kinds of comment, on by
  default, with no new post property or UI in this PR: the setup name
  once where its first section opens (`(SETUP: Setup1)`), the operation
  at each section's preamble with its name, kind and work offset
  (`(OPERATION: Contour1 - 2D CONTOUR - G54)`), and the tool just ahead
  of each tool call with number, library name, kind, diameter and corner
  radius (`(T01 EM-0600-4FL - FLAT END MILL D6 R0.5)`). Only comment lines
  are added; no motion, word or order changes. All 26 regenerated goldens
  were checked mechanically: removing the new comment lines gives back
  the old file byte for byte (Heidenhain: modulo block renumbering).
- **Stream:** four new optional PARAMs from `post::dump` -
  `OperationKind` (the op registry's panel label), `SetupName` (the
  operation's effective setup, as the setup sheet resolves it),
  `ToolKind` (the tool kind's label) and `ToolCornerRadius` (mm, only
  above 0). Each is written only when stated, so a hand-made or legacy
  stream reads as before; the engine passes unknown PARAMs to
  `onParameter`, which every script already ignores for names it does
  not know, so a user's own `.cps` keeps working unchanged. Documented
  in `docs/CNC-INTERMEDIATE.md`. `OperationName`, `ToolDiameter`,
  `WorkOffset` and the TOOL record's name were already in the stream.
- **Per dialect:** Fanuc, Fanuc lathe, Haas, Mazak and Tormach use
  `( )`, written like their existing machine-name line. Sinumerik uses
  `; text` on an unnumbered line, exactly as its machine-name line does
  (the N numbers of its blocks do not move). Heidenhain DIN/ISO uses a
  numbered comment block `N.. ; text*`: the TNC numbers every block and
  ends each at `*`, so a comment is a block of its own, and the program's
  later N numbers shift by one step per comment; the work offset is left
  out of its operation comment because this post writes the active
  preset, never a G54 word. The Fanuc lathe leaves the diameter out for
  blades (turning, grooving, threading inserts, form gauges, boring
  bars): their library diameter is no size an operator measures.
- **Cleaning user text:** names are cut to printable ASCII (anything else
  becomes `?`), whitespace runs collapse, and the text is capped at 40
  characters. `( ) % ;` become spaces in every dialect (a parenthesis
  would close a Fanuc-family comment early - controls do not nest them;
  `%` is the tape start/end mark; `;` opens a comment on several
  controls); Heidenhain also drops `*`, its block end. Every comment
  starts with our own fixed keyword (`SETUP:`, `OPERATION:`, `T..`), so a
  user name can never form a LinuxCNC active comment such as `(MSG,`.
- **Considered and dropped:** a post property to switch the comments off
  (#258 proposes a per-post property list and open draft PR #302 owns
  the Post dialog UI - adding a property now would collide with it; it
  can be added there later with the default left on); the tool's tip
  angle, flute count and material in the tool comment (more noise than
  help on one line; easy to add from the same PARAM pattern); a program
  header listing every tool up front (a later, separate feature - it
  needs the whole tool list before the first section, which the stream
  does not carry today); uppercasing user names (loses information the
  operator typed; only our own keywords and kind labels are uppercased).
  Tormach had two identical `onParameter` definitions (the later one
  wins in JS); both got the same one-line hook rather than a cleanup
  outside this issue's scope.
- **Cost of changing later:** comment text is golden-locked, so any
  wording change regenerates the ~26 affected goldens (the legacy-stream
  conformance goldens carry no comments and are untouched). Turning the
  comments off by property is one `if` in each script's `note()`.
- **Open for the user:** the Heidenhain comment form (`N.. ; text*`) is
  taken from the TNC's numbered-block rule; it has not been checked on a
  real TNC. If a control rejects it, the fallback is an unnumbered
  `; text` line or no comments for that dialect.
