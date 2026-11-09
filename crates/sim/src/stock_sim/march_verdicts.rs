//! The field arm's verdict readers (M7.1 C): `deviate`, `measure`,
//! and the viewer's sheet, judged from marched heights on the
//! stock's own lattice. Split from `march.rs` by the 400-line law -
//! the descent and its bracket stay there, these are its consumers.
//!
//! Provenance: the verdict laws are this codebase's own column
//! engine's (M2.1's deviation report, M2.3's program sim, the sheet
//! of `Dexel::surface`), re-sampled through the ray march - the
//! parity law of stage C is that a verdict does not learn which arm
//! read it. Nothing from vendor binaries.

use super::deviate::{Deviation, SimSurface};
use super::field::{center_of, SdfField};
use super::kernel::{part_surface, StockError, StockReport};
use super::march::{lattice_of, march, world, Read};
use super::wall_slack::{settle, Candidate};

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// The field arm's `deviate`: the march's heights judge the model's
/// `part_surface` on the stock's own lattice. The verdicts,
/// aggregates, and NaN-for-no-verdict rule are the column engine's,
/// line for line - one law, two samplings of it.
pub(crate) fn deviate_field(f: &SdfField, facets: &[Triangle], tol: f64) -> Result<Deviation, StockError> {
    if !tol.is_finite() {
        return Err(StockError::Invalid(format!("tolerance {tol} is not finite")));
    }
    let tol = tol.max(0.0);
    let lat = lattice_of(f)?;
    let part = part_surface(facets, lat.x0, lat.y0, lat.cell, lat.nx, lat.ny);
    let (z_hi, z_lo) = world(f);
    let a2 = lat.cell * lat.cell;
    let mut d = Deviation {
        cells_x: lat.nx,
        cells_y: lat.ny,
        cell: lat.cell,
        tol,
        dev: vec![f64::NAN; lat.nx * lat.ny],
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
    for iy in 0..lat.ny {
        for ix in 0..lat.nx {
            let k = ix + iy * lat.nx;
            let p = part[k];
            if !p.is_finite() {
                continue; // nothing to judge against: a march would only cost
            }
            let (x, y) = (
                center_of(lat.i0 + ix as i32, lat.cell),
                center_of(lat.j0 + iy as i32, lat.cell),
            );
            let sim = match march(f, &f.shell_crossings(x, y), x, y, z_hi, z_lo, true) {
                Read::Top(z) => z,
                _ => continue, // no verdict: NaN stays, never a zero
            };
            let v = sim - p;
            d.dev[k] = v;
            if v > tol {
                d.rest_cells += 1;
                d.rest_volume += v * a2;
                if v > d.max_rest {
                    d.max_rest = v;
                    d.max_rest_at = Some(Point3::new(x, y, sim));
                }
            } else if v < -tol {
                // #299: judged once the whole lattice is read - a
                // column within `tol` of a wall is not the wall's gouge.
                cands.push(Candidate { k, sim, v, at: Point3::new(x, y, sim) });
            }
        }
    }
    settle(&mut d, facets, lat.x0, lat.y0, &part, &cands);
    Ok(d)
}

/// The field arm's `measure`: the same verdict law as the column
/// engine's, and removed volume as the depth between the stock's own
/// marched top and what the program left - the `stock_z` reading
/// generalized to a model whose surface is not flat.
pub(crate) fn measure_field(
    f: &SdfField,
    model: &[Triangle],
    footprint: &crate::geo2d::Region,
    tol: f64,
) -> StockReport {
    let lat = match lattice_of(f) {
        Ok(l) => l,
        Err(_) => return StockReport::default(), // the verdict would have refused first
    };
    let part = part_surface(model, lat.x0, lat.y0, lat.cell, lat.nx, lat.ny);
    let (z_hi, z_lo) = world(f);
    let a2 = lat.cell * lat.cell;
    // tol rides exactly as the column's measure takes it (no max, no
    // gate): a verdict must not differ by which arm read the number.
    let mut rep = StockReport::default();
    for iy in 0..lat.ny {
        for ix in 0..lat.nx {
            let (x, y) = (
                center_of(lat.i0 + ix as i32, lat.cell),
                center_of(lat.j0 + iy as i32, lat.cell),
            );
            if !footprint.contains(Point2::new(x, y)) {
                continue;
            }
            let cx = f.shell_crossings(x, y);
            let top = march(f, &cx, x, y, z_hi, z_lo, true);
            let z_top = match top {
                Read::Top(z) => Some(z),
                _ => None,
            };
            let ptop = part[ix + iy * lat.nx];
            if let (Some(sim), true) = (z_top, ptop.is_finite()) {
                if sim > ptop + tol {
                    rep.rest_volume += (sim - ptop) * a2;
                    rep.rest_cells += 1;
                    rep.max_rest_height = rep.max_rest_height.max(sim - ptop);
                }
                if sim < ptop - tol {
                    rep.gouge_volume += (ptop - sim) * a2;
                    rep.gouge_cells += 1;
                    rep.max_gouge_depth = rep.max_gouge_depth.max(ptop - sim);
                }
            }
            if let (Some(z_init), Some(sim)) = (
                match march(f, &cx, x, y, z_hi, z_lo, false) {
                    Read::Top(z) => Some(z),
                    _ => None,
                },
                z_top,
            ) {
                rep.removed_volume += (z_init - sim).max(0.0) * a2;
            }
        }
    }
    rep
}

/// The field arm's sheet: marched tops on the verdict lattice, the
/// same quads-and-walls law as `Dexel::surface` - a column with no
/// verdict (air, or an unfinished march) carries no sheet, because a
/// surface that grew material it never found would be the field
/// asserting stock it does not own.
pub(crate) fn sheet_field(f: &SdfField, classes: &[f32]) -> SimSurface {
    let mut s = SimSurface {
        verts: Vec::new(),
        vcls: Vec::new(),
        tris: Vec::new(),
    };
    let lat = match lattice_of(f) {
        Ok(l) => l,
        Err(_) => return s,
    };
    let (z_hi, z_lo) = world(f);
    let mut tops = vec![f64::NAN; lat.nx * lat.ny];
    for iy in 0..lat.ny {
        for ix in 0..lat.nx {
            let (x, y) = (
                center_of(lat.i0 + ix as i32, lat.cell),
                center_of(lat.j0 + iy as i32, lat.cell),
            );
            if let Read::Top(z) = march(f, &f.shell_crossings(x, y), x, y, z_hi, z_lo, true) {
                tops[ix + iy * lat.nx] = z;
            }
        }
    }
    let at = |ix: usize, iy: usize| -> f64 { tops[ix + iy * lat.nx] };
    let cls = |ix: usize, iy: usize| -> f32 {
        classes
            .get(ix + iy * lat.nx)
            .copied()
            .unwrap_or(0.0)
    };
    let quad = |s: &mut SimSurface, a: Point3, b: Point3, c: Point3, ca: f32, cb: f32, cc: f32| {
        let n = s.verts.len() as u32;
        s.verts.extend([a, b, c]);
        s.vcls.extend([ca, cb, cc]);
        s.tris.push([n, n + 1, n + 2]);
    };
    for iy in 0..lat.ny {
        for ix in 0..lat.nx {
            let z = at(ix, iy);
            if !z.is_finite() {
                continue;
            }
            let c = cls(ix, iy);
            let (x0, y0) = (
                center_of(lat.i0 + ix as i32, lat.cell) - 0.5 * lat.cell,
                center_of(lat.j0 + iy as i32, lat.cell) - 0.5 * lat.cell,
            );
            let (x1, y1) = (x0 + lat.cell, y0 + lat.cell);
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
            if ix + 1 < lat.nx {
                let zn = at(ix + 1, iy);
                if zn.is_finite() && zn < z - 1e-12 {
                    let (a, b) = (Point3::new(x1, y0, z), Point3::new(x1, y1, z));
                    let (d, e) = (Point3::new(x1, y0, zn), Point3::new(x1, y1, zn));
                    quad(&mut s, a, d, b, c, c, c);
                    quad(&mut s, b, d, e, c, c, c);
                }
            }
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
