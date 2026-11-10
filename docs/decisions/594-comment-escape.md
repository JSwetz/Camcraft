## #594 - Comment text is escaped per dialect

**Decided.** Every piece of user text a post writes inside a comment goes
through that post's existing `clean()` (#257), the machine's name in the
program header included - the one text that had skipped it. Per dialect:

- **Fanuc, Fanuc lathe, Haas, Mazak, Tormach** (`( )` comments, RS-274 /
  ISO 6983; the Fanuc/Haas operator manuals' control-in/control-out):
  `(`, `)`, `%` (tape end mark) and `;` become spaces, anything outside
  printable ASCII becomes `?`, runs of spaces collapse, 40 characters max.
  `O0001 (Tormach 770MX (ML))` now posts `O0001 (Tormach 770MX ML)`.
- **Tormach / LinuxCNC** also neutralises active comments: a comment whose
  text would open with `MSG`, `DEBUG`, `PRINT`, `LOG`, `LOGOPEN`,
  `LOGAPPEND`, `LOGCLOSE`, `PROBEOPEN`, `PROBECLOSE`, `ABORT`, `AXIS`, `PY`
  or `PYRUN` (any case, as a whole word) is led by `- `, so a machine named
  `MSG, Mill` posts `(- MSG, Mill)` and never pops an operator message or
  opens a log. Source: the LinuxCNC G-code manual, sections Comments,
  Messages, Probe Logging, Logging, Debug/Print messages (and AXIS's
  `(AXIS,...)` hints). Applied in `note()`, i.e. to every comment the post
  writes; only the header can open with user text today.
- **Sinumerik** (`;` to end of line, Siemens 840D programming manual):
  the same `clean()` - the `; <name>` header now carries no second `;` and
  no parentheses.
- **Heidenhain** (TNC DIN/ISO: `;` comment, `*` block end): unchanged -
  its `clean()` already drops `*` too, and the machine name only appears
  as the program name, cut to `A-Z a-z 0-9 . _ -` since #50. Now pinned by
  a test.
- **PRELUDE `writeComment`** (the default `( )` comment the lifecycle
  defaults and the lathe's warning echo use): parentheses and control
  characters become spaces.

An empty name after cleaning falls back to `CAMCRAFT`, as an unnamed
machine already did. No uppercasing: existing goldens carry mixed case
(`TestBench lathe`) and must stay byte-identical; the controls named here
accept lower case in comments.

**Why.** The header wrote `machine.name` raw. Machine names are user text
(#478's Machines editor) and shipped files already hold parentheses
(`Tormach 770MX (ML)`, `Tormach PCNC 1100 (Series I)` and three more), so
PathPilot refused the program for a nested comment. Reusing `clean()`
keeps one rule per dialect for every comment, and every existing golden
(names without those characters) posts byte-identical.

**Considered and dropped.**
- Refusing such names when the machine file is saved: the shipped library
  already holds them, and a name is commentary, not a command - cleaning
  at post time fixes every door (file, editor, mirror) at once.
- Swapping `(`/`)` for `[`/`]`: brackets are expression delimiters on
  Fanuc macro B and LinuxCNC, so a space is the safer stand-in and matches
  what `clean()` already did for operation and tool names.
- One shared helper in the PRELUDE for all posts: the comment-legal set is
  per dialect (Heidenhain also drops `*`, Tormach guards active comments),
  and each post already carries its own `clean()`; the PRELUDE only fixes
  its own `writeComment` default.

**Cost to change later.** Small: one regex per post. Changing the stand-in
character would change every program whose names hold these characters,
none of which are in the goldens.
