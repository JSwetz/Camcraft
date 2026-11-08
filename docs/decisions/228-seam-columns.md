## #228 - a plain two-ring bar wraps; the seam refusal names its real causes

- **Cause found:** The rotary wrap refused a plain bar meshed as two rings
  because the axis fit leaned it by rounding, not because of the seam
  check's tolerance alone. On a two-ring bar every side facet is a long
  right triangle, so its circumcenter (where its normal line is anchored)
  is the midpoint of the long diagonal: the bar's mid-station. No normal
  line can see a tilt about that point. The eigen direction was exact
  (residual 7.4e-14 mm), but the Gauss-Newton refinement returned a tilt
  of 2.6e-5 rad at n 16 (2.5e-6 at n 64), and the fit kept it because its
  residual came out lower by rounding noise. At a 0.3 rad turn the noise
  fell the other way and the same bar wrapped. With the axis leaning, the
  cap centres sat about 1 um off it at arbitrary azimuths, every cap facet
  near the branch read as a seam straddler, and the seam check counted 15
  to 38 columns.
- **Chosen:**
  1. `axis_refine::worth_refining`: the fit does not refine an eigen
     line whose residual is already below `CONCURRENT = 1e-9` mm per mm
     of mean radius. It meets every normal line to rounding, and a
     refinement can only add an unseen tilt. A real lean (issue #62's
     uneven cone, microns of miss) is far above the floor and is still
     refined.
  2. `rotary_seam::SEAM_TH_TOL = 1 / WELD` (1e-6 rad, was 1e-9): seam
     vertices within the weld grid's grain, read as an angle, share a
     column. That is coarser than a float32 STL's azimuth noise. A flat's
     or keyway's ragged vertices stand whole degrees apart, and every
     D-flat refusal test still refuses.
  3. The seam refusal names both real causes: a feature on the seam, or
     diameters tessellated with different column counts. It also says
     what to do about each.
- **Considered and dropped:** Only widening the seam's tolerances (the
  issue's suggestion). With the axis 2 um off at the bar's end, the
  column profiles differed by 2 um in radius as well, so the tolerances
  would have had to grow past what tells a ragged seam from a round one.
  Widening `rho_eps` so the cap centres read as on the axis was dropped
  for the same reason: it treats the symptom of a leaning fit.
- **Not changed:** A stepped shaft whose diameters have different column
  counts is still refused (now by an accurate message). Wrapping it would
  need the seam law to accept columns that change at a shoulder, and
  nothing yet pins what such a wrap would cut.
- **Cost to change later:** Small. One floor constant and one tolerance
  constant, each documented where it is used.
