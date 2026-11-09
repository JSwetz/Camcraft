<!-- moved from RECOMMENDED.md for PR #213 (docs/decisions rule, PR #250) -->

## #207 - every Create door wears a glyph (2026-11-07)

- **Chosen:** the operation-section dropdown doors and the machine door
  get glyphs from the existing set (no new asset): Milling 2D → pocket,
  Milling 3D → 3D finish, Drilling → drill, Multi-axis → multi-axis
  rough, Turning → face; the machine door by workspace - Milling → face
  mill, Turning → turned bar (rotary contour), Mill-turning → rotary
  parallel. The View door stays caption-only (a layout list).
- **Why:** with no pins (every fresh install) the whole Create run was
  text-only, which users on Windows and Fedora reported as missing
  icons. The M6.2 "a door's caption is its affordance" law read well in
  the design notes and badly on screen.
- **Dropped:** the section's first kind's glyph (Milling 2D's first kind
  is Face - the same pixels as the Milling machine door beside it); new
  bespoke door glyphs (the op glyphs already say what each family makes).
