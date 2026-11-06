//! The committed golden programs as stock-engine fixtures (M7.3): each
//! golden CL dump paired with the tool its generator test cut with, a
//! block stock around it, and the part it is judged against. The 3D
//! finishes are judged against the very meshes their golden tests
//! generated on (rebuilt here from the same numbers); the 2D and
//! drilling goldens carry no part mesh, so they are judged against two
//! probe plates - one at the program's deepest tip (the at-floor case,
//! where a solver residue would have to cross the tolerance) and one
//! midway between that floor and the stock top (every cut column
//! gouges, every uncut one is rest: both verdicts at full exercise).
//! The engines only have to agree; the probes make them say a lot.
//! Since M13.1 four goldens also replay under a bull nose, so the
//! bull atom and the bull solid are held to the same agreement.
//! Provenance: our own test fixtures (tests/golden_cl.rs) - nothing
//! from vendor material.

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::geometry::{cube, Point3, Triangle};

use super::cl_read::{golden_text, parse_cl};

/// What a program is judged against.
pub enum Target {
    /// The 2D probe plates (see the module header).
    Plates,
    /// The generator test's own part mesh.
    Mesh(Vec<Triangle>),
}

/// One golden program, ready for either engine.
pub struct Program {
    /// The program's label: the golden file's name, plus the tool for
    /// a golden replayed under a second cutter (the bull-nose rows).
    pub file: &'static str,
    pub moves: Vec<Move>,
    pub radius: f64,
    pub ball: bool,
    /// The bull nose's corner radius (M13.1), None for every other tool.
    pub corner: Option<f64>,
    /// The stock block's top.
    pub top: f64,
    pub target: Target,
}

