//! Unit tests of the lathe body's clearance description (#369):
//! conservative for every point of every facet, tight on the spans it
//! states, refusing by name.

use super::*;

/// A closed axis-aligned box over `[lo, hi]`, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Tri> {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

/// A tiny deterministic generator (no dependencies in this crate).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

#[test]
fn every_point_of_every_facet_lies_within_its_bands_radii() {
    // A random soup of facets around the tip; for random points on them
    // and random tip radii (either side of the axis), the distance from
    // the spindle must sit inside the radii the band stack gives at
    // that point's z.
    let mut g = Lcg(0x369);
    let mut tris = Vec::new();
    for _ in 0..60 {
        let mut p = || [g.next() * 60.0 - 20.0, g.next() * 30.0 - 15.0, g.next() * 40.0 - 10.0];
        tris.push([p(), p(), p()]);
    }
    let c = LatheClearance::of(&tris).expect("a sane soup");
    for _ in 0..20_000 {
        let t = &tris[(g.next() * tris.len() as f64) as usize % tris.len()];
        let (mut a, mut b) = (g.next(), g.next());
        if a + b > 1.0 {
            (a, b) = (1.0 - a, 1.0 - b);
        }
        let p: Vec<f64> = (0..3).map(|k| t[0][k] + (t[1][k] - t[0][k]) * a + (t[2][k] - t[0][k]) * b).collect();
        let x_tip = g.next() * 80.0 - 40.0;
        let r = (x_tip + p[0]).hypot(p[1]);
        let reach = c.over(p[2], p[2]).expect("a facet point has a band");
        let (lo, hi) = reach.radii(x_tip);
        assert!(lo <= r + 1e-9 && r <= hi + 1e-9, "point {p:?} tip {x_tip}: r {r} outside [{lo}, {hi}]");
    }
}

#[test]
fn a_facet_crossing_bands_with_no_vertex_in_them_still_counts() {
    // A sliver from z 0 to z 20 whose vertices sit only at its ends:
    // every band between must still carry its x and its |y|.
    let tris = vec![[[30.0, 4.0, 0.0], [30.0, 5.0, 0.0], [30.0, 4.0, 20.0]], [[-1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 0.5]]];
    let c = LatheClearance::of(&tris).expect("a sane body");
    for k in 1..=79 {
        let z = f64::from(k) * 0.25;
        let r = c.over(z, z).expect("the sliver crosses every band");
        assert!(r.x_hi >= 30.0 && r.y_hi >= 4.0, "z {z}: {r:?}");
    }
}

#[test]
fn the_spans_are_tight_for_a_box_and_y_lifts_the_floor_off_the_plane() {
    // A holder shank beside the tool plane: x 5..40, y 3..8, z 0..20.
    let c = LatheClearance::of(&cuboid([5.0, 3.0, 0.0], [40.0, 8.0, 20.0])).expect("a box");
    let r = c.over(10.0, 10.0).expect("inside");
    assert!((r.x_lo - 5.0).abs() < 1e-5 && (r.x_hi - 40.0).abs() < 1e-5, "{r:?}");
    assert!((r.y_lo - 3.0).abs() < 1e-5 && (r.y_hi - 8.0).abs() < 1e-5, "{r:?}");
    // Tip at radius 20: the body's nearest point to the axis is at
    // (25, 3) - hypot 25.18 - not 25.
    let (lo, hi) = r.radii(20.0);
    assert!((lo - 25.0f64.hypot(3.0)).abs() < 1e-4 && (hi - 60.0f64.hypot(8.0)).abs() < 1e-4, "{lo} {hi}");
    // A box straddling the plane has no floor from y.
    let s = LatheClearance::of(&cuboid([5.0, -3.0, 0.0], [40.0, 8.0, 20.0])).expect("a box");
    assert_eq!(s.over(10.0, 10.0).map(|r| r.y_lo), Some(0.0));
    // Crossing the axis: the floor is 0 in x.
    assert!(r.radii(-20.0).0 <= 3.0 + 1e-5, "{:?}", r.radii(-20.0));
}

