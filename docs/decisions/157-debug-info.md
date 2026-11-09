<!-- moved from RECOMMENDED.md for PR #188 (docs/decisions rule, PR #250) -->

## #157 - a Debug info door in the status strip (2026-11-07)

- **Chosen:** a 16 px bug glyph at the status strip's right end opens an
  in-app `egui::Window` ("Debug info") with a plain-text report - version
  and build profile, OS/arch, the GPU adapter line the shell already
  logs, project path, units, machine, document counts with op status,
  and the last status line - and a Copy button (clipboard). View state
  only (`State::debug_info_open`), never a command, never saved.
- **Why there:** the status strip is the shell's diagnostic line already;
  a ribbon cell would shift the ribbon's fold layout and its pin budget.
- **Glyph:** drawn original by script (assets/ui/debug-info.png), noted
  in assets/ui/LICENSES.md; Tabler's bug glyph would also be licensed
  fine but there was no SVG rasterizer to hand.
- **Dropped:** writing the report to a file (Copy covers "paste where it
  needs to go"); including recent log lines (no logging framework yet -
  #163).
