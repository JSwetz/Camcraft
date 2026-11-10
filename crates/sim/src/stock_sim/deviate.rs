//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::kernel::{StockError, part_surface};
use super::dexel::Dexel;
use super::wall_slack::{settle, Candidate};

use crate::geometry::{Point3, Triangle};
/// What the finished program leaves, per column, against the model's
/// own surface: `top - part` where both stand (positive is rest,
/// negative is gouge), NaN where either is air. The aggregates locate
/// themselves - a worst value with no XY cannot be shown, jumped to,
/// or argued with, and "trusted before the machine sees it" is a
/// claim about a WHERE.
#[derive(Clone, Debug, PartialEq)]
pub struct Deviation {
    pub cells_x: usize,
    pub cells_y: usize,
    pub cell: f64,
    pub tol: f64,
    pub dev: Vec<f64>,
    pub rest_cells: usize,
    pub gouge_cells: usize,
    pub rest_volume: f64,
    pub gouge_volume: f64,
    pub max_rest: f64,
    pub max_gouge: f64,
    /// The sim's own surface at the worst cell - the material that
    /// stands, or the dent that was cut.
    pub max_rest_at: Option<Point3>,
    pub max_gouge_at: Option<Point3>,
}

impl Deviation {
    /// The program-level verdict: nothing stands over tolerance and
    /// nothing is cut below it. The color map's classes (NaN
    /// unjudged, |v| <= tol clean, positive rest, negative gouge)
    /// are this law read one column at a time by the viewer.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.max_rest <= self.tol && self.max_gouge <= self.tol
    }
}

