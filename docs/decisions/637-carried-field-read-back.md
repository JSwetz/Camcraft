# #637 step D - a carried field is read back as a column body, not a body enum

**Chosen:** `CarriedField::column_body(lattice, judged_before) -> Body`. It reads the carried, clipped field onto the new setup's lattice:
- **Per column:** one down-march for the top and the exact shell floor (step B) for the floor.
- **A flip:** swaps the two reads.
- **Clip:** the block trims each column, and an empty clip leaves every column empty.
- **Refusal:** a floor a cut reached is refused by name.

The result is the same `carry::Body` the column engine makes, so `deviate`, `display`, `volume` and `bounds` run unchanged on either engine.

**Why not the planned enum** (`Body = Column | Field`, from #637's research comment)? Every `Body` method (`deviate`'s two-sided verdict, `display`, `volume`, `bounds`, `carry`) would need a second field implementation. Each would be new verdict code with its own failure modes, and the agreement lane would then have to cover all of it. Reading back onto the lattice reuses the one verdict the column carry already proves. The field's exactness is kept where it matters: tops and floors are read from the field, never interpolated from another lattice. It's also the smaller change for step E (#586's holders keep `Body`).

**What it gives up:** the verdict runs at lattice resolution (as the column engine's does), not on the field directly. A field-native verdict can still be added later, behind the same `Body` API, if a fixture needs finer than the lattice.

**Pinned** by `carry_engine_agreement` (sim): the same block, cut by the same slot (shallow and deep), carried through all 8 lattice turns, flips included, on both engines. Tops and floors agree to 1e-6 per column, the empty columns match, and volumes agree to 1e-6.

**Considered and dropped:** the body enum (above); re-sampling the field onto the carried lattice by interpolation (breaks "re-index, never re-sample").
