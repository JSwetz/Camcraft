## #327 - A non-finite work-offset placement is refused by name, never posted as unrotated

**What was decided.**

1. **Post gate.** `posting_wcs_rotation.rs` now classifies every used
   offset with `offset_frame` into `Square` / `Rotated` / `NonFinite`. A
   NaN or infinite origin or rotation, or a zero-norm quaternion, is
   `NonFinite` and is refused with its own line: "the WCS (G54) has a
   non-finite placement (origin or rotation) - re-place the offset before
   posting (operations: ...)". Before, `angle_axis` returned angle 0 for
   a non-finite xyz and the offset posted as unrotated.
2. **`SetWcsOrigin`.** It already refused a non-finite rotation, a
   zero-norm rotation and a non-finite origin; the messages now name the
   setup's WCS by its label (`WCS (G55): rotation must be finite ...`;
   wording ported to the one-WCS law of #342 at the merge). No behaviour
   change beyond the wording.
3. **3-2-1 align.** No code change: `align_321` already refuses a
   non-finite frame, and the pane sends its result through `SetWcsOrigin`,
   which is the checked door (pinned by the `SetWcsOrigin` test).
4. **Project load.** `validate_kind` refuses a WCS row whose translation
   or rotation is non-finite or whose rotation has zero norm, naming the
   offset. JSON cannot spell NaN/inf (serde_json refuses them), so the
   reachable case is the zero-norm quaternion - which `model::set_transform`
   would otherwise silently turn into the identity, i.e. post a frame the
   file never described.
5. **Setup sheet.** Under each offset line, a rotated offset prints
   `rotated <deg> deg about <axis> - the post refuses a rotated offset`
   and a non-finite one prints `placement is not finite - the post
   refuses this offset`. An unrotated offset adds nothing, so every
   existing sheet stays byte-identical.

**Why.** Prime Directive: a value that is no frame at all must be refused
visibly, never read as the identity. One classifier (`offset_frame`)
serves the post gate and the sheet so the two can never disagree.

**Considered and dropped.**

- Refusing a zero-norm/non-finite placement on *every* loaded entity
  (models, stock, fixtures): wider than the issue; the model path's
  identity fallback with a logged diagnostic is an existing, separate
  law. Left as is.
- Making `model::set_transform` reject instead of falling back to the
  identity: changes every caller's contract; out of scope.
- Appending the rotation to the same sheet line as the translation:
  a separate indented line keeps the translation line (and every golden)
  unchanged and stays clear of PR #331's edits to `render_wcs`.
- Folding the non-finite case into the "is rotated" wording: it would
  print a meaningless angle ("NaN deg"); a distinct message is clearer.

**Cost to change later.** Low: the classifier is one function; the
wording lives in two places (post gate, sheet) and is pinned by
`tests/rotated_wcs.rs`.
