<!-- moved from RECOMMENDED.md for PR #249 (docs/decisions rule, PR #250) -->

## #246 - a flat facing the axis is a bearing of the envelope (2026-11-08)

- **Chosen:** `axis_point::faces_axis` - a facet square to the axis
  (|n . u| < 0.05) whose centroid's normal line misses it by less than
  half the facet's own size (`sqrt(area) / 2`) faces the axis: the foot
  of the axis falls on the face (a D-flat). Its normal now counts in both
  of #223's roundness readings - the coarseness gate's "facets that meet
  the axis" and the point-share rule's pencil skip.
- **Why:** #223 (rightly) stopped an off-axis boss lending its bearings,
  by reading only facets whose lines meet the axis within the
  concurrency tolerance. A D-flat's facets sit at the flat's width off
  that line (0.2-0.4 mm on the rotary_dflat bar), so they were set aside
  too, and a single-band D-flat bar - one wall pencil with a ~39 degree
  skip at the flat - read "share no rotation axis". `rotary_dflat::
  a_d_flat_bar_wraps_in_every_placement` was red on main from #223.
- **Why the boss stays out:** a boss's facets miss the part's axis by
  their offset (millimetres), far more than half their own size; every
  #208/#223 boss/prism pin stays green.
- **Dropped:** widening the concurrency tolerance (lets every outlier
  vouch); exempting flats from the point-share rule wholesale (a prism's
  faces are flats too - they must still not certify a 45 degree line).
