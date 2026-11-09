//! Diagnostics to the launching terminal (#163): one `tracing`
//! subscriber for every binary, printing to stderr, filtered by
//! `CAMCRAFT_LOG` (else `RUST_LOG`, else [`DEFAULT_FILTER`]). The seams
//! that emit - project save/load, post writes, generation, command
//! dispatch - carry their own `tracing` events; this module only
//! decides where they go.
//!
//! Provenance: the `tracing` / `tracing-subscriber` crates' documented
//! fmt + EnvFilter setup. Our own wiring, no vendor material.

use std::path::Path;
use std::time::Instant;

use tracing_subscriber::EnvFilter;

/// Milliseconds since `started`, saturating at `u64::MAX` rather than
/// truncating the `u128` `as_millis` returns (safe numeric conversion).
#[must_use]
pub fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The file name a log line shows for `path`: the name only, never the
/// folder, so a log pasted into a bug report does not expose the user's
/// directory layout. A path with no final name shows as given.
#[must_use]
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Quiet by default: the app's own info lines, the GPU stack's warnings.
/// `CAMCRAFT_LOG=debug` (or `RUST_LOG=camcraft_app=debug`) opens it up.
pub const DEFAULT_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,winit=warn";

/// The filter the environment asks for, falling back to the default.
/// Invalid directives are ignored by name in the filter itself, never a
/// panic (Prime Directive).
#[must_use]
pub fn filter_from_env() -> EnvFilter {
    EnvFilter::try_from_env("CAMCRAFT_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

/// Install the stderr subscriber. Safe to call more than once (a test
/// harness, a second entry point): only the first install wins, later
/// calls are no-ops.
pub fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter_from_env())
        .with_writer(std::io::stderr)
        .with_target(true)
        .try_init();
}
