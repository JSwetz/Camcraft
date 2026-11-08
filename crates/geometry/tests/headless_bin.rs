//! M13.3: the `camcraft-geometry` headless binary - a closed cube STL
//! reads clean; a missing, garbage or truncated file exits non-zero
//! with a message, never a panic (Prime Directive).
//!
//! Gate map (M4.4): covers camcraft-geometry src/bin/camcraft-geometry.rs.
//!
//! Provenance: our own fixtures; STL layout per the public format note.

use camcraft_geometry::geometry::{cube, Point3};
use std::path::PathBuf;
use std::process::{Command, Output};

fn tmp(name: &str, bytes: &[u8]) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cc-geom-{}-{name}", std::process::id()));
    std::fs::write(&p, bytes).expect("write fixture");
    p
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-geometry")).args(args).output().expect("spawn")
}

fn binary_stl() -> Vec<u8> {
    let m = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 20.0, 5.0));
    let mut b = vec![0u8; 80];
    b.extend((m.len() as u32).to_le_bytes());
    for t in m.facets.iter() {
        b.extend([0u8; 12]);
        for p in [t.a, t.b, t.c] {
            for v in [p.x, p.y, p.z] {
                b.extend((v as f32).to_le_bytes());
            }
        }
        b.extend([0u8; 2]);
    }
    b
}

fn assert_refused(o: &Output) {
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "stderr: {err}");
    assert!(err.starts_with("camcraft-geometry: "), "stderr: {err}");
    assert!(!err.contains("panicked"), "stderr: {err}");
}

#[test]
fn geometry_bin_reads_a_closed_cube() {
    let p = tmp("cube.stl", &binary_stl());
    let o = run(&[p.as_os_str()]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.contains("triangles: 12"), "{out}");
    assert!(out.contains("size: 10.0000 x 20.0000 x 5.0000"), "{out}");
    assert!(out.contains("closed: true"), "{out}");
    let _ = std::fs::remove_file(p);
}

#[test]
fn geometry_bin_reads_ascii() {
    let s = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\n\
             vertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
    let p = tmp("tri.stl", s.as_bytes());
    let o = run(&[p.as_os_str()]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("triangles: 1") && out.contains("closed: false"), "{out}");
    let _ = std::fs::remove_file(p);
}

#[test]
fn geometry_bin_refuses_bad_input() {
    assert_refused(&run(&["no-such-model.stl".as_ref()]));
    let garbage = tmp("garbage.stl", b"\x00\xffnot a model at all");
    assert_refused(&run(&[garbage.as_os_str()]));
    let mut cut = binary_stl();
    cut.truncate(cut.len() - 7);
    let truncated = tmp("trunc.stl", &cut);
    assert_refused(&run(&[truncated.as_os_str()]));
    let nan = tmp(
        "nan.stl",
        b"solid t\nfacet\nvertex NaN 0 0\nvertex 1 0 0\nvertex 0 1 0\nendfacet\n",
    );
    assert_refused(&run(&[nan.as_os_str()]));
    let open = tmp("open.stl", b"solid t\nfacet\nvertex 0 0 0\nvertex 1 0 0\n");
    assert_refused(&run(&[open.as_os_str()]));
    assert_refused(&run(&[]));
    for p in [garbage, truncated, nan, open] {
        let _ = std::fs::remove_file(p);
    }
}
