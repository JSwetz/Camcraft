//! The adaptive optimal load (M11.4 B): the user's number is the radial
//! width as a fraction of the tool diameter, the contact angle is derived
//! (`acos(1 - 2 ae/D)`), and the unit change moves no posted coordinate -
//! the default and every legacy degree value step the rings to the bit.
//!
//! Gate map (M11.4 B): covers motion::load, motion::machining2d (pocket
//! tiling step), motion::machining3d (Pocket3DParams tiling step).

use camcraft_motion::entry::engagement_step;
use camcraft_motion::load::{
    degrees_of_load, load_in_door, load_of_degrees, load_of_legacy_degrees, load_step,
    DEFAULT_OPTIMAL_LOAD, MAX_OPTIMAL_LOAD,
};

#[test]
fn the_default_load_is_the_old_45_degree_step_to_the_bit() {
    assert_eq!(DEFAULT_OPTIMAL_LOAD.to_bits(), load_of_degrees(45.0).to_bits());
    // 14.64% of D, the figure the old page hint quoted.
    assert!((DEFAULT_OPTIMAL_LOAD * 100.0 - 14.644_660_940_672_62).abs() < 1e-9);
    assert!((degrees_of_load(DEFAULT_OPTIMAL_LOAD) - 45.0).abs() < 1e-9);
}

#[test]
fn every_legacy_angle_steps_the_rings_bit_identically() {
    // The old law: (D/2)(1 - cos a). The new: ((1 - cos a)/2) D. Halving
    // is exact, so the two round the same real number once.
    let diameters = [0.5, 1.0, 3.175, 6.0, 8.0, 10.0, 12.7, 16.0, 20.0, 25.4, 63.0];
    for d in diameters {
        for k in 1..=900 {
            let deg = f64::from(k) * 0.1;
            let old = engagement_step(d * 0.5, deg);
            let new = load_step(d, load_of_legacy_degrees(deg));
            assert_eq!(old.to_bits(), new.to_bits(), "D {d} at {deg} deg: {old} vs {new}");
        }
    }
}

#[test]
fn the_angle_is_derived_from_the_load() {
    // Full-slot geometry checks: a quarter of D is 60 degrees of contact,
    // half of D is 90, a tenth is acos(0.8).
    assert!((degrees_of_load(0.25) - 60.0).abs() < 1e-9);
    assert!((degrees_of_load(0.5) - 90.0).abs() < 1e-9);
    assert!((degrees_of_load(0.1) - 0.8f64.acos().to_degrees()).abs() < 1e-12);
    // Monotone, and always a number - even for loads the door refuses.
    assert!(degrees_of_load(0.05) < degrees_of_load(0.2));
    assert!(degrees_of_load(-1.0).is_finite() && degrees_of_load(7.0).is_finite());
}

#[test]
fn the_legacy_door_keeps_its_verdicts() {
    // What the degree door refused, the load door still refuses - the
    // cosine's symmetry (-45) and period (405) never re-legalize a word.
    for bad in [0.0, -45.0, 90.5, 91.0, 180.0, 405.0, f64::NAN, f64::INFINITY] {
        assert!(!load_in_door(load_of_legacy_degrees(bad)), "{bad} must stay refused");
    }
    for good in [0.1, 10.0, 45.0, 60.0, 89.9, 90.0] {
        assert!(load_in_door(load_of_legacy_degrees(good)), "{good} must stay legal");
    }
    assert!(load_in_door(MAX_OPTIMAL_LOAD) && !load_in_door(0.0));
}
