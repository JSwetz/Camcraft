//! CamCraft UI shell (clean-room): winit + egui (docked panels) + wgpu viewport.
//!
//! Layering (AGENTS.md): ui -> app -> flecs. This crate renders and routes
//! input; every persistent world change goes through `camcraft_app::commands`.
//! The imgui stack that lived here until M1.4.2 M-g is retired: `egui_app`
//! is the shell, `interact` and `state` the shared law underneath it.

pub mod actions;
pub mod egui_app;
pub mod interact;
pub mod operations;
pub mod state;
pub mod trace;
pub mod viewport;

#[cfg(test)]
mod law_tests;
#[cfg(test)]
mod helix_view_tests;
#[cfg(test)]
mod field_view_tests;

/// Entry point used by `main.rs`; returns the process exit code. The egui
/// shell is the shell: `--headless` runs its no-window proof, anything else
/// opens the window. `--ui egui` still selects it (the M1.4.2 migration
/// flag); `--ui <anything else>` refuses by name - the imgui stack retired
/// at M-g and there is nothing left to select.
/// Provenance: our own shell, specced from public CAM workflow conventions.
pub fn run(args: &[String]) -> i32 {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--ui" {
            match it.next().map(String::as_str) {
                None => {
                    eprintln!("--ui needs a value: egui is the only shell (imgui retired at M1.4.2 M-g)");
                    return 2;
                }
                Some("egui") => {}
                Some(other) => {
                    eprintln!("unknown --ui {other}: the imgui stack retired at M1.4.2 M-g; egui is the only shell");
                    return 2;
                }
            }
        }
    }
    egui_app::run(args)
}
