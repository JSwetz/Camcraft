## #104 - warn when authored Top is below stock

- **Chosen:** For upright 2D/3D milling, diagnose an authored Top below the
  declared stock over the operation's actual controlled region. Clip stock
  facets to that region rather than using a remote feature's global height.
- **Prior cuts:** Reuse the existing lazy same-setup `PriorCell`. Simulate it
  only when the raw comparison would warn, and read the highest remaining
  cell over the region, including conservative boundary padding. Only analytic box/
  cylinder stock receives this credit: a sampled model-stock grid can miss a
  narrow raised feature, so model-stock history remains unknown for this advisory.
  An all-nonfinite sampled surface is unknown too. Full local
  removal to Top clears the warning; partial local removal or unknown history
  retains it. A local pocket can count without clearing the whole block.
- **Presentation and compatibility:** Store the advisory in existing path
  notes with the `Top below stock:` prefix and show it as a warning in the
  operation status banner. No new serialized verifier variant, project format,
  dependency, or command is needed. The existing command stores/replays the
  generated path, including the note, through normal cache and undo handling.
- **Safety boundary:** Do not add a post blocker or replace any collision
  verdict. A Top mismatch is not proof that a particular approach collides;
  the existing move-level checks remain authoritative and still block unsafe
  paths. Tilted, rotary and turning operations need frame-aware analysis and
  do not receive this world-Z diagnostic. Drilling retains its existing named
  R-plane refusal.
- **Considered and dropped:** Global AABB-only comparison (remote-stock false
  warnings), a new persisted warning variant (unnecessary format/API churn),
  early generation refusal (would also reject safe approaches), and treating
  partial/unknown removal as full clearance (could hide remaining stock).
- **Cost and limits:** Facet clipping and, only for a raw mismatch with earlier
  cuts, prior simulation add generation work. Boundary-cell padding may retain
  conservative warnings near a pocket wall. This is an advisory, not a new
  safety certification. Extending it to other frames requires their own
  geometry, not a comparison against world Z. Unit, native integration and UI
  display/quick-gate checks pin the diagnostic; validation results belong in
  the PR and issue, not an assertion of success in this decision record.
