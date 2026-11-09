## #226 - the rotary wrap turns its seam clear of a D-flat

- **Chosen:** When the default seam (the wrap basis' ±π branch) is not
  turned, the wrap searches for a seam angle that is
  (`multiaxis::rotary_seam_angle`). It then turns the azimuth basis
  (`e1`, `e2`) so its branch stands there, and lays the development out
  again (`multiaxis::rotary_sheet::lay`, the lap loop moved out of
  `rotary_offset`). The turned sheet is judged again by the same seam law
  (`rotary_seam::seam_columns`) before anything is cut. The world exit
  takes the turned basis, so every developed point maps back to its true
  azimuth. Only the place where the development splits moves, not where
  metal is cut.
- **Default seam first:** The search runs only when the default seam is
  refused. A part that wrapped before keeps its basis and its output byte
  for byte. No golden changed, and the rotary_offset, rotary_links,
  rotary_adaptive, rotary_verify, center_cut_multiaxis and operations
  suites are green unchanged.
- **Candidates:** Each candidate is the midpoint of a gap between
  neighbouring vertex azimuth columns. Gaps under 1e-4 rad are skipped.
  A seam on a column would read that column's vertices on both sides of
  the branch. A facet straddles a candidate when the candidate lies
  inside the shortest arc holding its off-axis vertices. One sweep with
  binary search hands each candidate its straddlers. A facet spread over
  more than half a turn is tested at every candidate, using the sheet's
  own spread-over-π rule. Each candidate is judged by the unchanged #54
  and #228 seam law: two meridian columns with the same profile.
- **Which turned candidate:** The one farthest from any refused candidate,
  which is the middle of the widest turned band. On a D-flat bar that is
  opposite the flat, away from its edges, where PR #210's review found
  the end-station gouge band. Ties go to the candidate nearest the default
  seam, then to the lower azimuth.
- **Refusal:** When no candidate is turned, the wrap still refuses by name
  with the same seam message, which now says "at every angle the seam
  could take (a feature all the way round)". The mixed-column-count shaft
  (#228) is still refused that way, because its neck and body columns
  disagree at every angle. The search also refuses by name, rather than
  hang, once it has handed out 20 million straddler vertices.
- **Pinned by:** `rotary_dflat::an_off_centre_flat_across_the_seam_never_puts_a_tip_in_the_part`,
  where all 36 placements now wrap with 0 tips inside the true
  cross-section (#210 refused every one across the default seam).
  `a_d_flat_bar_wraps_in_every_placement` now includes the default seam's
  placement, and every channel tip must stand at the flat's true
  azimuth. `a_flat_channel_across_the_default_seam_develops_under_adaptive_and_rings`
  replaces the refusal pin. `a_feature_all_the_way_round_is_still_refused_by_name`
  uses four short overlapping flats a quarter turn apart.
- **Considered and dropped:**
  - Always picking the best seam, even when the default is clean. That
    would change every existing wrap's developed coordinates and posted
    order for no gain.
  - Taking the first clean candidate nearest π. It can sit one column
    off the flat's edge, the band where #210's gouges lived. The middle
    of the widest clean band is the safer pick.
  - Rotating the mesh instead of the basis. That is the same thing with
    an extra inverse transform on the way out, and a second place for a
    2π slip.
  - Accepting a ragged seam by reshaping the cover. #210's review showed
    that this still gouges.
- **Cost to change later:** Small. The search is one module behind one
  call. The pick rule is a single comparison in `turned_seam`.
