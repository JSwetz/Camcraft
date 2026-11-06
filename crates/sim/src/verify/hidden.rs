//! Uncut stock under overhangs (M13.7 B): the solid-law backstop of the reach laws (c), (c3) and rapid law (d) - stock a plane section cannot show because part material hangs over it, read from the model's down-facing shadow (the projection identity of a closed solid's boundary, `slice::shadow_above`) on our own conservative rest model; no vendor source.
//!
//! The plane-section laws ask "is the part's section at height h over
//! this column, or was the column never cut down to h?" - and they only
//! ask it at the section walls and the rapid's three points. Stock under
//! a ledge stands OUTSIDE the section at h (the ledge is higher), so its
//! columns are never sampled: a shank or a rapid sliding under the ledge
//! through that stock was silent. This module reads it as a solid:
//!
//! (q, h) is hidden stock iff q is inside the stock footprint, h is
//! under the stock top, part material hangs over (q, h)
//! (`shadow_above(h)` holds q), the part itself is not at (q, h) (the
//! section at h does not hold q - that is the solid law b2's steel), and
//! no cutting chord at a plane at or below h passed within the tool
//! radius of q on a plane whose section left q open (the column clear
//! law of [`Rest::surface`], read at h instead of as a surface height).
//!
//! The stock solid is therefore "stock minus what earlier cuts of the
//! same path swept", with the sweep read the way `surface` reads it: a
//! chord clears its disc's column from its plane up. Exact in Z, sampled
//! in XY - the hidden region's boundary every [`STEP`] (nudged a hair
//! inside) and rings in the band annulus / lines along the rapid's disc.
//! A hidden sliver thinner than the sampling can be missed; a slice that
//! fails convicts (fail closed). It only ever ADDS convictions: every
//! caller asks it only where its plane-section law found nothing.

use super::laws::dist2_seg;
use super::rest::Rest;

use crate::geo2d::{Point2, Region};
use std::rc::Rc;

/// XY sample spacing of the hidden region's boundary and the rapid's
/// disc lines, mm.
const STEP: f64 = 0.5;
/// Inward nudge of a boundary sample, mm (membership unambiguous).
const NUDGE: f64 = 0.05;
/// Most boundary samples one plane keeps; past it the spacing widens.
const MAX_SAMPLES: usize = 50_000;
/// Ring sample count per radius in a band annulus.
const RING: usize = 24;

/// One plane's hidden stock: the region and its inside boundary samples.
pub(super) struct Hidden {
    region: Region,
    bounds: Option<(Point2, Point2)>,
    samples: Vec<Point2>,
}

