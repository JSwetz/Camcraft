## #339 - fixture library (slice 6)

**Decided:**

- **One library file, `fixtures.camfixtures`, in the user fixtures
  folder.** The folder is `CAMCRAFT_FIXTURES` (absolute), else
  `<platform data dir>/camcraft/fixtures`. That is the sibling of the
  `camcraft/machines` folder that #383 resolves the same way
  (`fixture_library::user_dir_for`, a pure twin of that resolver). A
  single file, not one file per fixture: the triage asked for "a library
  file", and one file keeps entry names unique by construction. The
  tool library (#300) has no fixed home; its files go wherever the user
  saves them. Fixtures are picked from the browser, so they need a known
  place.
- **Format `camcraft-fixtures`, version 1**, the `camcraft-tools/1`
  envelope shape: `{format, version, units: "mm", fixtures: [...]}`.
  - Each entry is `{name, note?, datum: {origin, rot}, facets}`.
  - Entries are written sorted by name, one facet per line, so a big
    mesh is a list of lines rather than one number per line.
  - save -> load -> save is byte-stable. The datum `rot` is kept exactly
    as written; it is normalized only where it is placed.
  - The reader refuses the whole file and names the entry for: bad JSON,
    a wrong format, version or units, a missing name, datum or facets,
    a name used twice, an empty mesh, a malformed facet, a non-rotation
    `rot`, over 2,000,000 facets, or the reserved `Machine/Table` name.
  - The writer runs the same per-entry check (`check_entry`), so a file
    we write always reads back.
- **What an entry stores.** Facets are the fixture's mesh in its own
  model frame: the exact bytes of its `Facets`, without its document
  pose. `datum` is a frame inside that model frame. On insert, the datum
  lands on the picked frame, so the model's `Location = P ∘ D⁻¹`
  (`insert_fixture::place_datum`).
  - The save dialog offers two datums. **Model origin** (the default) is
    identity, so a fixture saved and re-inserted at its old pose stands
    exactly where it stood (pinned by test). **Bottom centre** is the
    centre of the mesh's bounding-box bottom, which suits a vise or
    plate dropped onto a table point.
  - Dropped: storing the mesh re-expressed in the datum frame with no
    datum. That loses the original bytes, and the triage asks for a
    stored placement datum.
- **Duplicate names.**
  - **In the library: refuse, then ask.** Saving a name that is already
    in the library returns `Exists(names)`, and the dialog asks
    "Replace …?" (Replace / Cancel). In a library the name is the only
    handle the user has, so a silent "(2)" would leave twins they cannot
    tell apart.
  - **In the document: suffix.** Inserting an entry whose name is taken
    in the document lands as "Vise (2)", "Vise (3)", and so on
    (`insert_fixture::free_name`). Two of the same vise on one table is
    ordinary shop work.
- **Insert is one undoable command.** `InsertFixture` spawns the model
  (facets, rev 1), writes its pose and puts on the `FixtureDef` of the
  Fixtures node's setup (`fixed: false`). It is one undo unit, and its
  invert removes the model by name. It is refused at construction for:
  a setup that is gone, a non-finite pick or non-rotation, or an entry
  that fails `check_entry`. The UI re-reads the entry from the file at
  Insert time, so the dialog never inserts a stale copy.
- **No project-file change.** An inserted fixture is an ordinary tagged
  model. The pinning test saves, reloads and resaves, and the two files
  match byte for byte. The library source is not recorded: nothing reads
  it yet, and an optional key can be added later without a version rung.
- **Save and delete are file edits, not undo entries**, as with machine
  files (#383).
  - Delete asks first: "Delete '…' from the library? This edits the file
    and cannot be undone." with Delete / Keep.
  - Writes are atomic (temp file plus rename).
  - A refused or unreadable library file is never overwritten.
  - A missing file is the empty library. A corrupt file is named in the
    dialog, never shown as empty.
- **UI.**
  - The Fixtures node menu (slice 4) gains "Save fixture to library…" and
    "Insert from library…".
  - Both open a modal, drawn like the unsaved prompt.
  - **Save dialog:** the setup's fixtures as checkboxes. The selected
    ones start checked; a setup with a single fixture starts with it
    checked. Below them are the datum radio and a note.
  - **Insert dialog:** the entries (name, facet count, note), the datum
    origin (unit-lens fields, stored in mm) and roll / pitch / yaw
    (degrees, R = Rz ⊗ Ry ⊗ Rx, the Model page's convention), then
    Insert, Delete from library… and Close.
  - Picking the datum in the viewport is a follow-up. This slice takes
    numbers.
- **Not shared with #383's store yet.** The atomic write is a ~20-line
  copy of #383's `write_atomic`, because #383 is not merged under this
  stack. Fold the two into one helper once both land.
- **Clean room:** the library holds only what the user saves. No vendor
  fixture library, asset or format was read, consulted or shipped.
