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
    // Posed at the playhead: the tool runs from its tip up through the
    // body, whose foot sits the shank top above the tip.
    let f = sc.at(0.0);
    let (verts, tip) = tool_part(st).expect("the posed tool part");
    assert!((tip - f.pos.z).abs() < 1e-9);
    let zmin = verts.iter().map(|v| f64::from(v[2])).fold(f64::INFINITY, f64::min);
    let zmax = verts.iter().map(|v| f64::from(v[2])).fold(f64::NEG_INFINITY, f64::max);
    assert!((zmin - f.pos.z).abs() < 1e-3, "tip at {}: {zmin}", f.pos.z);
    assert!((zmax - (f.pos.z + top + 30.0)).abs() < 1e-3, "body top at tip + {top} + 30: {zmax}");
    st.sim = None;
    assert_eq!(world::collect(st).parts.len(), bare, "no transport, no body drawn");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// The user's #371 hands-on document: StepBlock, 90x70x28 stock, the
/// catalog end mill, Face1 generated (project v56, our own fixture).
const STEP_BLOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/egui_app/tests/step_block_371.camproj");

/// The posed tool part of the scene (the one model part that is no
/// document model) and the frame's tip height.
fn tool_part(state: &State) -> Option<(Vec<[f32; 3]>, f64)> {
    let models = model::model_entities(&state.world).len();
    let scene = world::collect(state);
    let solids: Vec<_> = scene.parts.into_iter().filter(|p| !p.stock && !p.fill).collect();
    let (_, sc) = state.sim_script_cache.as_ref()?;
    let tip = sc.at(state.sim?.t).pos.z;
    (solids.len() == models + 1).then(|| (solids.last().map(|p| p.verts.clone()).unwrap_or_default(), tip))
}

/// #371 feedback: on the user's document the collar floated a shank
/// length (55 mm) above the axis marker with nothing below it. A tool
/// with a body now draws whole in the simulation - cutter and shank up
/// into the body - at every playhead, stale or fresh.
#[test]
fn the_simulated_tool_reaches_from_its_tip_into_the_body_on_the_users_document() {
    let mut harness = shell_harness();
    harness.run();
    harness.state_mut().state.pending.push_back(CommandKind::OpenProjectFile(STEP_BLOCK.into()));
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    let path = temp_stl("user");
    prism_stl(&path, 20.0, 0.0, 50.0, 24);
    harness.state_mut().state.import_tool_body_from(&path);
    harness.state_mut().state.tool_body_ui.offset = 55.0;
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    assert!(body_source(&harness.state().state).is_some(), "status: {}", harness.state().state.status);
    harness.state_mut().state.close_tool_editor_window();
    harness.state_mut().state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    // Paused at 0, as the user saw it.
    harness.state_mut().state.sim = Some(crate::state::Sim { t: 0.0, playing: false, speed: 1.0 });
    harness.run();
    harness.run();
    let check = |st: &State, when: &str| {
        let (verts, tip) = tool_part(st).unwrap_or_else(|| panic!("{when}: no tool part in the scene"));
        let zmin = verts.iter().map(|v| f64::from(v[2])).fold(f64::INFINITY, f64::min);
        let zmax = verts.iter().map(|v| f64::from(v[2])).fold(f64::NEG_INFINITY, f64::max);
        assert!((zmin - tip).abs() < 1e-3, "{when}: the tool starts at its tip ({zmin} vs {tip})");
        assert!((zmax - (tip + 105.0)).abs() < 1e-3, "{when}: and ends at the body's top ({zmax})");
    };
    check(&harness.state().state, "t = 0, Face1 stale");
    let total = harness.state().state.sim_script_cache.as_ref().map(|(_, s)| s.total).unwrap();
    for t in [total * 0.5, total] {
        harness.state_mut().state.sim.as_mut().unwrap().t = t;
        harness.run();
        check(&harness.state().state, &format!("t = {t:.1}"));
    }
    // Regenerated (fresh), still drawn.
    harness.state_mut().state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    harness.run();
    harness.run();
    check(&harness.state().state, "after regenerate");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

