//! #315: a rotated work offset must never post silently. The post
//! stream carries the offset's translation only (OriginX/Y/Z), so a
//! G54 turned 90 deg about Z would post world-axis coordinates - a
//! program turned against the part. The gate refuses it by name; an
//! unrotated offset (identity within the gate's tolerance) posts
//! byte-identical to before.
//! Provenance: our own fixtures over the translation-only G54 law.
//!
//! Gate map (#315): covers app::posting (verification_report,
//! wcs_context, stream_records), app::commands (SetWcsOrigin, CreateSetup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, quat_from_euler};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// A 60 x 60 x 20 box faced at Z 19.5, generated, G54 not yet placed.
fn faced_block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, h)
}

fn place_g54(world: &World, h: &mut History, rot: [f64; 4]) {
    let loc = Location { pos: Point3::new(10.0, 5.0, 1.0), rot };
    h.execute(world, SetWcsOrigin::new(world, "G54", loc).expect("place G54"));
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-rotwcs-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn yaw90() -> [f64; 4] {
    quat_from_euler(0.0, 0.0, 90f64.to_radians())
}

#[test]
fn the_post_stream_carries_no_offset_rotation() {
    // The defect's evidence, kept as a law: the stream is the same for
    // a turned G54 as for a square one at the same spot - the rotation
    // never reaches the script, which is why the gate must refuse.
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    let square = posting::stream_records(&world).expect("stream");
    place_g54(&world, &mut h, yaw90());
    let turned = posting::stream_records(&world).expect("stream");
    assert_eq!(format!("{square:?}"), format!("{turned:?}"), "rotation is dropped by the stream");
}

#[test]
fn a_rotated_work_offset_is_refused_by_name() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, yaw90());
    let path = tmp_nc("refused");
    let err = posting::post_setup(&world, &path).expect_err("a turned G54 must not post");
    assert!(!path.exists(), "a refusal leaves no file");
    assert!(
        err.contains("work offset G54 is rotated 90.000 deg about Z")
            && err.contains("posting a rotated work offset is not supported yet")
            && err.contains("re-align it without rotation or post from an unrotated setup")
            && err.contains("(operations: Face1)"),
        "named refusal, got: {err}"
    );
    let report = posting::verification_report(&world);
    assert_eq!(report.problems.len(), 1, "one line per offset: {:?}", report.problems);
}

#[test]
fn a_setups_rotated_offset_names_the_setup() {
    let (world, mut h) = faced_block();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    place_g54(&world, &mut h, quat_from_euler(30f64.to_radians(), 0.0, 0.0));
    let err = posting::post_setup(&world, &tmp_nc("setup")).expect_err("refused");
    assert!(
        err.contains("setup Setup1: its work offset G54 is rotated 30.000 deg about X"),
        "names the setup and the axis, got: {err}"
    );
}

#[test]
fn an_unrotated_offset_posts_exactly_as_before() {
    let (world, mut h) = faced_block();
    place_g54(&world, &mut h, IDENTITY_Q);
    let path = tmp_nc("identity");
    posting::post_setup(&world, &path).expect("an identity G54 posts");
    let exact = std::fs::read_to_string(&path).expect("written");
    // Float noise from a normalized round trip is still the identity.
    let noisy = quat_from_euler(1e-13, -1e-13, 1e-13);
    assert_ne!(noisy, IDENTITY_Q, "the fixture really is off-identity by noise");
    place_g54(&world, &mut h, noisy);
    posting::post_setup(&world, &path).expect("noise-level rotation posts");
    let again = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert_eq!(exact, again, "byte-identical program");
    // Undo back to the turned offset refuses again; undo to identity posts.
    place_g54(&world, &mut h, yaw90());
    assert!(posting::post_setup(&world, &path).is_err());
    assert!(h.undo(&world));
    posting::post_setup(&world, &path).expect("undo restores the postable offset");
    let _ = std::fs::remove_file(&path);
}
