//! M13.3: the `camcraft-motion` headless binary - facing a rectangle
//! writes a CLFile that reads back as a chained path at the cut depth;
//! nonsense parameters exit non-zero with a message, never a panic
//! (Prime Directive).
//!
//! Gate map (M4.4): covers camcraft-motion src/bin/camcraft-motion.rs.
//!
//! Provenance: our own fixtures, from the facing generator's contract.

use camcraft_motion::cl::text;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-motion")).args(args).output().expect("spawn")
}

#[test]
fn motion_bin_faces_a_rectangle() {
    let f = std::env::temp_dir().join(format!("cc-motion-{}-face.cl", std::process::id()));
    let fs = f.to_string_lossy().into_owned();
    let o = run(&["face", "-o", &fs, "0", "0", "40", "20", "10", "6", "-1", "5"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let src = std::fs::read_to_string(&f).expect("output written");
    assert!(src.starts_with("; HSMW-CL v1"), "{src}");
    let t = text::from_text(&src).expect("reads back");
    assert!(t.is_chained() && t.len() > 3, "{src}");
    assert!(t.moves.iter().any(|m| (m.to.z + 1.0).abs() < 1e-9), "{src}");
    // stdout carries the same dump when no -o is given.
    let o = run(&["face", "0", "0", "40", "20", "10", "6", "-1", "5"]);
    assert_eq!(String::from_utf8_lossy(&o.stdout), src);
    let _ = std::fs::remove_file(f);
}

#[test]
fn motion_bin_refuses_bad_input() {
    let unwritable = std::env::temp_dir().join("cc-motion-no-such-dir").join("out.cl");
    let unwritable = unwritable.to_string_lossy().into_owned();
    for args in [
        &["face", "0", "0", "40", "20", "10", "0", "-1", "5"][..], // zero stepover
        &["face", "0", "0", "40", "20", "-10", "6", "-1", "5"],    // negative tool
        &["face", "40", "0", "0", "20", "10", "6", "-1", "5"],     // flipped box
        &["face", "0", "0", "40", "20", "10", "6", "5", "-1"],     // clearance below cut
        &["face", "0", "0", "40", "20", "10", "6", "inf", "5"],
        &["face", "0", "0"],
        &["pocket"],
        &["face", "-o", &unwritable, "0", "0", "40", "20", "10", "6", "-1", "5"],
        &[],
    ] {
        let o = run(args);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains("camcraft-motion: ") && !err.contains("panicked"), "{args:?}: {err}");
    }
}
