//! #343: each setup's own machine and post. A setup follows the
//! document's mount until it is given its own (`SetSetupMachine` /
//! `SetSetupPost`, undoable); the mounted machine and post are the
//! active setup's. A v54 file opens with every setup following the
//! document and posts byte-identically; a v55 file round-trips the own
//! mounts. Work that crosses mounts is refused by name: generating an
//! op of a setup mounting other steel than the active one, and posting
//! a program whose setups disagree on machine or post.
//! Provenance: our own fixtures over our own commands; no vendor material.
//!
//! Gate map (#343): covers app::setup_mount, app::commands
//! (SetSetupMachine, SetSetupPost, CreateSetup datum transfer),
//! app::machine (of, document_of, kind_of), app::posting (post_of,
//! post_setup, verification_report), app::project (SetupDto v55).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, GenerateOperation, History,
    SetSetupMachine, SetSetupPost, SetStock,
};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{machine, posting, setup_mount};

const TOOL: &str = "EM-0400-2FL";

fn face() -> OpParams {
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

/// Stock, tool, no document machine (the face sits above a VMC's
/// machine-coordinate stroke), `setups` setups (0 = none), one
/// generated face per setup.
fn doc(setups: usize) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    if setups == 0 {
        face_op(&world, &mut h, "Face1");
    }
    for i in 1..=setups {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        if i > 1 {
            let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
            h.execute(&world, stock.expect("setup stock"));
        }
        face_op(&world, &mut h, &format!("Face{i}"));
    }
    (world, h)
}

