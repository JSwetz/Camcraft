//! #371: the tool editor's Body section - Import body / Clear body show
//! on an existing tool, Apply body lands one undoable `SetToolBody`, a
//! degenerate placement is refused by name in the editor, and a pick
//! only fills the draft. Provenance: our own page law; the test holder
//! is our own geometry.

use super::*;

const TOOL: &str = "EM-0600-4FL";

/// A closed `n`-gon prism about model Z over `[z0, z1]` as a binary STL.
fn prism_stl(path: &std::path::Path, r: f32, z0: f32, z1: f32, n: usize) {
    let q = |k: usize, z: f32| {
        let a = std::f32::consts::TAU * k as f32 / n as f32;
        [r * a.cos(), r * a.sin(), z]
    };
    let mut tris: Vec<[[f32; 3]; 3]> = Vec::new();
    for k in 0..n {
        let (a0, a1, b0, b1) = (q(k, z0), q(k + 1, z0), q(k, z1), q(k + 1, z1));
        tris.push([a0, a1, b1]);
        tris.push([a0, b1, b0]);
        tris.push([[0.0, 0.0, z0], a1, a0]);
        tris.push([[0.0, 0.0, z1], b0, b1]);
    }
    let mut bytes = vec![0u8; 80];
    bytes.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for t in &tris {
        bytes.extend_from_slice(&[0u8; 12]);
        for v in t {
            for c in v {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0u8; 2]);
    }
    std::fs::write(path, bytes).unwrap();
}

fn temp_stl(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cc-371-ui-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("holder.stl")
}

/// The seeded harness with the editor open on the catalog end mill.
fn editor() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness
}

fn shank_top(state: &State) -> f64 {
    let t = model::find_by_name(&state.world, TOOL).and_then(|e| model::tool_of(&state.world, &e)).unwrap();
    t.flute_len + t.shank_len
}

fn body_source(state: &State) -> Option<String> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .map(|b| b.source)
}

#[test]
fn the_editor_offers_import_and_clear_body() {
    let harness = editor();
    assert!(harness.query_by_label("Import body…").is_some(), "Import body… stands in the editor");
    assert!(harness.query_by_label("Clear body").is_some(), "Clear body stands in the editor");
    assert!(harness.query_by_label("Apply body").is_none(), "no draft, nothing to apply");
}

#[test]
fn apply_body_lands_one_set_tool_body_and_undo_restores() {
    let mut harness = editor();
    let path = temp_stl("apply");
    prism_stl(&path, 15.0, 0.0, 30.0, 24);
    let top = shank_top(&harness.state().state);
    {
        let st = &mut harness.state_mut().state;
        st.import_tool_body_from(&path);
        // The model origin sits on the collar's foot: put it on the shank top.
        st.tool_body_ui.offset = top;
    }
    harness.run();
    assert!(body_source(&harness.state().state).is_none(), "importing alone never touches the world");
    assert!(harness.query_by_label("Pick tip").is_some() && harness.query_by_label("Pick axis").is_some());
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!(body_source(st).as_deref(), Some("holder.stl"), "status: {}", st.status);
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(body_source(&harness.state().state).is_none(), "one undo takes the body off - one step");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert_eq!(body_source(&harness.state().state).as_deref(), Some("holder.stl"), "redo puts it back");
    // Clear body is a command too, and undoes.
    harness.get_by_label("Clear body").click();
    harness.run();
    harness.run();
    assert!(body_source(&harness.state().state).is_none(), "cleared");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(body_source(&harness.state().state).is_some(), "undo restores the cleared body");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_degenerate_placement_is_refused_by_name_in_the_editor() {
    let mut harness = editor();
    let path = temp_stl("refuse");
    prism_stl(&path, 15.0, 5.0, 30.0, 24);
    let top = shank_top(&harness.state().state);
    {
        let st = &mut harness.state_mut().state;
        st.import_tool_body_from(&path);
        // +X runs beside a collar standing on model Z: the axis misses it.
        st.tool_body_ui.axis = [1.0, 0.0, 0.0];
        st.tool_body_ui.offset = top + 20.0;
    }
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    assert!(body_source(&harness.state().state).is_none(), "nothing lands");
    assert!(
        harness.query_all_by_label_contains("tool body not set").next().is_some(),
        "the refusal shows in the editor (status: {})",
        harness.state().state.status
    );
    assert!(harness.query_all_by_label_contains("surround the tool axis").next().is_some(), "by name");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_tip_pick_fills_the_offset_and_queues_nothing() {
    let mut harness = editor();
    let path = temp_stl("pick");
    prism_stl(&path, 15.0, 10.0, 30.0, 24);
    harness.state_mut().state.import_tool_body_from(&path);
    harness.run();
    harness.get_by_label("Pick tip").click();
    harness.run();
    let st = &mut harness.state_mut().state;
    assert_eq!(st.tool_body_ui.pick, crate::state::BodyPickMode::Tip);
    let hit = camcraft_app::tool_body_view::pick(&st.tool_body_ui.facets, [0.0, 1.0, -100.0], [0.0, 0.0, 1.0])
        .expect("the bottom cap is under the ray");
    st.apply_body_pick(hit);
    assert!((st.tool_body_ui.offset + 10.0).abs() < 1e-6, "the bottom corner at z 10 is the tip: {}", st.tool_body_ui.offset);
    assert_eq!(st.tool_body_ui.pick, crate::state::BodyPickMode::Off, "one pick per arming");
    assert!(st.pending.is_empty(), "a pick is not a command");
    assert!(body_source(st).is_none());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_missing_file_is_refused_by_name() {
    let mut harness = editor();
    harness.state_mut().state.import_tool_body_from(std::path::Path::new("/nonexistent/holder.stl"));
    harness.run();
    assert!(harness.query_all_by_label_contains("tool body not imported").next().is_some());
}

#[test]
fn the_simulation_draws_the_body_at_the_playhead_and_nothing_without_one() {
    let mut harness = editor();
    let st = &mut harness.state_mut().state;
    st.sim = Some(crate::state::Sim { t: 0.0, playing: false, speed: 1.0 });
    let sc = super::super::shell::cached_script(st);
    assert!(!sc.is_empty(), "the seed's Contour1 runs");
    let bare = world::collect(st).parts.len();
    let path = temp_stl("sim");
    prism_stl(&path, 15.0, 0.0, 30.0, 24);
    let top = shank_top(st);
    st.import_tool_body_from(&path);
    st.tool_body_ui.offset = top;
    st.queue_apply_tool_body();
    st.drain_actions();
    assert!(body_source(st).is_some(), "status: {}", st.status);
    let sc = super::super::shell::cached_script(st);
    let scene = world::collect(st);
    assert_eq!(scene.parts.len(), bare + 1, "the body is one more part");
    // Posed at the playhead: its lowest vertex sits the shank top above the tip.
    let f = sc.at(0.0);
    let body = scene
        .parts
        .iter()
        .find(|p| !p.stock && !p.fill && p.verts.len() == 96 * 3)
        .expect("the posed body part");
    let zmin = body.verts.iter().map(|v| f64::from(v[2])).fold(f64::INFINITY, f64::min);
    assert!((zmin - (f.pos.z + top)).abs() < 1e-3, "foot at tip + {top}: {zmin} vs {}", f.pos.z);
    st.sim = None;
    assert_eq!(world::collect(st).parts.len(), bare, "no transport, no body drawn");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
