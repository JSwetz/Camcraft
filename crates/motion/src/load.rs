//! Adaptive "optimal load" (M11.4 B): the radial width of cut a roughing
//! pass may take, stated as a fraction of the tool diameter (ae/D) - the
//! way machinists and tooling catalogs specify high-speed / constant-
//! engagement roughing - with the contact angle derived from it.
//! Provenance: the straight-wall engagement geometry of peripheral
//! milling, phi = acos(1 - 2 ae / D) (the radial-engagement angle of
//! machining handbooks, e.g. Altintas, *Manufacturing Automation*, ch. 2;
//! the same relation tooling catalogs tabulate for ae/D); our own reading,
//! no vendor implementation consulted.
//!
//! Exactness is the law that lets the field change units without moving
//! a single posted coordinate: the legacy degree value `deg` stepped the
//! rings by `(D/2) * (1 - cos deg)`; [`load_of_degrees`] stores
//! `(1 - cos deg) / 2` and [`load_step`] multiplies it by `D`. Halving and
//! doubling are exact in binary floating point, so both products are the
//! same real number `D (1 - cos deg) / 2` rounded once - bit-identical
//! for every diameter and every angle a file ever carried.

/// The default optimal load: exactly the fraction today's 45-degree
/// default stepped by, `(1 - cos 45deg) / 2` = 14.64...% of D (pinned
/// bitwise against [`load_of_degrees`]`(45.0)` by the motion tests).
pub const DEFAULT_OPTIMAL_LOAD: f64 = 0.1464466094067262;

/// The largest load the ring tiling can honour: half the diameter, the
/// 90-degree contact the old door closed at - past it consecutive rings
/// stop overlapping (the waterline reach law).
pub const MAX_OPTIMAL_LOAD: f64 = 0.5;

/// The legacy degree word converted to a load (ae/D), exact by the
/// module law: `(1 - cos deg) / 2`.
#[must_use]
pub fn load_of_degrees(deg: f64) -> f64 {
    (1.0 - deg.to_radians().cos()) * 0.5
}

/// A legacy file's degree word as a load, keeping the old door's verdict:
/// inside `(0, 90]` it is [`load_of_degrees`] (exact); a non-finite word
/// stays non-finite, a non-positive one reads as a zero load and one past
/// 90 degrees as a load past half the diameter - each still refused by
/// the door exactly where the degree was, never silently re-legalized by
/// the cosine's symmetry or period.
#[must_use]
pub fn load_of_legacy_degrees(deg: f64) -> f64 {
    if !deg.is_finite() {
        deg
    } else if deg <= 0.0 {
        0.0
    } else if deg <= 90.0 {
        load_of_degrees(deg)
    } else {
        load_of_degrees(deg.min(180.0)).max(f64::from_bits(MAX_OPTIMAL_LOAD.to_bits() + 1))
    }
}

/// The contact angle a load implies on a straight wall, in degrees:
/// `acos(1 - 2 ae/D)`. A display hint only - the generators never read
/// it back (the round trip through acos/cos is not exact). Out-of-range
/// loads are clamped into the formula's domain, so the hint is always a
/// number; the parameter door refuses them separately.
#[must_use]
pub fn degrees_of_load(load: f64) -> f64 {
    (1.0 - 2.0 * load).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The ring step a load sets for a tool of `diameter`: `load * D`.
#[must_use]
pub fn load_step(diameter: f64, load: f64) -> f64 {
    load * diameter
}

/// The door both pocket ladders share: a load in `(0, 0.5]` of D.
#[must_use]
pub fn load_in_door(load: f64) -> bool {
    load.is_finite() && load > 0.0 && load <= MAX_OPTIMAL_LOAD
}

/// The straight-wall load (ae/D) a contact angle `deg` read on a rim of
/// radius `rim` stands for: the wall that rim meets at that angle sits
/// `rim cos(deg)` from the center, so `ae = r - rim cos(deg)`. Clamped
/// to `[0, 1]` (a buried tool reads a full diameter).
#[must_use]
pub fn load_at_rim(deg: f64, diameter: f64, rim: f64) -> f64 {
    let r = diameter * 0.5;
    ((r - rim * deg.min(180.0).to_radians().cos()) / diameter).clamp(0.0, 1.0)
}

/// The inverse: the contact angle a straight wall at `load` reads on a
/// rim of radius `rim` (degrees; 0 when the wall is beyond the rim).
#[must_use]
pub fn deg_at_rim(load: f64, diameter: f64, rim: f64) -> f64 {
    let r = diameter * 0.5;
    ((r - load * diameter) / rim.max(1e-12)).clamp(-1.0, 1.0).acos().to_degrees()
}
