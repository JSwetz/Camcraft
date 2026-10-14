//! HSMWorks clean-room UI shell: winit + Dear ImGui (docking) + wgpu viewport.
//!
//! Layering (AGENTS.md): ui -> app -> flecs. This crate renders and routes
//! input; every persistent world change goes through `hsmworks_app::commands`.

pub mod actions;
pub mod app;
pub mod headless;
pub mod properties;
pub mod setup;
pub mod shell;
pub mod state;
pub mod tool_library;
pub mod tree;
pub mod viewport;

/// Entry point used by `main.rs`; returns the process exit code.
/// Provenance: our own shell, specced from public CAM workflow conventions.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--headless") {
        match headless::run_frames(10) {
            Ok(()) => 0,
            Err(msg) => {
                eprintln!("headless run failed: {msg}");
                1
            }
        }
    } else {
        app::run()
    }
}
