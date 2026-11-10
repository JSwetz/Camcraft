## #615 - GPU tests fall back to the hardware adapter and say when they skip

**Decided:** one adapter helper, `camcraft_ui::headless_gpu`, used by the
headless run (`--headless`, `headless_run_succeeds`, the two
`*_headless_cli_exit_code_is_zero` tests) and the GPU suites (`gpu_render`,
`trace_composite`, `trace_parity`). It requests wgpu's software adapter
first and, when that fails, the platform default hardware adapter
(`force_fallback_adapter: false`, low power), and reports which one it
got. With no adapter at all, a GPU test prints a loud
`*** <suite>: SKIPPED (no adapter) - NOT a pass ***` line to stderr and
returns; setting `CAMCRAFT_REQUIRE_GPU=1` turns that skip into a failure.

**Why:** macOS's Metal has no software adapter, so the forced-fallback
request failed on every Mac: the GPU suites passed without rendering
(18 silent skips) and the headless run failed with "No suitable graphics
adapter found". Software first keeps Linux/Windows CI on lavapipe/WARP
(deterministic, unchanged); hardware second makes a Mac render on its
real GPU. The headless app path shared the same bug, so it shares the
fix rather than keeping a second copy of the adapter request.

**Skip pattern:** the stderr SKIP line plus the env-var switch. Rust's
test harness has no skip state, so a skip still reports `ok`; the loud
line makes it visible in `--nocapture`/failure output, and a lane that
must render (a GPU-equipped gate, or a CI box known to have lavapipe)
sets `CAMCRAFT_REQUIRE_GPU=1` so a skip can't masquerade as a pass.

**Considered and dropped:**
- `#[ignore]`-gating the GPU suites into their own lane: they run in
  about a second and are exactly the coverage the quick/batch tiers
  should keep; ignoring them would hide them everywhere, Macs included.
- Panicking on no adapter by default: headless CI without any GPU or
  software rasterizer is a legitimate environment (issue: "a deliberate
  skip when no adapter stays possible").
- Hardware first: Linux/Windows output would then depend on whichever
  GPU the runner has; the issue asks for the software adapter there.

**Tolerances:** none changed. Every existing assertion (pixel-coverage
fractions, CPU/GPU agreement ratios, depth epsilons) passes unchanged on
an Apple M3 Max via Metal.

**Cost to change later:** one module; the env-var name is the only
contract (`headless_gpu::REQUIRE_GPU_ENV`).
