## #510 - Slot caps: the verifier's stadium is circumscribed, not inscribed

**Decided.** The region a typed Slot is verified against
(`operation::material::stadium`) now draws each round end as the polygon
CIRCUMSCRIBED about the true semicircle: tangent chords whose corners sit
at `h / cos(phi/2)`, the chord count picked per slot so a corner stands at
most `CAP_SAG_BUDGET = 0.001 mm` outside the arc (16..2048 chords per cap;
61 for a 3 mm half-width, ~250 for 50 mm). `GOUGE_TOL` is untouched.

**Why.** The old cap was 16 chords inscribed in the arc. A chord sits up to
`h (1 - cos(5.625 deg))` inside it; for the 6 mm cutter on a 6 mm slot a
centre on the cap centre read 2.9855 mm from the boundary against the
law's `r - tol = 2.99`, so every plunge and feed of a slot whose cutter
reaches its end (every full-width slot, every outer ring) was a Gouge and
the program never posted. The generator's passes ride the true arc in
closed form, so the false bite was pure tessellation error in the judge.
Circumscribed chords are tangent to the arc, so a tangent cutter is never
nearer the polygon than the arc; the corners give a real over-cut at most
1 um of extra slack (total 0.011 mm), so a 0.05 mm over-cut still convicts.

**Considered and dropped.**
- Loosening `GOUGE_TOL` or a Slot-only tolerance: hides real over-cuts on
  every op; forbidden by the Prime Directive.
- More inscribed chords: shrinks the false bite below the tolerance but
  never removes it (a large slot needs thousands of chords to get under
  0.01 mm, and any finite inscribed count still errs toward convicting).
- An exact arc test in the section law: right in principle but the law
  reads every op's region as polylines; a stadium special case there is a
  wider change than the bug needs.

**Top-Z pass (also reported).** A Slot whose Top is the material's top
opens with a zero-depth level at Top. That is the shared 2D ladder
convention (`cut_levels(top, bottom, step)` starts AT top) used by pocket,
contour, circular, trace, face and the 3D entries - not a Slot bug, and it
is wasted air, never a gouge. Changing it for Slot alone would make one op
disagree with every other ladder; changing it for all moves most goldens.
Left as is and pinned (`the_slot_ladder_starts_at_the_authored_top`);
recommended as a separate issue if the first level should be
`top - stepdown`.

**Cost to change later.** Local: one builder and its constants. The posted
G-code does not change (only the verdict and the drawn region outline,
which moves by at most 1 um).
