## #662 - Recent projects and models on Open and Model

**Decided:**

- **Where the logic lives.** The list logic and the file format are in
  `camcraft_app::recent` (add / de-dup / cap / clear, load, atomic save,
  the config-folder resolver). The UI keeps the lists on `State`
  (`state_recent.rs`) and draws them (`egui_app/recent_drop.rs`). Panels
  never touch the file.
- **The button is a drop-down, not a split button.** Open and Model
  open their recent list when it has entries. The list holds the files
  newest first, then "Clear recent", then "Browse…" (today's dialog).
  With an empty list, or with the feature off, the button goes straight
  to the dialog as before, so nothing changes until a file has been
  opened. Ctrl+O and Ctrl+Shift+O still go straight to the dialogs.
  - Dropped: a split-button arrow. The ribbon's cells are painted
    geometry (`ribbon_cells`), and a second hit zone per cell would mean
    changing the layout every ribbon test pins. The drop-down uses the
    same pattern as the Theme, View and machine doors.
- **No File menu exists.** The menu bar retired with the imgui shell
  (actions.rs), so there is no "Open Recent" submenu to add.
- **Storage.** The file is `recent.json` (our own `camcraft-recent/1`
  JSON). It lives in `<config>/camcraft/`:
  - Windows: `%APPDATA%`.
  - macOS: `~/Library/Application Support`.
  - Linux and other platforms: `$XDG_CONFIG_HOME` or `~/.config`.

  `CAMCRAFT_CONFIG_DIR` (an absolute folder) overrides it. On Windows
  and macOS this is beside the user machines folder (`camcraft/machines`,
  #287). On Linux the machines folder is the XDG *data* folder, but the
  issue asked for the config folder, and preferences belong there under
  the XDG spec.
  - No new dependency. The resolver has the same shape as
    `machine::store::user_dir_for`, and the write reuses
    `machine::store::write_atomic` (temp file, then rename).
- **The cap is a config key, not a Settings control.** The app has no
  Settings dialog yet. The cap is the file's `limit` key: default 10,
  0 turns recent files off (nothing is added and the buttons go straight
  to the dialogs), and it is clamped at 50. A Settings page can edit
  that same key later.
- **What adds an entry.** A successful project open, a successful
  project save (Save and Save As share one write path, so a plain Save
  also moves its project to the top, which is harmless), and a landed
  model import (a fresh model, an update, or an import as a fixture).
  A failed or refused open adds nothing.
- **Missing files.** A missing file stays listed, greyed, with
  "(missing)". A click on it sets a status line that names it, and
  nothing else changes. The file is checked again when the command
  runs, because it can vanish between the draw and the click.
  - A missing recent project skips the unsaved-changes prompt: there is
    nothing to open, so there is nothing to ask about.
  - Missing entries are not pruned automatically. The user may remount
    a drive, and Clear recent covers the rest.
- **Bad input.** A corrupt file or one with the wrong format, or one
  that can't be read, starts both lists empty. The warning goes to the
  log (`tracing::warn`) and the status line. A missing file is a quiet
  empty start. On read, relative paths, empty paths and repeats are
  dropped, and each list is cut to the cap.
- **Headless States never touch the user's file.** Only the real window
  (`Shell::new`) reads and writes the per-user file. `State::new()`
  keeps the lists in memory, so the test suites can't overwrite a
  developer's recents. The tests use `State::load_recent_from(<temp>)`.
- **Not undoable.** `OpenRecent` runs the same open or import as the
  dialog. The lists are preference state, and `ClearRecent` is outside
  the history. A recent project goes through the same unsaved-changes
  guard as Open.

**Cost to change later:**

- Adding a Settings UI for the cap means one numeric field that writes
  `RecentFiles::set_limit` and then saves.
- Moving to a split button means changing the ribbon geometry, with no
  change to the list logic.
- If two app instances run at once, the last one to write wins.
  Accepted for a convenience list.
