## #287 - Machines pane, first slice: browse and validate

**What was decided:** Ship the read-only half of the Machines pane first
(Refs #287, not Fixes). The app layer gains `machine::library` (the
catalogue: `builtins()`, `scan_dir(dir)`, `enumerate(world)`, one
`MachineEntry` per machine with name, source, kind, linear axes, rotaries,
units, serving built-in posts, a grouped property sheet and its
refusals) and `machine::validate` (`validate_text(text) -> Vec<Refusal>`,
each refusal a key and a rule, e.g. `X travel: min 10 must be below max
5`). The ui gains a dockable "Machines" tab, opened and closed from the
ribbon's View door like Browser and Properties, which lists the
catalogue, shows the selected machine's validation (pass, or every
refusal by name) and its properties in collapsible groups. No create,
edit, save, assign or file write in this slice.

**Why, choice by choice:**

- *Where user machines are found.* The app has no user machine folder or
  search path today; the only user files it reads are the one the
  document mounts through the Setup pane's "Load other" (`machine::of`'s
  path). So the catalogue is the shipped library plus that file, read
  fresh from disk. `scan_dir` (every `.mch` in a folder, unreadable files
  listed with their refusal) is built and tested as the seam a later
  slice points at a real user folder. Considered and dropped: inventing a
  per-user folder now (`~/.camcraft/machines`, the project's folder) - a
  new storage convention is a product decision with platform-path
  questions, and nothing could write to it yet. **Owner decision wanted
  (recommended option first):** a user machine folder resolved like
  theme files are (`crates/ui/src/theme_dir.rs`): a `CAMCRAFT_MACHINES`
  override, else a writable per-user folder under the platform's
  app-data directory (`<data dir>/camcraft/machines`) - scanned by the
  pane and written by the editor's Save As. Theme files resolve only
  read-only install folders (beside the exe, the app bundle), which an
  editor cannot write to, so the per-user folder is new. Alternative: a
  folder setting stored in the project file.
- *`3axis-vmc.mch` is not listed.* It is `DEFAULT_MCH`, the post's
  static-grammar mirror form (`kinematics.static`), not a checker-grammar
  machine file, and it is not in `posting::MACHINES`. The pane lists the
  library the Setup pane's picker lists. A test asserts every other
  `.mch` in `assets/machines` is registered and validates clean.
- *Compatible posts* are derived, not stored: the built-in posts for
  which `posting::post_serves(name, kind)` holds - the same listing law
  the Setup pane's post picker uses. Cheap and already authoritative.
- *Validation reads the raw JSON, not the parsed `Machine`.* The reader
  is forgiving by design (a backwards travel is dropped, a zero rate
  reads "not stated") - right for the checker, wrong for a file a person
  is about to trust. The rules: travel min < max with both numbers;
  feeds/rapids (`max_normal_speed`, `max_rapid_speed`), spindle
  `min_speed`/`max_speed`/`power`, spindle ranges and axis `resolution`
  must be numbers above 0 when stated (a stated 0 refuses - "leave the
  key out when unstated"; null and absent stay legal, the unstated-limit
  law); spindle min <= max; coolant entries named, codes an `M` word
  (`M8`) or a bare whole number (the reader's two spellings); `live_tool`
  all four words or none; `tooling.number_of_tools` a whole number above
  0; `units.length` mm/inch/inches, `units.angle` degrees; a rotary's
  direction three finite numbers, not zero; no letter stated twice;
  `general.kind` a known word. Required keys: `general.model`,
  `kinematics.default.parts`, and per kind the linear axes it moves
  (mill X/Y/Z, lathe X/Z, mill-turn X/Y/Z plus a rotary). Malformed JSON
  or a non-object is one refusal keyed `file`. Considered and dropped:
  refusing zero tool-station dimensions (`body_diameter: 0.0` is how
  shipped lathe files say "no envelope"), requiring a spindle or a tool
  count (unstated is legal; the posts' guards keep their silence), and
  refusing a declared kind that disagrees with the inferred one (a lathe
  with an extra axis is not wrong; the library test already pins
  declared == inferred for shipped files).
- *The sheet shows the file's own numbers in the file's own unit* (an
  inch machine reads `-14 .. 14 in`), from the raw JSON, not the reader's
  mm restatement - what a person typed is what they recognise and what
  the editor will write back.
- *Pane placement and state.* Opening follows the existing panel
  convention exactly: a `Panel::Machines` row in the View door, a
  `LayoutRequest::Toggle`, a `Tab::Machines` in the dock - view state,
  never a command, never saved. It opens as a tab in the Properties
  pane's leaf (or alone at the right if Properties is closed); it is
  not in the default dock, so Reset layout closes it. Selection and the
  cached catalogue live in `State::machines_pane`, ephemeral like the
  dock; the catalogue is re-read when the document mounts a different
  machine and on the pane's Refresh button. No icon: the View door rows
  and dock tab titles carry none.
- *Tests are structural*, no golden PNG: the app suite
  `crates/app/tests/machine_library.rs` and the ui frame tests
  `crates/ui/src/egui_app/tests/machines_pane.rs`.

**Left for follow-up slices:** create (from a library template or
blank), edit as a property grid with save that preserves unknown keys,
validation blocking save, the user machine folder (owner decision
above), "Use for this setup" through an undoable command (the existing
`LoadMachine` is the natural carrier), and deciding whether the
existing machine editor window (Setup pane) folds into the pane.

**Cost to change later:** low. The catalogue and validator are pure
functions over text; a user folder is one `scan_dir` call in
`enumerate`. Rules are additive; a rule that proves too strict for
real files is one match arm.
