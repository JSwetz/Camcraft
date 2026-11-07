//! The program ahead of one operation, cut at most once and only when a
//! law asks (issue #89, after #96's review): the drilling laws (reach,
//! R plane, holder) and verify's vertical-rapid law both read the grid
//! the setup's earlier operations leave standing, and both ask only
//! when the raw stock would refuse - a clean job never simulates.
//!
//! A lathe's axial drill reads the grid revolved ([`revolved_surface`]):
//! the spindle turns the work under every turning pass, so a column cut
//! anywhere on a ring about the axis is cut all the way round it.
//!
//! Provenance: our own design notes (lazy evaluation of the re-machining
//! rest grid; a body of revolution's surface as a function of radius);
//! no vendor material consulted.

use crate::geo2d::Point2;
use crate::rest::PriorPlan;
use crate::stock_sim::Dexel;

/// The plan, simulated on first [`PriorCell::get`]. A plan that cannot
/// be cut (a posed predecessor the column grid refuses) is logged and
/// read as unknown - the raw stock, never air.
pub struct PriorCell {
    plan: Option<PriorPlan>,
    dex: Option<Dexel>,
}

impl PriorCell {
    pub(super) fn new(plan: Option<PriorPlan>) -> Self {
        Self { plan, dex: None }
    }

    /// The grid the program ahead leaves standing, or `None` when
    /// nothing ahead has cut, or what it cut cannot be read.
    pub(super) fn get(&mut self) -> Option<&Dexel> {
        if let Some(plan) = self.plan.take() {
            match plan.simulate() {
                Ok(d) => self.dex = Some(d),
                Err(e) => eprintln!("camcraft: the program ahead could not be cut, the raw stock stands: {e}"),
            }
        }
        self.dex.as_ref()
    }
}

/// The prior grid as one drill reads it: under the disk on a mill
/// ([`crate::verify::prior_surface`]), revolved about the axis for a
/// lathe's axial hole ([`revolved_surface`]).
#[derive(Clone, Copy)]
pub(super) struct PriorRead<'a> {
    pub dex: &'a Dexel,
    pub revolved: bool,
}

impl PriorRead<'_> {
    pub(super) fn surface(&self, q: Point2, r: f64) -> Option<f64> {
        if self.revolved { revolved_surface(self.dex, q, r) } else { crate::verify::prior_surface(self.dex, q, r) }
    }
}

/// The highest material the prior grid holds over the disk (`c`, `r`)
/// of a work turning about `c` (a lathe's axial hole, on the spindle):
/// at each radius sampled out to `r`, the LOWEST of four compass cells
/// on that ring (the spindle carried any cut all the way round; fewer
/// samples can only read higher), then the highest of those rings.
/// `None` when no ring is on the grid or a height is unreadable - an
/// unknown prior is the raw stock.
fn revolved_surface(dex: &Dexel, c: Point2, r: f64) -> Option<f64> {
    if !(r.is_finite() && r >= 0.0 && dex.cell > 0.0 && c.x.is_finite() && c.y.is_finite()) {
        return None;
    }
    let step = dex.cell * 0.5;
    let n = (r / step).ceil().clamp(0.0, 1.0e5) as usize;
    let mut top: Option<f64> = None;
    for i in 0..=n {
        let rho = (i as f64 * step).min(r);
        let mut ring: Option<f64> = None;
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            if let Some(k) = dex.col(Point2::new(c.x + dx * rho, c.y + dy * rho)) {
                let z = dex.top_at(k);
                if z.is_nan() {
                    return None;
                }
                ring = Some(ring.map_or(z, |t| t.min(z)));
            }
        }
        if let Some(z) = ring {
            top = Some(top.map_or(z, |t| t.max(z)));
        }
    }
    top
}
