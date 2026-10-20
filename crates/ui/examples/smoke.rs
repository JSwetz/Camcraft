//! Headless smoke: import -> undo -> redo -> save -> reopen, via the same
//! command queue the shell feeds. Prints a pass/fail line per stage.
use std::path::PathBuf;

use hsmworks_app::model;
use hsmworks_ui::state::{CommandKind, State};

fn count(state: &State) -> usize {
    model::named_entities(&state.world).len()
}

fn main() {
    let mut ok = true;
    let mut check = |label: &str, cond: bool, state: &State| {
        println!("{} {} (status: {})", if cond { "PASS" } else { "FAIL" }, label, state.status);
        if !cond { ok = false; }
    };

    let mut state = State::new();
    check("starts empty", count(&state) == 0, &state);

    state.pending.push_back(CommandKind::ImportFile(PathBuf::from("testdata/models/cube.stl")));
    state.pending.push_back(CommandKind::ImportFile(PathBuf::from("testdata/models/box.obj")));
    state.settle();
    check("two imports land", count(&state) == 2, &state);

    state.pending.push_back(CommandKind::Select(Some("cube".into())));
    state.settle();
    check("select cube", state.has_selection(), &state);

    state.pending.push_back(CommandKind::DeleteSelected);
    state.settle();
    check("delete selected", count(&state) == 1, &state);

    state.pending.push_back(CommandKind::Undo);
    state.settle();
    check("undo restore", count(&state) == 2, &state);

    state.pending.push_back(CommandKind::Redo);
    state.settle();
    check("redo delete", count(&state) == 1, &state);

    state.pending.push_back(CommandKind::Undo);
    state.pending.push_back(CommandKind::ImportFile(PathBuf::from("testdata/models/garbage.stl")));
    state.settle();
    check("corrupt import refused, world intact", count(&state) == 2, &state);

    let tmp = std::env::temp_dir().join("hsmworks-smoke.hsmproj");
    state.pending.push_back(CommandKind::SaveProjectFile(tmp.clone()));
    state.settle();
    check("save", state.status.starts_with("saved"), &state);

    let mut reopened = State::new();
    reopened.pending.push_back(CommandKind::OpenProjectFile(tmp.clone()));
    reopened.settle();
    check("reopen matches", count(&reopened) == count(&state), &reopened);

    // Byte-stability: load -> save must reproduce the file exactly.
    let first = std::fs::read(&tmp).unwrap_or_default();
    let mut saver = State::new();
    saver.pending.push_back(CommandKind::OpenProjectFile(tmp.clone()));
    saver.settle();
    let again = std::env::temp_dir().join("hsmworks-smoke-resave.hsmproj");
    saver.pending.push_back(CommandKind::SaveProjectFile(again.clone()));
    saver.settle();
    check("save is byte-stable", first == std::fs::read(&again).unwrap_or_default(), &saver);

    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&again);
    println!("SMOKE {}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}
