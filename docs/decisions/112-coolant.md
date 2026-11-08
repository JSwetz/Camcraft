<!-- moved from RECOMMENDED.md by PR #147 (docs/decisions rule, PR #250) -->

## #112 - coolant per operation: Off / Flood / Mist / Through tool (2026-11-07)

- **Chosen:** a per-operation `coolant` field on `Operation` (the post
  crate's `CoolantMode`, one enum from field to stream), default
  **Flood** - every built-in post already switched the machine's Flood
  code on per section and M9 off at its end, so Flood keeps every
  golden byte-exact. Edited from a Coolant row in the op page's header
  grid, under Tool, through `EditOperationParams::with_coolant` (one
  undo unit; the stamp survives, since coolant shapes no geometry).
  The project file writes `"coolant": "mist"` etc. only off the
  default (no version bump; absent reads Flood - the `area` precedent).
- **Stream law:** Flood is the silent default - a section states
  `PARAM Coolant off|mist|through-tool` only when it asks something
  else, so every legacy `.cl`/stream posts unchanged. The scripts look
  the code up in the MACHINE's coolant list by family name (Mist;
  Through-spindle / Through-tool), never a hard-wired M7: the machine
  is truth, and that covers Heidenhain too (its own .mch codes). An
  ask the machine states no code for is a counted `message()` - the
  gate blocks the post by name rather than cut dry.
- **Switching:** kept the existing per-section bracket (on after the
  spindle, M9 before M5 at section end), so coolant is always off
  across a tool change and at program end; a dry section writes
  neither word. Dropped: modal coolant carried across sections with M9
  only on change (fewer lines, but a tool change under running fluid
  and a golden churn for every multi-op program).
- **Dropped:** a per-tool default (the tool library carries no coolant
  today; can seed the op default later), and offering Through tool only
  for machines that state it (the row would change with the mount; the
  post-time refusal names the gap instead).

## #150 - coolant review follow-ups (2026-11-08)

- **Refusals name the op:** every post captures the section's
  `OperationName` PARAM and prefixes its coolant `message()` with
  `operation <name>:`; a legacy stream with no name keeps the bare text.
- **Machine editor:** the coolant name is a pick of the posts'
  vocabulary (`COOLANT_KINDS`: Flood, Mist, Through-spindle,
  Through-tool) plus "custom" (a text field, since a shop's own name
  like `Brine` is a tested flood law). The posts ALSO normalize names
  (trim, case-blind, spaces/underscores as hyphens) so hand-edited
  files read. Dropped: editor-only normalization (a hand-typed `.mch`
  would still miss).
- **Flood with no flood code:** refused by name when the machine's
  list is stated but holds only other kinds (Mist-only no longer posts
  a Flood op as M7). Kept silent: an EMPTY/unstated list (the
  unstated-limit law, pinned by `a_mounted_machine_names_the_program_
  and_speaks_only_its_own_coolant` and relied on by every turning fixture
  that mounts a coolant-less machine) and a codeless Flood entry (the
  operator's own valve). Considered and dropped: refusing the empty
  list too - it would block every program on a machine that never
  stated coolant, a behaviour change wider than this review asked.
- **User posts:** a manifest flag `coolantModes = true` (read once at
  load, like the hook table). A script without it gets a counted engine
  warning naming the op on any `PARAM Coolant` - so it refuses
  Off/Mist/Through-tool rather than flood them. Flood-only programs
  need no flag, so existing user posts keep posting what they did.
  Dropped: sniffing the script text for "Coolant" (fragile).
