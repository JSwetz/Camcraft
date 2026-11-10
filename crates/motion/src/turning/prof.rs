//! Profile turning (M5.2 B): the kinds that machine the turned
//! silhouette itself - the finish as one ride of the nose on the
//! offset curve, the rough as concentric offset rungs eating the
//! stock shell down to a constant leave.
//!
//! Both kernels stand on the same construction: the CL curve at
//! distance d from the material ([`super::offset`]'s max-of-claims
//! ride), walked bottom-up with a radial plunge in and a radial
//! retract out. That construction is what makes NO GOUGE a theorem
//! here: every CL point is at distance >= d from the model minus
//! the chord budget, so the nose's swept envelope reaches the
//! finished surface tangentially and no deeper than the tolerance
//! the caller asked for. The verifier judges the path like every
//! other kind; nothing here leans on it.
//!
//! The ROUGH's ladder is radial, not axial: rung j rides at
//! d_j = nose + stock + j * stepdown, run outermost first, and each
//! rung's nose cuts exactly the shell [d_j - nose, d_j] of distance-
//! to-part - so stepdown over twice the nose would leave a standing
//! sheet no rung touches, and that is refused at the door, facing's
//! ridge law read radially. The finish pass then removes the last
//! `stock` of leave as its own ride at d = nose.
//!
//! Provenance: OD profiling and G71-style concentric roughing on a
//! lathe, from machine-tool handbooks and the RS-274 vocabulary;
//! the distance-to-part shell reading of the rung ladder is our
//! own, on the same nose envelope the verifier measures with. No
//! vendor strategy consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::offset;
use super::profile;
use super::stock::StockEnv;

/// The profile finish as the kernel reads it: the two heights that
/// bound the walk along the bar, the rapids plane, the feed in
/// cut, the plunge feed for the radial lead, the insert's nose and
/// the chord tolerance the silhouette and its ride must settle at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileFinishParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
    pub tolerance: f64,
}

/// Profile roughing adds the RADIAL ladder: the rung spacing (a
/// shell width - the same 2 * nose ceiling facing's ridge law puts
/// on its axial band) and the stock left standing for the finish.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileRoughParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub stock_to_leave: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
    pub tolerance: f64,
}

/// A rung cap with room to spare - facing's number, the same
/// promise. `pub(crate)` from M5.2 E: multi-pass turning runs the
/// same ladder law.
pub(crate) const MAX_RUNGS: f64 = 100_000.0;

/// What a profile kernel walks on: the frame, the silhouette's
/// sampled (station, radius) points, the whole profile polyline,
/// and the window's station pair.
type Ride = (profile::Frame, Vec<(f64, f64)>, profile::Profile, (f64, f64));

/// What the two kernels refuse in common before either walks: the
/// numbers are numbers, the nose is an arc, the model revolves
/// about a walkable axis, and the height window holds bar.
/// `pub(crate)` from M5.2 E: the contour kinds stand the same door
/// before their rides.
pub(crate) fn setup(
    facets: &[Triangle],
    clearance_z: f64,
    top_z: f64,
    bottom_z: f64,
    nose: f64,
    tol: f64,
) -> Result<Ride, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to turn".into()));
    }
    if ![clearance_z, top_z, bottom_z, nose, tol]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(GenError::Invalid(
            "profile turning was asked with a number that is not one".into(),
        ));
    }
    if nose <= 0.0 {
        return Err(GenError::Invalid(format!(
            "profiling is an arc, not a line - a nose radius of {nose:.3} is a \
             mathematical point, and a point cannot ride a profile"
        )));
    }
    if tol <= 0.0 {
        return Err(GenError::Invalid(format!(
            "the profile tolerance must be a positive size, not {tol}"
        )));
    }
    let f = profile::Frame::about(facets)?;
    // The chord budget splits: a quarter for the silhouette's own
    // sampling, a quarter for the ride's, both signs of error
    // counted - the surface left behind then stands within tol of
    // the model wherever the nose can reach.
    let pts = profile::silhouette(facets, &f, tol * 0.25)?;
    let prof = profile::profile_of(facets, &f)?;
    let s_lo = f.station_of(bottom_z).max(pts[0].0);
    let s_hi = f.station_of(top_z).min(pts[pts.len() - 1].0);
    if s_hi <= s_lo {
        return Err(GenError::Empty(
            "nothing to turn - the height window holds no length of the bar".into(),
        ));
    }
    Ok((f, pts, prof, (s_lo, s_hi)))
}

/// How a ride leaves its cut at the window's top (#229).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exit {
    /// Nothing stands past the pass end: a rapid out to the clear radius.
    Rapid,
    /// The bar runs on past the pass end, so the move out cuts through
    /// its uncut end: at the plunge feed, the ride's radial lead.
    Feed,
}

impl Exit {
    /// The exit for a ride ending at station `s_end` inside `stock`.
    pub(crate) fn inside(stock: Option<StockEnv>, s_end: f64) -> Exit {
        if StockEnv::runs_past(stock, s_end) {
            Exit::Feed
        } else {
            Exit::Rapid
        }
    }
}

