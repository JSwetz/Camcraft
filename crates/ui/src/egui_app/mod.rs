//! The egui shell (M1.4.2 migration), selected with `--ui egui`.
//!
//! Graduated from the M1.4.1 spike binary: `shell` runs the loop, `world`
//! seeds the demo document and mirrors app.rs's scene collectors, `theme`
//! carries the color tokens, `painted` is the draw-list port (ribbon cells,
//! tree rows), `panes` holds the keyboard layer, the ribbon and the
//! browser (M-b) and the still-stubbed property pages (M-c). Both stacks
//! coexist as entries until M-g retires imgui
//! (docs/spike-egui-go-no-go.md, migration law); the generators, commands
//! and persistence underneath are shared and untouched.
//! Provenance: our own code, from our own spike.

mod painted;
mod panes;
mod shell;
mod theme;
mod world;

#[cfg(test)]
mod tests;

/// Entry for `--ui egui`; returns the process exit code. `--headless`
/// selects the no-window proof (seeded document, fallback adapter, View3D
/// readback); anything else opens the windowed shell.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--headless") {
        match shell::headless::run() {
            Ok(()) => {
                println!("egui shell headless: seeded document, command world, View3D green");
                0
            }
            Err(err) => {
                eprintln!("egui shell headless FAILED: {err}");
                1
            }
        }
    } else {
        shell::run_windowed()
    }
}
