//! Issue #96 review: law (d)'s vertical dives read the stock the setup's earlier operations left standing (the re-machining rest grid), not only the raw block - our own reading of the shop rule "never traverse through uncut stock" on the dexel model; no vendor material consulted.

use super::types::Warning;

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::stock_sim::Dexel;

/// `warns` without the convictions of vertical rapids DOWN that the
/// program ahead of this operation already made safe. Law (d) judges a
/// vertical dive against `min(this op's own cuts before it, the stock
/// top)` at its column; the operations ahead of it in the same setup
/// have cut too, and their rest grid `prior` is the third term of that
/// min. So a conviction stands exactly while the dive also ends below
/// the prior surface - an acquittal by the prior is the same verdict
/// the column law would give with the prior in its min.
///
/// Read conservatively: the prior surface under a dive is the HIGHEST
/// grid column the tool's disc of radius `tool_r` covers (the whole
/// footprint must have been cut, not just the centre); cells off the
/// grid are outside the stock (air); a disc that covers no cell, or a
/// cell whose height is not a number, leaves the conviction standing -
/// an unknown prior is the raw stock. Lateral rapids keep every
/// conviction: only the vertical dive's verdict is re-read here.
pub fn acquit_by_prior(path: &Toolpath, prior: &Dexel, tool_r: f64, z_tol: f64, warns: &mut Vec<Warning>) {
    warns.retain(|w| {
        let Warning::RapidInStock { at } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        let vertical_down = m.kind == MoveKind::Rapid && dx * dx + dy * dy <= z_tol * z_tol && m.to.z < m.from.z;
        if !vertical_down {
            return true;
        }
        match prior_surface(prior, Point2::new(m.to.x, m.to.y), tool_r) {
            Some(top) => m.to.z < top - z_tol,
            None => true,
        }
    });
}

/// The highest material the prior grid holds under the disc (q, r):
/// `None` when the disc covers no grid cell or a height is unreadable.
/// Issue #89: drilling's reach, R-plane and holder laws read the same
/// conservative surface the rapid law does.
pub fn prior_surface(dex: &Dexel, q: Point2, r: f64) -> Option<f64> {
    if !(r.is_finite() && r >= 0.0 && dex.cell > 0.0 && q.x.is_finite() && q.y.is_finite()) {
        return None;
    }
    // Every cell whose centre lies in the disc, plus q's own cell (a
    // disc narrower than a cell still stands on one).
    let rr = r + dex.cell * 0.5;
    let lo = |v: f64, o: f64| ((v - rr - o) / dex.cell).floor().max(0.0);
    let hi = |v: f64, o: f64, n: usize| ((v + rr - o) / dex.cell).ceil().min(n as f64);
    let (ix0, ix1) = (lo(q.x, dex.x0), hi(q.x, dex.x0, dex.nx));
    let (iy0, iy1) = (lo(q.y, dex.y0), hi(q.y, dex.y0, dex.ny));
    let mut top: Option<f64> = dex.col(q).map(|k| dex.top_at(k));
    let r2 = r * r;
    let (mut iy, mut ix);
    iy = iy0;
    while iy < iy1 {
        ix = ix0;
        while ix < ix1 {
            let c = dex.center(ix as usize, iy as usize);
            let (ex, ey) = (c.x - q.x, c.y - q.y);
            if ex * ex + ey * ey <= r2
                && let Some(k) = dex.col(c)
            {
                let z = dex.top_at(k);
                top = Some(top.map_or(z, |t| t.max(z)));
                if z.is_nan() {
                    return None;
                }
            }
            ix += 1.0;
        }
        iy += 1.0;
    }
    top.filter(|t| !t.is_nan())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::Move;
    use crate::geometry::Point3;

    fn dive(z0: f64, z1: f64) -> Toolpath {
        let mv = |kind, a: f64, b: f64| Move {
            rotary: None,
            kind,
            from: Point3::new(0.0, 0.0, a),
            to: Point3::new(0.0, 0.0, b),
            feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
            arc: None,
            axis: None,
        };
        Toolpath {
            moves: vec![mv(MoveKind::Rapid, z0, z1), mv(MoveKind::Plunge, z1, z1 - 4.0)],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
        }
    }

    /// A 40 x 40 block, top Z 25, with a 10 x 10 pocket at the origin
    /// cleared to Z 15 by an earlier operation.
    fn pocketed() -> Dexel {
        let mut d = match Dexel::block(-20.0, -20.0, 20.0, 20.0, 25.0, 0.25) {
            Ok(d) => d,
            Err(e) => panic!("block: {e:?}"),
        };
        for iy in 0..d.ny {
            for ix in 0..d.nx {
                let c = d.center(ix, iy);
                if c.x.abs() < 5.0
                    && c.y.abs() < 5.0
                    && let Some(k) = d.col(c)
                {
                    d.lower(k, 15.0);
                }
            }
        }
        d
    }

    #[test]
    fn a_dive_into_a_pocket_an_earlier_op_cleared_is_acquitted() {
        // A drill rapids 35 -> 16 over the pocket floor (Z 15): its own
        // law convicts it against the raw top Z 25; the prior says air.
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(35.0, 16.0), &pocketed(), 2.0, 1e-3, &mut w);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_dive_past_the_prior_floor_still_stands() {
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(35.0, 14.0), &pocketed(), 2.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }

    #[test]
    fn a_tool_wider_than_the_cleared_pocket_still_stands() {
        // A 12 mm disc over a 10 mm pocket: its rim meets the raw top.
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(35.0, 16.0), &pocketed(), 6.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }

    #[test]
    fn a_lateral_conviction_is_never_re_read() {
        let mut p = dive(35.0, 16.0);
        if let Some(m) = p.moves.get_mut(0) {
            m.to.x = 3.0;
        }
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&p, &pocketed(), 2.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }
}
