## #379 - the scripting suite's SIGSEGV was flecs' first-world race

**Decided:** `world::new_world` now calls
`flecs_ecs::core::ecs_os_api::ensure_initialized` once, behind a
`std::sync::Once`, before it builds any world. A new test binary,
`crates/app/tests/world_init_race.rs`, pins the fix with a stress test.

**Why:** flecs keeps its OS API (malloc, free, abort, the clock) in one
process-global C struct, and the first world fills it in without a lock.
`ensure_initialized` claims that first fill under a mutex but writes the
struct after the mutex is released. Meanwhile `ecs_mini`, the C side of
`World::new`, copies the struct and writes it back. So a second thread
building its first world at that moment could copy a half-written
struct. Then `ecs_mini` hit `ecs_os_has_heap() == false` and aborted
(SIGABRT), or later called a NULL hook (SIGSEGV).

Stack under lldb, scripting binary, aarch64-apple-darwin:
`abort <- ecs_mini.cold.1 <- ecs_mini (flecs.c:21288, the
ECS_MISSING_OS_API check) <- ecs_init <- World::default <-
camcraft_app::world::new_world <- script::Env::fresh <- script::run`. At
the same moment, a sibling test thread was inside another world's
`ecs_set_hooks_id`.

After that first fill, flecs marks the API initialised. From then on a
world's construction only reads the struct, because the write-back is
refused. Finishing the fill once while every other caller waits
therefore closes the race. Every world in the repo is built through
`new_world` (grep: no other `World::new`).

**Evidence:**

| Run | Before the fix | After the fix |
|---|---|---|
| Scripting suite: 8 copies of `--test-threads=32` running in parallel | 7 crashes in 1200 runs (6 exit 139, 1 exit 134), all before the first test finished | 0 in 1600 runs |
| `world_init_race`: 32 threads at a barrier per child, all cores spinning | 6 crashed children in 1920 | 0 in 3840 |
| `world_init_race` test, 720 children | failed 4 of 6 runs | passes |
| project_roundtrip, 6 copies x 60 runs | not measured | 0 in 360 |

project_roundtrip's earlier one-off SIGABRT has the same signature, an
abort at startup under load. It is very likely the same race, but it was
not reproduced before the fix.

**Considered and dropped:**
- Retrying or ignoring the crash, or running the tests with
  `--test-threads=1`. That hides a real crash that a multi-threaded host
  (the MCP server or batch jobs) could also hit.
- Patching the vendored flecs C source. We would carry a fork for a
  one-line ordering fix that the Rust side can enforce.
- Calling `ecs_set_os_api_impl` through FFI. That is unsafe code for no
  gain over the crate's own safe `ensure_initialized`.
- Creating and dropping a throwaway world inside the `Once`. It works
  too, but it costs more than the init call alone.

**Not fixed here, noted:**
- flecs' `ecs_os_api_init_count` is a plain `int`, and each world's
  creation and destruction changes it without atomics. On POSIX the
  init and fini hooks it guards are NULL, so it is harmless. On Windows
  a lost update could call `win_fini` early. Nothing has been seen, so
  this is a candidate for a future issue.
- A separate observation, not the cause: in debug, every script run
  needs about 950 KB of stack (boa's `Context` setup plus the world).
  Test threads have 2 MiB, so the headroom is about 2x. With
  `RUST_MIN_STACK=786432` it overflows deterministically, and Rust
  reports that as "has overflowed its stack" (SIGABRT), not SIGSEGV.

**Cost to change:** none. It is one `Once` at the single world entry
point. If flecs_ecs makes `ensure_initialized` race-free upstream, the
guard is still harmless.
