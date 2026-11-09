//! The contour kinds' shared door and ride: what the authored
//! numbers promise against the silhouette, and the profile ride
//! every contour kind answers with (split from `contour.rs` by the
//! M5.2 E 400-line seam; the law is documented in `mod.rs`).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::machining2d::GenError;

use crate::turning::{offset, prof, profile, StockEnv};

/// The station equality hair: the door skips samples AT the window
/// ends (they are the corners and the end faces the authored line
/// says nothing about), and equality lives at this scale.
const EPS_S: f64 = 1e-9;

/// The hair a station counts as "at" a window end for the radius
/// read, and the reach scale the door's inside-the-window reading
/// lives at: a ride sample within a nose and this of a window end
/// belongs to the end face the tool visits tangentially, not to
/// the face the parameters promise.
const S_IN: f64 = 0.1;

/// The silhouette's radius at a station: the largest among samples
/// standing there (a corner pair states the surface, the faces
/// below it state other features), and between samples the
/// chord the silhouette itself drew. The window ends the setup
/// quotes are always inside the sampled domain, so the fallback
/// answers a question that cannot be asked.
pub(super) fn r_at(pts: &[(f64, f64)], s: f64) -> f64 {
    let mut best = f64::NEG_INFINITY;
    for q in pts {
        if (q.0 - s).abs() <= S_IN && q.1 > best {
            best = q.1;
        }
    }
    if best.is_finite() {
        return best;
    }
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a.0 - s) * (b.0 - s) < 0.0 {
            let t = (s - a.0) / (b.0 - a.0);
            best = best.max(a.1 + t * (b.1 - a.1));
        }
    }
    if best.is_finite() {
        best
    } else {
        pts[0].1
    }
}

/// The door's reading: the worst distance the silhouette's skin
/// strictly between the window ends stands from the authored line.
/// It walks the POLYLINE SEGMENTS, not the vertex samples - a
/// straight wall carries no sample in a short window and is no
/// less a lie for it. Two lines differ most at an overlap's end,
/// so each segment clipped to the band is read at its overlap
/// endpoints; a vertical segment (a face, the wall of a shoulder)
/// deviates only as far as the line stands outside the radius
/// interval it sweeps - a line crossing the interval touches the
/// skin it promises, wherever along the face.
pub(super) fn worst_dev(
    pts: &[(f64, f64)],
    (s_lo, s_hi): (f64, f64),
    line: &(dyn Fn(f64) -> f64 + Sync),
) -> (f64, f64) {
    let mut w = 0.0f64;
    let mut at = s_lo;
    let (lo, hi) = (s_lo + EPS_S, s_hi - EPS_S);
    let see = |s: f64, d: f64, w: &mut f64, at: &mut f64| {
        if d > *w {
            *w = d;
            *at = s;
        }
    };
    for q in pts.windows(2) {
        let (a, b) = (q[0], q[1]);
        let (s0, s1) = (a.0.min(b.0), a.0.max(b.0));
        if s1 <= lo || s0 >= hi {
            continue;
        }
        if s1 - s0 <= EPS_S {
            let lc = line((s0 + s1) * 0.5);
            let (r0, r1) = (a.1.min(b.1), a.1.max(b.1));
            let d = if lc >= r0 && lc <= r1 {
                0.0
            } else {
                (r0 - lc).abs().min((r1 - lc).abs())
            };
            see(s0, d, &mut w, &mut at);
        } else {
            for s in [s0.max(lo), s1.min(hi)] {
                let t = (s - a.0) / (b.0 - a.0);
                see(s, ((a.1 + t * (b.1 - a.1)) - line(s)).abs(), &mut w, &mut at);
            }
        }
    }
    (w, at)
}

/// The ride every contour kind shares with the profile finish: the
/// nose's offset walk along the model's own skin inside the window,
/// lead in radially, slide out radially, rapids clear of every CL
/// point by the tolerance - [`prof::profile_finish`]'s construction
/// stated once so the contour kinds cannot drift from it.
#[allow(clippy::too_many_arguments)] // the ride's whole frame (window, nose, feed, clear) - a struct would only rename the threading
pub(super) fn face_ride(
    f: &profile::Frame,
    pts: &[(f64, f64)],
    win: (f64, f64),
    nose: f64,
    tol: f64,
    clearance_z: f64,
    feed: f64,
    plunge_feed: f64,
    stock: Option<StockEnv>,
) -> Result<Toolpath, GenError> {
    let cl = offset::profile_offset(pts, nose, tol * 0.25, win.0, win.1)?;
    let rho_clear = cl.iter().map(|q| q.1).fold(0.0f64, f64::max) + tol;
    let part_r = pts.iter().map(|q| q.1).fold(0.0f64, f64::max);
    let rho_clear = StockEnv::rapid_radius(stock, part_r, rho_clear, nose);
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(clearance_z)));
    prof::ride(&mut b, f, &cl, rho_clear, feed, plunge_feed, prof::Exit::inside(stock, win.1));
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(clearance_z)),
        None,
    );
    Ok(b.finish())
}
