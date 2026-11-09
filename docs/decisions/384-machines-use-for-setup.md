## #384 - Machines pane: "Use for this setup"

**What was decided:** The Machines pane's detail view gains a "Use for
this setup" button under the validation verdict. It queues
`CommandKind::UseCatalogueMachine(MachineSource)`, which the state drain
routes exactly like every other machine verb on main (#343,
`state_setup_mount::route_machine`, `MachineVerb::Mount`): onto the
ACTIVE setup's own machine (`SetSetupMachine`) when that setup owns one,
else onto the document's machine (`LoadMachine`), which every setup that
owns none follows. Undo/redo are the History's. The machine is read and
validated first by `machine::library::read_for_assign(source)`, shared by
the ui route and by the two command doors added for app callers,
`LoadMachine::from_catalogue` and `SetSetupMachine::from_catalogue`. A
machine that cannot be assigned leaves the button disabled, with the
reason shown as a line under it and as the disabled hover text
(`MachineEntry::assign_block`): "fails validation - <first refusal> (and
N more)", or "it is already this setup's machine" - read off
`machine::of`, which on main is the active setup's EFFECTIVE machine
(its own, else the document's).

**Why, choice by choice:**

- *The #343 route, not a pane-specific choice.* An earlier draft of this
  PR (cut before #343 reached its base) assumed the machine was only the
  document singleton and always used `LoadMachine`; on main a setup can
  own its machine, and an active setup that does would then have kept
  its own machine while the pane changed the document's behind it - the
  button would have "worked" and changed nothing the user cuts with.
  Routing through `route_machine` means the pane, the Setup pane's
  machine group, the ribbon's machine door and the editor's Apply all
  land on the same mount, and undo restores what that mount held.
  Considered and dropped: always writing the active setup (making it own
  a machine as a side effect of "use" - an implicit ownership switch the
  user did not ask for; the setup tree's own/follow toggle stays the one
  place that decides ownership).
- *Validated at assignment.* `read_for_assign` reads the built-in text or
  the file again and runs `validate_text`, refusing by the first refusal,
  rather than trusting the pane's cached entry: a file edited on disk
  since the list was read cannot mount past the check, and the forgiving
  reader (which drops a backwards travel) never mounts a machine whose
  stated limits it quietly lost. The disabled button is the visible
  half; the route is the safety half.
- *Built-in entries mount at `builtin_path(name)`*, the same path the
  Setup pane's combo records, so the combo shows the name after a pane
  assignment and "already this setup's machine" is one string compare
  (`MachineSource::mount_path`).
- *Disabled with the reason visible, not refused after the click.* A
  refusal after the click would put an error in the feedback bar for
  something the pane already knew; a hover alone is easy to miss, so the
  reason is also a line under the button.
- Considered and dropped: a new `AssignMachine` command type (it would
  duplicate `LoadMachine` / `SetSetupMachine` apply/invert exactly).

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

**Cost to change later:** low. The button, the command arm and
`mount_catalogue` are a few lines; any change to which mount a machine
verb edits is made once in `route_machine` and the pane follows it.
