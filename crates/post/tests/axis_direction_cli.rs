//! The command-line post's door for an inverted machine (#373): until the
//! post translates inverted axes (#374), `post --machine` with a file that
//! states one refuses by name and writes nothing.
//!
//! Gate map (M4.4): covers camcraft_post::machine, the post binary.

use std::process::Command;

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft_axisdir_cli_{tag}_{}", std::process::id()))
}

#[test]
fn the_cli_refuses_an_inverted_machine_by_name() {
    let mch = tmp("flip.mch");
    let cps = tmp("any.cps");
    let cnc = tmp("empty.cnc");
    let out = tmp("out.nc");
    let file = r#"{"general": {"model": "Flip 1"}, "kinematics": {"default": {"parts": [
        {"type": "linear", "name": "X", "direction": -1}]}}}"#;
    std::fs::write(&mch, file).expect("write mch");
    std::fs::write(&cps, "function onOpen() {}\n").expect("write cps");
    std::fs::write(&cnc, "").expect("write cnc");
    let run = Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--machine")
        .arg(&mch)
        .arg(&cps)
        .arg(&cnc)
        .arg(&out)
        .output()
        .expect("run post");
    let err = String::from_utf8_lossy(&run.stderr).to_string();
    for p in [&mch, &cps, &cnc] {
        let _ = std::fs::remove_file(p);
    }
    assert!(!run.status.success(), "refused");
    assert!(
        err.contains("machine Flip 1 has an inverted X axis; posting for inverted axes is not supported yet (#374)"),
        "{err}"
    );
    assert!(!out.exists(), "nothing written");
}
