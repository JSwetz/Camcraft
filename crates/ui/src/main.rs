fn main() {
    camcraft_app::logging::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(camcraft_ui::run(&args));
}
