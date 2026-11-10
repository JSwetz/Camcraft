//! #606: the Setups page's typed part-alignment turn pivots on the
//! setup's WCS origin by default - the point of the part on the origin
//! stays on it (the maintainer's "it shifts the model relative to the
//! origin") - or on the parts' bounds centre by choice, the presets'
//! pivot, which is unchanged. One undoable `AlignPartToSetup`.
//! Provenance: our own fixtures over plain re-clamp practice; no vendor
//! material consulted.
//!
//! Gate map (#606): covers app::setup_turn, app::commands
//! (AlignPartToSetup::turn, ::angles, ::preset).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AlignPartToSetup, CreateSetup, History, ImportModel, SetWcsOrigin};
use camcraft_app::geometry::{Point3, cube, transform_point};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::setup_turn::{TurnPivot, wcs_origin};
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;

/// A 40 x 20 x 8 part spanning (-20, -10, 0) .. (20, 10, 8), with Setup1
/// active and, if given, its WCS placed at `wcs`.
fn part(wcs: Option<[f64; 3]>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 8.0))));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if let Some([x, y, z]) = wcs {
        let at = Location { pos: Point3::new(x, y, z), ..Location::default() };
        h.execute(&world, SetWcsOrigin::new(&world, at).expect("place the WCS"));
    }
    (world, h)
}

/// A document point as the setup holds it.
fn held(world: &World, p: [f64; 3]) -> [f64; 3] {
    let a = setup_align::stored(world, "Setup1").unwrap_or_default();
    let q = transform_point(Point3::new(p[0], p[1], p[2]), a.pos, a.rot);
    [q.x, q.y, q.z]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < TIGHT)
}

#[test]
fn a_typed_turn_keeps_the_point_on_the_wcs_origin_fixed_and_undo_redo_walk_it() {
    // The WCS on the part's SW bottom corner.
    let o = [-20.0, -10.0, 0.0];
    let (w, mut h) = part(Some(o));
    assert!(near([wcs_origin(&w, "Setup1").x, wcs_origin(&w, "Setup1").y, wcs_origin(&w, "Setup1").z], o));
    h.execute(&w, AlignPartToSetup::turn(&w, "Setup1", [0.0, 0.0, 90.0], TurnPivot::WcsOrigin).expect("turned"));
    assert!(near(held(&w, o), o), "the corner on the origin stays on it: {:?}", held(&w, o));
    // The rest turns about it: the NE bottom corner (40, 20 away) lands
    // at (-20, 40) from the origin.
    assert!(near(held(&w, [20.0, 10.0, 0.0]), [-40.0, 30.0, 0.0]), "{:?}", held(&w, [20.0, 10.0, 0.0]));
    // Not the centre pivot: the bounds centre moved.
    assert!(!near(held(&w, [0.0, 0.0, 4.0]), [0.0, 0.0, 4.0]), "the centre is not the pivot");
    assert_eq!(setup_align::angles_of(setup_align::stored(&w, "Setup1")), [0.0, 0.0, 90.0]);
    let after = setup_align::stored(&w, "Setup1");
    assert!(h.undo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), None, "undo restores as modelled");
    assert!(h.redo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), after, "redo turns it again");
}

#[test]
fn with_no_wcs_placed_the_frame_zero_is_the_pivot() {
    let (w, mut h) = part(None);
    h.execute(&w, AlignPartToSetup::turn(&w, "Setup1", [30.0, 0.0, 45.0], TurnPivot::WcsOrigin).expect("turned"));
    assert!(near(held(&w, [0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]), "{:?}", held(&w, [0.0; 3]));
}

#[test]
fn a_turned_part_keeps_the_point_now_on_the_origin_and_the_same_angles_change_nothing() {
    let o = [-20.0, -10.0, 0.0];
    let (w, mut h) = part(Some(o));
    // A preset quarter turn about the centre first: the part point now on the
    // origin is the flipped part's, not the as-modelled one.
    h.execute(&w, AlignPartToSetup::preset(&w, "Setup1", AlignPreset::TurnZ90).expect("preset"));
    let a = setup_align::stored(&w, "Setup1").expect("held");
    let inv = setup_align::inverse(a);
    let on_o = transform_point(Point3::new(o[0], o[1], o[2]), inv.pos, inv.rot);
    let on_o = [on_o.x, on_o.y, on_o.z];
    h.execute(&w, AlignPartToSetup::turn(&w, "Setup1", [0.0, 0.0, 180.0], TurnPivot::WcsOrigin).expect("turned"));
    assert!(near(held(&w, on_o), o), "the point that was on the origin stays: {:?}", held(&w, on_o));
    // Re-applying the same angles about the origin is the same placement.
    let again = AlignPartToSetup::turn(&w, "Setup1", [0.0, 0.0, 180.0], TurnPivot::WcsOrigin);
    assert!(again.is_err_and(|e| e.to_string().contains("already holds")), "the same turn changes nothing");
}

#[test]
fn the_part_centre_pivot_is_the_presets_and_all_zero_is_as_modelled() {
    let (w, mut h) = part(Some([-20.0, -10.0, 0.0]));
    let by_centre = AlignPartToSetup::turn(&w, "Setup1", [180.0, 0.0, 0.0], TurnPivot::PartCentre).expect("turn");
    h.execute(&w, by_centre);
    assert!(near(held(&w, [0.0, 0.0, 4.0]), [0.0, 0.0, 4.0]), "the centre stays put");
    let stored = setup_align::stored(&w, "Setup1");
    assert!(h.undo(&w));
    h.execute(&w, AlignPartToSetup::preset(&w, "Setup1", AlignPreset::FlipX).expect("preset"));
    assert_eq!(setup_align::stored(&w, "Setup1"), stored, "the preset turns about the same centre");
    for pivot in TurnPivot::ALL {
        let mut h2 = History::default();
        h2.execute(&w, AlignPartToSetup::turn(&w, "Setup1", [0.0; 3], pivot).expect("back"));
        assert_eq!(setup_align::stored(&w, "Setup1"), None, "{pivot:?}: all zero is as modelled");
        assert!(h2.undo(&w));
    }
}

#[test]
fn a_non_finite_angle_and_a_missing_setup_are_refused_by_name() {
    let (w, _h) = part(None);
    let e = AlignPartToSetup::turn(&w, "Setup1", [f64::NAN, 0.0, 0.0], TurnPivot::WcsOrigin).err().map(|e| e.to_string());
    assert!(e.is_some_and(|m| m.contains("finite")));
    let e = AlignPartToSetup::turn(&w, "Nope", [0.0, 0.0, 90.0], TurnPivot::WcsOrigin).err().map(|e| e.to_string());
    assert!(e.is_some_and(|m| m.contains("Nope")));
}
