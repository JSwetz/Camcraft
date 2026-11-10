## #707 - Makera Carvera (C1) and Carvera Air

**Sources:**
- **Machine figures:** the "Specs" tables on makera.com/products/carvera and makera.com/pages/carvera-air (read 2026-11-10). The Carvera instruction manual has no spec table; it points to makera.com.
- **Machine zero:** the manual (§ coordinates) puts machine zero in the upper right corner of the machine, so machine coordinates are generally negative.
- **Codes:** the Carvera firmware's supported-codes tables (Makera's firmware is Smoothieware-based; the community firmware documents the same tables at carvera-community.gitbook.io, firmware → supported commands).

**Step 1, the machine files:**

| | Carvera | Carvera Air |
|---|---|---|
| Work area (mm) | 360 × 240 × 140 | 300 × 200 × 130 |
| Spindle | 200 W, 0–15000 rpm | 200 W, 0–13000 rpm |
| Max travel speed | 6000 mm/min | 4000 mm/min |
| Tool library | 6 (auto tool changer) | none stated (quick tool changer) |

- **Travel** is the full stroke either way in program coordinates (±X, ±Y, ±Z), as on every shipped mill (the travel band is relative to the work zero; see `machine/mod.rs`).
- **Speeds:** Makera states one travel speed for all axes, so each axis takes it as both feed and rapid limit.
- **No `min_speed`:** "0 – 15000 rpm" publishes no lowest working speed, and the validator rightly refuses 0, so the file states none rather than inventing one.
- **Coolant:** the firmware's M7 / M9 are "airflow", and there is no M8. Each file has one coolant, `Air` on M7.
- **Units: mm.** The firmware's G20 "will halt the machine", so the machines are metric only.
- **The 4th axis module is left out of these files.** It's an optional add-on (Ø92 × 240 / 200 mm, harmonic drive), and how its table-side A axis is modelled needs its own look, so it's a later step. The base machines are 3-axis.

**Step 2, the post (`assets/posts/carvera.cps`, registered as "Makera Carvera"):** it writes only words the firmware's tables list.

| Area | Choice and reason |
|---|---|
| Header | `G21 G17 G90 G94`. No G40 / G49 / G80, because the tables list none of them. No G91.1, because the firmware's arc centres are relative by default. An inch machine is refused by name, because G20 halts the firmware. |
| Tool change | `M6 Tn`, the firmware's auto tool change (the Air's quick changer prompts through the same word). The firmware measures tool length itself, so no G43 follows. Tool 0 is refused by name, because T0 is the wireless probe. Tool numbers past the C1's six slots are the machine check's refusal, from `tooling.number_of_tools`. |
| Spindle and air | `M3 S…` (the tables' own example order). The air is the machine's M7, off with M9 at section end. |
| Drilling | The tables list no canned cycle, so drilling posts as its expanded chain, as `heidenhain.cps` does. A dwell becomes `G4 P<seconds>` at the hole bottom. Tapping is refused by name: no cycle, and M4 isn't wired on the stock machine. |
| Other refusals | Cutter compensation in the control (no G41 / G42). Tilted moves (the 4th axis module isn't supported yet). Probe operations: the engine refuses them for a post without probe hooks, because how the firmware reports a G38 trip isn't in the sources. |
| Program end | `M2`, the tables' "end program, stop spindle and air". M30 is listed as "no action on the Carvera". No machine-coordinate retract is written: the manual puts machine zero at the top right but states no safe G53 height. |

**Not exercised:** no run on a real Carvera or in Carvera Controller. A contributor with the machine is asked to run the two-tool pocket golden before the post is trusted on a machine.

**Considered and dropped:**
- **The PDF manual as the source:** it has no spec table.
- **Retailer listings:** one (280 × 120 × 60 mm, 30k rpm, 5 tools) describes another machine entirely, and Makera's own tables agree with each other.
