## #230 - a snug cylinder fit (2026-11-08)

- **Chosen:** Fit to Models' cylinder takes its radius from the models'
  largest horizontal distance from the fit's vertical axis through the
  bounds' XY centre (`model::solid_world_radius_about`) plus the margin
  (`stock::fit_cylinder_about`), instead of the padded footprint's
  diagonal. For a turned part standing on its axis that is its revolved
  max radius: a round R part gets an R + allowance bar, not ~1.41 R.
- **Why it is safe for every cylinder fit (mill too):** the radius is the
  farthest vertex from the axis, so the cylinder still contains the whole
  model; it is never larger than the old diagonal bound.
- **Changes for existing documents:** none on load - a stock already
  fitted keeps its explicit numbers; only a new Fit (cylinder) is snugger.
- **Dropped:** fitting about the spindle axis of a lathe's G54 instead of
  the bounds' centre (a part off the origin is refused by the lathe
  spindle law anyway, and the fit has always centred on the models).
