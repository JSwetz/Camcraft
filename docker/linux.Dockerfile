# syntax=docker/dockerfile:1
# Linux x86_64 release build in a container: the same tar.gz, deb, rpm and
# AppImage the release.yml `linux` job ships, from the same scripts.
# Provenance: CamCraft's .github/workflows/release.yml (linux job) and the
# Dockerfile reference (docs.docker.com/reference/dockerfile).
#
#   docker build -f docker/linux.Dockerfile --output type=local,dest=dist .
#
# Pinned to the runner's distro (ubuntu-22.04: glibc 2.35, which the .deb's
# `libc6 (>= 2.35)` dependency names) by digest, so a rebuild sees the
# same base the day it was pinned.
FROM ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11 AS toolchain

ARG DEBIAN_FRONTEND=noninteractive
# release.yml's "Install Linux build and package tools" list, verbatim,
# plus what the hosted runner has preinstalled and a bare image lacks:
# a C toolchain (flecs_ecs_sys compiles flecs.c through the cc crate),
# curl + ca-certificates (rustup, linuxdeploy) and file (linuxdeploy).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        pkg-config libwayland-dev libxkbcommon-dev libx11-dev rpm imagemagick \
        build-essential ca-certificates curl file \
 && rm -rf /var/lib/apt/lists/*

# The hosted runner builds with its preinstalled stable Rust; the container
# pins one so two builds of one commit use one compiler. Must stay >= the
# workspace rust-version (Cargo.toml, 1.90). Override with
# --build-arg RUST_TOOLCHAIN=<version>.
ARG RUST_TOOLCHAIN=1.99.1
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -LfsS https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal \
            --default-toolchain "$RUST_TOOLCHAIN" \
 && rustc --version && cargo --version

# release.yml's "Install pinned linuxdeploy" step: same URL, same SHA-256.
# package-appimage.sh only --appimage-extract's it and runs the bundled
# appimagetool, so no FUSE is needed inside the container.
RUN curl -LfsS -o /opt/linuxdeploy-x86_64.AppImage \
        https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
 && echo 'c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d  /opt/linuxdeploy-x86_64.AppImage' | sha256sum -c - \
 && chmod +x /opt/linuxdeploy-x86_64.AppImage

FROM toolchain AS build
WORKDIR /src
COPY . .
# release.yml's "Build release binaries" step, verbatim. The cache mounts
# keep the registry and target/ between `docker build` runs (BuildKit).
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p camcraft-ui -p camcraft-app -p camcraft-post \
 && mkdir -p /out/bin \
 && cp target/release/camcraft-ui target/release/camcraft-cli target/release/post /out/bin/
# "Assemble Linux packages" and "Assemble AppImage", from the copied-out
# binaries (the target/ cache mount is gone after the RUN above).
ENV CAMCRAFT_BIN_DIR=/out/bin \
    CAMCRAFT_DIST_DIR=/src/dist \
    LINUXDEPLOY=/opt/linuxdeploy-x86_64.AppImage
RUN scripts/package-linux.sh && scripts/package-appimage.sh
# release.yml's "Check package contents", minus the runner paths.
RUN tar -tzf dist/*.tar.gz | grep '/camcraft-ui$' \
 && dpkg-deb --contents dist/*.deb | grep '/usr/lib/camcraft/camcraft-ui$' \
 && rpm -qpl dist/*.rpm | grep '/usr/lib/camcraft/camcraft-ui$' \
 && test -s dist/*.AppImage \
 && check=$(mktemp -d) \
 && (cd "$check" && /src/dist/*.AppImage --appimage-extract >/dev/null) \
 && cmp /out/bin/camcraft-ui "$check/squashfs-root/usr/bin/camcraft-ui" \
 && test -x "$check/squashfs-root/AppRun" \
 && test -d "$check/squashfs-root/usr/bin/assets/themes" \
 && rm -rf "$check"

# `--output type=local,dest=dist` writes exactly these files to the host.
FROM scratch AS packages
COPY --from=build /src/dist/*.tar.gz /src/dist/*.deb /src/dist/*.rpm /src/dist/*.AppImage /
