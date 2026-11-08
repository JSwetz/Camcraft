//! Parting (M5.2 C, split from blade.rs for #130): the cutoff as a banded revolved cut - one center, a depth ladder through the bar's column and the through-axis rung that severs it; machine-tool handbook cutoff practice, the column reading our own, on the silhouette the profile kinds ride. No vendor strategy consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::blade::{plunge, rung, PartParams, MAX_RUNGS};
use super::profile;
use super::revolve;
use super::stock::StockEnv;

/// Parting: one center at the parting plane, a depth ladder down
/// through the bar's full column, and the through-axis rung that
/// severs it. The column is the model's own (its top is the span
/// over the kerf band, its bottom the axis); `top_z` shapes
/// nothing here - it rides the trio to keep the heights law
/// uniform, and stands only for the finite-number check.
pub fn part(facets: &[Triangle], p: &PartParams) -> Result<Toolpath, GenError> {
    part_in(facets, p, None)
}

/// [`part`] inside a setup stock (#130): the depth ladder steps down
/// from the bar's OD - the kerf's column is the bar's, not only the
/// part's - and every rapid clears it.
pub fn part_in(facets: &[Triangle], p: &PartParams, stock: Option<StockEnv>) -> Result<Toolpath, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to part".into()));
    }
    if ![
        p.tolerance,
        p.clearance_z,
        p.top_z,
        p.parting_z,
        p.stepdown,
        p.feed,
        p.plunge_feed,
        p.blade_width,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err(GenError::Invalid(
            "parting was asked with a number that is not one".into(),
        ));
    }
    if p.blade_width <= 0.0 {
        return Err(GenError::Invalid(format!(
            "a blade of width {:.3} cuts no band - the kerf's width IS the tool; \
             mount a grooving blade with a real width",
            p.blade_width
        )));
    }
    if p.tolerance <= 0.0 {
        return Err(GenError::Invalid(format!(
            "the tolerance must be a positive size, not {}",
            p.tolerance
        )));
    }
    if p.stepdown <= 0.0 {
        return Err(GenError::Invalid("parting needs a positive stepdown".into()));
    }
    let f = profile::Frame::about(facets)?;
    let pts = profile::silhouette(facets, &f, p.tolerance * 0.25)?;
    let prof = profile::profile_of(facets, &f)?;
    let sg = revolve::segs(&pts);
    let (d_lo, d_hi) = (pts[0].0, pts[pts.len() - 1].0);
    let s_c = f.station_of(p.parting_z);
    let half = p.blade_width * 0.5;
    let (lo, hi) = ((s_c - half).max(d_lo), (s_c + half).min(d_hi));
    if hi <= lo {
        return Err(GenError::Empty(
            "nothing to part - the parting plane stands off the end of the bar".into(),
        ));
    }
    // The column the kerf must fall through: the model's own top
    // across the band (its limits ride the chords' clipped ends)
    // and at the plane itself.
    let ref_top =
        revolve::span_max(&sg, s_c - half, s_c + half).max(revolve::material(&sg, s_c));
    if ref_top <= 0.0 {
        return Err(GenError::Empty(
            "nothing to part - the parting plane stands in the air past the bar's top".into(),
        ));
    }
    let ref_top = StockEnv::top(stock, prof.r_max, ref_top);
    let rho_clear = StockEnv::rapid_radius(stock, prof.r_max, prof.r_max + p.tolerance, 0.0);
    let n = (ref_top / p.stepdown).ceil();
    if n > MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the parting ladder would run {:.0} rungs - the stepdown is far too fine \
             for the bar's column",
            n
        )));
    }
    let mut tips: Vec<f64> = (1..=n as usize)
        .map(|j| rung(ref_top - (j as f64) * p.stepdown, 0.0))
        .collect();
    // The through-axis rung: the blade tip stands ON the axis, the
    // kerf's whole column is above it, and the verdict reads the
    // separation.
    if tips.last().is_none_or(|&t| t > 0.0) {
        tips.push(0.0);
    }
    tips.dedup();
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(p.clearance_z)));
    plunge(&mut b, &f, s_c, rho_clear, &tips, p.plunge_feed);
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(p.clearance_z)),
        None,
    );
    Ok(b.finish())
}
