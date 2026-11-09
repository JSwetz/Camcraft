## #449 - The toolbar version label is pinned under test

**Decided:** the toolbar's build badge reads its version through a
toolbar-only constant, `TOOLBAR_VERSION`, which is `"0.0.0"` under
`#[cfg(test)]` and the real `CARGO_PKG_VERSION` otherwise. Both the full
and compact lines (`toolbar_lines`) and the narrowest version-only label in
`panes.rs` (`toolbar_version()`) go through it. The debug-info dialog keeps
reading the real `VERSION`.

**Why:** the shell snapshots render the badge, and the version bump is
committed after the release gate runs, so every cut turned main red by one
glyph. Pinning the version beside the already-pinned build time and source
sha makes the snapshots independent of the version. `"0.0.0"` is never a
released version, so a test can tell the pin from the real value.

**Considered and dropped:** masking the badge region in the snapshot
comparison (hides real layout regressions of the badge); a test string the
same width as the current version (it would drift from the real width
anyway on the next bump that changes the digit count).

**Cost to change:** one constant and three regenerated snapshots.