/// Measure the grid against the model on the grid's own lattice. A
/// gouge is judged with `tol` of lateral slack as well as vertical
/// (`wall_slack`, #299): a column within `tol` of a wall is not
/// charged the wall's height.
pub fn deviate(dex: &Dexel, facets: &[Triangle], tol: f64) -> Result<Deviation, StockError> {
    if !tol.is_finite() {
        return Err(StockError::Invalid(format!("tolerance {tol} is not finite")));
    }
    let tol = tol.max(0.0);
    let part = part_surface(facets, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
    let a2 = dex.cell * dex.cell;
    let mut d = Deviation {
        cells_x: dex.nx,
        cells_y: dex.ny,
        cell: dex.cell,
        tol,
        dev: vec![f64::NAN; dex.nx * dex.ny],
        rest_cells: 0,
        gouge_cells: 0,
        rest_volume: 0.0,
        gouge_volume: 0.0,
        max_rest: 0.0,
        max_gouge: 0.0,
        max_rest_at: None,
        max_gouge_at: None,
    };
    let mut cands: Vec<Candidate> = Vec::new();
    for iy in 0..dex.ny {
        for ix in 0..dex.nx {
            let k = ix + iy * dex.nx;
            let (sim, p) = (dex.top_at(k), part[k]);
            if !(sim.is_finite() && p.is_finite()) {
                continue; // NaN stays: no material, or nothing to judge against
            }
            let v = sim - p;
            d.dev[k] = v;
            if v > tol {
                d.rest_cells += 1;
                d.rest_volume += v * a2;
                if v > d.max_rest {
                    d.max_rest = v;
                    d.max_rest_at = Some(Point3::new(dex.center(ix, iy).x, dex.center(ix, iy).y, sim));
                }
            } else if v < -tol {
                // #299: judged once the whole lattice is read - a
                // column within `tol` of a wall is not the wall's gouge.
                cands.push(Candidate { k, sim, v, at: Point3::new(dex.center(ix, iy).x, dex.center(ix, iy).y, sim) });
            }
        }
    }
    settle(&mut d, facets, dex.x0, dex.y0, &part, &cands);
    Ok(d)
}

// ---- M2.3 stage B: the grid as a viewer can show it --------------------

/// Per-column color classes of the deviation field: 0.0 clean (within
/// tolerance, and the untouched flat top of a program that simply owns
/// that column), 1.0 rest, 2.0 gouge, 3.0 unjudged - NaN gets its own
/// class because a verdict the sim cannot make must not render as the
/// absence of a problem. The four numbers are the kernel's side of a
/// colormap; the viewer picks the paints (and says what they mean).
#[must_use]
pub fn color_classes(dev: &Deviation) -> Vec<f32> {
    dev.dev
        .iter()
        .map(|v| {
            if v.is_nan() {
                3.0
            } else if *v > dev.tol {
                1.0
            } else if *v < -dev.tol {
                2.0
            } else {
                0.0
            }
        })
        .collect()
}

/// A sheet through the material columns of the grid: one top quad per
/// standing column, and a vertical wall across every step to a lower
/// neighbor. Air neighbors leave the edge open - the grid stores tops,
/// never bottoms, and a sheet that grew a floor it does not know would
/// be the grid asserting stock it never saw. A wall stands for its
/// step, so it carries the class of the upper column: the one that
/// reads too high, or was cut too deep, is the column the verdict is
/// about. Positions are world mm on the grid's own lattice.
#[derive(Debug)]
pub struct SimSurface {
    pub verts: Vec<Point3>,
    /// One class per vertex, from `color_classes`' law - the shader's
    /// color map reads it exactly like `sel` reads selection.
    pub vcls: Vec<f32>,
    /// Triangle indices into `verts`, front face by right-hand rule.
    pub tris: Vec<[u32; 3]>,
}

impl Dexel {
    /// Build the sheet. `classes` is `color_classes(&deviate(..))` (or
    /// all-zeros for a plain stock surface); a column whose entry is
    /// missing reads as class 0 - the surface never invents a verdict.
    #[must_use]
    pub fn surface(&self, classes: &[f32]) -> SimSurface {
        let at = |ix: usize, iy: usize| -> f64 { self.top_at(ix + iy * self.nx) };
        let cls = |ix: usize, iy: usize| -> f32 {
            classes.get(ix + iy * self.nx).copied().unwrap_or(0.0)
        };
        let mut s = SimSurface { verts: Vec::new(), vcls: Vec::new(), tris: Vec::new() };
        let quad = |s: &mut SimSurface, a: Point3, b: Point3, c: Point3, ca: f32, cb: f32, cc: f32| {
            let n = s.verts.len() as u32;
            s.verts.extend([a, b, c]);
            s.vcls.extend([ca, cb, cc]);
            s.tris.push([n, n + 1, n + 2]);
        };
        for iy in 0..self.ny {
            for ix in 0..self.nx {
                let z = at(ix, iy);
                if !z.is_finite() {
                    continue; // no material stood here: no sheet
                }
                let c = cls(ix, iy);
                let x0 = self.x0 + ix as f64 * self.cell;
                let y0 = self.y0 + iy as f64 * self.cell;
                let (x1, y1) = (x0 + self.cell, y0 + self.cell);
                // Top: two CCW triangles seen from +Z, one cell.
                quad(
                    &mut s,
                    Point3::new(x0, y0, z),
                    Point3::new(x1, y0, z),
                    Point3::new(x1, y1, z),
                    c,
                    c,
                    c,
                );
                quad(
                    &mut s,
                    Point3::new(x0, y0, z),
                    Point3::new(x1, y1, z),
                    Point3::new(x0, y1, z),
                    c,
                    c,
                    c,
                );
                // East wall: this column stands above a finite lower one.
                if ix + 1 < self.nx {
                    let zn = at(ix + 1, iy);
                    if zn.is_finite() && zn < z - 1e-12 {
                        let (a, b) = (Point3::new(x1, y0, z), Point3::new(x1, y1, z));
                        let (d, e) = (Point3::new(x1, y0, zn), Point3::new(x1, y1, zn));
                        quad(&mut s, a, d, b, c, c, c);
                        quad(&mut s, b, d, e, c, c, c);
                    }
                }
                // South wall.
                if iy > 0 {
                    let zn = at(ix, iy - 1);
                    if zn.is_finite() && zn < z - 1e-12 {
                        let (a, b) = (Point3::new(x0, y0, z), Point3::new(x1, y0, z));
                        let (d, e) = (Point3::new(x0, y0, zn), Point3::new(x1, y0, zn));
                        quad(&mut s, a, d, b, c, c, c);
                        quad(&mut s, b, d, e, c, c, c);
                    }
                }
            }
        }
        s
    }
}

// ---- M2.3 stage B: the document's program, simulated -------------------

/// The deviation verdict tolerance: a column reading within this of the
/// model is clean for the color map. Deliberately a display law, not a
/// re-use of any operation's authoring tolerance - the op's tolerance is
/// what its generator promised to hold; the map answers "what stands
/// after the WHOLE program", and per-op tolerances would make one
/// tolerance field edit re-color columns another op authored. 10 um is
/// the finest scallop the shipped lattice can resolve visually.
pub const DEV_TOL: f64 = 0.01;

/// One run of the program sim: the per-column verdict and the sheet
/// that paints it. Held by the UI as view state (the `Sim`/`PostView`
/// exemption in AGENTS.md) - it is frozen evidence of one specific
/// program state, like `PostView`'s program text, so an edit that
/// makes it stale never silently re-runs: re-arming re-judges.
#[derive(Debug)]
pub struct ProgramSim {
    pub dev: Deviation,
    pub surface: SimSurface,
}