/// One rung's walk: plunge in radially at the window's bottom,
/// ride the offset ascending, slide off radially at the top - by
/// rapid, or at the plunge feed where the bar runs on past the top
/// ([`Exit`], #229).
/// `pub(crate)` from M5.2 E: multi-pass turning chains blocks of
/// these rides onto one builder.
pub(crate) fn ride(
    b: &mut PathBuilder,
    f: &profile::Frame,
    cl: &[(f64, f64)],
    rho_clear: f64,
    feed: f64,
    plunge_feed: f64,
    exit: Exit,
) {
    let p = |q: &(f64, f64)| f.point(q.1, q.0);
    b.push(MoveKind::Rapid, f.point(rho_clear, cl[0].0), None);
    b.push(MoveKind::Feed, p(&cl[0]), Some(plunge_feed));
    for q in &cl[1..] {
        b.push(MoveKind::Feed, p(q), Some(feed));
    }
    let out = f.point(rho_clear, cl[cl.len() - 1].0);
    match exit {
        Exit::Rapid => b.push(MoveKind::Rapid, out, None),
        Exit::Feed => b.push(MoveKind::Feed, out, Some(plunge_feed)),
    }
}

/// The finished ride: one pass of the nose along the profile,
/// leave-free by construction, within tolerance by the chord law.
pub fn profile_finish(facets: &[Triangle], p: &ProfileFinishParams) -> Result<Toolpath, GenError> {
    profile_finish_in(facets, p, None)
}

/// [`profile_finish`] inside a setup stock (#130): the approach and
/// retract stand clear of the bar's OD, not only the part's, and a bar
/// running on past the window's top is left at feed (#229).
pub fn profile_finish_in(
    facets: &[Triangle],
    p: &ProfileFinishParams,
    stock: Option<StockEnv>,
) -> Result<Toolpath, GenError> {
    let (f, pts, prof, (s_lo, s_hi)) = setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.nose_radius,
        p.tolerance,
    )?;
    let cl = offset::profile_offset(&pts, p.nose_radius, p.tolerance * 0.25, s_lo, s_hi)?;
    // A rapid that clears every CL point of the pass by at least
    // the tolerance - the facing ladder's clearance law, stated at
    // the exact reach of this one's ride.
    let rho_clear = cl.iter().map(|q| q.1).fold(0.0f64, f64::max) + p.tolerance;
    let rho_clear = StockEnv::rapid_radius(stock, prof.r_max, rho_clear, p.nose_radius);
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(p.clearance_z)));
    ride(&mut b, &f, &cl, rho_clear, p.feed, p.plunge_feed, Exit::inside(stock, s_hi));
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(p.clearance_z)),
        None,
    );
    Ok(b.finish())
}

/// The concentric ladder: offset rungs outermost first, each one
/// eating a radial shell stepdown wide (2 * nose wide at the most),
/// down to the leave the finish will take off.
pub fn profile_rough(facets: &[Triangle], p: &ProfileRoughParams) -> Result<Toolpath, GenError> {
    profile_rough_in(facets, p, None)
}

/// [`profile_rough`] inside a setup stock (#130): the ladder starts at
/// the bar's OD - the outermost rung's nose reaches the bar's skin -
/// and steps down to the part plus the leave; a rung riding wholly
/// outside the bar is air and is dropped; every rapid clears the OD,
/// and where the bar runs on past the window's top each rung leaves
/// its cut at feed instead (#229).
pub fn profile_rough_in(
    facets: &[Triangle],
    p: &ProfileRoughParams,
    stock: Option<StockEnv>,
) -> Result<Toolpath, GenError> {
    let (f, pts, prof, (s_lo, s_hi)) = setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.nose_radius,
        p.tolerance,
    )?;
    if p.stepdown <= 0.0 {
        return Err(GenError::Invalid(
            "profile roughing needs a positive stepdown".into(),
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
            "stock to leave of {:.3} is stock to remove - the finish pass owns the \
             profile itself, negative leave is not a thing",
            p.stock_to_leave
        )));
    }
    // The deepest material point of the bar below its tallest
    // skin bounds what any rung must eat.
    let e_max = StockEnv::top(stock, prof.r_max, prof.r_max) - pts.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
    let reach = 2.0 * p.nose_radius + p.stock_to_leave;
    let rungs = if e_max <= reach {
        0.0
    } else {
        ((e_max - reach) / p.stepdown).ceil()
    };
    if rungs + 1.0 > MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the roughing ladder would run {:.0} radial rungs - the stepdown is far too \
             fine for the stock to eat",
            rungs + 1.0
        )));
    }
    let n = rungs as usize + 1;
    let mut cl_by_rung = Vec::with_capacity(n);
    for j in (0..n).rev() {
        let d = p.nose_radius + p.stock_to_leave + (j as f64) * p.stepdown;
        cl_by_rung.push(offset::profile_offset(
            &pts,
            d,
            p.tolerance * 0.25,
            s_lo,
            s_hi,
        )?);
    }
    // A rung whose nose never reaches the bar cuts nothing: dropped
    // (the innermost is the leave line and always rides).
    if let Some(e) = StockEnv::usable(stock, prof.r_max) {
        let last = cl_by_rung.len().saturating_sub(1);
        let mut k = 0usize;
        cl_by_rung.retain(|cl| {
            let keep = k == last || cl.iter().map(|q| q.1).fold(f64::INFINITY, f64::min) - p.nose_radius < e.r_out;
            k += 1;
            keep
        });
    }
    let Some(outer) = cl_by_rung.first() else {
        return Err(GenError::Empty("the roughing ladder holds no rung".into()));
    };
    let rho_clear = outer.iter().map(|q| q.1).fold(0.0f64, f64::max) + p.tolerance;
    let rho_clear = StockEnv::rapid_radius(stock, prof.r_max, rho_clear, p.nose_radius);
    let exit = Exit::inside(stock, s_hi);
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(p.clearance_z)));
    for cl in &cl_by_rung {
        ride(&mut b, &f, cl, rho_clear, p.feed, p.plunge_feed, exit);
    }
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(p.clearance_z)),
        None,
    );
    Ok(b.finish())
}
