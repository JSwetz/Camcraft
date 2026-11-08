<!-- moved from RECOMMENDED.md for PR #234 (docs/decisions rule, PR #250) -->

## #195 - a chunkier, shorter origin triad (2026-11-07)

- **Chosen:** the origin triads are additionally stroked in screen space
  by the egui painter - 3 px axis colour over a 5 px dark outline - on
  top of the 1 px GPU overlay lines (a wgpu line has no width, so the 3D
  overlay cannot be thickened without a new quad pipeline). Shorter:
  15 % of the model extent with an 8 mm floor (was 25 % / 10 mm). A
  brighter Z blue (0.38, 0.64, 1.0) and world-axis dimming 0.8 (was
  0.55 for lines, 0.65 for letters). Axis letters 1.4x the small font
  (at least 14 px), bold-ish via a 1 px dark shadow.
- **Why:** the reporter saw a ~1 px origin with small text and could not
  see Z against the background (HiDPI makes a 1 px line thinner still).
- **Dropped:** a thick-line GPU pipeline (more code for the same pixels;
  the screen-space layer already projects labels with the same camera
  math); removing the GPU lines (the overlay's line-count laws and the
  picking code read them).
