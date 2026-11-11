# Decision Record: #614 - Doosan DNM + Fanuc Post Processor

## #614 - Doosan DNM Post Processor dialect on the Fanuc base

### What was decided

1. **Post Script `assets/posts/doosan-dnm.cps`:**
   - Base dialect: Fanuc 0i / 31i-M (RS-274 / ISO 6983 conforming).
   - Provenance: Clean-room implementation based on published technical manuals and verified production G-code:
     * **DN Solutions Co., Ltd.** (formerly Doosan Machine Tools): *Operation Manual: Introduction / Precaution / Basic Operation (Vertical Machining Center)*, Document No.: `MV0009OPE241_v0` (Revision v0, Published 2024-06-11, Build `MMYP0003`). Covers DNM 4000/4500/5700/6700 series on Fanuc 0i-MF / 31i-B controls.
     * **FANUC Corporation:** *FANUC Series 0i-MODEL F Operator's Manual*, Publication No. `B-64604EN-2/01` (Section 3.3, 3.3.2, 14.1).
     * **Renishaw plc:** *MP700 Probe System Installation and User's Guide*, Publication No. `H-2000-5142-02-A` (Chapters 4 and 5).
     * **Production Machine Evidence:** In-tree sample program [`docs/evidence/614-dnm5700-sample.nc`](file:///G:/My%20Drive/Z_Temp_Trash/tangency-labs/Camcraft/docs/evidence/614-dnm5700-sample.nc) provided by community user `@spikeypineapple` on Issue #614 comment [6764944619](https://github.com/NeoTech/Camcraft/issues/614#issuecomment-6764944619) confirming real-world DNM 5700 (Fanuc 0i-MF) syntax.
     * Clean-room protocol: original implementation specced from documentation and verified machine output; no vendor script text or proprietary templates consulted.
   - Manifest declares `capabilities = 1` (milling), `coolantModes = true`, `afterModes = true`.
   - Unverified manifest flags (`axisInversion` and `probeAxisInversion`) were removed until corresponding mirrored-stream equivalence tests are developed.

2. **Through-Spindle Coolant (TSC) & Strict Refusal:**
   - Sourced from DN Solutions Operation Manual `MV0009OPE241_v0` Part 5 §3 (p. 180) and Part 6 §2.6 (p. 217): `M88` High-Pressure TSC ON, `M89` High-Pressure TSC OFF, `M08` Flood ON, `M07` Mist ON, `M09` All Coolant OFF.
   - Machine coolants table in `machine.coolants` is truth: when `through-tool` coolant is requested, the post queries the machine definition for a declared through-tool/through-spindle code (`M88`).
   - If the machine lacks a through-tool coolant entry and `useM8ForTSC` is not set, the post strictly refuses by name (`the machine states no through-tool coolant code - pick another coolant for the operation or add it to the machine`) rather than guessing unverified defaults.
   - A configurable option `useM8ForTSC` allows shops whose DNM coolant plumbing triggers TSC via `M08` to post `M08` instead.

3. **Rotary Table Clamping / Unclamping (A-Axis Brake on Separate Blocks):**
   - Sourced from DN Solutions Operation Manual `MV0009OPE241_v0` Part 5 §3 (p. 180) (`M10`: 4th-axis brake clamp, `M11`: 4th-axis brake unclamp) and Part 8 §3 (p. 232) Alarm 2323 "4TH AXIS CLAMP/UNCLAMP ALARM" (verifies clamp/unclamp status timer, confirming separate block sequencing).
   - `M10` and `M11` are strictly emitted on their own separate blocks, never inline with axis motion. This prevents control races where Fanuc controls execute M-codes prior to or during motion.
   - On rapid indexing (`G00 A...`), `M11` unclamp executes on its own line before motion; after positioning completes, `M10` clamp executes on its own line before cutting.
   - Continuous 4th-axis contouring unclamps before motion and restores clamping at section end.
   - **No Machine-Name Sniffing:** Dropped heuristic sniffing on machine names containing `"doosan"` or `"dnm"`. Rotary brake codes require explicit declaration via post options or properties (`rotaryClamp`/`rotaryUnclamp` or `rotaryBrake: true`), or explicitly declared on the machine definition. If rotary motion (`A`) is commanded without declared rotary brake codes, the post strictly refuses by name (`rotary clamp/unclamp codes not declared for this machine or post options`).

4. **High-Speed Smoothing (Fanuc AI Contour Control / AICC):**
   - Sourced from FANUC Series 0i-MODEL F Operator's Manual (`B-64604EN-2/01`):
     * Section 3.3 (p. 19/33, p. 20/34), Section 14.1 (p. 377-386): Machining Condition Selecting Function commands `G5.1 Q1 Rx` (where `x` is condition level 1 to 10; R1 roughing/speed to R10 fine finishing/precision) and cancels with `G5.1 Q0`.
     * Control option requirements: Fanuc AICC I (A02B-0340-S603) or AICC II (A02B-0340-S605).
     * Section 3.3.2 (p. 22/36): Canned drilling cycles (`G73`, `G81-G89`, `G74`, `G76`) cannot execute with AICC active (`G80` required, `G5.1 Q0` required).
   - Aligned with real Doosan DNM 5700 sample program ([`docs/evidence/614-dnm5700-sample.nc`](file:///G:/My%20Drive/Z_Temp_Trash/tangency-labs/Camcraft/docs/evidence/614-dnm5700-sample.nc)): default output is `G5.1 Q1 R5` on entry, and `G5.1 Q0` on cancel.
   - Strict bound enforcement: requests with `smoothingLevel` outside the valid 1..10 range are refused by name (`smoothing R level must be between 1 and 10`).

5. **Renishaw Inspection Plus Probing Workflow:**
   - Sourced from Renishaw MP700 Probe System Installation and User's Guide (Publication No. `H-2000-5142-02-A`):
     * Chapter 4 (p. 4-2 to 4-17): Optical machine start `G65 P9832` (probe optical turn on) and `G65 P9833` (probe optical spin-off / sleep).
     * Chapter 5: `G65 P9810` protected positioning, `G65 P9811` surface measurement, `G65 P9814` bore/boss measurement.
   - Confirmed by DNM 5700 production sample program ([`docs/evidence/614-dnm5700-sample.nc`](file:///G:/My%20Drive/Z_Temp_Trash/tangency-labs/Camcraft/docs/evidence/614-dnm5700-sample.nc)).
   - When `probingStyle: "inspection-plus"` is enabled:
     * `G65 P9832` is emitted on its own block prior to any probe positioning moves.
     * `G65 P9810` is used for protected positioning moves.
     * `G65 P9811` is used for surface touch measurement.
     * `G65 P9833` is emitted on its own block at the end of the probing sequence before spindle stop/retract.
   - Unknown probing styles are strictly refused by name.

6. **Chip Conveyor / Auger:**
   - Sourced from DN Solutions Operation Manual `MV0009OPE241_v0` Part 5 §3 (p. 180) and Part 5 §2.13 (p. 137) (`M24`/`M25` standard, optional custom packages using `M31`/`M33`).
   - Controlled via `chipConveyor` option.
   - Codes must be declared via post options (`chipConveyorStart` / `chipConveyorStop`) or machine definition; if chip conveyor is requested without declared codes, the post strictly refuses by name (`chip conveyor codes not declared for this machine or post options`). Hardcoded defaults are prohibited.

7. **Machine Definition `assets/machines/doosan-dnm.mch`:**
   - Registered in `posting::MACHINES` as `"Doosan DNM 5700"` to make it available in the machine library and picker.
   - Updated with cited DN Solutions DNM 4500/5700/6700 Specification Catalog numbers:
     * Travels: X 1020 mm, Y 570 mm, Z 510 mm; Rapids: X 36 m/min, Y 36 m/min, Z 30 m/min; Max feed: 15 m/min.
     * Spindle: 40-12,000 RPM, 18.5 kW, rigid tapping.
     * Tooling: 30-tool ATC, max tool length 300 mm, max tool diameter 80 mm.
     * Rotaries: declared 4th-axis A rotary table rotating about X [1, 0, 0].
     * Coolants: M8 Flood, M7 Mist, M88 Through-tool.

### Why

- The Doosan DNM series with Fanuc 0i-MF/31i-M controls is a predominant vertical machining center standard in industry.
- Emitting separate blocks for M10/M11, standard `G5.1 Q1 R5` smoothing, and `G65 P9832`/`P9833` optical probe control directly matches actual machine shop execution on real DNM machines.
- Refusing undeclared machine codes by name upholds the Prime Directive ("the machine is truth; never invent or guess M-codes").

### What was considered and dropped

- **Inline M10 on motion blocks (`G00 ... A90 M10`):** Dropped because Fanuc controls can evaluate M-codes before or during axis motion, risking brake engagement while the rotary table is rotating. Refactored to separate lines: `M11`, motion block, `M10`.
- **Defaulting to M88 when machine definition lacks TSC:** Dropped per clean-room protocol and maintainer review. If the machine states no TSC code and `useM8ForTSC` is not set, the post refuses by name.
- **Machine-name sniffing for rotary clamp codes:** Dropped. Previously, the post inspected `machine.name` / `machine.model` for `"doosan"` or `"dnm"` and silently injected `M10`/`M11`. This was removed so that rotary clamp codes require explicit declaration (`rotaryBrake: true` or `rotaryClamp`/`rotaryUnclamp`), matching how chip conveyor is handled.
- **Unbounded G05.1 Q1:** Dropped in favor of `G5.1 Q1 Rx` with strict range checking (1..10) and default R5 matching the proven machine sample.
- **Unverified manifest inversion flags:** Dropped until mirrored-stream test goldens are established.

### Cost to change later

- Minimal: all post behavior is isolated in `assets/posts/doosan-dnm.cps` and comprehensively tested in `crates/app/tests/doosan_dnm_post.rs` and `conformance.rs`.
