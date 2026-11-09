## #383 - Machines pane: create, edit and save `.mch` machine files

**What was decided:** The Machines pane (#287, PR #382) now creates and
edits machine FILES. The app layer gains three modules beside
`library` / `validate`:

- `machine::store` resolves the user machines folder, as decided on #287:
  `CAMCRAFT_MACHINES` (absolute only) wins; otherwise the platform data
  folder plus `camcraft/machines` (Windows `%APPDATA%`, macOS
  `~/Library/Application Support`, other platforms `$XDG_DATA_HOME`,
  falling back to `~/.local/share`). It also writes a file atomically:
  a temp file in the same folder, `sync_all`, then rename. The temp file
  is removed on failure. `save_text` validates first and never writes a
  refused file.
- `machine::fields` is the property grid. It has nine collapsible groups:
  General, Axes and travels, Rotaries and limits, Spindle range, Feeds,
  Tool changer and stations, Coolant codes, Live-tool codes and Units.
  - Each field is bound to one JSON pointer into the file. Its tooltip
    names the key, and it lists the validator keys whose refusals belong
    on it.
  - `set` edits the JSON in place. Missing objects are created, and an
    empty text removes the key (unstated, never a zero). Objects that the
    removal leaves empty are pruned, so clearing all four live-tool words
    leaves no `live_tool: {}` that would itself be refused.
- `machine::draft::MachineDraft` holds the file's whole JSON, plus its
  path, a read-only flag and a dirty flag. It offers `open` (a built-in,
  or a file in a shipped `assets/machines` folder, opens read-only),
  `from_template` (model + " copy", never saved), `blank(kind)` and
  `save` / `save_as`. `save` refuses a shipped or never-saved machine with
  `NeedsSaveAs`. `save_as` refuses a name that is a path, and reports
  `Exists` unless the replace was confirmed.

`library::enumerate` now lists the shipped library, then
`scan_dir(user folder)`, then the document's mounted file unless it is
already listed. `enumerate_with(world, dir)` takes an explicit folder, so
tests never read the tester's real folder.

The UI adds `machine_file_editor.rs`, named apart from the existing Setup
pane `machine_editor.rs`, which edits the mounted machine through the
undoable `EditMachineApply`. The pane gains a toolbar with a kind combo,
New blank, New from template and Edit machine.
- In the editor, each field's refusals are drawn in red under it. A
  refusal that no field carries (a missing required axis, say) is listed
  above the grid.
- "Save machine" and "Save machine as..." are disabled while any refusal
  stands.
- Save asks "Write <path>?" first. Save As asks for a file name
  (suggested from the model) in the user folder, and asks again before
  replacing an existing file. Close with unsaved edits asks before
  discarding them.
- After a save, the catalogue is re-read and the new file is selected.

**Why:**

- *Edit the raw JSON, not the parsed `Machine`.* The reader drops what it
  doesn't know (and some of what it does, such as a backwards travel), so
  writing back from it would lose unknown keys. Pointer edits on the raw
  `serde_json::Value` carry every unknown key through by construction.
- *Key order is not kept.* The workspace `serde_json` does not enable
  `preserve_order`, so a saved file's keys come out sorted. The content,
  unknown keys included, round-trips exactly (pinned by test). Turning on
  `preserve_order` would change every JSON writer in the workspace,
  project files included, which is a wider change than this issue owns.
  Considered and dropped for now. It can follow as its own issue if
  hand-diffing saved files matters.
- *A number field keeps a non-number as text,* so the validator names it
  (for example "min and max must both be numbers"), rather than the editor
  silently eating the keystroke. The pane keeps a per-field buffer so a
  half-typed `1.` is not reformatted under the cursor.
- *A file save is not a command* (#287 rules, restated on the issue).
  Nothing queues a command or lands on the undo history. The confirmation
  prompt is the safety, and assigning a machine to a setup stays #384.
- *Shipped machines are read-only:* a built-in, or any file whose folder
  is `.../assets/machines`. Save on one opens Save As, and the editor
  never writes into the install tree.
- *New blank carries only what its kind requires:* model, kind, units,
  and the axis tree (mill X/Y/Z, lathe X/Z, mill-turn X/Y/Z plus a C
  rotary on +Z). It also gets one head, which carries the spindle and tool
  station, and a Flood M8 coolant, so the coolant group has a row to edit.
  Travels, feeds and speeds are left unstated (the unstated-limit law),
  and the blank validates clean.
- *"Add coolant"* appends a named entry to the first head's station. Its
  code is typed in its new row. There is no remove button: clearing a
  code leaves the coolant with no code, which is legal. Removing an entry
  can be a follow-up.
- *Feeds rows* are written under
  `controller.synced_configuration.parts.<letter>`, creating the block
  when the file has none.

**Cost to change later:** low. The grid is a table of pointers, so a new
field is one `add` line. Turning on `preserve_order` is a one-line
Cargo feature change, but it needs a workspace-wide review.

### M-code words in the machine editor (GUI-test finding on PR #478)

**Chosen:** every M code a machine file states, meaning each coolant's
`mCode` and each of the four `live_tool` words, must be one M word: an
uppercase `M` followed by a whole number from 0 to 9999. Leading zeros
are fine (`M08`, `M0008`). A bare `M`, a letter after it (`MX`), a
decimal (`M8.5`), a sign (`M-1`), two words (`M8 M7`), lowercase (`m8`)
and anything past `M9999` are refused. The refusal names the field and
says why ("no number after M", "above M9999"), and it sits under that
field. A live-tool word's refusal is keyed `live_tool.<word>`, so it
shows only under its own row, not under all four. An empty field stays
legal (unstated). A bare JSON number code (`"mCode": 8`) stays legal
too, because the reader turns it into `M8`. It is held to the same
0..=9999 bound.

**Why:** a post writes the stated word into the program as it stands
(the Fanuc lathe post emits `block([lt.cAxisOn])`). A malformed word
becomes a malformed block that a control rejects or misreads. Before
this fix, the live-tool words were checked only for "all four or none",
and lowercase passed the coolant check.

- *Bound 0..=9999.* The published standards give M two digits.
  RS-274/NGC (NIST IR 6556, Kramer et al. 2000) defines M0 to M99, and
  ISO 6983-1 uses a two-digit M address. Real controls go past that:
  LinuxCNC documents user M codes M100 to M199, and builders use
  three-digit codes (for example M154/M155 C-axis engage on some
  mill-turns). Our own `c_axis_live` fixture uses M1001 to M1004. Fanuc
  sets the allowed number of M digits per control by parameter, so no
  public ceiling fits every control. Four digits is a sanity bound that
  stops runaway text, not a cap any standard sets. It keeps the four
  digits the validator already allowed.
- *Lowercase refused, not normalised.* The editor never rewrites what
  someone types. The same validator also judges hand-written files in
  the catalogue, where a silent rewrite would only happen in the editor
  and `m8` would still reach the post from a file. The refusal says to
  use an uppercase M.
- *One word only.* No shipped machine or test fixture uses a multi-word
  code. The survey covered every `mCode`/`mcode` and live-tool word in
  `assets/machines/*.mch`, `assets/machines/AGENTS.md` and the app/post
  test fixtures. It found only `M7`, `M8` and `M88`, plus the crafted
  `"G8"` refusal fixture and bare numbers `5`/`8`. No shipped file
  states `live_tool`. If a builder needs `M64 P1`-style words later, the
  rule can be widened to accept each word on its own.
- *Seam.* The rule lives in `machine::validate::validate_text`, which
  is used by both the editor (`MachineDraft::refusals`, which blocks
  saving) and the Machines pane catalogue (`library`). A hand-written
  `.mch` with `"M"` therefore shows as refused in the catalogue as well.
  The post crate's reader (`camcraft_post::machine::parse`) stays
  deliberately forgiving and was not changed. Making it refuse belongs
  to a separate seam, in another crate.

**Considered and dropped:** a 0..=99 bound (it breaks three-digit
builder codes and our own fixture). Normalising `m8` to `M8` on save
(it makes the editor and the catalogue disagree about the same file).
