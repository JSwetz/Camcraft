//! The three contour rides and their doors, each stated once
//! (split from `contour.rs` by the M5.2 E 400-line seam; the law is
//! documented in `mod.rs`).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use crate::turning::{offset, prof, StockEnv};

use super::{ChamferParams, MultiPassParams, TaperParams};
use super::door::{self, face_ride};

/// The spring-pass cap: past this many finishing rides a program is
/// a polishing sequence, not a turning op.
const MAX_PASSES: u32 = 64;

/// Chamfering: one cone face across the window's corner, the cone
/// standing `angle_deg` off the cylinder. The face may run down
/// toward either window end - the authored angle only states the
/// slope's size, and the model says which way it falls - so both
/// readings are tried and the door judges the honest one.
pub fn chamfer(facets: &[Triangle], p: &ChamferParams) -> Result<Toolpath, GenError> {
    chamfer_in(facets, p, None)
}

/// [`chamfer`] in setup stock: rapids clear its OD and an exit inside its far end uses the plunge feed (#323).
pub fn chamfer_in(facets: &[Triangle], p: &ChamferParams, stock: Option<StockEnv>) -> Result<Toolpath, GenError> {
    if !p.angle_deg.is_finite() {
        return Err(GenError::Invalid(
            "chamfer turning was asked with a number that is not one".into(),
        ));
    }
    if p.angle_deg <= 0.0 {
        return Err(GenError::Invalid(format!(
            "a chamfer of {:.1} degrees is a cylinder - the kind only speaks a cone; \
             turn it with a profile ride",
            p.angle_deg
        )));
    }
    if p.angle_deg >= 90.0 {
        return Err(GenError::Invalid(format!(
            "a chamfer of {:.1} degrees is a face - facing is its own kind",
            p.angle_deg
        )));
    }
    let (f, pts, _prof, win) = prof::setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.nose_radius,
        p.tolerance,
    )?;
    let (s_lo, s_hi) = win;
    let tan = p.angle_deg.to_radians().tan();
    let a = door::r_at(&pts, s_lo);
    let b = door::r_at(&pts, s_hi);
    let l_lo = |s: f64| a - (s - s_lo) * tan;
    let l_hi = |s: f64| b - (s_hi - s) * tan;
    let (w_lo, st_lo) = door::worst_dev(&pts, win, &l_lo);
    let (w_hi, st_hi) = door::worst_dev(&pts, win, &l_hi);
    let (dev, at) = if w_lo <= w_hi {
        (w_lo, st_lo)
    } else {
        (w_hi, st_hi)
    };
    if dev > p.tolerance {
        // #677: names no spot yet - the station is in the turning meridian, which has no world map.
        return Err(GenError::Invalid(format!(
            "the model is not a {:.1}-degree chamfer in this window - its skin stands \
             {dev:.3} off the authored cone at station {at:.3}. A chamfer op is a promise \
             over a MODELED chamfer; model the face or narrow the window to it",
            p.angle_deg
        )));
    }
    face_ride(
        &f,
        &pts,
        win,
        p.nose_radius,
        p.tolerance,
        p.clearance_z,
        p.feed,
        p.plunge_feed,
        stock,
    )
}

/// Taper turning: the cone the two diameters state, stood across
/// the window. A straight reading (equal diameters) is a cylinder
/// the door still judges honestly; the ride does not care which
/// name the op carries.
pub fn taper(facets: &[Triangle], p: &TaperParams) -> Result<Toolpath, GenError> {
    taper_in(facets, p, None)
}

/// [`taper`] in setup stock: rapids clear its OD and an exit inside its far end uses the plunge feed (#323).
pub fn taper_in(facets: &[Triangle], p: &TaperParams, stock: Option<StockEnv>) -> Result<Toolpath, GenError> {
    if !p.bottom_dia.is_finite() || !p.top_dia.is_finite() {
        return Err(GenError::Invalid(
            "taper turning was asked with a number that is not one".into(),
        ));
    }
    if p.bottom_dia < 0.0 || p.top_dia < 0.0 {
        return Err(GenError::Invalid(format!(
            "a diameter of {:.3} is not a size - the taper's ends stand on the axis or \
             off it, never through it",
            p.bottom_dia.min(p.top_dia)
        )));
    }
    if p.bottom_dia == 0.0 && p.top_dia == 0.0 {
        return Err(GenError::Invalid(
            "a taper that stands on the axis at both ends is a line, not a part - \
             there is no core between the flanks"
                .into(),
        ));
    }
    let (f, pts, _prof, win) = prof::setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.nose_radius,
        p.tolerance,
    )?;
    let (s_lo, s_hi) = win;
    let r_a = p.bottom_dia * 0.5;
    let r_b = p.top_dia * 0.5;
    let line = |s: f64| r_a + (r_b - r_a) * (s - s_lo) / (s_hi - s_lo);
    let (dev, at) = door::worst_dev(&pts, win, &line);
    if dev > p.tolerance {
        // #677: names no spot yet - the station is in the turning meridian, which has no world map.
        return Err(GenError::Invalid(format!(
            "the model is not the authored cone - its skin stands {dev:.3} off the \
             taper line at station {at:.3}. A taper op rides a MODELED taper; rough the \
             bar near it or profile the cone into the model"
        )));
    }
    face_ride(
        &f,
        &pts,
        win,
        p.nose_radius,
        p.tolerance,
        p.clearance_z,
        p.feed,
        p.plunge_feed,
        stock,
    )
}