#[test]
fn a_gap_along_the_spindle_reads_as_no_body() {
    // Two lugs, z 0..5 and z 15..20: nothing between.
    let mut tris = cuboid([0.0, -1.0, 0.0], [10.0, 1.0, 5.0]);
    tris.extend(cuboid([0.0, -1.0, 15.0], [10.0, 1.0, 20.0]));
    let c = LatheClearance::of(&tris).expect("two lugs");
    assert!(c.over(7.0, 13.0).is_none(), "{:?}", c.over(7.0, 13.0));
    assert!(c.over(4.9, 7.0).is_some() && c.over(13.0, 15.1).is_some());
    assert!(c.over(-5.0, -1.0).is_none() && c.over(21.0, 30.0).is_none());
    assert!((c.z_lo()).abs() < 1e-12 && (c.z_hi() - 20.0).abs() < 1e-9);
    // The occupied bands, in order, cover the two lugs (a band whose
    // edge lies on a lug's end face counts it - inclusive, conservative).
    let spans: Vec<(f64, f64)> = c.spans().iter().map(|&(a, b, _)| (a, b)).collect();
    let step = c.step();
    assert!(spans.iter().all(|&(a, b)| b > a && (b <= 5.0 + step + 1e-9 || a >= 15.0 - step - 1e-9)), "{spans:?}");
    assert!((spans[0].0).abs() < 1e-12 && (spans[spans.len() - 1].1 - 20.0).abs() < 1e-9);
    // A prismatic lug is one run per distinct span (its end-face bands
    // differ only by the face's own clip), never one per fine band.
    assert!(spans.len() <= 6, "{spans:?}");
}

#[test]
fn placement_maps_the_model_into_the_tool_frame() {
    // A model with its shank along model -Y and the spindle along model
    // +X, the tip at (2, 3, 4): the shank end at (2, -97, 4) is x 100.
    let p = LathePlacement { radial: [0.0, -1.0, 0.0], along: [1.0, 0.0, 0.0], tip: [2.0, 3.0, 4.0] };
    let placed = p.place(&[[[2.0, -97.0, 4.0], [12.0, 3.0, 4.0], [2.0, 3.0, 9.0]]]).expect("square");
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-12);
    assert!(close(placed[0][0], [100.0, 0.0, 0.0]), "{:?}", placed[0][0]);
    assert!(close(placed[0][1], [0.0, 0.0, 10.0]), "{:?}", placed[0][1]);
    // y = z x x is model -Z here; only |y| is ever read.
    assert!((placed[0][2][1].abs() - 5.0).abs() < 1e-12 && placed[0][2][0].abs() < 1e-12 && placed[0][2][2].abs() < 1e-12);
}

#[test]
fn degenerate_placements_and_meshes_refuse_by_name() {
    let ok = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0; 3] };
    let bad = |p: LathePlacement| p.basis().err();
    assert_eq!(bad(LathePlacement { radial: [0.0; 3], ..ok }), Some(LatheBodyError::Direction));
    assert_eq!(bad(LathePlacement { along: [f64::NAN, 0.0, 1.0], ..ok }), Some(LatheBodyError::Direction));
    assert_eq!(bad(LathePlacement { along: [0.1, 0.0, 1.0], ..ok }), Some(LatheBodyError::NotSquare));
    assert_eq!(bad(LathePlacement { tip: [f64::INFINITY, 0.0, 0.0], ..ok }), Some(LatheBodyError::Tip));
    assert!(ok.basis().is_ok());
    // A hair off square is squared, not refused.
    assert!(LathePlacement { along: [1e-5, 0.0, 1.0], ..ok }.basis().is_ok());
    let body = cuboid([0.0, -5.0, 0.0], [40.0, 5.0, 20.0]);
    assert_eq!(LatheClearance::of(&[]).err(), Some(LatheBodyError::Empty));
    let mut nan = body.clone();
    nan[0][0][0] = f64::NAN;
    assert_eq!(LatheClearance::of(&nan).err(), Some(LatheBodyError::NonFinite));
    assert_eq!(LatheClearance::of(&[[[0.0; 3], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]]]).err(), Some(LatheBodyError::Degenerate));
    assert_eq!(LatheClearance::of(&[[[0.0; 3], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]]]).err(), Some(LatheBodyError::Flat));
    assert_eq!(LatheClearance::of(&cuboid([0.0, -5.0, 0.0], [2500.0, 5.0, 20.0])).err(), Some(LatheBodyError::TooLarge));
    assert_eq!(LatheClearance::of(&cuboid([60.0, -5.0, 0.0], [90.0, 5.0, 20.0])).err(), Some(LatheBodyError::TipAway));
    assert!(LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0; 3] }.place(&body).is_ok());
    // Every refusal says what it is.
    for e in [LatheBodyError::Direction, LatheBodyError::NotSquare, LatheBodyError::Tip, LatheBodyError::TipAway, LatheBodyError::Flat] {
        assert!(e.to_string().contains("lathe body") || e.to_string().contains("tip"), "{e}");
    }
}

