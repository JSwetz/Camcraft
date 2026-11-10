# Decision Record: #614 - Doosan DNM + Fanuc Post Processor

## #614 - Doosan DNM Post Processor dialect on the Fanuc base

### What was decided

1. **New Post Script `assets/posts/doosan-dnm.cps`:**
   - Base dialect: Fanuc 0i / 31i-M (RS-274 / ISO 6983 conforming).
   - Provenance: Clean-room implementation based on published Fanuc 0i/31i operator programming manuals and Doosan Infracore / DN Solutions DNM series programming guides. No vendor script text or proprietary templates were consulted or paraphrased.
   - Declares `capabilities = 1` (milling), `coolantModes = true`, `afterModes = true`, `axisInversion = true`, and `probeAxisInversion = true`.

2. **Through-Spindle Coolant (TSC):**
   - Implements `through-tool` coolant support.
   - When a machine definition specifies a custom code for `through-tool` or `through-spindle`, that code is used.
   - By default, Doosan DNM uses high-pressure TSC M-code `M88` (with `M89` and `M09` on shut-off).
   - A configurable option `useM8ForTSC` allows shops whose DNM coolant plumbing triggers TSC via `M08` to post `M08` instead.

3. **Rotary Table Clamping / Unclamping (A-Axis Brake):**
   - Doosan DNM 4th-axis rotary table (A-axis) requires explicit pneumatic/hydraulic brake unclamp (`M11`) before indexing and clamp (`M10`) before 3-axis cutting.
   - On rotary index positioning (`G00 A...`), the post commands `M11` prior to rotation, followed by `M10` brake clamping after positioning.
   - Continuous 4th-axis contouring leaves the clamp off until the section end, where `M10` restores the clamped state.

4. **High-Speed Smoothing (Fanuc AI Contour Control / AICC):**
   - Configurable smoothing support (`G05.1 Q1` / `G05.1 Q0`).
   - Follows Fanuc safety interlocks: `G05.1 Q0` is commanded prior to canned drilling cycles (`G73`, `G81-G89`), probing moves (`G31`), tool changes (`M06`), and program close (`M30`).
   - Automatically re-engages `G05.1 Q1` on subsequent linear/circular cutting moves.

5. **Chip Conveyor / Auger:**
   - Optional chip conveyor control: commands `M31` (chip conveyor forward) at operation start and `M33` (chip conveyor stop) at program end.

6. **Probing Compatibility:**
   - Full support for standard Fanuc `G31` macro-B skip moves, skip signal verification (`#5061-#5063`), and tolerance/contact alarms (`#3000`).
   - Optional Renishaw Inspection Plus format (`G65 P9810` protected positioning, `G65 P9811` surface measurement) via `probingStyle: "inspection-plus"`.

7. **Machine Definition `assets/machines/doosan-dnm.mch`:**
   - Added Doosan DNM 5700 machine definition (X: 1020 mm, Y: 570 mm, Z: 510 mm, 12,000 RPM, rapid 36/36/30 m/min, coolants M8 flood, M7 mist, M88 through-tool).

### Why

- The Doosan DNM series is one of the most widely used CNC vertical machining centers in industry.
- Providing standard Fanuc G-code with Doosan-specific M-codes (TSC `M88`, rotary brake `M10`/`M11`, AICC `G05.1 Q1`, conveyor `M31`/`M33`) enables drop-in compatibility without operator post editing.
- Option flags (`useM8ForTSC`, `probingStyle`, `chipConveyor`) address shop-to-shop variations in DNM machine options cleanly.

### What was considered and dropped

- **Hardcoding M8 for all TSC:** Some DNM shops use standard M8 for through-spindle, while standard Doosan manuals specify M88/M89. Dropped in favor of defaulting to M88 with a `useM8ForTSC` option.
- **Unconditional G05.1 Q1 on every block:** Fanuc controls alarm if G05.1 Q1 remains active during canned drilling cycles (G81-G89) or G31 probing moves. Dropped in favor of modal deactivation before cycles/probing and reactivation on cutting moves.

### Cost to change later

- Minimal: all Doosan DNM parameters reside in `assets/posts/doosan-dnm.cps` and are covered by `crates/app/tests/doosan_dnm_post.rs`.
