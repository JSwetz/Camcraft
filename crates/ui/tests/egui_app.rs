//! M-a: the `--ui egui` entry contract, in-process (the same spawn law
//! tests/shell.rs runs for imgui — here the "test_harness learns to spawn
//! it" line of the migration plan: run() itself must route the flag).
//! Provenance: our own tests.

#[test]
fn egui_headless_cli_exit_code_is_zero() {
    assert_eq!(
        hsmworks_ui::run(&[
            String::from("--ui"),
            String::from("egui"),
            String::from("--headless")
        ]),
        0
    );
}
