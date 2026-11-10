//! #339 slice 6 in the shell, structurally: the Fixtures node's menu
//! offers "Save fixture to library…" and "Insert from library…"; the
//! save dialog writes the checked fixture into the library file and
//! asks before replacing a name; the insert dialog lists the entries,
//! and its Insert queues one `InsertFixture` (undo takes it away);
//! Delete edits the file only after its confirmation. Driven through
//! the real dialog code in a kittest harness - no pixels (the snapshot
//! lane is release).
//! Provenance: our own tests over our own shell; no vendor material.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{ImportModel, TagFixture};
use camcraft_app::fixture;
use camcraft_app::fixture_library;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;

use crate::state::{CommandKind, FixtureLibDialog, State, TreeMenuTarget};

use super::fixture_library_dialog::{insert_command, window};
use super::fixture_menu::menu_rows;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

/// A state with one fixture "Vise" and its own library folder.
fn state_with_vise(tag: &str) -> State {
    let mut state = State::new();
    let dir = std::env::temp_dir().join(format!("camcraft-ui-fixlib-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    state.fixture_lib_dir = Some(dir);
    state.history.execute(&state.world, ImportModel::from_mesh("Vise", Mesh::from_facets(cube(p(0.0, 0.0, 0.0), p(80.0, 30.0, 20.0)))));
    state.history.execute(&state.world, TagFixture::new(&state.world, "Vise", None).expect("tag"));
    state.doc_version += 1;
    state
}

fn harness(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 900.0)).build_ui_state(|ui, st: &mut State| window(ui.ctx(), st), state)
}

fn library_names(state: &State) -> Vec<String> {
    let path = state.fixture_library_path().expect("path");
    fixture_library::load(&path).expect("reads").into_iter().map(|e| e.name).collect()
}

#[test]
fn the_fixtures_node_menu_offers_save_and_insert_from_library() {
    let state = State::new();
    let rows = menu_rows(&state, &TreeMenuTarget::Fixtures("Setup2".into()));
    let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, ["Fixtures", "Import fixture…", "Save fixture to library…", "Insert from library…"]);
    assert!(matches!(&rows[2].1, Some(CommandKind::FixtureLibrarySaveDialog(s)) if s == "Setup2"));
    assert!(matches!(&rows[3].1, Some(CommandKind::FixtureLibraryInsertDialog(s)) if s == "Setup2"));
}

#[test]
fn the_save_dialog_writes_the_fixture_and_asks_before_replacing() {
    let mut state = state_with_vise("save");
    state.pending.push_back(CommandKind::FixtureLibrarySaveDialog(String::new()));
    state.drain_actions();
    let Some(FixtureLibDialog::Save(d)) = &state.fixture_lib else { panic!("save dialog open") };
    assert_eq!(d.fixtures, [("Vise".to_owned(), true)], "a lone fixture starts checked");
    let mut h = harness(state);
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "saved and closed: {:?}", h.state().fixture_lib);
    assert_eq!(library_names(h.state()), ["Vise"]);

    h.state_mut().open_fixture_library_save(String::new());
    h.run();
    h.get_by_label("Save").click();
    h.run();
    let Some(FixtureLibDialog::Save(d)) = &h.state().fixture_lib else { panic!("still open") };
    assert_eq!(d.confirm_replace.as_deref(), Some(&["Vise".to_owned()][..]), "a clash asks first");
    h.get_by_label("Replace").click();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "replaced and closed");
    assert_eq!(library_names(h.state()), ["Vise"]);
}

#[test]
fn the_insert_dialog_queues_one_undoable_insert_and_delete_asks_first() {
    let mut state = state_with_vise("insert");
    state.open_fixture_library_save(String::new());
    state.save_fixtures_to_library(false);
    state.open_fixture_library_insert(String::new());
    let Some(FixtureLibDialog::Insert(d)) = state.fixture_lib.as_mut() else { panic!("insert dialog open") };
    assert_eq!(d.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Vise"]);
    assert_eq!(d.selected.as_deref(), Some("Vise"), "the first entry is picked");
    d.pos = [100.0, 0.0, 0.0];
    let cmd = insert_command(d).expect("a command");
    assert!(matches!(&cmd, CommandKind::InsertFixtureFromLibrary { entry, setup: None, pos, .. } if entry == "Vise" && pos[0] == 100.0));

    let mut h = harness(state);
    h.run();
    h.get_by_label("Insert").click();
    h.run();
    h.state_mut().drain_actions();
    let w = &h.state().world;
    let copy = model::find_by_name(w, "Vise (2)").expect("inserted under the next free name");
    assert!(fixture::def_of(w, &copy).is_some(), "tagged as a fixture");
    assert_eq!(model::transform_of(w, &copy).map(|l| l.pos.x), Some(100.0));
    assert!(h.state().fixture_lib.is_none(), "insert closes the dialog");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(model::find_by_name(&st.world, "Vise (2)").is_none(), "one undo takes the insert");

    h.state_mut().open_fixture_library_insert(String::new());
    h.run();
    h.get_by_label("Delete from library…").click();
    h.run();
    assert_eq!(library_names(h.state()), ["Vise"], "nothing deleted before the confirmation");
    h.get_by_label("Delete").click();
    h.run();
    assert!(library_names(h.state()).is_empty(), "confirmed delete edits the file");
    let Some(FixtureLibDialog::Insert(d)) = &h.state().fixture_lib else { panic!("still open") };
    assert!(d.rows.is_empty() && d.selected.is_none());
}

#[test]
fn a_corrupt_library_is_named_in_the_dialog_not_listed_as_empty() {
    let mut state = State::new();
    let dir = std::env::temp_dir().join(format!("camcraft-ui-fixlib-bad-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join(fixture_library::LIBRARY_FILE), "{\"format\":\"camcraft-fixtures\",\"version\":9}")
        .expect("write");
    state.fixture_lib_dir = Some(dir.clone());
    state.open_fixture_library_insert(String::new());
    let Some(FixtureLibDialog::Insert(d)) = &state.fixture_lib else { panic!("open") };
    assert!(d.error.as_deref().is_some_and(|e| e.contains("version 9")), "{:?}", d.error);
    assert!(insert_command(d).is_none(), "nothing to insert");
    let _ = std::fs::remove_dir_all(&dir);
}
