//! #558: a setup's WCS is a point fixed on the part in ITS setup's frame.
//! The viewport shows the active setup's frame, so another setup's WCS
//! is drawn through the part's transform between the two setups
//! (`setup_relative::wcs_view_pose`) - it stays on the same spot of the
//! part, never at its raw stored place. The active setup's own WCS, and
//! every document without setups, draw the stored pose unchanged.
//!
//! Gate map (#558): covers app::setup_relative (wcs_view_pose,
//! transform_between), app::setup (wcs_entity_name).
//! Provenance: our own fixtures; rigid-body transform algebra.

use camcraft_app::commands::{ActivateSetup, AlignPartToSetup, CreateSetup, History, ImportModel, SetStock, SetWcsOrigin};
use camcraft_app::geometry::{cube, transform_point, Point3};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use camcraft_app::{model, setup, setup_relative};
use flecs_ecs::prelude::World;

/// The part's top corner the user zeroes Setup1 on (document frame).
const CORNER: [f64; 3] = [40.0, 20.0, 10.0];

fn activate(world: &World, h: &mut History, s: &str) {
    if setup::active(world).as_deref() != Some(s) {
        h.execute(world, ActivateSetup::new(world, Some(s)).expect("activate"));
    }
}

/// A 40 x 20 x 10 block; Setup1 as modelled with its WCS on the block's
/// top corner; Setup2 holding the part turned (`preset`), active.
fn two_setups(preset: AlignPreset) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0))));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 12.0 }, Location::default()).expect("stock"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    activate(&world, &mut h, "Setup1");
    let at = Location { pos: Point3::new(CORNER[0], CORNER[1], CORNER[2]), ..Default::default() };
    h.execute(&world, SetWcsOrigin::new(&world, at).expect("Setup1's WCS on the corner"));
    activate(&world, &mut h, "Setup2");
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", preset).expect("turn the part in Setup2"));
    (world, h)
}

/// Where the corner sits in `s`'s frame: the part's own alignment there.
fn corner_in(world: &World, s: &str) -> Point3 {
    let c = Point3::new(CORNER[0], CORNER[1], CORNER[2]);
    match setup_align::stored(world, s) {
        Some(a) => transform_point(c, a.pos, a.rot),
        None => c,
    }
}

fn stored(world: &World, name: &str) -> Location {
    let e = model::find_by_name(world, name).expect("wcs entity");
    model::transform_of(world, &e).expect("wcs pose")
}

fn close(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6 && (a.z - b.z).abs() < 1e-6
}

#[test]
fn another_setups_wcs_stays_on_its_spot_of_the_part() {
    for preset in [AlignPreset::TurnZ90, AlignPreset::FlipX, AlignPreset::TurnZ180] {
        let (world, _h) = two_setups(preset);
        let s1 = setup::wcs_entity_name(&world, Some("Setup1"));
        let pose = setup_relative::wcs_view_pose(&world, &s1, stored(&world, &s1))
            .expect("mappable")
            .expect("another setup's WCS is mapped");
        let want = corner_in(&world, "Setup2");
        assert!(close(pose.pos, want), "{preset:?}: drawn at {:?}, the corner is at {want:?}", pose.pos);
        assert!(!close(pose.pos, stored(&world, &s1).pos), "{preset:?}: not left at its raw stored place");
    }
}

#[test]
fn the_active_setups_own_wcs_and_a_setupless_document_draw_as_stored() {
    let (world, mut h) = two_setups(AlignPreset::TurnZ90);
    let s2 = setup::wcs_entity_name(&world, Some("Setup2"));
    if model::find_by_name(&world, &s2).is_some() {
        assert_eq!(setup_relative::wcs_view_pose(&world, &s2, stored(&world, &s2)), Ok(None), "the active setup's own WCS");
    }
    // Showing Setup1 again: its own WCS is drawn where it is stored.
    activate(&world, &mut h, "Setup1");
    let s1 = setup::wcs_entity_name(&world, Some("Setup1"));
    assert_eq!(setup_relative::wcs_view_pose(&world, &s1, stored(&world, &s1)), Ok(None));

    let bare = new_world();
    assert_eq!(setup_relative::wcs_view_pose(&bare, "G54", Location::default()), Ok(None), "no setups: unchanged");
}

#[test]
fn switching_the_shown_setup_keeps_the_wcs_on_the_part() {
    let (world, mut h) = two_setups(AlignPreset::TurnZ90);
    let s1 = setup::wcs_entity_name(&world, Some("Setup1"));
    let in_s2 = setup_relative::wcs_view_pose(&world, &s1, stored(&world, &s1)).expect("ok").expect("mapped");
    activate(&world, &mut h, "Setup1");
    // Back in its own setup it is the stored corner again, and the part is
    // as modelled there - the same physical spot.
    assert_eq!(setup_relative::wcs_view_pose(&world, &s1, stored(&world, &s1)), Ok(None));
    assert!(close(stored(&world, &s1).pos, corner_in(&world, "Setup1")));
    activate(&world, &mut h, "Setup2");
    let again = setup_relative::wcs_view_pose(&world, &s1, stored(&world, &s1)).expect("ok").expect("mapped");
    assert!(close(in_s2.pos, again.pos), "the same spot every time Setup2 is shown");
}
