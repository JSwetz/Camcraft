## #745 - Machine overview as a label | value grid

**Decided.** A new `Machine::rows(unit)` (crates/post/src/machine/rows.rs)
returns the overview as label/value rows - Kind, Machine, Spindle, Axes,
Tool changer, Coolant, Workspace - and the Machine page and the machine
editor header both draw it as an `egui::Grid` (crates/ui/src/egui_app/
machine_grid.rs). The page appends a Source row (Built-in: <name>, or the
path), which only the page knows. `summary()` is untouched for the setup
sheet and `boundary_draft`.

**Why.** One structured reading means the page and the editor cannot
drift; `summary()` stays so its one-line callers and tests are unchanged.
Unknowns read "not stated" (spindle: "up to N rpm (min not stated)", axes:
"Y stroke not stated"), never `?`. Lengths follow the document unit;
rotary strokes stay in degrees. Values wrap (`Label::wrap`).

**Dropped.** Rewriting `summary()` on top of the rows (churns the setup
sheet text and its tests for no user gain); showing the editor header's
grid inline with Apply/Revert (kept above them instead). The Kind row says
"not stated" when the file declares none, while Workspace always shows the
inferred workspace.

**Cost to change.** Row labels live in rows.rs; tests pin them.
