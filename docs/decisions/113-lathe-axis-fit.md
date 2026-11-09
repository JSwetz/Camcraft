## #113 - a large keyway or flat no longer refuses a revolved part

- **Cause:** The axis fit claims an axis only when 90 % of the answering
  (non-pole) surface meets the plain least-squares line within the
  sagitta band (3 % of the radius). The robust pass of #71, which sets
  keyways and flats aside, runs only after that gate. A 10 mm wide, 20 mm
  long keyway with its floor 6 mm deep on a D20 shaft drags the plain
  line 0.57 mm off the axis, about twice the 0.29 mm band. Half the honest
  wall then misses the line, and the part was refused as "share no
  rotation axis". A long keyway also fails on share alone: a 50 mm one sets
  aside 17 % of the surface.
- **Chosen:** a consensus fallback (`motion::multiaxis::axis_consensus`).
  It runs only when no candidate passes the plain gate.
  1. Each candidate direction the plain fit tried (the two eigen
     directions and the pole directions; never on an isotropic part) is
     fitted again by consensus. The facets whose normal lines meet the
     line within a cut are kept, and the line is refitted on them. The
     cut is 2.5 MAD sigmas of the kept misses, and it is at least halved
     each round until it reaches the sagitta band.
  2. The gate is then read about the consensus line at a lower share,
     `CONSENSUS_SHARE = 0.75`. The coarseness gate and the pencil gate
     (`borne_out`) are unchanged.
  3. The consensus line then takes `axis_robust::reweigh` (the #116
     direction refinement and tightened band), with every refit judged at
     the same 0.75 share (`reweigh` and `judge_at` now take the share as
     a parameter). `Fit::plain` still reports the plain least-squares
     line, and `outlier_share` the share set aside. So the spindle law's
     generation note and its misplaced-part refusal name the keyway as
     they do for #71's small ones.
  4. A part that passes the plain gate never reaches the fallback, so
     every existing fit and golden is unchanged bit for bit.
- **Why 0.75:** a turned part whose keyways, flats and holes take more
  than a quarter of its non-face surface is mostly a milled part, and it
  stays refused. The value sits between the robust pass's 50 % backstop
  (`MIN_KEPT_SHARE`) and the plain gate's 90 %. The large-keyway fixtures
  set aside 5 % (deep 20 mm key) and 17 % (50 mm key). An 8 x 8 x 50
  block beside a D20 x 60 shaft sets aside 30 % and is refused by name.
- **Why the direction is held during the consensus:** the first version
  refined the direction on the kept set, and it broke a #208 guard. On a
  100 x 100 x 10 plate with an off-centre boss, the kept set was the
  plate's two-triangle faces. Their normal lines all meet at the plate's
  centre, so the refined direction swung to a 45-degree line through that
  point, and that line passed every gate. With the direction held at the
  candidate's, the plate is refused as before. Direction refinement
  happens only afterwards in `reweigh`, about a line the gate has already
  accepted, where the revolved surface pins it.
- **Why the cut halves:** a pure MAD cut could not finish a 50 mm keyway.
  The drag (1.7 mm) inflates the honest wall's scatter until the cut
  (5 mm) keeps the keyway's walls, and the set stops changing above the
  band. Halving the cut each round (graduated, as in annealing) forces
  it down to the band. Each round, the median scatter can only make the
  cut smaller.
- **Safety (never gouge):** the fallback only decides which line is the
  axis. The turning profile is still the part's silhouette about that
  line, so a feature set aside is left standing and never cut into. The
  generation note names the set-aside share. The spindle law still
  checks the part against the spindle at 0.5 um, so a deep-keyed shaft
  1 mm off G54 is refused as misplaced.
- **Untrusted refusal text:** the message is built by a new function,
  `spindle_law::refusal`, which `check` calls. Unit tests
  (`operation/spindle_law_tests.rs`) cover the untrusted text (biased
  fit, share named, "moving the work offset may not fix it", never
  "0.0%") and the misplaced text. No geometric fixture reaches the
  untrusted branch through the full fit. It needs a plain fit that
  passes 90 % and a refit that then fails the gate, and none was found
  that is not contrived. So the text is tested directly.
- **Considered and dropped:**
  - Lowering `OK_SHARE` itself to 0.75. That would change every fit,
    the 5-axis and rotary kinds included, and it would gate the dragged
    plain line, not the consensus.
  - Least trimmed squares from the lower half, as in `axis_poles`. On
    a dragged line, the lower half is the wall whose bearings run along
    the drag, and that wall barely constrains the coordinate the drag
    moved.
  - A RANSAC search over random minimal sets. It is not deterministic,
    and the fit must give the same answer on every run.
- **Known limit:** a block standing on a meridian (its two walls square
  to the axis) has walls whose normal lines meet the axis. The
  concurrency law reads them as flats, so they count as kept. This was
  already true for the plain gate, and the silhouette profile still
  leaves such a block standing.
- **Cost to change later:** `CONSENSUS_SHARE` is one constant. Removing
  the fallback means deleting one call in `axis::fit_report`.