#[test]
fn an_insert_zone_leaves_out_only_what_lies_inside_it() {
    // #396: the random soup again, with a zone about the tip. Every
    // facet point outside the zone still lies within its band's radii;
    // a point inside may be left out, never one outside.
    use crate::lathe_insert_zone::InsertZone;
    let zone = InsertZone { lo: [-3.0, -4.0, -2.0], hi: [8.0, 2.0, 6.0] };
    let inside = |p: &[f64]| (0..3).all(|k| p[k] > zone.lo[k] && p[k] < zone.hi[k]);
    let mut g = Lcg(0x396);
    let mut tris = Vec::new();
    for _ in 0..60 {
        let mut p = || [g.next() * 60.0 - 20.0, g.next() * 30.0 - 15.0, g.next() * 40.0 - 10.0];
        tris.push([p(), p(), p()]);
    }
    let c = LatheClearance::of_zoned(&tris, Some(&zone)).expect("a sane soup");
    let mut checked = 0;
    for _ in 0..20_000 {
        let t = &tris[(g.next() * tris.len() as f64) as usize % tris.len()];
        let (mut a, mut b) = (g.next(), g.next());
        if a + b > 1.0 {
            (a, b) = (1.0 - a, 1.0 - b);
        }
        let p: Vec<f64> = (0..3).map(|k| t[0][k] + (t[1][k] - t[0][k]) * a + (t[2][k] - t[0][k]) * b).collect();
        if inside(&p) {
            continue;
        }
        checked += 1;
        let x_tip = g.next() * 80.0 - 40.0;
        let r = (x_tip + p[0]).hypot(p[1]);
        let reach = c.over(p[2], p[2]).expect("a point outside the zone has a band");
        let (lo, hi) = reach.radii(x_tip);
        assert!(lo <= r + 1e-9 && r <= hi + 1e-9, "point {p:?} tip {x_tip}: r {r} outside [{lo}, {hi}]");
    }
    assert!(checked > 10_000, "{checked}");
    // A plate under the tip and a shank above: the zone over the plate
    // lifts the floor near the tip to the shank's, and the shank's own
    // bands are untouched.
    let mut body = cuboid([-0.8, -4.0, -0.8], [6.0, 0.0, 6.0]);
    body.extend(cuboid([2.0, -6.0, 0.5], [40.0, 6.0, 10.0]));
    let plain = LatheClearance::of(&body).expect("a body");
    let zoned = LatheClearance::of_zoned(&body, Some(&InsertZone { lo: [-1.0, -5.0, -1.0], hi: [1.0, 1.0, 6.5] })).expect("a body");
    assert!(plain.over(1.0, 1.0).expect("plate").x_lo < -0.79);
    assert!(zoned.over(1.0, 1.0).expect("shank").x_lo > 0.99, "{:?}", zoned.over(1.0, 1.0));
    assert_eq!(plain.over(8.0, 9.0), zoned.over(8.0, 9.0));
    // Nothing outside: refused by name; a zone off the tip: refused.
    let plate = cuboid([-0.8, -4.0, -0.8], [6.0, 0.0, 6.0]);
    assert_eq!(LatheClearance::of_zoned(&plate, Some(&InsertZone { lo: [-1.0, -5.0, -1.0], hi: [7.0, 1.0, 7.0] })).err(), Some(LatheBodyError::ZoneHoldsAll));
    assert_eq!(LatheClearance::of_zoned(&plate, Some(&InsertZone { lo: [1.0, -5.0, -1.0], hi: [7.0, 1.0, 7.0] })).err(), Some(LatheBodyError::ZoneInvalid));
}