fn face_op(world: &World, h: &mut History, name: &str) {
    let op = CreateOperation::new(world, name, TOOL, SrcRef::Stock, face());
    h.execute(world, op.expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-343-{tag}-{}.{ext}", std::process::id()));
    path
}

fn saved(world: &World, tag: &str) -> String {
    let path = tmp(tag, "ccam");
    project::save(world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    text
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let path = tmp(tag, "nc");
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out
}

#[test]
fn a_v54_file_follows_the_document_mount_and_posts_byte_identically() {
    for setups in [0, 1] {
        let (world, _) = doc(setups);
        let expected = post(&world, &format!("v54-now-{setups}")).expect("posts");
        let now = saved(&world, &format!("v54-{setups}"));
        assert!(!now.contains("\"post\": \"builtin\""), "no own mount is written: {now}");
        let mut v: Value = serde_json::from_str(&now).expect("json");
        v["version"] = json!(54);
        let w = new_world();
        project::open(&w, &serde_json::to_string_pretty(&v).expect("text")).expect("a v54 file opens");
        if setups == 1 {
            assert_eq!(setup_mount::own_machine(&w, Some("Setup1")), None, "follows the document");
            assert_eq!(setup_mount::own_post(&w, Some("Setup1")), None);
        }
        assert_eq!(machine::of(&w), machine::document_of(&w));
        assert_eq!(post(&w, &format!("v54-old-{setups}")).expect("posts"), expected, "byte-identical");
        assert_eq!(saved(&w, &format!("v54-resave-{setups}")), now, "saves as it was, at v55");
    }
}

#[test]
fn a_setups_own_machine_and_post_round_trip_and_undo() {
    let (world, mut h) = doc(1);
    let expected = post(&world, "rt-before").expect("posts");
    h.execute(&world, SetSetupMachine::builtin(&world, "Setup1", "Tormach 770M").expect("own machine"));
    h.execute(&world, SetSetupPost::new(&world, "Setup1", Some(PostRef::Builtin)).expect("own post"));
    assert_eq!(machine::of(&world).0, posting::builtin_path("Tormach 770M"), "the active setup's machine is mounted");
    assert_eq!(machine::document_of(&world), (String::new(), None), "the document keeps its own");
    let text = saved(&world, "rt");
    assert!(text.contains("\"post\": \"builtin\""), "{text}");
    let w = new_world();
    project::open(&w, &text).expect("a v55 file opens");
    assert_eq!(machine::of(&w).0, posting::builtin_path("Tormach 770M"));
    assert_eq!(setup_mount::own_post(&w, Some("Setup1")), Some(PostRef::Builtin));
    assert_eq!(saved(&w, "rt-resave"), text);
    // Undo both: the setup follows the document again, posting as before.
    h.undo(&world);
    h.undo(&world);
    assert_eq!(setup_mount::own_machine(&world, Some("Setup1")), None);
    assert_eq!(setup_mount::own_post(&world, Some("Setup1")), None);
    assert_eq!(post(&world, "rt-after").expect("posts"), expected);
    // A repeat is refused by name, never a silent no-op in the history.
    let again = SetSetupPost::new(&world, "Setup1", None);
    assert!(again.is_err_and(|e| e.to_string().contains("already follows the document's post")));
    let ghost = SetSetupMachine::builtin(&world, "Setup1", "No such mill");
    assert!(ghost.is_err_and(|e| e.to_string().contains("not in the built-in library")));
    let bad = SetSetupPost::new(&world, "Setup1", Some(PostRef::Named("Nope".into())));
    assert!(bad.is_err_and(|e| e.to_string().contains("not in the registry")));
}

#[test]
fn two_setups_on_different_posts_are_refused_by_name() {
    let (world, mut h) = doc(2);
    let both = post(&world, "two-same").expect("one mount posts");
    h.execute(&world, SetSetupPost::new(&world, "Setup2", Some(PostRef::Named("Haas".into()))).expect("Setup2 posts Haas"));
    // Setup2 is active (the last created): Setup1 follows the Fanuc default.
    let why = post(&world, "two-diff").expect_err("refused");
    assert!(why.contains("setup 'Setup1' posts with") && why.contains("activate 'Setup1'"), "{why}");
    assert!(why.contains("one program is written by one post for one machine"), "{why}");
    // Generating Setup1's op while Setup2 is mounted is refused too.
    let refused = GenerateOperation::new(&world, "Face1");
    assert!(refused.is_err_and(|e| e.to_string().contains("activate 'Setup1' first")));
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    let why = post(&world, "two-diff-1").expect_err("refused from the other side too");
    assert!(why.contains("setup 'Setup2' posts with Haas (built-in)"), "{why}");
    // Giving it back to the document posts the same program as before.
    h.execute(&world, SetSetupPost::new(&world, "Setup2", None).expect("follow the document"));
    assert_eq!(post(&world, "two-back").expect("posts"), both);
}

#[test]
fn two_setups_on_different_machines_are_refused_by_name_and_kinds_follow_the_active() {
    let (world, mut h) = doc(2);
    h.execute(&world, SetSetupMachine::builtin(&world, "Setup2", "Lathe").expect("Setup2 on a lathe"));
    assert_eq!(machine::kind_of(&world), machine::Kind::Lathe, "Setup2 is active: the ribbon reads a lathe");
    let why = post(&world, "mach-diff").expect_err("refused");
    assert!(why.contains("setup 'Setup1' is machined on") && why.contains("activate 'Setup1'"), "{why}");
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    assert_eq!(machine::kind_of(&world), machine::Kind::Mill);
    assert!(posting::verification_gate(&world).iter().any(|p| p.contains("setup 'Setup2' is machined on")));
}

#[test]
fn the_same_own_post_on_every_setup_posts_like_the_document() {
    let (world, mut h) = doc(2);
    let expected = post(&world, "same-before").expect("posts");
    for s in ["Setup1", "Setup2"] {
        h.execute(&world, SetSetupPost::new(&world, s, Some(PostRef::Builtin)).expect("own post"));
    }
    assert_eq!(post(&world, "same-after").expect("one mount posts"), expected);
}

#[test]
fn a_datum_transfer_keeps_the_source_setups_mount() {
    let (world, mut h) = doc(1);
    h.execute(&world, SetSetupPost::new(&world, "Setup1", Some(PostRef::Named("Haas".into()))).expect("own post"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("datum transfer"));
    assert_eq!(setup_mount::own_post(&world, Some("Setup2")), Some(PostRef::Named("Haas".into())));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("fresh setup"));
    assert_eq!(setup_mount::own_post(&world, Some("Setup3")), None, "a fresh setup follows the document");
}
