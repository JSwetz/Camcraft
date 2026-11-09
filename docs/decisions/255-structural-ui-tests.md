## #255 - Pixel snapshot goldens leave the PR flow; structural UI tests instead

- **Chosen:** the three pixel snapshot tests (`egui_shell_snapshot`,
  `egui_shell_snapshot_light`, `tool_editor_snapshot`, all in
  `crates/ui/src/egui_app/tests.rs`) are `#[ignore]`d with a "release lane
  (#255)" reason, not deleted. Their intent is asserted in the default
  suite by three new structural tests in
  `crates/ui/src/egui_app/tests/structural.rs` (`shell_structure_dark`,
  `shell_structure_light`, `tool_editor_structure`). A new
  `ui snapshot lane` step in the release tier of both `scripts/gate.ps1`
  and `scripts/gate.sh` runs them. AGENTS.md (Testing), the PR-reviewer
  agent and the review-prs skill now say that a PR must not change
  `crates/ui/tests/snapshots/*.png` and that the goldens are regenerated
  once, at the cut.
- **Why:** PNGs can't be merged, so every UI PR conflicted with the others,
  and the conflict fix (`UPDATE_SNAPSHOTS=1`) accepted whatever the merged
  tree rendered. The goldens also differ by platform: on this macOS run
  the light and tool-editor goldens fail at the 0.6 threshold (1 and 6
  pixels) on an unchanged tree, so on macOS they now fail only in the
  release lane, not in a PR's gate.
- **Structural surface:** the ribbon, the browser rows and the dock's tab
  titles are painted, not widgets, so they don't appear in the AccessKit
  tree. The structural tests read them from the frame's painted text
  shapes (`harness.output().shapes`, `Shape::Text` galleys) and check
  only coarse placement: ribbon above the tabs; browser, viewport and
  properties from left to right; rows inside the browser leaf; status on
  the bottom strip. Widgets (viewport toggles, property page sections,
  Apply/Align WCS disabled, Generate enabled, Debug info button,
  tool-editor fields and their values) are checked through AccessKit. No
  colour beyond the token identity (`panel_fill == Tokens::{DARK,LIGHT}.ink`)
  and no pixel position finer than "which side of which" is asserted.
- **Snapshot lane mode:** debug, `--lib -- --ignored snapshot`, CWD
  `crates/ui`. Debug, because the goldens have always been made from debug
  runs. The `snapshot` filter keeps the two ignored release-only playback
  benches (`play_bench_tests.rs`) out of this lane. They were not in the
  release tier before and are still not; adding them is separate work.
- **Light test restore:** `shell_structure_light` restores the dark theme
  before it re-raises any skeleton failure (`catch_unwind`), so a failing
  assertion can't leave the process-global theme table light for other
  tests. It holds the existing `snapshot_guard` lock, as the dark and
  editor twins do.
- **#[test] count:** 414 before, 417 after in `crates/ui` (3 moved to the
  ignored lane, 3 structural tests added); no test deleted.
- **Dropped:** (a) deleting the PNG tests, which the issue's "Done means"
  forbids; (b) the optional loose-tolerance "renders non-blank" pixel smoke
  test, because the ignored goldens already cover rendering at the cut and
  a loose pixel test would bring platform noise back into the PR lane;
  (c) asserting browser rows through AccessKit, which isn't possible
  without adding accessibility nodes to the painted strips (worth having
  for screen readers, but that is a product change, not a test change).
- **Later cost:** a UI change now updates `structural.rs`, which
  merges as text. The cut gains one step: run the snapshot lane and, after
  looking at the `.diff.png`, regenerate with `UPDATE_SNAPSHOTS=1` in one
  commit on the release branch. Goldens made on one OS will still fail on
  another; the cut should run on the OS that made them (Windows so far).
