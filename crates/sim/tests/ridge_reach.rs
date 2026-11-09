//! #212: the scallop law's widened reach. The flank reads a flat
//! raster's cusp past the old reach (`4·tol` past the ball) all the
//! way to the generator's ceiling, while its cusp geometry - the
//! nearest feet, one per pass, each in contact with the part - keeps a
//! crown above every pass, a wall's top edge the rings run beside and
//! the ground under an air pass on rest's ground, never the spacing's.
//! Provenance: scallop height of adjacent ball passes,
//! `h = r - sqrt(r^2 - (s/2)^2)`, the floor point under the cusp
//! `sqrt(r^2 + (s/2)^2) - r` outside both balls (machining handbooks);
//! our own fixtures, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim (envelope scan, ridge law, the flank's reach and cusp geometry).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim;

const R: f64 = 3.0;
const TOL: f64 = 0.1;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move {
        rotary: None,
        kind: MoveKind::Feed,
        from,
        to,
        feed: Some(500.0),
        arc: None,
        axis: None,
    }
}

/// The verifier's gate for this scan: tolerance plus the lattice floor.
fn gate(cell: f64) -> f64 {
    TOL + cell * cell / (2.0 * R) + 1e-9
}

/// A flat sheet, top at z = 0, over [-10, 10]^2.
fn plate() -> Vec<Triangle> {
    vec![
        Triangle::new(p(-10., -10., 0.), p(10., -10., 0.), p(10., 10., 0.)),
        Triangle::new(p(-10., -10., 0.), p(10., 10., 0.), p(-10., 10., 0.)),
    ]
}

/// Tip-height raster rows along x across the plate, `s` apart from
/// y = -10, ends joined serpentine so the path is one chain.
fn raster(s: f64) -> Vec<Move> {
    let n = (20.0 / s).floor() as i64;
    let mut out = Vec::new();
    for k in 0..=n {
        let y = -10.0 + s * k as f64;
        let (a, b) = if k % 2 == 0 { (-12.0, 12.0) } else { (12.0, -12.0) };
        out.push(feed(p(a, y, 0.0), p(b, y, 0.0)));
        if k < n {
            out.push(feed(p(b, y, 0.0), p(b, y + s, 0.0)));
        }
    }
    out
}

#[test]
fn a_flat_raster_past_the_near_reach_is_judged_at_its_cusp() {
    // Before #212 the flank saw a cusp only within `band + 3·tol`, so
    // s = 4 on the r3 ball (0.606 mm outside both balls under the cusp)
    // verified silent. Every spacing from the old reach to just under
    // the generator's 2r ceiling now reads its own cusp: no more than
    // the floor point's exact distance, no less than the nearest
    // column's (half a cell off the cusp) minus the lattice floor.
    for s in [3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 5.9] {
        let rep = stock_sim::envelope_scan(&plate(), &raster(s), R, TOL).expect("scan");
        let cusp = (R * R + (s / 2.0) * (s / 2.0)).sqrt() - R;
        let floor = rep.cell * rep.cell / (2.0 * R);
        let lo = ((s / 2.0 - rep.cell / 2.0).powi(2) + R * R).sqrt() - R - floor - 1e-6;
        assert!(rep.gouge_mm <= gate(rep.cell), "s {s}: a flat ride bites nothing: {}", rep.gouge_mm);
        assert!(rep.rest_mm <= gate(rep.cell), "s {s}: scallop is its own reading, not rest: {}", rep.rest_mm);
        assert!(rep.scallop_mm > gate(rep.cell), "s {s}: the cusp must convict: {}", rep.scallop_mm);
        assert!(
            rep.scallop_mm <= cusp + 1e-6 && rep.scallop_mm >= lo,
            "s {s}: scallop {} reads the cusp {cusp} (lattice floor {lo}) at {:?}",
            rep.scallop_mm,
            rep.scallop_at
        );
        let row = (rep.scallop_at.y + 10.0) / s;
        assert!((row - row.floor() - 0.5).abs() < 0.1, "s {s}: convicted off the cusp at {:?}", rep.scallop_at);
    }
}