/// The region's boundary every `STEP`, each point nudged to whichever
/// side of its edge the region holds (loop orientation not assumed).
fn inside_samples(region: &Region) -> Vec<Point2> {
    let total: f64 = region
        .loops()
        .iter()
        .map(|l| {
            let p = l.points();
            (0..p.len())
                .map(|k| dist2_seg(p[k], p[(k + 1) % p.len()], p[(k + 1) % p.len()]).sqrt())
                .sum::<f64>()
        })
        .sum();
    let step = if total.is_finite() && total / STEP > MAX_SAMPLES as f64 { total / MAX_SAMPLES as f64 } else { STEP };
    let mut out = Vec::new();
    for l in region.loops() {
        let p = l.points();
        for k in 0..p.len() {
            let (a, b) = (p[k], p[(k + 1) % p.len()]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if !(len.is_finite() && len > 0.0) {
                continue;
            }
            let (nx, ny) = (-dy / len, dx / len);
            let n = ((len / step).ceil() as usize).clamp(1, MAX_SAMPLES);
            for i in 0..n {
                let t = (i as f64 + 0.5) / n as f64;
                let q = Point2::new(a.x + dx * t, a.y + dy * t);
                for s in [1.0, -1.0] {
                    let c = Point2::new(q.x + s * NUDGE * nx, q.y + s * NUDGE * ny);
                    if region.contains(c) {
                        out.push(c);
                        break;
                    }
                }
            }
        }
    }
    out
}

impl Rest<'_, '_> {
    /// The hidden stock at plane `h` (cached by its bits): the shadow of
    /// what hangs over `h`, inside the stock footprint, less the section
    /// at `h`. `None` is a slice or clip that failed.
    fn hidden_at(&self, h: f64) -> Option<Rc<Hidden>> {
        let key = h.to_bits();
        if let Some(hit) = self.hidden_cache.borrow().get(&key) {
            return hit.clone();
        }
        let cx = self.cx();
        let fresh = cx.model.and_then(|f| {
            let hang = crate::geometry::slice::shadow_above(f, h).ok()?;
            let region = if hang.is_empty() {
                hang
            } else {
                let foot = cx.stock.unwrap_or(cx.region);
                let sec = self.islands_at(f, h)?;
                hang.intersect(foot).ok()?.difference(&sec).ok()?
            };
            let samples = inside_samples(&region);
            Some(Rc::new(Hidden { bounds: region.bounds(), region, samples }))
        });
        self.hidden_cache.borrow_mut().entry(key).or_insert_with(|| fresh.clone());
        fresh
    }

    /// Whether (q, h) is uncut stock: hidden at `h` and in no cutting
    /// chord's cleared column at or below `h`.
    fn uncut(&self, hid: &Hidden, q: Point2, h: f64) -> bool {
        let cx = self.cx();
        if !(hid.region.contains(q) && self.in_stock(q)) {
            return false;
        }
        let r = cx.tool.radius;
        let r2 = (r + 1e-9) * (r + 1e-9);
        let top = h + cx.z_tol;
        let cut = self.cut();
        let n = cut.partition_point(|c| c.2 <= top);
        !cut[..n].iter().any(|(a, b, z)| {
            dist2_seg(q, *a, *b) <= r2 && cx.model.is_none_or(|f| !self.island_holds(f, *z, q))
        })
    }

    /// The plane's hidden stock when it can stand within `reach` of the
    /// box [lo, hi]: `Err(())` fails closed, `Ok(None)` is nothing there.
    fn hidden_near(&self, h: f64, lo: Point2, hi: Point2, reach: f64) -> Result<Option<Rc<Hidden>>, ()> {
        let cx = self.cx();
        if cx.model.is_none() || !h.is_finite() || h >= cx.stock_top_z {
            return Ok(None);
        }
        let hid = self.hidden_at(h).ok_or(())?;
        match hid.bounds {
            Some((b0, b1))
                if b0.x <= hi.x + reach && b1.x >= lo.x - reach && b0.y <= hi.y + reach && b1.y >= lo.y - reach =>
            {
                Ok(Some(hid))
            }
            _ => Ok(None),
        }
    }

    /// Law (c)/(c3) backstop: uncut hidden stock inside the band annulus
    /// `r_in < |q - pc| <= r_out` at the band's bottom `h`.
    pub(super) fn hidden_in_annulus(&self, pc: Point2, r_in: f64, r_out: f64, h: f64) -> bool {
        if !(r_out > r_in && pc.x.is_finite() && pc.y.is_finite()) {
            return false;
        }
        let hid = match self.hidden_near(h, pc, pc, r_out) {
            Err(()) => return true,
            Ok(None) => return false,
            Ok(Some(hid)) => hid,
        };
        let (i2, o2) = (r_in * r_in, (r_out + 1e-9) * (r_out + 1e-9));
        let in_ring = |q: &Point2| {
            let dd = dist2_seg(*q, pc, pc);
            dd > i2 && dd <= o2
        };
        let rings = [r_in + (r_out - r_in) * 0.02, (r_in + r_out) * 0.5, r_out].into_iter().flat_map(|rad| {
            (0..RING).map(move |k| {
                let t = std::f64::consts::TAU * k as f64 / RING as f64;
                Point2::new(pc.x + rad * t.cos(), pc.y + rad * t.sin())
            })
        });
        rings
            .chain(hid.samples.iter().copied())
            .filter(in_ring)
            .any(|q| self.uncut(&hid, q, h))
    }

    /// Law (d) backstop: uncut hidden stock inside the tool disc of
    /// half-width `w` swept from `a` to `b` at height `h`.
    pub(super) fn hidden_along(&self, a: Point2, b: Point2, h: f64, w: f64) -> bool {
        let w = if w.is_finite() { w.max(0.0) } else { 0.0 };
        let lo = Point2::new(a.x.min(b.x), a.y.min(b.y));
        let hi = Point2::new(a.x.max(b.x), a.y.max(b.y));
        let hid = match self.hidden_near(h, lo, hi, w) {
            Err(()) => return true,
            Ok(None) => return false,
            Ok(Some(hid)) => hid,
        };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if !len.is_finite() {
            return true; // the NonFinite law names it; never a silent pass
        }
        let (nx, ny) = if len > 0.0 { (-dy / len, dx / len) } else { (0.0, 0.0) };
        let n = ((len / STEP).ceil() as usize).clamp(1, 4096);
        let w2 = (w + 1e-9) * (w + 1e-9);
        let lines = (0..=n).flat_map(|k| {
            let t = k as f64 / n as f64;
            let c = Point2::new(a.x + dx * t, a.y + dy * t);
            [0.0, 0.5, 1.0, -0.5, -1.0].map(|s| Point2::new(c.x + s * w * nx, c.y + s * w * ny))
        });
        lines
            .chain(hid.samples.iter().copied().filter(|q| dist2_seg(*q, a, b) <= w2))
            .any(|q| self.uncut(&hid, q, h))
    }
}
