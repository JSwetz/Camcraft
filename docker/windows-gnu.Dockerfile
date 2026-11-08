# syntax=docker/dockerfile:1
# Windows x86_64 cross build from Linux (x86_64-pc-windows-gnu, mingw-w64):
# a portable zip with the release zip's layout, for building and hand-testing
# Windows binaries without a Windows box. NOT the shipped artifact - see
# docker/README.md ("Windows").
# Provenance: CamCraft's .github/workflows/release.yml (windows job's zip
# layout), rustup's cross-compilation docs and the mingw-w64 toolchain.
#
#   docker build -f docker/windows-gnu.Dockerfile --output type=local,dest=dist .
FROM ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11 AS toolchain

ARG DEBIAN_FRONTEND=noninteractive
# gcc-mingw-w64-x86-64 is the C cross compiler flecs_ecs_sys's cc build
# needs (and the linker rustc's windows-gnu target calls); zip packs the
# portable archive and unzip lists it back.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl gcc-mingw-w64-x86-64 unzip zip \
 && rm -rf /var/lib/apt/lists/*

ARG RUST_TOOLCHAIN=1.99.1
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
RUN curl --proto '=https' --tlsv1.2 -LfsS https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal \
            --default-toolchain "$RUST_TOOLCHAIN" \
            --target x86_64-pc-windows-gnu \
 && rustc --version

FROM toolchain AS build
WORKDIR /src
COPY . .
ARG TARGET=x86_64-pc-windows-gnu
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release --target "$TARGET" \
        -p camcraft-ui -p camcraft-app -p camcraft-post \
 && mkdir -p /out/bin \
 && cp "target/$TARGET/release/camcraft-ui.exe" "target/$TARGET/release/camcraft-cli.exe" \
       "target/$TARGET/release/post.exe" /out/bin/
# The release.yml windows job's "Assemble portable zip" layout. The version
# comes from the workspace manifest: a Linux host cannot run post.exe.
RUN ver=$(sed -n 's/^version = "\([0-9.]*\)"$/\1/p' Cargo.toml | head -n1) \
 && test -n "$ver" \
 && stage="/dist/camcraft-$ver-win-x64-gnu" \
 && mkdir -p "$stage/assets" \
 && cp /out/bin/*.exe "$stage/" \
 && for a in posts machines ui scripts themes; do cp -a "assets/$a" "$stage/assets/$a"; done \
 && cp RELEASE_NOTES.md TESTING.md LICENSE "$stage/" \
 && (cd /dist && zip -qr "camcraft-$ver-win-x64-gnu.zip" "camcraft-$ver-win-x64-gnu") \
 && unzip -l "/dist/camcraft-$ver-win-x64-gnu.zip" | grep 'camcraft-ui.exe$'

FROM scratch AS packages
COPY --from=build /dist/*.zip /
