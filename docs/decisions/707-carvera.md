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

**Step 2 (the post):** follows on the same PR before it's flagged ready, so the machines never ship without a post that speaks their firmware.

**Considered and dropped:**
- **The PDF manual as the source:** it has no spec table.
- **Retailer listings:** one (280 × 120 × 60 mm, 30k rpm, 5 tools) describes another machine entirely, and Makera's own tables agree with each other.
