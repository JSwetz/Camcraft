## #384 - Machines pane: "Use for this setup"

**What was decided:** The Machines pane's detail view gains a "Use for
this setup" button under the validation verdict. It queues
`CommandKind::UseCatalogueMachine(MachineSource)`, which the state drain
turns into the existing `LoadMachine` command through a new constructor,
`LoadMachine::from_catalogue(world, &MachineSource)`. Undo/redo are the
History's, as for every other machine change. A machine that cannot be
assigned leaves the button disabled, with the reason shown as a line
under it and as the disabled hover text (`MachineEntry::assign_block`):
"fails validation - <first refusal> (and N more)", or "it is already
this setup's machine".

**Why, choice by choice:**

- *`LoadMachine`, not a per-setup command.* There is no `SetSetupMachine`
  and no per-setup machine in the schema: the machine is the document
  singleton `MachineDef`, and the Setup pane's machine picker (library
  combo, "Load other", the editor's Apply, the ribbon's machine door) all
  write it through `LoadMachine`. "The active setup's machine" therefore
  is that singleton today, and the pane uses the same carrier, so the
  pane, the Setup pane and the ribbon cannot disagree about which machine
  is mounted or what undo restores. A per-setup machine would be a schema
  and project-file change (a separate issue if it is ever wanted).
- *The command re-reads and re-validates.* `from_catalogue` does not
  trust the pane's cached entry: it reads the built-in text or the file
  again, runs `validate_text`, and refuses by the first refusal. A file
  edited on disk since the list was read can't mount past the check,
  and the forgiving reader (which drops a backwards travel) never mounts
  a machine whose stated limits it quietly lost. The disabled button is
  the visible half; the command door is the safety half.
- *Built-in entries mount at `builtin_path(name)`*, the same path the
  Setup pane's combo records, so the combo shows the name after a pane
  assignment and "already this setup's machine" is one string compare
  (`MachineSource::mount_path`).
- *Disabled with the reason visible, not refused after the click.* A
  refusal after the click would put an error in the feedback bar for
  something the pane already knew; a disabled button with the reason
  beside it (a hover alone is easy to miss) says it before the click.
- Considered and dropped: a new `AssignMachine` command type (it would
  duplicate `LoadMachine`'s apply/invert exactly); disabling only on
  refusals and letting a re-assign of the mounted machine reach the
  command (it refuses as a no-op, which would be a needless error line).

**Machine editor window (`machine_editor.rs`) vs the Machines pane
(#148): decided, not done here.** Recommended: fold the editor into the
Machines pane as its edit mode when the pane's edit slice lands (the
property grid with save that preserves unknown keys, #287's follow-up),
and retire the floating window then; until that slice exists the Setup
pane keeps its "Edit" button and window unchanged. Why: two editors for
one machine definition would drift (the window edits the parsed
`Machine` and applies to the mounted copy only; the pane's edit is meant
to edit the file's own JSON, unknown keys kept, and save it), and the
pane already owns the list, the sheet and the validation an editor needs.
The Setup pane's "Edit" would then open the Machines pane on the mounted
machine. Dropped: folding it now (a large refactor this issue rules out,
and the pane has no edit grid to fold into yet); keeping both long term
(two code paths for one job).

**Cost to change later:** low. The button and the command arm are a few
lines; a per-setup machine would only change what `from_catalogue`
writes, not the pane.
