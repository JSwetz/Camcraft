---
name: camcraft-mcp
description: Guide and reference for driving CamCraft headlessly via its Model Context Protocol (MCP) server (camcraft-cli mcp). Covers the agent workflow, tool table, parameter DTO shapes, undo/redo, error handling, and known gaps.
---

# CamCraft MCP Agent Guide

CamCraft exposes a Model Context Protocol (MCP) server on stdio (`camcraft-cli mcp`, #128), allowing an AI agent to drive a complete CAM programming session headlessly: importing models, defining setups and stock, selecting tools, creating and editing operations, selecting geometric features, simulating/verifying, and posting NC programs.

## Architecture & Principles

- **Transport & Protocol:** Standard JSON-RPC 2.0 over newline-delimited stdio (`initialize`, `tools/list`, `tools/call`, `ping`).
- **Everything Is a Command:** The MCP server owns one headless document (`crates/app/src/script/mod.rs:Env`) and one command `History`. Every mutation is dispatched through a concrete undoable command (`env.history.execute`), guaranteeing that `undo` and `redo` work headlessly exactly as in the GUI.
- **Refusals as Data:** Failures return `{ isError: true, content: [{ "type": "text", "text": "<tool_name>: <reason>" }] }` naming the exact parameter or physical violation. Tool handlers catch panics defensively and never crash the session.

---

## Standard Agent Workflow

```
       1. Import / Open
   (import_model / open_project)
              │
              ▼
    2. Stock, Setup & WCS
 (set_stock, add_setup, set_wcs,
  set_work_offset, load_machine)
              │
              ▼
         3. Fixtures
 (add_fixture, move_fixture)
              │
              ▼
          4. Tooling
 (add_tool, import_tool_library)
              │
              ▼
   5. Operations & Geometry
 (add_operation, list_features,
  set_operation_geometry, edit_operation)
              │
              ▼
         6. Generate
         (generate)
              │
              ▼
     7. Verify & Inspect
 (verify, get_fixture_findings,
         render_stock)
              │
              ▼
          8. Post
          (post)
```

### 1. Document Lifecycle & Inspection
- `open_project { "path": string }`: Load an existing `.camproj` file.
- `save_project { "path"?: string }`: Save the current document to disk.
- `import_model { "path": string, "name"?: string }`: Import a mesh model (STEP, STL, OBJ).
- `get_job {}`: Dump the entire document state (setups, stock, WCS, models, tools, operations, status).
- `undo {}` / `redo {}`: Invert or replay the previous mutating command.

### 2. Stock, Setup & Machine
- `set_stock`: Define active setup stock as a box (`{ "kind": "box", "box": [x0, y0, z0, x1, y1, z1] }`), cylinder (`{ "kind": "cylinder", ... }`), or model copy (`{ "kind": "model", "model": "part_name" }`).
- `add_setup { "name": string, "datum_from"?: string }`: Create a new setup.
- `activate_setup { "name": string }`: Switch active setup context.
- `set_wcs { "origin"?: [x, y, z], "stock_point"?: string, "offset"?: [dx, dy, dz], "rotation"?: [qx, qy, qz, qw] }`: Place the work coordinate system origin and orientation.
- `set_work_offset { "offset": "G54" | "G55" | ... | null, "setup"?: string }`: Set the posted G-code work offset coordinate word.
- `load_machine { "name"?: string, "setup"?: string }`: Mount a machine configuration (`.mch` or built-in preset) for document-wide or setup-specific kinematics.

### 3. Fixtures & Workholding
- `add_fixture { "model": string, "setup"?: string }`: Tag an imported model as fixture steel.
- `move_fixture { "name": string, "position": [x, y, z], "rotation"?: [qx, qy, qz, qw] }`: Move a vise/clamp body into position.
- `get_fixture_findings {}`: Query live fixture collision checks against generated toolpaths.

### 4. Tool Management
- `list_tool_catalog {}`: List built-in tool definitions.
- `add_tool { "name": string, ... }`: Add a tool from catalog name or custom parameter definition.
- `import_tool_library { "path": string }`: Import tools from a `.camtools` library file.
- `export_tool_library { "path": string }`: Export document tools to a `.camtools` library file.

### 5. Operation Creation & Feature Selection
- `list_operation_types {}`: Inspect all supported 2D and 3D operation types, their parameter schemas, defaults, and compatible tool types.
- `add_operation { "kind": string, "name": string, "tool": string, "model"?: string, "params"?: object }`: Add an operation to the active setup.
- `edit_operation { "name": string, "params": object }`: JSON merge-patch operation parameters (e.g. `stepdown`, `stepover`, `feed`, `spindle_speed`).
- `list_features { "model": string }`: List pickable faces, loops, edge chains, and vertices with stable IDs.
- `set_operation_geometry { "name": string, "picks": [id1, id2, ...] }`: Bind specific geometric features to machine.
- `set_operation_area { "name": string, "containment": "inside" | "outside" | "center", "offset"?: number }`: Configure tool containment boundaries.
- `detect_holes { "name": string }`: Automatically populate hole positions for Drill, Bore, or Thread operations from the source model.

### 6. Generation, Simulation & NC Posting
- `generate { "name"?: string }`: Generate toolpaths for a named operation or all stale operations.
- `verify { "name"?: string }`: Check toolpaths against safety rules (spindle speed, rapids touching material, shaft collision, machine limits).
- `render_stock { "width"?: number, "height"?: number, "view"?: "iso" | "top" }`: Render simulated stock to a PNG image with rest/gouge color shading.
- `post { "post"?: string, "path"?: string }`: Post-process verified toolpaths into machine NC code via `.cps` post processor.

---

## Refusals and Error Format

Every tool returns structured responses. When an action cannot be performed safely:

```json
{
  "content": [
    {
      "type": "text",
      "text": "set_stock: stock dimensions exceed machine envelope (X 500 > 400)"
    }
  ],
  "isError": true
}
```

Agents must inspect `isError` on each step. Never assume an operation succeeded without checking `isError: false`.

---

## Rule for Contributors

Per `AGENTS.md` ("Everything Is a Command"):
> A PR that adds a user-facing command also adds its MCP tool (and script verb where it fits) in `crates/app/src/mcp/`, or files a `source:agent` follow-up issue `MCP: <command>` and names it in the PR body.
