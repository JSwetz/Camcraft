## #370 - Read a vendor tool STEP's ISO 13399 coordinate systems

- **Names, confirmed from public ISO/TS 13399 preview pages before any
  code** (the free iTeh sample PDFs of the published Technical
  Specifications; behaviour only, no text carried into the UI):
  - **PCS** - "primary coordinate system": the item's right-handed
    reference system. ISO/TS 13399-303 §3.2, -401 §3.2, -405 §3.2.
  - **MCS** - "mounting coordinate system": where a tool item virtually
    mounts onto an adaptive item, at the start of its protruding length.
    ISO/TS 13399-303 §3.2, -405 §3.3 (-401 §3.5 by title).
  - **CIP** - "coordinate system in process": the coordinate system at
    the cutting part; its z axis points to the PCS, collinear with the
    PCS z axis. ISO/TS 13399-303 §3.3.
  - **CSIF** - "coordinate system interface": an optional component
    interface that includes the CIP. ISO/TS 13399-303 §3.3.
  - **CSW / CSWx / CSWx_y** - "coordinate system workpiece side": one
    system (`CSW`), one per level (`CSW1`, `CSW2`, counted from the
    workpiece side toward +z) or several per level (`CSWx_y`, x the
    level, y the number counted counter-clockwise from three o'clock).
    ISO/TS 13399-401 §3.4, -405 §3.4.
  - **CRP** (cutting reference point) is a point, not a coordinate
    system (ISO/TS 13399-303 §3.5); not read here.

  Sources:
  - <https://cdn.standards.iteh.ai/samples/60275/61617f41de0a4c28a09b1e023984d459/ISO-TS-13399-303-2016.pdf>
    (Part 303, solid end mills)
  - <https://cdn.standards.iteh.ai/samples/60433/19d70fbbd8e047b9acbf2893472de6a3/ISO-TS-13399-401-2016.pdf>
    (Part 401, converting, extending and reducing adaptive items)
  - <https://cdn.standards.iteh.ai/samples/63601/cd0f5fbf46584959a48af592f6551940/ISO-TS-13399-405-2016.pdf>
    (Part 405, collets)
  - Placement semantics: ISO 10303-42 `AXIS2_PLACEMENT_3D` (location,
    axis, ref_direction, x = ref_direction projected normal to the axis).
  - A ToolsUnited page (<https://info.toolsunited.com/?p=28628>) was
    searched for the STEP side; it confirmed only "PCS" in prose, so
    nothing rests on it.

- **Chosen (scope, set by the coordinator mid-task):** the STEP side
  only. `geometry::import::step_csys::read_tool_csys(bytes)` returns
  every named `AXIS2_PLACEMENT_3D` as plain data (name, entity, role,
  origin in mm, unit z, unit x; y = z x x) plus a list of refusals.
  `CsysRole::classify` maps the ISO 13399 names above; any other name is
  `Other` (kept, never used for alignment). `find(role)` gives the one
  system with a role or a reason; `none_found()` gives the named reason
  a caller falls back to numeric placement. No tool-body component,
  command, project field, UI or placement into a tool frame: #368 owns
  the tool-body model and a later round wires this into it.
- **Rules taken:**
  - A placement counts as a coordinate system only when it has a name;
    `''` and `'NONE'` are surface placements and are skipped.
  - Names classify ignoring case and surrounding blanks (`pcs` is the
    PCS); nothing looser (`PCS_1`, `CSW_A`, `CSW1_2_3` are `Other`). Two
    names landing on one role (`PCS` and `pcs`) make `find` refuse as
    ambiguous rather than pick one.
  - A name written twice at the same placement (within 1e-9) is one
    system; written at different placements, every copy is refused
    ("written at 2 different placements (#a, #b)").
  - Absent axis is +Z and absent ref_direction is +X (ISO 10303-42
    defaults); a ref_direction parallel to the axis, a zero or non-finite
    direction, a non-finite origin, a dangling or wrong-kind reference
    or a non-3D point refuses that placement by name. Nothing panics.
  - Origins scale by the file's length unit (the #306 reader); a unit
    that can't be converted fails the whole read as the mesh import does.
- **Considered and dropped:**
  - Aligning a body now (tool-frame transform from PCS/CIP): needs
    #368's body model; doing it here would fork that model.
  - Exact-case matching only: the standard writes upper case; a
    lower-case export is the same name, and a looser match is not taken.
  - Composing assembly transforms (`ITEM_DEFINED_TRANSFORMATION`):
    coordinates are reported in their representation's frame. A vendor
    tool package is normally one part; two same-named systems in
    different frames surface as the ambiguity refusal, not a silent pick.
- **Not exercised:** a real vendor package (none is committed; the
  fixtures are hand-written), assembly-level placement, CRP.
- **Cost to change later:** Small. One module plus three fixtures; the
  wiring round adds a caller, not a change here.
