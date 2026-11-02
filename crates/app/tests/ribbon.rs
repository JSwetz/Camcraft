//! M6.2 acceptance: the ribbon's taxonomy is the registry (one
//! Section column, a derived Family, a machine-filtered section list),
//! the pins are a document setting that rides no command, and the
//! project file carries them under the v42 law - absent key = unpinned.
//!
//! Gate map (M4.4): covers app::operation (kinds table), app::ribbon,
//! app::schema (RibbonPins), app::project (pinned key, v42).

use camcraft_app::machine::Kind;
use camcraft_app::operation::{self, Family, KindInfo, Section, KIND_COUNT};
use camcraft_app::project;
use camcraft_app::ribbon::{pinned_of, pinned_set};
use camcraft_app::schema::RibbonPins;
use camcraft_app::world::new_world;

#[test]
fn sections_partition_the_registry() {
    // One home for every row, no row in two homes, homes are runs in
    // the table (the product order the ribbon and combo both draw on).
    let mut covered = vec![0usize; KIND_COUNT];
    for s in Section::ORDER {
        let kinds = s.kinds();
        assert!(!kinds.is_empty(), "{s:?} owns no kind");
        for w in kinds.windows(2) {
            assert_eq!(w[1], w[0] + 1, "{s:?} is not a contiguous run");
        }
        for i in &kinds {
            covered[*i] += 1;
        }
    }
    assert!(
        covered.iter().all(|c| *c == 1),
        "sections must cover the table exactly once: {covered:?}"
    );
    for (i, k) in KindInfo::ALL.iter().enumerate() {
        assert!(k.section.kinds().contains(&i), "row {i} homeless");
    }
}

#[test]
fn family_is_derived_not_forked() {
    // The old `family` field's truth, restated as a law over `section`:
    // the turning rows (the trailing run - the M5.2 ladder, wherever
    // the middle insertions have shifted it since) are the Turning
    // section and nothing Turn-likes lives elsewhere - so
    // `KindInfo::family()` cannot drift from `Section::family()`. The
    // boundary is the table's, not a remembered number: a middle
    // insertion moves the rows, not the law (M2.4 B).
    let turn_start = KindInfo::ALL
        .iter()
        .position(|k| k.section == Section::Turning)
        .expect("turning rows exist");
    for (i, k) in KindInfo::ALL.iter().enumerate() {
        let want = if i >= turn_start {
            Family::Turn
        } else {
            Family::Mill
        };
        assert_eq!(k.family(), want, "row {i} ({}) family drifted", k.label);
    }
    for s in Section::ORDER {
        for i in s.kinds() {
            assert_eq!(
                KindInfo::ALL[i].family(),
                s.family(),
                "row {i} disagrees with its section's family"
            );
        }
    }
}

#[test]
fn shown_for_is_the_machine_filter() {
    // The hiding law in one table: a mill document shows the four
    // mill sections in ORDER, a lathe only Turning, a mill-turn all
    // five - and nothing else is ever listed, whatever else changes.
    assert_eq!(
        operation::shown_for(Kind::Mill),
        [Section::Mill2D, Section::Mill3D, Section::Drilling, Section::MultiAxis]
    );
    assert_eq!(operation::shown_for(Kind::Lathe), [Section::Turning]);
    assert_eq!(operation::shown_for(Kind::MillTurn), Section::ORDER);
}

#[test]
fn pins_clean_on_write_and_strangers_on_read() {
    let world = new_world();
    // Write law: order kept, duplicates and out-of-range dropped.
    pinned_set(&world, &[0, 0, KIND_COUNT, 5, 9999, 29]);
    assert_eq!(pinned_of(&world), vec![0, 5, 29], "write must clean");
    // Read law (the theme's stranger rule in the pin's clothes): a
    // list that reaches past this build's table still reads back the
    // kinds this build can name.
    world.set(RibbonPins(vec![29, KIND_COUNT, 3]));
    assert_eq!(pinned_of(&world), vec![29, 3], "read must filter");
    // No singleton at all (a virgin world) reads as unpinned.
    let fresh = new_world();
    assert!(pinned_of(&fresh).is_empty());
}

