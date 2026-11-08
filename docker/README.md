# docker/ - container builds per shipped target

The release artifacts are built by `.github/workflows/release.yml` on a
`vX.Y.0` tag. This directory lets anyone with Docker (BuildKit) rebuild the
Linux packages locally from the same scripts, and cross-build Windows
binaries for testing, without a GitHub runner and without spending Actions
minutes. No workflow runs these images: they are local tooling.

Run every command from the repository root (the build context is the repo;
`.dockerignore` keeps `target/`, `dist/`, `.git` and `.claude/` out of it).

| Shipped target | In Docker? | How |
|---|---|---|
| Linux x86_64 tar.gz, deb, rpm, AppImage | yes | `docker/linux.Dockerfile` |
| Linux snap | no - stays on the runner | see "Snap and Flatpak" |
| Linux Flatpak | no - stays on the runner | see "Snap and Flatpak" |
| Windows zip + Inno installer (msvc) | no - stays on the runner | see "Windows" |
| Windows portable zip (gnu, unofficial) | yes, cross from Linux | `docker/windows-gnu.Dockerfile` |
| macOS universal zip + dmg | no - stays on the runner | see "macOS" |

## Linux

    docker build -f docker/linux.Dockerfile --output type=local,dest=dist .
    # or: docker/build.sh

`dist/` receives `camcraft-<ver>-linux-x86_64.{tar.gz,deb,rpm,AppImage}`.
The image mirrors the release `linux` job: the `ubuntu-22.04` base (pinned
by digest), its apt package list verbatim, the same `cargo build --locked
--release` line, `scripts/package-linux.sh`, `scripts/package-appimage.sh`
with the same pinned linuxdeploy and SHA-256, and the job's package-content
checks. Differences from the hosted runner, all forced by a bare image:

- `build-essential`, `ca-certificates`, `curl` and `file` are installed
  (the runner has them preinstalled; flecs' C sources need the compiler).
- Rust is pinned (`--build-arg RUST_TOOLCHAIN=...`, default 1.99.1) where
  the runner uses whatever stable it ships that week. Keep it at or above
  the workspace `rust-version`.
- The AppImage is assembled without FUSE: `package-appimage.sh` only
  extracts linuxdeploy and runs its bundled appimagetool, which works in an
  unprivileged container.

BuildKit cache mounts keep the cargo registry and `target/` between runs, so
a rebuild after a source edit recompiles only what changed.

## Snap and Flatpak

Both stay on the `snap` / `flatpak` runner jobs. `snapcraft` needs a running
`snapd` (systemd, snap confinement), and `flatpak-builder` builds inside
`bwrap` sandboxes that need user namespaces and mounts; both need a
`--privileged` container at best, which a reproducible build image should
not ask for. Both only repackage the Linux tarball, so a tarball from the
Linux image can be fed to `scripts/package-snap.sh` /
`scripts/package-flatpak.sh` on a Linux host that has those tools.

## Windows

The shipped Windows build stays native on `windows-latest`:

- It is an **msvc** build (`x86_64-pc-windows-msvc`), and the installer is
  built by Inno Setup (`scripts/installer.ps1`, ISCC.exe) and optionally
  signed by `signtool` (`scripts/sign.ps1`) - all Windows-only tools.
  Cross-building msvc from Linux (cargo-xwin) would mean downloading the
  Microsoft CRT and Windows SDK under their license, inside an image we
  would then distribute; we do not do that.

`docker/windows-gnu.Dockerfile` instead cross-compiles the
**x86_64-pc-windows-gnu** target with mingw-w64 (the C cross compiler builds
flecs' sources; wgpu, winit and rfd are pure Rust on Windows) and packs a
portable zip with the release zip's layout, named `...-win-x64-gnu.zip` so
it is never mistaken for a release asset:

    docker build -f docker/windows-gnu.Dockerfile --output type=local,dest=dist .
    # or: docker/build.sh windows

It is for building and hand-testing Windows binaries from a Linux or macOS
machine. It is unsigned, has no installer, and links the mingw runtime
instead of the MSVC one, so it is not published.

## macOS

Stays on the `macos-15` runner. Building for Apple targets needs the macOS
SDK and Apple's linker, and Apple's Xcode/SDK license allows them only on
Apple hardware; `hdiutil` (the dmg) exists only on macOS. A Linux container
cannot build the universal app legally, so there is no macOS image.
