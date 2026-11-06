# CamCraft

A CAM system that turns 3D models into NC programs (G-code) for CNC milling
machines and lathes. Written in Rust on a Flecs ECS core with an egui
interface.

CamCraft is a clean-room implementation of HSMWorks' user-visible behavior:
its operation types, parameters and workflow, specced from published
documentation and general machining knowledge. "HSMWorks" is a trademark of
Autodesk; this project is independent and unaffiliated. No HSMWorks code,
text, assets or post processors were used.

> **Status: pre-1.0, for testing.** Every toolpath is verified against the
> part, the stock and the fixtures before it can be posted, and anything that
> cannot be proven safe is refused by name. Still, check every program in
> your machine's own simulation and dry-run it before cutting metal.

## What it does

- **Import** STL, OBJ and STEP models; repair meshes (weld vertices, fill
  holes).
- **Set up** stock, work offsets (G54–G59), fixtures and machines (mill,
  5-axis, lathe, mill-turn).
- **2D milling:** face, contour (with G41/G42 compensation), pocket,
  adaptive clearing, slot, trace, engrave, chamfer, circular, bore and
  thread milling.
- **3D milling:** adaptive roughing per level, waterline, parallel,
  scallop, radial, spiral, morph, flow, pencil and steep/shallow finishing.
- **Drilling:** spot, drill, peck, deep, tap and bore cycles, with hole
  recognition.
- **Multi-axis:** 3+2 indexing, rotary contour/parallel/offset, 5-axis
  contour and swarf.
- **Turning:** facing, profile roughing and finishing, grooving, parting,
  threading, boring and axial drilling.
- **Area control:** pick faces, edges or loops, then set containment,
  offset, extension and extra or open boundaries on every operation that
  has an area.
- **Verify and simulate:** a solid-law verifier (gouge, rest, rapid and
  fixture checks), stock simulation with playback, and a deviation view.
- **Post:** Fanuc, Haas, Mazak, Sinumerik, Heidenhain and Fanuc Lathe,
  written as scriptable `.cps` post processors. The setup sheet and tool
  list print alongside.

## Download

Windows builds are attached to each
[GitHub release](https://github.com/NeoTech/Camcraft/releases). Unzip and run
`camcraft-ui.exe`. `TESTING.md` in the zip is a short hand-test checklist
for what changed in that release.

## Build from source

Requires Rust 1.90 or newer (edition 2024).

```sh
cargo build --release -p camcraft-ui        # the desktop app
cargo run   --release -p camcraft-ui        # build and start it
cargo build --release -p camcraft-app --bin camcraft-cli   # headless CLI
cargo build --release -p camcraft-post --bin post          # standalone post processor
```

Windows is the primary platform. The core crates also build and test on
Linux and macOS, and the post engine compiles for `wasm32-unknown-unknown`.

## Workspace layout

| Crate | Role |
|---|---|
| `crates/geometry` | Meshes, slicing, 2D regions, mesh repair |
| `crates/cl` | The cutter-location (CL) move stream |
| `crates/tools` | Tool library and tool laws |
| `crates/motion` | Toolpath generators (2D, 3D, drilling, multi-axis, turning) |
| `crates/sim` | Stock simulation and the verifier |
| `crates/post` | The `.cps` post-processor engine |
| `crates/app` | Document model, commands (undo/redo), operations, posting |
| `crates/ui` | The egui desktop interface |
| `crates/kernel` | Shared kernel types |

`assets/` holds the post processors, machine definitions, UI icons, fonts
and themes. `testdata/` holds the test models.

## Testing

```sh
cargo test -p camcraft-app -p camcraft-motion -p camcraft-sim \
           -p camcraft-geometry -p camcraft-cl -p camcraft-tools -p camcraft-post
cargo test -p camcraft-ui                        # run from crates/ui
cargo test --release -p camcraft-app -p camcraft-motion -- --ignored   # slow verdicts
```

Golden toolpath and G-code outputs live under each crate's `tests/golden/`.
CI runs the full suite on Windows and the portable suites on Linux and
macOS; see `.github/workflows/`.

## Project documents

- `ROADMAP.md`: milestones, what is done and what is owed, and the
  release train.
- `RECOMMENDED.md`: the decision ledger, with what was chosen, why, and
  what was dropped.
- `AGENTS.md`: engineering rules (clean-room protocol, the never-crash /
  never-gouge directive, layering, testing).
- `RELEASE_NOTES.md`, `TESTING.md`: the current release.

## Reporting issues

Use the issue templates on GitHub: **Toolpath / path planning bug** for
anything about the machine's motion, **UI / UX bug** for the interface, and
the general bug and feature templates for the rest. If a path would gouge or
collide and the app did not warn you, please say so. Those reports matter
most.

## License

MIT. See [`LICENSE`](LICENSE). Third-party notices are in
`assets/ui/LICENSES.md`.
