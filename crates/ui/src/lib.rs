//! HSMWorks clean-room UI shell: winit + Dear ImGui (docking) + wgpu viewport.
//!
//! Layering (AGENTS.md): ui -> app -> flecs. This crate renders and routes
//! input; every persistent world change goes through `hsmworks_app::commands`.

pub mod actions;
pub mod app;
pub mod browser;
pub mod egui_app;
pub mod form;
pub mod headless;
pub mod icons;
pub mod operations;
pub mod ribbon;
pub mod setup;
pub mod shell;
pub mod state;
pub mod viewport;

/// Entry point used by `main.rs`; returns the process exit code.
/// `--ui egui` selects the migrating egui shell (M1.4.2); without it the
/// imgui app is the default until M-g retires it.
/// Provenance: our own shell, specced from public CAM workflow conventions.
pub fn run(args: &[String]) -> i32 {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--ui" && it.next().is_some_and(|v| *v == "egui") {
            return egui_app::run(args);
        }
    }
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
