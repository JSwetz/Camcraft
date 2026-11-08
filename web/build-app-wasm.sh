#!/usr/bin/env bash
# build-app-wasm.sh - builds camcraft-app (the domain layer, flecs included)
# for wasm32-unknown-unknown and links camcraft-cli.wasm (#31, M13.3 rung 1).
# Provenance: our own build script; the toolchain facts come from the public
# wasi-sdk release notes and the cc / cargo `--config` documentation.
#
# What it works around, all outside our sources (native builds untouched):
#   1. flecs' C needs libc headers and libc itself; wasm32-unknown-unknown has
#      neither. The pinned wasi-sysroot gives both: its headers compile
#      flecs.c, its libc.a satisfies malloc/printf/pthread/clock at link.
#      `-D__COSMOCC__` puts flecs on its generic-POSIX path (no execinfo, no
#      MSVC *_s string calls) and `-D__wasi__` lets the wasi headers load.
#   2. flecs_ecs_sys 0.2.1 / flecs_ecs 0.2.2 do not compile for wasm32 as
#      shipped (upstream: "wasm32-unknown-unknown support in progress"):
#      bindings.rs imports libc::FILE (absent on this target) and declares
#      "C-unwind" where flecs_ecs's wasm cfg expects "C"; meta_fn_types.rs
#      gates the WorldRef/Entity import off on wasm while still using them.
#      The three edits are applied to COPIES under the tools dir and wired
#      in with `cargo --config patch.crates-io...` for this build only.
# The result links with nine wasi_snapshot_preview1 imports (clock_time_get,
# fd_* , poll_oneoff, proc_exit) that a browser page must shim.
#
# Usage: web/build-app-wasm.sh [--release]
# Needs: clang with the wasm32 target, curl, tar, sha256sum, rustup target
# wasm32-unknown-unknown.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE_ARGS=()
if [[ "${1:-}" == "--release" ]]; then PROFILE_ARGS=(--release); fi

WASI_SDK=34
WASI_SYSROOT="wasi-sysroot-${WASI_SDK}.0"
WASI_SHA256="9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be"
FLECS_VER="0.2.2"
FLECS_SYS_VER="0.2.1"

# Windows (Git Bash) clang and cargo want C:/ paths; elsewhere a no-op.
native_path() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }
die() { echo "build-app-wasm: $*" >&2; exit 1; }

TOOLS="$ROOT/target/wasm-tools"
mkdir -p "$TOOLS"

# --- 1. the pinned wasi sysroot ------------------------------------------
SYSROOT="$TOOLS/$WASI_SYSROOT"
if [[ ! -d "$SYSROOT/include/wasm32-wasip1" ]]; then
    tarball="$TOOLS/$WASI_SYSROOT.tar.gz"
    curl -fsSL -o "$tarball" \
        "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK}/${WASI_SYSROOT}.tar.gz" \
        || die "cannot download $WASI_SYSROOT"
    got="$(sha256sum "$tarball" | cut -d' ' -f1)"
    [[ "$got" == "$WASI_SHA256" ]] || die "$WASI_SYSROOT sha256 $got, expected $WASI_SHA256"
    tar xzf "$tarball" -C "$TOOLS" || die "cannot unpack $tarball"
fi

# --- 2. patched copies of the two flecs crates ----------------------------
grep -q "^version = \"$FLECS_VER\"" <(grep -A1 '^name = "flecs_ecs"$' "$ROOT/Cargo.lock") \
    || die "Cargo.lock no longer pins flecs_ecs $FLECS_VER - re-check the patch points"
grep -q "^version = \"$FLECS_SYS_VER\"" <(grep -A1 '^name = "flecs_ecs_sys"$' "$ROOT/Cargo.lock") \
    || die "Cargo.lock no longer pins flecs_ecs_sys $FLECS_SYS_VER - re-check the patch points"
(cd "$ROOT" && cargo fetch --locked) || die "cargo fetch failed"
REG="${CARGO_HOME:-$HOME/.cargo}/registry/src"
src_of() { find "$REG" -maxdepth 2 -type d -name "$1" | head -1; }
FLECS_SRC="$(src_of "flecs_ecs-$FLECS_VER")"
FLECS_SYS_SRC="$(src_of "flecs_ecs_sys-$FLECS_SYS_VER")"
[[ -n "$FLECS_SRC" && -n "$FLECS_SYS_SRC" ]] || die "flecs sources not found under $REG"

PATCHED="$TOOLS/patched"
rm -rf "$PATCHED"
mkdir -p "$PATCHED"
cp -r "$FLECS_SRC" "$PATCHED/flecs_ecs"
cp -r "$FLECS_SYS_SRC" "$PATCHED/flecs_ecs_sys"

B="$PATCHED/flecs_ecs_sys/src/bindings.rs"
sed -i 's/^use libc::FILE;$/pub type FILE = core::ffi::c_void;/' "$B"
sed -i 's/"C-unwind"/"C"/g' "$B"
grep -q '^pub type FILE = core::ffi::c_void;$' "$B" || die "patch point moved: libc::FILE in bindings.rs"
! grep -q '"C-unwind"' "$B" || die "patch point moved: C-unwind in bindings.rs"
M="$PATCHED/flecs_ecs/src/addons/meta/meta_fn_types.rs"
[[ "$(head -2 "$M")" == $'#[cfg(not(target_family = "wasm"))]\nuse crate::core::{Entity, WorldRef};' ]] \
    || die "patch point moved: the WorldRef import in meta_fn_types.rs"
sed -i '1d' "$M"

# --- 3. the build ---------------------------------------------------------
NSYS="$(native_path "$SYSROOT")"
export CFLAGS_wasm32_unknown_unknown="-isystem $NSYS/include/wasm32-wasip1 -D__wasi__ -D__COSMOCC__"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C link-arg=-L$NSYS/lib/wasm32-wasip1 -C link-arg=-lc"
cd "$ROOT"
# The --config patch rewrites Cargo.lock's flecs entries (path source, no
# checksum); the tracked lock is put back whatever happens.
cp Cargo.lock "$TOOLS/Cargo.lock.keep"
trap 'cp "$TOOLS/Cargo.lock.keep" "$ROOT/Cargo.lock"' EXIT
cargo build -p camcraft-app --target wasm32-unknown-unknown "${PROFILE_ARGS[@]}" \
    --config "patch.crates-io.flecs_ecs.path='$(native_path "$PATCHED/flecs_ecs")'" \
    --config "patch.crates-io.flecs_ecs_sys.path='$(native_path "$PATCHED/flecs_ecs_sys")'"

OUT="${CARGO_TARGET_DIR:-$ROOT/target}/wasm32-unknown-unknown/$([[ ${#PROFILE_ARGS[@]} -gt 0 ]] && echo release || echo debug)/camcraft-cli.wasm"
[[ -f "$OUT" ]] || die "build reported success but $OUT is missing"
echo "build-app-wasm: linked $OUT"
