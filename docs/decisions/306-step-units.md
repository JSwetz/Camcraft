## #306 - STEP import reads the file's length unit

- **Chosen:** Before tessellating, the importer reads every
  representation context's `GLOBAL_UNIT_ASSIGNED_CONTEXT` and resolves
  its `LENGTH_UNIT` to millimetres (`geometry::import::step_units`):
  - an `SI_UNIT(prefix, .METRE.)` by its prefix (none is the metre);
  - a `CONVERSION_BASED_UNIT(name, #m)` by its `LENGTH_MEASURE_WITH_UNIT`
    value times that measure's own unit ('INCH' is 25.4 x millimetre).

  The tessellated vertices are then scaled to millimetres. Scaling after
  tessellating is safe because truck's tolerance schedule is relative to
  each shell's extent, so the mesh has the same quality in any unit.
- **Refusals:** a stated unit this reader can't convert (another SI unit,
  an unknown prefix, a measure with no positive value, a chain deeper
  than four) and contexts that state two different length units are
  refused by name. Guessing a part's scale is the silent wrong answer.
  A file with no unit context keeps millimetres, as every import did
  before.
- **Why:** The importer read every coordinate as millimetres. The repo's
  own `sample_ap214.step` (metres) imported 1000x too small, and an inch
  export would import 25.4x too small, then generate, verify and post at
  that size.
- **Considered and dropped:**
  - Reading only the first `LENGTH_UNIT` entity in the file: a file can
    hold length units that no context uses (the metre a conversion is
    based on), so the context's own reference is the one that counts.
  - An import-units choice in the UI: it's the right answer for STL and
    OBJ, which state no unit, but a STEP file already says what it means.
- **Cost to change later:** Small. One module, one call in `step::parse`.
  Another conversion-based unit (`'FOOT'`) already works through its
  measure.
