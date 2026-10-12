//! Headless frame loop for tests and CI: real ImGui frames, no GPU/window.
//! Provenance: our own harness.

use std::sync::Mutex;

use dear_imgui_rs::{ConfigFlags, Context};

use crate::shell;
use crate::state::State;

/// Dear ImGui allows one live context per process (thread lease), so
/// concurrent harness runs must not overlap.
static CONTEXT_LOCK: Mutex<()> = Mutex::new(());

/// Run `n` complete UI frames on a fresh document. Errors are strings so
/// callers (and the CLI) can report without panicking.
pub fn run_frames(n: u32) -> Result<(), String> {
    let _guard = CONTEXT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut ctx = Context::try_create().map_err(|err| err.to_string())?;
    ctx.set_ini_filename(None::<String>)
        .map_err(|err| err.to_string())?;
    ctx.io_mut()
        .set_config_flags(ConfigFlags::DOCKING_ENABLE);
    ctx.io_mut().set_display_size([800.0, 600.0]);
    ctx.io_mut().set_delta_time(1.0 / 60.0);
    // No renderer here, so ImGui wants the explicit legacy atlas path.
    ctx.font_atlas()
        .try_claim_legacy_renderer()
        .map_err(|err| format!("font atlas: {err}"))?
        .build();

    let mut state = State::new();
    for _ in 0..n.max(1) {
        state.drain_actions();
        let frame = ctx.begin_frame();
        shell::build(frame.ui(), &mut state);
        drop(frame);
    }
    Ok(())
}
