## #370 - Tool bodies: align a vendor STEP by its tool coordinate systems (ISO 13399 / GTC)

- **What was decided:**
  - Auto-alignment wires the ISO 13399 `step_csys` coordinate system reader into the tool body pipeline (`ToolBodySpec`, `SetToolBody`, `tool_body_view`, and editor UI):
    - `ToolCsysReport::align_tool_body()` extracts the tool axis (`cip.z`, or `csif.z` if CIP is absent) and calculates the tip offset (`-dot(origin, axis)`) so that the cutting reference point sits at $Z = 0$ in the tool frame, pointing up into the spindle along $+Z$.
    - `ToolBodySpec::from_step_auto(&path)` reads both the mesh facets and coordinate systems from a vendor STEP file, aligning it with zero manual offsets.
    - `SetToolBody::from_step_auto(&world, tool_name, &path)` attaches the auto-aligned tool body as one undoable command.
    - `tool_body_view::load_aligned(&path)` returns `LoadedBody` carrying the auto-alignment result or fallback reason, preserving backwards compatibility for `tool_body_view::load`.
    - In the tool editor (`ToolBodyUi`), importing a STEP file with ISO 13399 coordinate systems automatically sets the axis and tip offset with the note "aligned by ISO 13399 coordinate systems"; importing a file without them or with degenerate/ambiguous coordinate systems falls back to default numeric placement ([0, 0, 1] axis, 0 offset) with a note naming why.
- **Why:**
  - ISO 13399-303 §3.3 specifies CIP as the coordinate system located at the cutting part of the tool with its Z-axis pointing toward the PCS (collinear with spindle axis).
  - This allows vendor STEP packages (Sandvik, Kennametal, etc.) containing standard tool coordinate systems to snap into place without requiring the user to manually measure or enter offsets or pick vertices.
  - Degenerate, ambiguous, or absent coordinate systems fall back safely by name without panicking or creating invalid tool bodies, adhering to the Prime Directive.
- **Considered and dropped:**
  - Falling back to PCS or MCS when CIP is missing: dropped because PCS / MCS define mounting and part references, not the cutting point; attempting to deduce the tip from them without CIP would be heuristic guessing, which is forbidden.
  - Automatically guessing axis from bounding boxes: dropped, numeric fallback with user review is safer and strictly deterministic.
