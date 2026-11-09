<!-- moved from RECOMMENDED.md for PR #232 (docs/decisions rule, PR #250) -->

## #192 - a refusal on the status line stands out (2026-11-07)

- **Chosen:** the status strip reads its own line: a refusal (the
  message `State::refuse` recorded, a "cannot …" gate note, or one of the
  shell's refusal phrasings - "refused", "failed", "not set:", "not
  changed:", "not aligned:", "no longer", "not generated") is drawn in
  the palette's error ink (colour-vision aware), bold, behind a "!"
  marker, on an error-tinted band across the strip - until the next
  status replaces it. Ordinary status stays dim.
- **Why:** blocked features seemed to "fail for no reason" because the
  reason sat in dim text bottom-left (reporter).
- **Dropped:** a toast/popup (another surface to dismiss, and the strip
  already holds the line); a timed flash (gone before the user looks);
  a `status_tone` field on every status write (dozens of writers - the
  phrasing list is the honest, testable seam until the shell grows a
  typed status).
- **#215 folded in (same PR):** routine status is drawn in the body text
  ink (the theme's `text`, contrast-gated against the strip fill in both
  built-in themes) instead of egui's dim "weak" grey, and "not created:"
  joins the refusal phrasings (the operation-create refusals #215 quotes
  were already alerts through the feedback path; the phrase makes them
  alerts on any path).