#[test]
fn pins_ride_the_project_and_the_default_writes_no_key() {
    // v42: an unpinned document pays nothing for the field...
    let path = std::env::temp_dir().join(format!(
        "camcraft_ribbon_rt_{}.camproj",
        std::process::id()
    ));
    let world = new_world();
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        !text.contains("\"pinned\""),
        "the default must omit the key (the mm/units law)"
    );
    // ...a pinned one names the indices in pin order...
    pinned_set(&world, &[28, 2]);
    project::save(&world, &path).expect("resave");
    let pinned_text = std::fs::read_to_string(&path).expect("reread");
    let file: serde_json::Value = serde_json::from_str(&pinned_text).expect("json");
    assert_eq!(file["pinned"], serde_json::json!([28, 2]));
    assert_eq!(file["version"], serde_json::json!(project::PROJECT_VERSION));
    // ...and loading lands them exactly, through the apply seam (the
    // leak law: a stale singleton cannot survive an open).
    let world2 = new_world();
    pinned_set(&world2, &[7]);
    project::load(&world2, &path).expect("load");
    assert_eq!(pinned_of(&world2), vec![28, 2], "load must land, not merge");
    // Round-trip byte-shape: the reopened document saves as it came.
    project::save(&world2, &path).expect("resave2");
    let second = std::fs::read_to_string(&path).expect("read2");
    let _ = std::fs::remove_file(&path);
    assert_eq!(pinned_text, second, "save/load/save must be byte-identical");

    // The v41 shape: the key simply was not there, and the file opens.
    let world3 = new_world();
    project::open(&world3, r#"{"version": 41, "entities": []}"#).expect("v41 opens");
    assert!(pinned_of(&world3).is_empty(), "absent means unpinned");
}

#[test]
fn v42_pins_shift_for_the_twins_row_insert() {
    // M2.4 A put two rows in the MIDDLE of the 2D mill's run, so a
    // v42 file's pin indices name rows that moved: 0..=4 still name
    // the same kind, and from the Drill row up each gains the two
    // new rows' worth. The rungs CHAIN (M2.4 B proved it from the
    // other side): v42 -> v43 by the twins' rung, then v43 -> v44 by
    // the M2.4 B rung, so the old Drill row lands three + two past
    // its old home in the final table. The PostDto law seen from the
    // other side - names survive growth, indices need a rung.
    let world = new_world();
    project::open(
        &world,
        r#"{"version": 42, "pinned": [4, 5, 29], "entities": []}"#,
    )
    .expect("a v42 pinned file steps up");
    let pins = pinned_of(&world);
    assert_eq!(
        pins,
        vec![4, 10, 34],
        "Trace stays; the Drill row and the last turning row take both rungs"
    );
    assert_eq!(KindInfo::ALL[pins[1]].ty, operation::OpType::Drill);
    assert_eq!(KindInfo::ALL[pins[2]].ty, operation::OpType::BoringTurning);
}

#[test]
fn v43_pins_shift_for_the_m24b_row_insert() {
    // The same law one rung closer: M2.4 B put three rows after
    // Trace (tags 7, 8, 9), so a v43 file's indices 0..=6 still name
    // their kinds and 7 up gains three. The Offset row (6) is the
    // run's last old name; the Drill row (7) is the run's first
    // shifted one.
    let world = new_world();
    project::open(
        &world,
        r#"{"version": 43, "pinned": [6, 7, 31], "entities": []}"#,
    )
    .expect("a v43 pinned file steps up");
    let pins = pinned_of(&world);
    assert_eq!(pins, vec![6, 10, 34], "below the insert stays, from Drill up shifts");
    assert_eq!(KindInfo::ALL[pins[0]].ty, operation::OpType::Offset2D);
    assert_eq!(KindInfo::ALL[pins[1]].ty, operation::OpType::Drill);
    assert_eq!(KindInfo::ALL[pins[2]].ty, operation::OpType::BoringTurning);
}
