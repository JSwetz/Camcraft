## #718 - pocket recognition asks the winding verdict first

**Chosen:** `recognize::pockets_of` runs #387's body-winding verdict (`pick::winding::winding_of` / `verdict_for`) over the whole model before reading any floor or wall. Any verdict but `Outward` is refused by name: inside-out, open, non-manifold, mixed, flat, or no readable facet. A part whose outside can't be trusted has no pockets that can be trusted.

**Why:** recognition reads "faces up" and "leans into the floor" straight off the facet winding. The maintainer's review of #512 showed that an inside-out 10 mm cube came back as a closed pocket over its own outline, 10 mm deep. `detect_pockets create` would then make a Pocket over the whole footprint, cut to the base.

**Wording:** the refusal texts are recognition's own ("before detecting pockets"). `pick::winding::refusal` ends in "before aligning to it", which would read wrong here. The verdict and its classes are shared, only the words differ.

**Considered and dropped:**
- **Per-pocket checks** (refuse only the pockets on a bad body): any pocket on that body is read off the same untrusted winding, so the whole model is refused.
- **Silently returning no pockets:** a refusal says why and what to fix. An empty list would look like "no pockets here".
