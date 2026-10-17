//! The egui entry point (M1.4.2): a thin launcher for the lib's
//! `--ui egui` shell, kept so hand-tests and the gate ladder can start the
//! migrating stack without touching the default imgui binary.
//! Provenance: our own code; the shell itself lives in `hsmworks_ui::egui_app`.

fn main() {
    let mut args = vec![String::from("--ui"), String::from("egui")];
    args.extend(std::env::args().skip(1));
    std::process::exit(hsmworks_ui::run(&args));
}
