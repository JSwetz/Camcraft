//! M13.4 face origin snap through the real shell: the seed's Contour1
//! carries the Block's TOP face, so its chip offers "Set origin" (the
//! centroid) and "Set origin (Z only)" (flat face). Both queue `SetWcs`;
//! Z only keeps the slot's XY.
//! Provenance: our own page law; no vendor material consulted.

use super::*;

fn g54(h: &Harness<'static, ShellUi>) -> [f64; 3] {
    let st = &h.state().state;
    let e = model::find_by_name(&st.world, WCS_SLOTS[0]).expect("G54");
    let p = model::transform_of(&st.world, &e).unwrap_or_default().pos;
    [p.x, p.y, p.z]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs() < 1e-6
}

#[test]
fn face_chip_sets_origin_at_centroid_and_z_only_keeps_xy() {
    let mut harness = shell_harness();
    harness.run();
    // Where the top face sits in the world: its centroid through the
    // Block's placement (the seed block is 60 x 60 x 30).
    let top = {
        let st = &harness.state().state;
        let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
        let loc = model::transform_of(&st.world, &block).unwrap_or_default().pos;
        [loc.x + 30.0, loc.y + 30.0, loc.z + 30.0]
    };

    harness.get_by_label("Set origin").click();
    harness.run_steps(2);
    assert!(near(g54(&harness), top), "G54 moved to the top face's centre: {:?}", g54(&harness));

    // Elsewhere first, then Z only: XY stays, Z lands on the face.
    harness.state_mut().state.pending.push_back(CommandKind::SetWcs {
        slot: WCS_SLOTS[0].to_string(),
        pos: [5.0, 7.0, -3.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    harness.run_steps(2);
    assert!(near(g54(&harness), [5.0, 7.0, -3.0]));
    harness.get_by_label("Set origin (Z only)").click();
    harness.run_steps(2);
    assert!(near(g54(&harness), [5.0, 7.0, top[2]]), "Z only keeps XY: {:?}", g54(&harness));

    // Undoable like every origin move.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(near(g54(&harness), [5.0, 7.0, -3.0]), "undo restores the origin: {:?}", g54(&harness));
}