/// (file, tool radius, ball nose, stock top, part mesh when it has one)
type Spec = (&'static str, f64, bool, f64, Option<Vec<Triangle>>);

/// (label, golden file, tool radius, corner radius, stock top, part mesh)
type BullSpec = (&'static str, &'static str, f64, f64, f64, Option<Vec<Triangle>>);

/// Every committed golden that a 3-axis stock sim can replay, with the
/// tool its golden test named (the drilling cycles sweep the disc,
/// `plan_program`'s law; the trace golden names no tool - it gets a
/// 2 mm engraver, stated here).
pub fn programs() -> Result<Vec<Program>, String> {
    let flat = |file: &'static str, radius: f64, top: f64| (file, radius, false, top, None);
    let specs: Vec<Spec> = vec![
        flat("contour_disc_hole.cl", 3.0, 0.0),
        flat("contour_tangential_lead.cl", 3.0, 0.0),
        flat("contour_plate_with_hole.cl", 3.0, 0.0),
        flat("face_square_40x40.cl", 5.0, 0.0),
        flat("face_square_40x40_blended.cl", 5.0, 0.0),
        flat("face_square_40x40_stepped.cl", 5.0, 2.0),
        flat("pocket_zigzag_plate_with_island.cl", 4.0, 0.0),
        flat("pocket_waterline_plate_with_island.cl", 4.0, 0.0),
        flat("slot_fullwidth_centerline.cl", 5.0, 0.0),
        flat("slot_wide_channel_rows.cl", 3.0, 0.0),
        flat("trace_plate_with_hole.cl", 1.0, 0.0),
        flat("drill_plate.cl", 3.0, 0.0),
        flat("drill_peck.cl", 3.0, 0.0),
        flat("tap_plate.cl", 5.0, 0.0),
        ("finish3d_waterline_cone.cl", 3.0, true, 21.0, Some(cone())),
        ("finish3d_scallop_plateau.cl", 3.0, true, 11.0, Some(plateau())),
        ("finish3d_ramp_parallel_plateau.cl", 3.0, true, 11.0, Some(plateau())),
        ("finish3d_spiral_plateau.cl", 3.0, true, 11.0, Some(plateau())),
        ("finish3d_contour_plateau.cl", 3.0, true, 11.0, Some(plateau())),
        ("finish3d_radial_dome.cl", 3.0, true, 13.0, Some(dome())),
        ("finish3d_project_dome.cl", 3.0, true, 13.0, Some(dome())),
    ];
    let mut out = Vec::with_capacity(specs.len() + 4);
    for (file, radius, ball, top, mesh) in specs {
        let moves = parse_cl(&golden_text(file)?).map_err(|e| format!("{file}: {e}"))?;
        let target = match mesh {
            Some(m) => Target::Mesh(m),
            None => Target::Plates,
        };
        out.push(Program { file, moves, radius, ball, corner: None, top, target });
    }
    // M13.1: committed goldens replayed under a bull nose - the 2.5-D
    // floors and walls (level, plunge, helix and arc chords: the
    // closed-form arms) and a 3D finish (oblique chords: the golden
    // section). The paths were planned for other cutters, so these
    // programs gouge and leave rest freely; the engines only have to
    // agree on what the bull leaves.
    let bulls: [BullSpec; 4] = [
        ("pocket_zigzag_plate_with_island.cl+bull(r4,c1)", "pocket_zigzag_plate_with_island.cl", 4.0, 1.0, 0.0, None),
        ("contour_plate_with_hole.cl+bull(r3,c1)", "contour_plate_with_hole.cl", 3.0, 1.0, 0.0, None),
        ("slot_wide_channel_rows.cl+bull(r3,c0.5)", "slot_wide_channel_rows.cl", 3.0, 0.5, 0.0, None),
        ("finish3d_scallop_plateau.cl+bull(r3,c1.5)", "finish3d_scallop_plateau.cl", 3.0, 1.5, 11.0, Some(plateau())),
    ];
    for (label, source, radius, corner, top, mesh) in bulls {
        let moves = parse_cl(&golden_text(source)?).map_err(|e| format!("{source}: {e}"))?;
        let target = match mesh {
            Some(m) => Target::Mesh(m),
            None => Target::Plates,
        };
        out.push(Program { file: label, moves, radius, ball: false, corner: Some(corner), top, target });
    }
    Ok(out)
}

impl Program {
    /// The stock's XY box: every move endpoint and arc circle, plus
    /// the target mesh, dilated by the tool and two millimeters, snapped
    /// outward to the cell so both engines share one lattice.
    pub fn stock_box(&self, cell: f64) -> (f64, f64, f64, f64) {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut eat = |x: f64, y: f64| {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        };
        for m in &self.moves {
            for p in [m.from, m.to] {
                eat(p.x, p.y);
            }
            if let Some(a) = m.arc {
                let r = (m.from.x - a.center.x).hypot(m.from.y - a.center.y);
                eat(a.center.x - r, a.center.y - r);
                eat(a.center.x + r, a.center.y + r);
            }
        }
        if let Target::Mesh(f) = &self.target {
            for t in f {
                for v in [t.a, t.b, t.c] {
                    eat(v.x, v.y);
                }
            }
        }
        let pad = self.radius + 2.0;
        (
            ((x0 - pad) / cell).floor() * cell,
            ((y0 - pad) / cell).floor() * cell,
            ((x1 + pad) / cell).ceil() * cell,
            ((y1 + pad) / cell).ceil() * cell,
        )
    }

    /// The deepest tip any cutting move reaches.
    pub fn floor(&self) -> f64 {
        self.moves
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid)
            .flat_map(|m| [m.from.z, m.to.z])
            .fold(f64::INFINITY, f64::min)
    }

    /// The (label, part) pairs this program is judged against.
    pub fn targets(&self, cell: f64) -> Vec<(String, Vec<Triangle>)> {
        match &self.target {
            Target::Mesh(m) => vec![("part".into(), m.clone())],
            Target::Plates => {
                let (x0, y0, x1, y1) = self.stock_box(cell);
                let plate = |z: f64| cube(Point3::new(x0, y0, z - 20.0), Point3::new(x1, y1, z)).facets.to_vec();
                let fl = self.floor();
                let mid = 0.5 * (fl + self.top);
                vec![
                    (format!("floor plate z={fl:.3}"), plate(fl)),
                    (format!("mid plate z={mid:.3}"), plate(mid)),
                ]
            }
        }
    }
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The waterline golden's 45-degree cone: apex (0,0,20), R20 base.
fn cone() -> Vec<Triangle> {
    let n = 96;
    let mut facets = Vec::new();
    for j in 0..n {
        let a = std::f64::consts::TAU * j as f64 / n as f64;
        let b = std::f64::consts::TAU * (j + 1) as f64 / n as f64;
        facets.push(Triangle::new(p3(0.0, 0.0, 20.0), p3(20.0 * a.cos(), 20.0 * a.sin(), 0.0), p3(20.0 * b.cos(), 20.0 * b.sin(), 0.0)));
        facets.push(Triangle::new(p3(0.0, 0.0, 0.0), p3(20.0 * b.cos(), 20.0 * b.sin(), 0.0), p3(20.0 * a.cos(), 20.0 * a.sin(), 0.0)));
    }
    facets
}

/// The scallop golden's 20x20x10 plateau: top and walls, no bottom.
fn plateau() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| [Triangle::new(p[0], p[1], p[2]), Triangle::new(p[0], p[2], p[3])];
    let mut facets = Vec::new();
    for qd in [[e, f, g, h], [a, b, f, e], [b, c, g, f], [c, d, h, g], [d, a, e, h]] {
        facets.extend(quad(qd));
    }
    facets
}

/// The radial/project goldens' spherical cap: apex (0,0,12), rim at
/// polar 60 degrees, closed by a flat base.
fn dome() -> Vec<Triangle> {
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let mut facets = Vec::new();
    let apex = p3(0.0, 0.0, radius);
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
    let center = p3(0.0, 0.0, radius * polar_max.cos());
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    facets
}
