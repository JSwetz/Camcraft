## #578 - Errors, warnings and status notes still print millimeters in inch documents

**Decided:** split the mm-speaking strings into two tiers and lens only the
first one now. **Tier 1 - transient strings** (status-line notes, command
refusals, the gate's gouge clause, the Deviate readout) are formatted at
print time through `app::units::of(world)` and the pure formatters in
`geometry::units` (`len`, `feed`, `area`, and a new `vol`). **Tier 2 -
persisted notes** (warnings stored on operations: `stock_sim.rs`'s carried
stock note, `commands/sync_stock.rs`'s two stale-stock notes,
`operation/carried_stock.rs`'s carried-tool note) stay mm for now; they
need the structured-note pattern (`sim::verify::Warning` stores numbers
and the UI formats at read time) to stop freezing the unit into the
project file. A regex sweep over "N mm" strings was rejected outright.

**Why:** a tier-1 string speaks the document's own numbers the user is
already reading on screen, so it must ride the same lens the fields use -
and `len`/`feed`/`area`'s law (mm mode byte-identical, inches = the mm
precision plus extra decimals) makes the mm sentences pin-test
indistinguishable. Tier-2 notes are different in kind: they are written
into the `.camproj` and re-read under a possibly different unit, so
formatting them at write time would make them wrong in a new way. The
structured-note rewrite is a real change with its own pin surface; it
belongs in its own issue, not folded into this fix.

**The new `vol` formatter:** `units::vol(v_mm3, unit, mm_prec)` mirrors
`area` (inches get `mm_prec + 3` decimals, one cubic inch being ~16.4
cubic centimeters' worth of digits) and `Unit::vol_suffix` returns the
superscript forms "mm³"/"in³" to match `area_suffix`'s "mm²"/"in²" - the
plain "mm3"/"in3" spelling would have changed the mm strings. Homes in
`geometry::units` (pure data, R0/R2 split), re-exported by `app::units`.

**Signatures moved:** `ui::actions::deviate_readout` now takes a
`camcraft_app::units::Unit` parameter, mirroring its sibling
`sim_readout(&sc, t, Unit)`; both callers pass the world lens. The
`SetStock` refusal's dimension changed shape: `{MAX_STOCK_DIM:e}` printed
"1e6 mm", the lens prints "1000000 mm" - the issue text's "2000 mm" was
wrong, and no test pinned either spelling.

**Considered and dropped:**
- Machine-limit strings (`app::machine`'s travel refusals,
  `commands/cutting_data`'s spindle limits, the tool-body reach note):
  these speak the MACHINE's numbers, not the document's, and the `.mch`
  grammar has its own unit story. Adjacent and separate: inch machine
  defs (`tormach-pcnc440.mch`) declare `"length": "inch"` but nothing
  converts on read, so `check_ops` compares mm program coordinates
  straight against inch-declared bands. Reported on the issue, not fixed
  here - the fix belongs with the machine-file unit grammar.
- Generator refusals in `operation/circle_fit` and the `operation::*`
  generate paths, and the pure-crate `Display` refusals (`tools`:
  `spun`, `holder_profile`, `lathe_holder`, `lathe_insert_zone`;
  `motion`: `descent`, `probing`): these live below the app - pure
  functions and pure-crate types with no world and no unit in scope.
  Threading a `Unit` through those APIs for a handful of refusal
  sentences is a wide signature change with no lens to hang it on; those
  refusals quote tool and feature geometry the user typed in fields that
  already ride the lens.
- Tier-2 persisted notes: see Decided; needs structured notes
  (Warning-shaped: store numbers, format at display) - worth its own
  issue.
- Sweeping "(\d+) mm" with a regex at display time: parses English back
  into numbers, breaks the mm-byte-identical law, and hides sites
  instead of lensing them.

**Pinned:** `blind_floor::the_gouge_clause_speaks_the_document_s_units`
(same gouging world, inch lens: divides by 25.4, says "in at (...)",
never "mm"; flip back to mm and the sentence is unchanged) and
`egui_app::tests::deviate_readout_rides_the_units_lens` (both REST and
GOUGE sentences pinned in inches; the mm pins sit next to them
untouched).

**Cost to change later:** tier 2 wants one structured-note type; tier-1
lensing already shows every site, so the sweep is done. Machine strings
wait on the `.mch` unit-conversion fix; the pure-crate Displays want a
`Unit` argument, additive once anyone wants it.
