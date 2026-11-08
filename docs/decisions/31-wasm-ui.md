## #31 - App and UI on wasm: rung 1, the domain layer links (2026-11-08)

- **Chosen:** land the wasm build in rungs and ship rung 1 now.
  `camcraft-app` (flecs' C, boa, truck, clipper2, rayon) builds for
  `wasm32-unknown-unknown` and `camcraft-cli.wasm` links, through
  `web/build-app-wasm.sh`. The `wasm` job in `ci.yml` runs it, and like
  every job there it is `workflow_dispatch` only. Nothing in our sources or
  manifests changed, so native builds are byte-for-byte what they were.
- **How flecs gets there:** the pinned wasi-sysroot 34 (sha256-checked)
  provides libc headers for flecs.c, and its `libc.a` covers flecs' libc
  calls at link time. `-D__COSMOCC__` puts flecs on its generic-POSIX path
  and `-D__wasi__` lets the wasi headers load. flecs_ecs 0.2.2 and
  flecs_ecs_sys 0.2.1 do not compile for wasm32 as published (upstream
  lists wasm32-unknown-unknown support as in progress), so the script
  makes three edits to copies of them: `libc::FILE` becomes `c_void`, the
  pre-generated bindings change from `"C-unwind"` to `"C"` (flecs_ecs's
  wasm cfg expects `"C"`), and the cfg that gates off meta_fn_types'
  `WorldRef`/`Entity` import is removed. The copies are wired in with
  `cargo --config patch.crates-io...` for that build only, and the tracked
  `Cargo.lock` is restored afterwards. Each edit checks that its patch
  point is still there and refuses if a flecs bump moved it.
- **Considered and dropped:**
  - Vendoring patched flecs crates in the repo with a permanent
    `[patch.crates-io]`. That puts about 3 MB of third-party C in the tree
    and moves the native build onto a fork. Too big a call to make
    unattended.
  - A GitHub fork as a git dependency. It is outward-facing and it moves
    the native build too.
  - emscripten (`wasm32-unknown-emscripten`). It is a second Rust target,
    and wgpu and winit expect `-unknown-unknown` on the web.
  - Waiting for upstream flecs wasm support. Nothing newer than 0.2.2 has
    been published as of 2026-11-08.
- **What the next rungs owe (why #31 stays open):**
  1. **The web shell.** egui-winit 0.36.2 does not compile for wasm32: its
     `DroppedFile` impl lacks `bytes_async`. The browser shell therefore
     needs eframe's web runner, or the egui web glue, in place of
     winit + egui-winit. That means moving `shell_layout` out of
     `egui_app/shell.rs` (1587 lines, over the 400-line law already) so
     both shells call it, and making `shell.rs` / `gpu_init.rs` native-only.
  2. **Async GPU bring-up.** `pollster::block_on` runs at about ten sites
     (gpu_init, gpu, trace, composite). In the browser, adapter, device and
     error-scope pops must be awaited (`wasm_bindgen_futures::spawn_local`)
     rather than blocked on.
  3. **File dialogs.** `rfd::FileDialog` is synchronous and has no web
     version (state.rs, 9 sites). The web build needs `AsyncFileDialog` or
     an `<input type=file>` seam.
  4. **Clock and threads.** `std::time::Instant` panics on this target
     (34 non-test sites, in app/stock_play_worker, geometry/slice and ui).
     The fix is a `web_time` alias seam. The `std::thread::spawn` sites
     (generate.rs, job.rs, stock_play_worker, state, cpu_eye) need a
     run-inline fallback. rayon is expected to run on the calling thread
     when it cannot spawn (not yet seen at runtime).
  5. **The page.** The linked module imports nine
     `wasi_snapshot_preview1` functions (`clock_time_get`, `fd_close`,
     `fd_fdstat_get`, `fd_prestat_get`, `fd_prestat_dir_name`, `fd_seek`,
     `fd_write`, `poll_oneoff`, `proc_exit`), which a small JS shim must
     supply. The page also needs wasm-bindgen CLI 0.2.129 to match the
     lock (the installed CLI is 0.2.106).
- **Cost to change later:** low. The script is the whole mechanism. It
  goes away when flecs publishes wasm32 support: drop the patch block and
  keep the sysroot.
