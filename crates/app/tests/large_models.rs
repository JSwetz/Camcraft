//! M5.3 E: the large-model laws. The STL door streams (the file never
//! coexists with its triangles), the stream and buffer readers agree
//! verdict-for-verdict, and the undo history holds a mesh by refcount
//! instead of copying the solid it merely remembers.
//!
//! Gate map (M4.4): covers app::geometry::import::stl, app::commands,
//! app::model, geometry::Mesh storage.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use camcraft_app::commands::{Command, UpdateModel};
use camcraft_app::geometry::import::{self, Format};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::world::new_world;

fn tmp_stl(tag: &str, bytes: &[u8]) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-large-models-{}-{tag}-{n}.stl", std::process::id()));
    std::fs::write(&p, bytes).expect("temp file writes");
    p
}

/// The equivalence law: the buffered `parse_model` and the streaming
/// `load_model` door must accept the same files to the same mesh, and
/// refuse them with the same message. One table, both readers.
fn agree(tag: &str, bytes: &[u8]) {
    let path = tmp_stl(tag, bytes);
    let buf = import::parse_model(bytes, Format::Stl);
    let file = import::load_model(&path);
    std::fs::remove_file(&path).ok();
    match (buf, file) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{tag}: stream and buffer meshes differ"),
        (Err(a), Err(b)) => assert_eq!(
            format!("{a}"),
            format!("{b}"),
            "{tag}: stream and buffer verdicts differ"
        ),
        (a, b) => panic!("{tag}: verdicts diverge - buffer {a:?} vs file {b:?}"),
    }
}

fn facet_rec(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [u8; 50] {
    let mut rec = [0u8; 50];
    for (pi, p) in [&a, &b, &c].iter().enumerate() {
        for (ci, v) in p.iter().enumerate() {
            let at = 12 + pi * 12 + ci * 4;
            rec[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    rec
}

fn binary_stl(header: &[u8; 80], count: u32, recs: &[[u8; 50]]) -> Vec<u8> {
    let mut out = header.to_vec();
    out.extend_from_slice(&count.to_le_bytes());
    for r in recs {
        out.extend_from_slice(r);
    }
    out
}

fn f32s(a: Point3) -> [f32; 3] {
    [a.x as f32, a.y as f32, a.z as f32]
}

fn ascii_two_facets() -> String {
    let p = |q: Point3| format!("{} {} {}", q.x, q.y, q.z);
    let o = Point3::ZERO;
    let x = Point3::new(1.0, 0.0, 0.0);
    let y = Point3::new(0.0, 1.0, 0.0);
    let z = Point3::new(0.0, 0.0, 1.0);
    format!(
        "solid t\nfacet normal 0 0 1\nouter loop\nvertex {}\nvertex {}\nvertex {}\nendloop\nendfacet\n\
         facet normal 1 0 0\nouter loop\nvertex {}\nvertex {}\nvertex {}\nendloop\nendfacet\nendsolid t\n",
        p(o), p(x), p(y), p(o), p(y), p(z)
    )
}

const ZERO80: [u8; 80] = [0u8; 80];
const SOLID80: [u8; 80] = {
    let mut h = [0u8; 80];
    h[0] = b's';
    h[1] = b'o';
    h[2] = b'l';
    h[3] = b'i';
    h[4] = b'd';
    h
};

fn two_f4s() -> (Point3, Point3, Point3) {
    (
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    )
}

#[test]
fn stream_and_buffer_agree_on_every_flavor() {
    let (a, b, c) = two_f4s();
    let rec_ok = facet_rec(f32s(a), f32s(b), f32s(c));
    let mut rec_nan = rec_ok;
    rec_nan[12] = 0; // first vertex x = 0 already; poison z instead
    rec_nan[20..24].copy_from_slice(&f32::NAN.to_le_bytes());
    let rec_flat = facet_rec([0.0; 3], [0.0; 3], [0.0; 3]);

    // Happy paths, both flavors; the `solid` header rides the same law.
    agree("bin2", &binary_stl(&ZERO80, 2, &[rec_ok, rec_ok]));
    agree("bin2-solid", &binary_stl(&SOLID80, 2, &[rec_ok, rec_ok]));
    let ascii = ascii_two_facets();
    agree("ascii", ascii.as_bytes());
    agree(
        "ascii-no-end",
        ascii.replace("endsolid t\n", "").as_bytes(),
    );

    // Refusals: empty, short, size-law, caps.
    agree("empty", b"");
    agree("short", b"ab");
    agree("short-solid", b"solid");
    agree("count0", &binary_stl(&ZERO80, 0, &[]));
    agree("short-body", &binary_stl(&ZERO80, 2, &[rec_ok]));
    agree("extra-body", &binary_stl(&ZERO80, 1, &[rec_ok, rec_ok]));
    agree("count-max", &binary_stl(&ZERO80, u32::MAX, &[]));

    // Per-facet law through the shared `accepted`.
    agree("nan-vertex", &binary_stl(&ZERO80, 1, &[rec_nan]));
    agree("flat", &binary_stl(&ZERO80, 1, &[rec_flat]));
    agree(
        "ascii-degen",
        b"solid\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 0 0 0\nvertex 0 0 0\nendloop\nendfacet\nendsolid\n",
    );

    // Grammar refusals ride the shared FSM.
    agree("ascii-garbage", b"solid\nhelix 1 2 3\nendsolid\n");
    agree("ascii-before-solid", b"facet normal 0 0 1\n");
    agree("ascii-nested", b"solid a\nsolid b\nendsolid a\n");
    agree("ascii-four-verts", b"solid\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nvertex 1 1 0\nendloop\nendfacet\nendsolid\n");
    agree("ascii-zero-facets", b"solid\nendsolid\n");

    // UTF-8 after the solid closed: the buffer validates the whole file,
    // so the stream must keep reading past `endsolid` to agree.
    let mut tail = ascii.into_bytes();
    tail.extend_from_slice(&[0xff, 0xfe]);
    agree("ascii-bad-utf8-tail", &tail);
    let mut mid = b"solid\n".to_vec();
    mid.extend_from_slice(&[0xff]);
    agree("ascii-bad-utf8-mid", &mid);

    // A `solid` header whose bytes do NOT match a binary size: the
    // prefix loses to the law and the ASCII reader gets the garbage.
    agree("solid-lies", &binary_stl(&SOLID80, 1, &[rec_ok, rec_ok]));
}

#[test]
fn load_model_streams_stl_without_changing_the_verdict_of_the_fixture() {
    // The shipped cube rides the streaming door and agrees with the
    // buffered one on the very file (an equivalence spot-check on data).
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join("cube.stl");
    let bytes = std::fs::read(&path).expect("fixture reads");
    let streamed = import::load_model(&path).expect("cube.stl streams");
    let buffered = import::parse_model(&bytes, Format::Stl).expect("cube.stl buffers");
    assert_eq!(streamed, buffered);
}

#[test]
fn history_holds_the_mesh_by_refcount_not_by_copy() {
    let world = new_world();
    let m = model::spawn(&world, "m");
    let seed = cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0));
    model::set_facets(&world, &m, seed.facets.clone());

    let prev = model::facets_of(&world, &m).unwrap();
    let replacement = Mesh::from_facets(vec![Triangle::new(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(2.0, 0.0, 0.0),
        Point3::new(0.0, 2.0, 0.0),
    )]);
    let handle = replacement.facets.clone();
    let cmd = UpdateModel::new(&world, "m", replacement).expect("update constructs");

    // The snapshot is the SAME allocation the entity held - history never
    // copied the solid it merely remembers.
    assert!(
        std::sync::Arc::ptr_eq(&prev, &model::facets_of(&world, &m).unwrap()),
        "construction must snapshot, not copy"
    );

    cmd.apply(&world);
    let now = model::facets_of(&world, &m).unwrap();
    assert!(std::sync::Arc::ptr_eq(&now, &handle), "apply installs the new mesh");
    // handle + now + cmd.mesh + entity: four owners, ONE 12-facet
    // allocation - the Vec world would have memcpy'd it twice by here.
    assert_eq!(
        std::sync::Arc::strong_count(&handle),
        4,
        "a history entry is a refcount, not a memcpy"
    );

    cmd.invert(&world);
    let back = model::facets_of(&world, &m).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&back, &prev),
        "undo restores the very allocation, byte-identical by identity"
    );
}

