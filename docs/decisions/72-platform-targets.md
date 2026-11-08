## #72 - AUR, FreeBSD and WebAssembly build targets (2026-11-08)

- **Chosen:** keep the AUR source recipe pinned to the latest published tag,
  v0.14.0, with a checked source archive hash. Compare normalized `.SRCINFO`
  fields in the Arch workflow: the earlier run failed only because makepkg
  moved `options = !lto` after the dependencies. The Arch job follows Linux
  in `release.yml`; on a tag it downloads that tag's archive, checks its
  SHA-256, and builds a matching package even before the tracked AUR recipe
  receives its next version update.
- **Chosen:** build FreeBSD amd64 natively in a FreeBSD 14 VM and package the
  three executables with adjacent assets in a tarball. Manual and tagged
  release runs produce the archive. A native VM supplies
  FreeBSD libc and system libraries, which a Linux build cannot provide.
- **Chosen:** leave Web/WASM untouched until issue #31 completes its browser
  UI port. Its existing domain build remains in the manual CI workflow.
- **Dropped:** publishing the AUR recipe automatically before the Arch build
  is validated and an AUR maintainer is configured; calling a linked WASM
  CLI a browser UI; treating a Linux binary as a FreeBSD release.
- **Cost to change:** low for the package and workflow wiring. A runnable
  browser UI is a separate application port, not a packaging change.
