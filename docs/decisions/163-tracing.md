## #163 - terminal logging through `tracing` (2026-11-07, first slice)

- **Chosen:** `tracing` + `tracing-subscriber` (fmt + env-filter, stderr),
  installed once by `camcraft_app::logging::init()` from both binaries
  (`camcraft`, `camcraft-cli`). Filter from `CAMCRAFT_LOG`, else
  `RUST_LOG`, else `info` with the GPU stack (`wgpu_core`, `wgpu_hal`,
  `naga`, `winit`) at `warn`; `tracing-log` bridges their `log` records.
- **Seams instrumented:** project save/open (path, bytes, or why not),
  NC post (path, bytes, ops, lines, ms - or "NC program not written:
  <why>"), every generation (op name span, kind, moves, warnings, ms, or
  the refusal at warn; the job carries the op name so batch runs log it
  too), command dispatch (`execute command=<Type>` at debug, undo too).
- **Not in this slice:** converting the ~185 `println!`/`eprintln!`, and
  per-limit "which budget fired" events inside generators (their refusal
  text already rides the warn line) - follow-up work on #163.
- **Dropped:** `log` + `env_logger` (no spans, so batch generations
  could not carry their op name); a log file (the issue asks for the
  launching terminal).