/// The streaming door's reason to exist: a model far larger than the
/// whole old heap must import in seconds, not minutes, and not at twice
/// its disk size in RAM. Release lane only (the wall is the verdict).
#[test]
#[ignore = "heavy lane (M5.3 E): ~500 MB synthetic model over disk, milestone close-out timing"]
fn ten_million_facets_stream_under_the_wall() {
    const N: u32 = 10_000_000;
    let path = {
        let mut p = std::env::temp_dir();
        p.push(format!("camcraft-ten-million-{}.stl", std::process::id()));
        p
    };
    let t0 = Instant::now();
    {
        use std::io::Write;
        let file = std::fs::File::create(&path).expect("temp file creates");
        let mut w = std::io::BufWriter::with_capacity(8 << 20, file);
        w.write_all(&ZERO80).expect("header");
        w.write_all(&N.to_le_bytes()).expect("count");
        // One valid facet, stepped along x so no two coincide: any
        // single record repeats a degenerate-free triangle.
        let rec = facet_rec([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        for _ in 0..N {
            w.write_all(&rec).expect("facet");
        }
        w.flush().expect("flush");
    }
    let wrote = t0.elapsed();
    let t1 = Instant::now();
    let mesh = import::load_model(&path).expect("ten million facets stream in");
    let read = t1.elapsed();
    std::fs::remove_file(&path).ok();
    assert_eq!(mesh.facets.len(), N as usize);
    println!(
        "M5.3 E wall: wrote {N} facets in {wrote:?}, streamed them back in {read:?}"
    );
    // Generous: this is the debug-lane budget blown up 5x, so a release
    // run failing it means the stream regressed, not that the disk lied.
    assert!(read.as_secs_f64() < 60.0, "streaming import took {read:?}");
}
