//! `camcraft-cli` - the headless batch lane (M5.3 G): argv in,
//! verdict out. All the work is `app::cli::run`; this is the door.

fn main() -> std::process::ExitCode {
    camcraft_app::logging::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::ExitCode::from(camcraft_app::cli::run(&args) as u8)
}
