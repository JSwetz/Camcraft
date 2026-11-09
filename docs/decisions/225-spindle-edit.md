## #225 - spindle speed editable on the Feeds tab (2026-11-08)

- **Chosen:** an editable "Spindle speed" row in the Feeds tab's Cutting
  data group, with an rpm / surface switch (surface = m/min in a mm
  document, SFM in an inch one, through the tool's diameter: Vc = pi D n).
  The speed lives on the op's tool (`Tool::rpm`, what the post writes as
  S), so a committed value is one undoable `ChangeTool` - commit on drag
  release / Enter / focus loss, a draft in egui's temp memory meanwhile,
  never a command per frame - and the row's hover says it edits the tool
  for every operation using it (the tool-material and flutes rows'
  law). The switch is view state (`State::spindle_as_vc`).
- **Dropped:** a per-operation spindle override (a schema + post change:
  every op on one tool then needs its own S, and the post's tool-change
  block would have to follow the op, not the tool) - worth its own issue
  if the shop wants two speeds on one cutter.