/// Multi-pass turning: the concentric rung ladder eating down to
/// the leave, then `passes` finishing rides sharing the leave out -
/// the last one on the profile itself. The spring-pass law: each
/// ride's distance-to-part is the last ride's shell-width halved
/// onward (ride k of n stands at nose + stock * (n - k) / n), so a
/// program with more rides polishes toward the model instead of
/// cutting stock - which is exactly what the extra passes are for.
pub fn multi_pass(facets: &[Triangle], p: &MultiPassParams) -> Result<Toolpath, GenError> {
    multi_pass_in(facets, p, None)
}

/// [`multi_pass`] in setup stock: rapids clear its OD and an exit inside its far end uses the plunge feed (#323).
pub fn multi_pass_in(facets: &[Triangle], p: &MultiPassParams, stock: Option<StockEnv>) -> Result<Toolpath, GenError> {
    if !(p.stepdown.is_finite() && p.stock_to_leave.is_finite()) {
        return Err(GenError::Invalid(
            "multi-pass turning was asked with a number that is not one".into(),
        ));
    }
    if p.passes < 1 {
        return Err(GenError::Invalid(
            "multi-pass turning with no finishing ride leaves the stock standing - \
             the finish owns the profile"
                .into(),
        ));
    }
    if p.passes > MAX_PASSES {
        return Err(GenError::Invalid(format!(
            "{passes} finishing rides is a polishing sequence, not a turning program - \
             the kind caps the ladders at {MAX_PASSES}",
            passes = p.passes
        )));
    }
    let (f, pts, prof_pd, win) = prof::setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.nose_radius,
        p.tolerance,
    )?;
    if p.stepdown <= 0.0 {
        return Err(GenError::Invalid(
            "multi-pass turning needs a positive stepdown".into(),
        ));
    }
    if p.stepdown > 2.0 * p.nose_radius {
        return Err(GenError::Invalid(format!(
            "a stepdown of {:.3} is past the nose's reach: a ball of radius {:.3} clears \
             a shell {:.3} thick, and the next rung in would leave a sheet nothing touches \
             - tighten the stepdown",
            p.stepdown,
            p.nose_radius,
            2.0 * p.nose_radius
        )));
    }
    if p.stock_to_leave < 0.0 {
        return Err(GenError::Invalid(format!(
            "stock to leave of {:.3} is stock to remove - the finishing rides own the \
             profile itself, negative leave is not a thing",
            p.stock_to_leave
        )));
    }
    // The ladder, profile_rough's law: rungs outermost first, the
    // innermost standing at nose + stock - the floor every
    // finishing ride then walks from.
    let e_max = StockEnv::top(stock, prof_pd.r_max, prof_pd.r_max) - pts.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
    let reach = 2.0 * p.nose_radius + p.stock_to_leave;
    let rungs = if e_max <= reach {
        0.0
    } else {
        ((e_max - reach) / p.stepdown).ceil()
    };
    if rungs + 1.0 > prof::MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the roughing ladder would run {:.0} radial rungs - the stepdown is far too \
             fine for the stock to eat",
            rungs + 1.0
        )));
    }
    let n = rungs as usize + 1;
    let mut blocks: Vec<Vec<(f64, f64)>> = Vec::with_capacity(n + p.passes as usize);
    for j in (0..n).rev() {
        let d = p.nose_radius + p.stock_to_leave + (j as f64) * p.stepdown;
        blocks.push(offset::profile_offset(
            &pts,
            d,
            p.tolerance * 0.25,
            win.0,
            win.1,
        )?);
    }
    // The clipped window may miss the part's deepest radius. As in
    // profile_rough_in, discard roughing rungs that never reach stock;
    // keep the leave floor and append every finishing ride afterwards.
    if let Some(e) = StockEnv::usable(stock, prof_pd.r_max) {
        let last = blocks.len().saturating_sub(1);
        let mut k = 0usize;
        blocks.retain(|cl| {
            let keep = k == last || cl.iter().map(|q| q.1).fold(f64::INFINITY, f64::min) - p.nose_radius < e.r_out;
            k += 1;
            keep
        });
    }
    let passes = p.passes as f64;
    for k in 1..=p.passes {
        let d = p.nose_radius + p.stock_to_leave * (passes - k as f64) / passes;
        blocks.push(offset::profile_offset(
            &pts,
            d,
            p.tolerance * 0.25,
            win.0,
            win.1,
        )?);
    }
    let rho_clear = blocks
        .iter()
        .flat_map(|cl| cl.iter().map(|q| q.1))
        .fold(0.0f64, f64::max)
        + p.tolerance;
    let rho_clear = StockEnv::rapid_radius(stock, prof_pd.r_max, rho_clear, p.nose_radius);
    let exit = prof::Exit::inside(stock, win.1);
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(p.clearance_z)));
    for cl in &blocks {
        prof::ride(&mut b, &f, cl, rho_clear, p.feed, p.plunge_feed, exit);
    }
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(p.clearance_z)),
        None,
    );
    Ok(b.finish())
}
