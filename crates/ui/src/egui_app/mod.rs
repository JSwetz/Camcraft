//! The egui shell (M1.4.2 migration), selected with `--ui egui`.
//!
//! Graduated from the M1.4.1 spike binary: `shell` runs the loop, `world`
//! collects the scene through the shared `crate::interact` law and holds the
//! demo seed, `theme` carries the color tokens, `painted` is the draw-list
//! port (ribbon cells, tree rows), `panes` holds the keyboard layer and the
//! painted ribbon and browser (M-b), and `pages` the property pages (M-c).
//! Since M-d the window opens on a plain empty document - `world::seed` is
//! what the harnesses and the headless proof call directly. Since M-e the
//! viewport takes pointer gestures: `shell`'s viewport panel drives the same
//! `crate::interact` pick, hover and camera functions the imgui shell does,
//! from egui's own input state. Since M-f the surfaces here fire commands
//! through the shared `crate::actions` layer - the entry every surface,
//! here and (until it retired) on the imgui side, calls - so the effects
//! live once. At M-g imgui retired and this is the shell
//! (docs/spike-egui-go-no-go.md, migration law); the generators, commands
//! and persistence underneath were shared and untouched throughout.
//! Provenance: our own code, from our own spike.

mod icons;
mod painted;
mod pages;
mod panes;
mod shell;
mod theme;
// pub(crate) past egui_app: the crate-level law tests read the scene
// digest straight (the imgui panels' fold checks moved there at M-g).
pub(crate) mod world;

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
