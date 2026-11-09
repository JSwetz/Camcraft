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
