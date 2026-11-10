//! A carried field read back as a column body (#637 step D): per column
//! of a target lattice in the NEW frame, the top and the floor of the
//! carried, clipped solid - so the carried verdict, display and volume
//! run on either engine through one `Body` API, and an agreement lane
//! can hold the two engines to the same answer.
//!
//! Under a lattice turn a vertical column of the new frame is a vertical
//! column of the field's own (Z kept on +-Z), so each read is one march:
//! the top by the field's down-march (`march`), the floor by the exact
//! shell floor (`floor_at`, step B). A flip swaps them: the new top is
//! the old floor. A floor a cut reached is refused by name (step B's
//! law) - never a guess.
//!
//! Provenance: our own design (#637 research comment, steps A-D).

use super::carry::Body;
use super::dexel::Dexel;
use super::field_carry::{CarriedField, Clip};
use super::field_floor::FloorRead;
use super::march::{march, world, Read};
use crate::geometry::Point3;

/// The lattice a carried field is read back onto (new frame).
#[derive(Clone, Copy, Debug)]
pub struct Lattice {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
}

/// One column's (top, floor) in the field's own frame; `None`: no material.
fn read_column(c: &CarriedField, x: f64, y: f64) -> Result<Option<(f64, f64)>, String> {
    let f = &c.field;
    let (z_hi, z_lo) = world(f);
    let top = match march(f, &f.shell_crossings(x, y), x, y, z_hi, z_lo, true) {
        Read::Top(z) => z,
        Read::NoMaterial => return Ok(None),
        Read::Unreadable => return Err(format!("the carried field cannot read its column at ({x:.4}, {y:.4})")),
    };
    match f.floor_at(x, y) {
        // A NaN or infinite read is no verdict: refused, never an empty
        // column (review of #656 - an empty column is material the
        // verdict never sees).
        FloorRead::Floor(z) if !(top.is_finite() && z.is_finite()) => {
            Err(format!("the carried stock read a non-finite top or floor at ({x:.4}, {y:.4}) - refused rather than guessed"))
        }
        FloorRead::Floor(z) => Ok(Some((top, z))),
        FloorRead::NoMaterial => Ok(None),
        FloorRead::CutFromBelow => Err(format!(
            "a cut reached the floor of the carried stock at ({x:.4}, {y:.4}) - an upward read there is not exact yet, refused rather than guessed"
        )),
        FloorRead::Unreadable => Err(format!("the carried stock's shell cannot be read at ({x:.4}, {y:.4})")),
    }
}

impl CarriedField {
    /// The carried solid as a column body on `lat` (new frame).
    /// `judged_before` is the incoming body's floor flag; a flip sets it
    /// (the floor has faced the spindle), as `Body::carry` does.
    ///
    /// # Errors
    /// A non-finite or empty lattice; a column the field cannot read; a
    /// floor a cut reached (see the module law).
    pub fn column_body(&self, lat: Lattice, judged_before: bool) -> Result<Body, String> {
        if !(lat.x0.is_finite() && lat.y0.is_finite() && lat.cell.is_finite() && lat.cell > 0.0) || lat.nx == 0 || lat.ny == 0 {
            return Err("the carried lattice is not a finite, non-empty grid".to_owned());
        }
        let n = lat.nx.checked_mul(lat.ny).ok_or("the carried lattice is too large")?;
        // The turn's Z: z' = s z + tz, with s = -1 on a flip.
        let (s, tz) = (if self.turn.flips() { -1.0 } else { 1.0 }, self.turn.apply(Point3::new(0.0, 0.0, 0.0)).z);
        let mut top = vec![f64::NEG_INFINITY; n];
        let mut floor = vec![f64::INFINITY; n];
        for jy in 0..lat.ny {
            for jx in 0..lat.nx {
                let cx = lat.x0 + (jx as f64 + 0.5) * lat.cell;
                let cy = lat.y0 + (jy as f64 + 0.5) * lat.cell;
                let (lo_z, hi_z) = match self.clip {
                    Some(Clip::Empty) => continue,
                    Some(Clip::Block(lo, hi)) => {
                        if !(cx > lo.x && cx < hi.x && cy > lo.y && cy < hi.y) {
                            continue;
                        }
                        (lo.z, hi.z)
                    }
                    None => (f64::NEG_INFINITY, f64::INFINITY),
                };
                let q = self.turn.unapply(Point3::new(cx, cy, 0.0));
                let Some((t, f)) = read_column(self, q.x, q.y)? else { continue };
                let (t, f) = if s > 0.0 { (t + tz, f + tz) } else { (tz - f, tz - t) };
                let (t, f) = (t.min(hi_z), f.max(lo_z));
                if t > f {
                    let j = jx + jy * lat.nx;
                    top[j] = t;
                    floor[j] = f;
                }
            }
        }
        let stock_z = top.iter().copied().filter(|v| v.is_finite()).fold(f64::NEG_INFINITY, f64::max);
        let stock_z = if stock_z.is_finite() { stock_z } else { 0.0 };
        let dex = Dexel::from_grid(lat.x0, lat.y0, lat.cell, lat.nx, lat.ny, stock_z, top);
        Body::from_parts(dex, floor, judged_before || self.turn.flips())
    }
}