#[test]
fn the_wider_flank_never_softens_the_old_reading() {
    // Inside the old reach nothing moved: s = 2.8 reads the same
    // cusp the M13.8 flank always read, and a sound spacing is quiet.
    let rep = stock_sim::envelope_scan(&plate(), &raster(2.8), R, TOL).expect("scan");
    let cusp = (R * R + 1.96f64).sqrt() - R;
    assert!(rep.scallop_mm > gate(rep.cell) && rep.scallop_mm <= cusp + 1e-6, "s 2.8: {}", rep.scallop_mm);
    let rep = stock_sim::envelope_scan(&plate(), &raster(1.0), R, TOL).expect("scan");
    assert!(rep.scallop_mm <= gate(rep.cell), "a sound raster read scallop {} at {:?}", rep.scallop_mm, rep.scallop_at);
}

/// The hill of `ridge_rest.rs`: a sphere of radius 8 at the origin cut
/// at polar 45°, 5 bands of 48, closed with a disc.
fn hill() -> Vec<Triangle> {
    let (radius, polar_max, n_lat, n_lon) = (8.0f64, std::f64::consts::FRAC_PI_4, 5usize, 48usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
                p(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let mut facets = Vec::new();
    let apex = p(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let (ra, rb) = (ring(k), ring(k + 1));
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    let rim = ring(n_lat);
    let center = p(0.0, 0.0, radius * polar_max.cos());
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    facets
}

/// A helix down the hill: the ball's centre rides the sphere of radius
/// 11 (tangent to the hill), its polar angle growing by `lap` per turn
/// from `polar0`, 96 chords a turn, tips stored. No move is a closed
/// iso-Z ring, so every one is its own pass to `closed_ring_ids`.
fn helix(polar0: f64, lap: f64, turns: usize) -> Vec<Move> {
    let at = |k: usize| {
        let th = std::f64::consts::TAU * k as f64 / 96.0;
        let ph = polar0 + lap * k as f64 / 96.0;
        p(11.0 * ph.sin() * th.cos(), 11.0 * ph.sin() * th.sin(), 11.0 * ph.cos() - R)
    };
    (0..96 * turns).map(|k| feed(at(k), at(k + 1))).collect()
}

#[test]
fn a_crown_above_the_top_lap_stays_rest_not_scallop() {
    // Laps 1/8 rad apart (1 mm on the hill: a 0.04 cusp, sound), the
    // first at polar 0.4: the crown inside it stands ~1.8 mm outside
    // every ball, its nearest feet the top lap all round it - inside
    // the flank's diameter, past the old reach. One lap curving
    // round a point is one pass: the crown is rest machining's ground.
    let rep = stock_sim::envelope_scan(&hill(), &helix(0.4, 0.125, 3), R, TOL).expect("scan");
    assert!(rep.gouge_mm <= gate(rep.cell), "a tangent helix bites nothing: {}", rep.gouge_mm);
    assert!(
        rep.scallop_mm <= gate(rep.cell),
        "the crown above the top lap read as scallop: {} at {:?}",
        rep.scallop_mm,
        rep.scallop_at
    );
    // The same helix with laps 0.7 rad apart (5.6 mm on the hill) DOES
    // leave a cusp between laps, past the old reach: the flank
    // reads it there, on the flank, not at the crown.
    let rep = stock_sim::envelope_scan(&hill(), &helix(0.0, 0.7, 2), R, TOL).expect("scan");
    assert!(rep.scallop_mm > gate(rep.cell), "the cusp between wide laps must convict: {}", rep.scallop_mm);
    let rho = rep.scallop_at.x.hypot(rep.scallop_at.y);
    assert!(rho > 1.0, "convicted at the crown, not between the laps, at {:?}", rep.scallop_at);
}

/// A 20 x 20 x 10 box centred on the Z axis, bottom at z = 0.
fn block() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| p(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |s: [Point3; 4]| vec![Triangle::new(s[0], s[1], s[2]), Triangle::new(s[0], s[2], s[3])];
    let mut out = Vec::new();
    for s in [[e, f, g, h], [a, c, b, d], [a, b, f, e], [b, c, g, f], [c, d, h, g], [d, a, e, h]] {
        out.extend(quad(s));
    }
    out
}

/// One closed waterline ring round the block at tip height `z_tip`:
/// centres `off` outside the walls, the corners rounded by 8 chords.
fn block_ring(z_tip: f64, off: f64) -> Vec<Move> {
    let mut pts = Vec::new();
    for (k, (cx, cy)) in [(10.0, -10.0), (10.0, 10.0), (-10.0, 10.0), (-10.0, -10.0)].into_iter().enumerate() {
        let a0 = -std::f64::consts::FRAC_PI_2 + std::f64::consts::FRAC_PI_2 * k as f64;
        for j in 0..=8 {
            let a = a0 + std::f64::consts::FRAC_PI_2 * j as f64 / 8.0;
            pts.push(p(cx + off * a.cos(), cy + off * a.sin(), z_tip));
        }
    }
    let first = pts[0];
    pts.push(first);
    pts.windows(2).map(|w| feed(w[0], w[1])).collect()
}

#[test]
fn a_wall_top_edge_the_rings_run_beside_stays_rest_not_scallop() {
    // Waterline rings round a block: the top one rides the top edge
    // (tip at the top face, centres over the edge), the rest stand off
    // the walls below it. A top-face column near a corner then has
    // the top ring a radius-and-a-half away and a lower ring nearly a
    // diameter away on the far side of the corner - a fan of feet the
    // tangent plane calls opposite. No spacing left that material:
    // the face beyond the top ring is the shelf's own (parallel's)
    // ground, and the column stands far nearer one pass than the
    // other, which a cusp never does.
    let mut moves = block_ring(10.0, 0.0);
    for z in [7.5, 5.0, 2.5, 0.0] {
        moves.extend(block_ring(z, R));
    }
    let rep = stock_sim::envelope_scan(&block(), &moves, R, TOL).expect("scan");
    assert!(rep.gouge_mm <= gate(rep.cell), "the rings bite nothing: {}", rep.gouge_mm);
    assert!(
        rep.scallop_mm <= gate(rep.cell),
        "the block's top edge read as scallop: {} at {:?}",
        rep.scallop_mm,
        rep.scallop_at
    );
}

/// One raster row along x at `y`, its tip `lift` above the plate.
fn row(y: f64, lift: f64) -> Move {
    feed(p(-12.0, y, lift), p(12.0, y, lift))
}

#[test]
fn a_pass_clear_of_the_part_flanks_nothing() {
    // Two rows 4.4 apart, both riding the plate: the column midway
    // stands 0.83 mm outside both balls - a cusp, read past the old
    // reach. Lift the second row 1 mm off the plate and it has no
    // contact anywhere: the material between is the first row's edge
    // and the air pass's leftover (a ladder's skim over stock it left
    // to the next op), rest's ground, never the spacing's scallop.
    let both = vec![row(-2.2, 0.0), row(2.2, 0.0)];
    let rep = stock_sim::envelope_scan(&plate(), &both, R, TOL).expect("scan");
    let cusp = (R * R + 2.2f64 * 2.2).sqrt() - R;
    assert!(rep.scallop_mm > gate(rep.cell) && rep.scallop_mm <= cusp + 1e-6, "two contact rows: {}", rep.scallop_mm);
    let skim = vec![row(-2.2, 0.0), row(2.2, 1.0)];
    let rep = stock_sim::envelope_scan(&plate(), &skim, R, TOL).expect("scan");
    assert!(
        rep.scallop_mm <= gate(rep.cell),
        "a row 1 mm clear of the plate read as a cusp's side: {} at {:?}",
        rep.scallop_mm,
        rep.scallop_at
    );
}
