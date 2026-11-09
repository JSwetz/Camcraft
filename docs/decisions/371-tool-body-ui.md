## #371 - tool bodies in the tool editor, the viewport and the simulation (milling)

- **Chosen: a Body column in the tool editor window** (third column,
  beside the profile preview): Import body… / Clear body, the axis
  (six model-direction presets, or a picked face normal shown as
  "picked (x, y, z)"), the tip offset (the model origin's height above
  the tip, the number `SetToolBody` takes), Pick tip / Pick axis, a
  placement note, and Apply body. Why a column and not rows under the
  preview: the fields column is already ~600 px tall, and stacking put
  Apply body below a 720 px screen. The window's default anchor moved
  left by the column's width so it stays centred (and clear of the
  library pane's Delete button). Dropped: a separate body window (one
  more floating surface to keep in sync with the editor's tool).
- **Picking happens in a small 3D preview inside the editor**, painted
  with egui (flat-shaded, depth-sorted triangles, orthographic, drag to
  orbit), not in the main viewport. Why: the main viewport draws the
  document in world space; picking a mesh that is not yet attached to
  anything there would need a temporary scene part, a placement for it,
  and a pick mode that competes with the face/edge/vertex picks. The
  in-editor preview needs no GPU, runs under the test harness, and its
  ray (`View::ray`) feeds the existing ray/triangle crossing
  (`app::pick::soup_hit`). Over 30 000 facets it paints every n-th facet;
  picks always read them all.
- **Pick tip** takes the hit facet's corner nearest the click and sets
  `offset = -dot(corner, axis)`, so that corner lands at height 0 (the
  tip). Snapping to a corner, not the raw hit, gives a repeatable number
  on a coarse mesh. **Pick axis** takes the clicked facet's outward
  normal: the user clicks a face looking toward the spindle (the
  holder's top). Dropped: snapping a picked normal to the nearest preset
  - a vendor body modelled on a tilted frame would then be unalignable.
- **Picks and orbit are ephemeral; Apply body is the one command.** The
  draft (`ToolBodyUi`, `state_tool_body.rs`) is pane state like
  `ToolUi`. Apply body queues `CommandKind::SetToolBody`, which builds
  `SetToolBody` (place, envelope, tool fit) and either executes it (one
  undo step) or refuses on the Tool pane by name ("tool body not set:
  ..."), leaving the draft as it was for correction. Clear body on a
  landed body is the same command with `None` (undoable); on an
  unapplied import it only drops the draft. The draft re-mirrors when
  the world body changes by identity (`Arc::ptr_eq` on the facets):
  Apply, Undo, Redo or Open re-mirror, while an unrelated edit (Apply
  on the tool numbers) keeps an unapplied import. A landed body
  re-edits in its tool frame (+Z, offset 0), which re-applies to the
  same facets; the original model frame is not stored (#368 stores the
  placed facets only).
- **Live placement note:** the draft's placement runs through
  `tool_body_view::preview` (the same `place` + `build` the command
  runs, cached on mesh/axis/offset) and the editor shows either "placed:
  foot .. to top .." or the envelope's refusal before Apply. The profile
  preview overlays the placed body's revolved envelope bands in amber
  over the cylinders. A tool with no body draws exactly as before.
- **Simulation:** while the transport is up, the running operation's
  tool body is posed at the playhead (`tool_body_view::posed`: tool
  frame to the frame's tip and axis by the shortest rotation, Rodrigues;
  the turn about the axis is free because the body is judged by its
  revolved envelope) and drawn as one more model part - depth-tested
  and shaded by the existing GPU pipeline. It reads the cached timeline
  only when it is this document version's, so it never re-walks the
  program per frame. (First cut: without a body nothing was added -
  superseded by the follow-up below, which draws the tool and its
  holder cylinder.)
- **Hands-on follow-up: the simulation always draws the running
  tool.** The first cut drew the body alone; on the user's document the
  collar floated a shank length (55 mm) above the axis marker. Now the
  tool at the playhead is drawn whole, posed by one function into one
  part: cutter and shank revolved from `Tool::profile`, then
  - with a body, the body (the shank carried up to its foot if it stands
    clear - the same gap the fixture laws fill with shank);
  - without a body (user re-check: "no body shows only the marker"),
    the holder cylinder from the tool's own numbers, Ø holder_d ×
    holder_len on the shank top - the `HolderStack::cylinder` reading
    the collision laws judge, so the picture is the verdict's.
  The lathe insert kinds (Turning, Groove, Thread, Form) keep the axis
  marker only: their `profile` is a stand-in envelope (their truth is in
  the work's meridian), and spinning it would draw a tool that is not
  there. Briefly Frame also took the posed tool in; that was reverted on
  the user's word (zoom out to see a tall holder). Not done:
  auto-framing on Simulate; a static tool marker outside playback (none
  exists).
- **Preview zoom (user request):** wheel or pinch (Ctrl+wheel) over the
  3D body preview zooms about the cursor (log2 zoom clamped to
  [-2, 5], i.e. 1/4x to 32x of the fit view; the pan keeps the point
  under the cursor fixed). Double-click or the Fit button resets orbit,
  zoom and pan. View state only, never a command.
- **File read is synchronous** on the UI thread (`tool_body_view::load`,
  the model importer). A holder file is small next to a part; the
  worker-thread import path is wired to `ImportModel` and would need a
  second result kind. Revisit if users import heavy assemblies.
- **App seam:** everything the UI needs from a body lives in
  `app::tool_body_view` (load, presets, preview, pick, tip offset,
  posing, body of an operation's tool); the UI has no geometry of its
  own beyond the preview's view math.
- **Snapshot:** the tool editor's pixels change (the new column).
  Pixel snapshots are release-lane (#255); `egui_tool_editor.png` is not
  regenerated here. Structural tests pin the flow instead.
- **Not in this slice:** lathe holders (#369), the spun holder-profile
  editor (#93), ISO 13399 frames (#370), a rotation about the axis for
  display, and drawing a body outside the simulation.
