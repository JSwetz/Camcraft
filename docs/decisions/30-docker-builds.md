## #30 - Docker builds per shipped target

**Decided:** a `docker/` directory with one Dockerfile per target that can
honestly be built in a container - `linux.Dockerfile` (tar.gz, deb, rpm,
AppImage, mirroring the release `linux` job) and `windows-gnu.Dockerfile`
(an unofficial mingw cross-build portable zip) - plus `docker/build.sh` and
a README table that names, for every shipped target, whether it builds in
Docker and why not when it does not (snap, Flatpak, the msvc Windows zip +
Inno installer, macOS). No workflow: the images are local tooling.

**Why:** "Done means: documented or CI Docker build for each shipped target."
The Linux packages are the only release assets a stock container can
produce bit-for-bit from the release scripts. A local image costs no Actions
minutes; a `docker.yml` would only re-run what release.yml's linux job
already does on the same runner, so it was not added.

**Pinning:** the base is `ubuntu:22.04` pinned by digest (the runner's
distro: glibc 2.35, which the .deb's `libc6 (>= 2.35)` names). Rust is
pinned by a build arg (default 1.99.1, current stable) because a container
build should be reproducible, while the runner keeps its preinstalled
stable; the two can drift by a release - accepted, and overridable.

**Considered and dropped:**
- cargo-xwin for an msvc cross build: it downloads Microsoft's CRT and
  Windows SDK under their license into the image; not something to bake
  into a distributable image, and Inno Setup / signtool would still need
  Windows. The gnu zip is named `-win-x64-gnu` so it is never mistaken for
  the release zip.
- snap / Flatpak in Docker: need snapd / bwrap, i.e. a `--privileged`
  container; left on the runner jobs, which only repackage the tarball.
- macOS via osxcross: needs the Apple SDK, licensed for Apple hardware only.
- A `workflow_dispatch` `docker.yml`: duplicates the release linux job and
  spends minutes for nothing new.

**Cost to change later:** adding a target is one more Dockerfile and a case
in `docker/build.sh`. Bumping the base means a new digest; bumping Rust is
the build arg default.
