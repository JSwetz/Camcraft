//! #326 in the surface: the Setups page's part-alignment control queues
//! `AlignPart` (never a world poke), the drain lands it as one undo unit,
//! Ctrl+Z / Ctrl+Y walk it, the typed angles reach the same command, and
//! a refused choice pushes nothing. The work offset is not touched.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, transform_point};
use camcraft_app::setup_align::{self, AlignPreset};

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_part() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 8.0)));
    state.history.execute(&state.world, part);
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    state
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_align_pick::picker(ui, st, "Setup1");
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

fn held(state: &State) -> String {
    super::align_text(setup_align::stored(&state.world, "Setup1"))
}

#[test]
fn the_alignment_control_turns_the_part_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(frame, with_part());
    h.run();
    assert_eq!(held(h.state()), "As modelled");
    assert!(h.state().pending.is_empty(), "drawing the control queues nothing");
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("As modelled")).click();
    h.run();
    h.get_by_label("Flip over X").click();
    h.run();
    assert_eq!(held(h.state()), "Flip over X", "the drain lands the flip");
    assert_eq!(h.state().status, "Setup1 part alignment: Flip over X");
    assert_eq!(camcraft_app::setup::work_offset(&h.state().world, "Setup1"), None, "the work offset is untouched");
    key(&mut h, Key::Z);
    assert_eq!(held(h.state()), "As modelled", "undo restores as modelled");
    key(&mut h, Key::Y);
    assert_eq!(held(h.state()), "Flip over X", "redo restores the flip");
}

#[test]
fn typed_angles_reach_the_same_command_and_a_refusal_says_why() {
    let mut state = with_part();
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 90.0] });
    assert_eq!(held(&state), AlignPreset::TurnZ90.label());
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [30.0, 0.0, 0.0] });
    assert_eq!(held(&state), "turned X 30°, Y 0°, Z 0°");
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [30.0, 0.0, 0.0] });
    assert!(state.status.contains("part alignment not changed"), "{}", state.status);
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [f64::NAN, 0.0, 0.0] });
    assert!(state.status.contains("finite"), "{}", state.status);
    // Two turns landed, the refusals pushed nothing.
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(held(&state), "As modelled");
}

/// The anchors of the active setup's "model bounds" box, at the bottom
/// level: (SW, NE, C) points.
fn model_anchors(state: &mut State) -> [[f64; 3]; 3] {
    state.wcs_ui.anchor_source = 1;
    state.wcs_ui.anchor_z = 0;
    assert_eq!(crate::interact::anchor_reference(state).map(|(_, w)| w), Some("model bounds"));
    let pts = crate::interact::anchor_points(state);
    assert_eq!((pts[6].1, pts[2].1, pts[4].1), ("SW", "NE", "C"));
    [pts[6].0, pts[2].0, pts[4].0]
}

/// The part's own eight corners carried through the stored alignment by
/// plain transform math, boxed: what the operator sees on the table.
fn aligned_corner_box(state: &State) -> ([f64; 3], [f64; 3]) {
    let a = setup_align::stored(&state.world, "Setup1").unwrap_or_default();
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for x in [-20.0, 20.0] {
        for y in [-10.0, 10.0] {
            for z in [0.0, 8.0] {
                let p = transform_point(Point3::new(x, y, z), a.pos, a.rot);
                for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                    lo[i] = lo[i].min(v);
                    hi[i] = hi[i].max(v);
                }
            }
        }
    }
    (lo, hi)
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// Review of PR #341: the work-offset anchors read the part in the active
/// setup's frame, so an aligned part's corner is the corner on the table,
/// and a G54 placed on it lands there. An untouched setup is unchanged.
#[test]
fn model_bounds_anchors_follow_the_aligned_part() {
    let mut state = with_part();
    assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup1"));
    // As modelled: the 40 x 20 x 8 part's own corners, as before #326.
    let plain = model_anchors(&mut state);
    assert!(near(plain[0], [-20.0, -10.0, 0.0]) && near(plain[1], [20.0, 10.0, 0.0]), "{plain:?}");
    assert!(near(plain[2], [0.0, 0.0, 0.0]), "{plain:?}");

    // Flipped over X about its centre: the box keeps its footprint.
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [180.0, 0.0, 0.0] });
    let flipped = model_anchors(&mut state);
    let (lo, hi) = aligned_corner_box(&state);
    assert!(near(flipped[0], [lo[0], lo[1], lo[2]]) && near(flipped[1], [hi[0], hi[1], lo[2]]), "{flipped:?}");
    assert!(near(flipped[0], [-20.0, -10.0, 0.0]), "{flipped:?}");

    // Flipped AND quarter-turned: the part now runs 20 along X, 40 along Y.
    for angles in [[0.0, 0.0, 90.0], [180.0, 0.0, 90.0]] {
        run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles });
        let turned = model_anchors(&mut state);
        let (lo, hi) = aligned_corner_box(&state);
        assert!(near(turned[0], [lo[0], lo[1], lo[2]]), "{angles:?} SW {turned:?} vs {lo:?}");
        assert!(near(turned[1], [hi[0], hi[1], lo[2]]), "{angles:?} NE {turned:?} vs {hi:?}");
        assert!(near(turned[0], [-10.0, -20.0, 0.0]), "{angles:?} SW pinned: {turned:?}");
        assert!(near(turned[1], [10.0, 20.0, 0.0]), "{angles:?} NE pinned: {turned:?}");
        assert!(near(turned[2], [0.0, 0.0, 0.0]), "{angles:?} C pinned: {turned:?}");
    }

    // A G54 placed on the SW corner lands on the turned part's corner.
    let sw = model_anchors(&mut state)[0];
    run(&mut state, CommandKind::SetWcs { slot: "G54".into(), pos: sw, roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    let g54 = camcraft_app::setup::wcs_entity(&state.world, Some("Setup1"), 0)
        .and_then(|e| camcraft_app::model::transform_of(&state.world, &e))
        .map(|l| [l.pos.x, l.pos.y, l.pos.z]);
    assert!(g54.is_some_and(|p| near(p, [-10.0, -20.0, 0.0])), "G54 at {g54:?}");

    // Back to as modelled: the anchors are the untouched ones again.
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 0.0] });
    assert_eq!(model_anchors(&mut state), plain);
}
