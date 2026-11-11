# Decision Record: #614 - Doosan DNM + Fanuc Post Processor

## #614 - Doosan DNM Post Processor dialect on the Fanuc base

### What was decided

1. **Post Script `assets/posts/doosan-dnm.cps`:**
   - Base dialect: Fanuc 0i / 31i-M (RS-274 / ISO 6983 conforming).
   - Provenance: Clean-room implementation based on published Fanuc 0i-MODEL F operator programming manuals, Renishaw MP700 / Inspection Plus manuals, Doosan Infracore / DN Solutions DNM series programming guides, and the real Doosan DNM machine sample program from PR #665 / Issue #614. No vendor script text or proprietary templates were consulted or paraphrased.
   - Manifest declares `capabilities = 1` (milling), `coolantModes = true`, `afterModes = true`.
   - Unverified manifest flags (`axisInversion` and `probeAxisInversion`) were removed until corresponding mirrored-stream equivalence tests are developed.

2. **Through-Spindle Coolant (TSC) & Strict Refusal:**
   - Sourced from Doosan DNM Instruction / Operation Manual Table 4-1 (M88 High-Pressure TSC ON, M89 High-Pressure TSC OFF, M08 Flood ON, M09 All Coolant OFF).
   - Machine coolants table in `machine.coolants` is truth: when `through-tool` coolant is requested, the post queries the machine definition for a declared through-tool/through-spindle code (`M88`).
   - If the machine lacks a through-tool coolant entry and `useM8ForTSC` is not set, the post strictly refuses by name (`the machine states no through-tool coolant code - pick another coolant for the operation or add it to the machine`) rather than guessing unverified defaults.
   - A configurable option `useM8ForTSC` allows shops whose DNM coolant plumbing triggers TSC via `M08` to post `M08` instead.

3. **Rotary Table Clamping / Unclamping (A-Axis Brake on Separate Blocks):**
   - Sourced from Doosan DNM Instruction Manual Table 4-1 (`M10`: 4th-axis brake clamp, `M11`: 4th-axis brake unclamp).
   - `M10` and `M11` are strictly emitted on their own separate blocks, never inline with axis motion. This prevents control races where Fanuc controls execute M-codes prior to or during motion.
   - On rapid indexing (`G00 A...`), `M11` unclamp executes on its own line before motion; after positioning completes, `M10` clamp executes on its own line before cutting.
   - Continuous 4th-axis contouring unclamps before motion and restores clamping at section end.
   - If rotary motion (`A`) is commanded on a machine or setup without declared rotary brake codes, the post strictly refuses by name (`rotary clamp/unclamp codes not declared for this machine or post options`).

4. **High-Speed Smoothing (Fanuc AI Contour Control / AICC):**
   - Sourced from FANUC Series 0i-MODEL F Operator's Manual (B-64604EN-2/01):
     * Section 3.3 (p. 19/33, p. 20/34), Section 14.1 (p. 377-386): Machining Condition Selecting Function commands `G5.1 Q1 Rx` (where `x` is condition level 1 to 10; R1 roughing/speed to R10 fine finishing/precision) and cancels with `G5.1 Q0`.
     * Control option requirements: Fanuc AICC I (A02B-0340-S603) or AICC II (A02B-0340-S605).
     * Section 3.3.2 (p. 22/36): Canned drilling cycles (`G73`, `G81-G89`, `G74`, `G76`) cannot execute with AICC active (`G80` required, `G5.1 Q0` required).
   - Aligned with real Doosan DNM sample program: default output is `G5.1 Q1 R5` on entry, and `G5.1 Q0` on cancel.
   - Strict bound enforcement: requests with `smoothingLevel` outside the valid 1..10 range are refused by name (`smoothing R level must be between 1 and 10`).

5. **Renishaw Inspection Plus Probing Workflow:**
   - Sourced from Renishaw MP700 Probe System Installation and User's Guide (Publication No. H-2000-5142-02-A):
     * Chapter 4 (p. 4-2 to 4-17): Optical machine start `G65 P9832` (probe optical turn on) and `G65 P9833` (probe optical spin-off / sleep).
     * Chapter 5: `G65 P9810` protected positioning, `G65 P9811` surface measurement, `G65 P9814` bore/boss measurement.
   - When `probingStyle: "inspection-plus"` is enabled:
     * `G65 P9832` is emitted on its own block prior to any probe positioning moves.
     * `G65 P9810` is used for protected positioning moves.
     * `G65 P9811` is used for surface touch measurement.
     * `G65 P9833` is emitted on its own block at the end of the probing sequence before spindle stop/retract.
   - Unknown probing styles are strictly refused by name.

6. **Chip Conveyor / Auger:**
   - Sourced from Doosan DNM Instruction Manual Table 4-1 (`M31` chip conveyor forward, `M33` chip conveyor stop).
   - Controlled via `chipConveyor` option.

7. **Machine Definition `assets/machines/doosan-dnm.mch`:**
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
- **Unbounded G05.1 Q1:** Dropped in favor of `G5.1 Q1 Rx` with strict range checking (1..10) and default R5 matching the proven machine sample.
- **Unverified manifest inversion flags:** Dropped until mirrored-stream test goldens are established.

### Cost to change later

- Minimal: all post behavior is isolated in `assets/posts/doosan-dnm.cps` and comprehensively tested in `crates/app/tests/doosan_dnm_post.rs` and `conformance.rs`.
