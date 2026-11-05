//! Turning-point blends (M11.1 A): a feed reversal rides ONE tangent
//! circular arc instead of a feed-stop-feed corner. Provenance: RS-274 /
//! ISO 6983 G2/G3 circular interpolation (public standards) and the
//! path-filleting practice of general machining knowledge; clean-room.
//!
//! Scope: the facing serpentine's row-end reversals. Contour corners and
//! between-pass links are later stages (ROADMAP M11.1) and do not read
//! this module yet.
//!
//! # The legality law (Prime Directive: never gouge, never silent)
//!
//! A reversal between rows `y0 < y1` (stride `s = y1 - y0`) at the row
//! end `xe` - the footprint's bbox edge plus a one-radius overrun - is,
//! sharp: feed to `(xe, y0)`, feed to `(xe, y1)`. Blended, the rows are
//! untouched and the step becomes a semicircle of radius `rho = s/2`
//! about `(xe, (y0 + y1)/2)`, bulging AWAY from the footprint: same
//! endpoints, same feed, one G2/G3 block. The arc is legal iff:
//!
//! - **footprint (live geometry)**: every arc point has its center at
//!   least `xe`, i.e. one tool radius outside the footprint - exactly
//!   where the sharp step stood. The arc's disk trail therefore never
//!   enters the footprint, the rows are byte-identical, and the
//!   footprint's coverage is unchanged to the point. This is also the
//!   verifier's boundary law (`verify` law (b): every law point - an
//!   arc's chord joints - holds `r` off the region boundary), so the
//!   blended path verifies exactly as clean as the sharp one. (The
//!   inward variant - rows stopped `rho` short, apex on the old step -
//!   sweeps a strict subset of the sharp path but parks row ends within
//!   `r - rho` of the footprint edge, which that law reads as a gouge
//!   and the post gate refuses; rejected for that reason.)
//! - **tool radius**: `rho <= r`. The arc reaches `rho` past the sharp
//!   overrun, outside the footprint; bounding it by one tool radius
//!   keeps the extra reach within the same scale the generator's own
//!   overrun already claims. A wider stride is refused - sharp, named.
//!   Whatever stands out there (fixtures, travel limits) is judged by
//!   the existing verify/machine laws on the arc's chords, which block
//!   the post on a hit; nothing is assumed.
//! - **resolution**: `rho >= MIN_BLEND_RADIUS`. A stride narrower than
//!   that (the band's last snapped row can be arbitrarily close) is a
//!   circle no control can interpolate against a micron-lattice center:
//!   refused, sharp, named.
//! - **tolerance / rest**: no deviation reaches the footprint, so an op
//!   tolerance does not bind the facing radius (contour-corner blends,
//!   later, are where it will). The rounding's rest - the overrun corners
//!   between rows the sharp step swept and the arc does not - lies
//!   outside the footprint; where stock stands there, the stock sim
//!   measures and reports it like any rest. Nothing is hidden here.

/// The smallest blend radius emitted, mm: below it the arc is finer than
/// what a control's arc-endpoint check tolerates against a center on the
/// 1 um lattice ([`crate::cl::lattice`]), so the corner stays sharp.
pub const MIN_BLEND_RADIUS: f64 = 0.01;

/// Why a reversal kept its sharp corner - the named refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sharp {
    /// Stride wider than the tool diameter: the arc would reach more
    /// than one tool radius past the overrun.
    Wide,
    /// Stride below twice [`MIN_BLEND_RADIUS`].
    Tiny,
}

/// The legality law for one reversal: the blend radius, or the reason
/// the corner stands. Non-finite input is never an arc.
pub(super) fn reversal_radius(stride: f64, r: f64) -> Result<f64, Sharp> {
    let rho = stride * 0.5;
    if !(rho.is_finite() && r.is_finite()) || rho > r {
        return Err(Sharp::Wide);
    }
    if rho < MIN_BLEND_RADIUS {
        return Err(Sharp::Tiny);
    }
    Ok(rho)
}

/// The reversal plan for sorted row centers `ys`: entry `i` is the turn
/// from row `i` to row `i + 1` - `true` blends it, `false` keeps it
/// sharp. With `enabled == false` every turn is sharp and nothing is
/// said: the legacy serpentine, byte-for-byte. The notes name every
/// sharp corner the law refused (counted per reason), so a facing that
/// asked for blends and got fewer says so on the status line.
pub(super) fn plan_reversals(ys: &[f64], r: f64, enabled: bool) -> (Vec<bool>, Vec<String>) {
    let turns = ys.len().saturating_sub(1);
    if !enabled {
        return (vec![false; turns], Vec::new());
    }
    let mut plan = Vec::with_capacity(turns);
    let (mut wide, mut tiny) = (0usize, 0usize);
    for w in ys.windows(2) {
        match reversal_radius(w[1] - w[0], r) {
            Ok(_) => plan.push(true),
            Err(Sharp::Wide) => {
                wide += 1;
                plan.push(false);
            }
            Err(Sharp::Tiny) => {
                tiny += 1;
                plan.push(false);
            }
        }
    }
    let mut notes = Vec::new();
    if wide > 0 {
        notes.push(format!(
            "turn blends: {wide} of {turns} row reversal(s) kept sharp - row spacing wider than the tool diameter, the arc would reach past the overrun by more than a tool radius"
        ));
    }
    if tiny > 0 {
        notes.push(format!(
            "turn blends: {tiny} of {turns} row reversal(s) kept sharp - row spacing below the {:.3} mm arc resolution",
            2.0 * MIN_BLEND_RADIUS
        ));
    }
    (plan, notes)
}

#[cfg(test)]
mod law {
    use super::*;

    #[test]
    fn radius_is_half_the_stride_up_to_the_tool_radius() {
        assert_eq!(reversal_radius(8.0, 5.0), Ok(4.0));
        assert_eq!(reversal_radius(10.0, 5.0), Ok(5.0));
        assert_eq!(reversal_radius(10.5, 5.0), Err(Sharp::Wide));
        assert_eq!(reversal_radius(0.001, 5.0), Err(Sharp::Tiny));
        assert_eq!(reversal_radius(f64::NAN, 5.0), Err(Sharp::Wide));
        assert_eq!(reversal_radius(4.0, f64::INFINITY), Err(Sharp::Wide));
    }

    #[test]
    fn disabled_plans_nothing_and_says_nothing() {
        let (plan, notes) = plan_reversals(&[5.0, 13.0, 21.0], 5.0, false);
        assert_eq!(plan, vec![false, false]);
        assert!(notes.is_empty());
        let (plan, notes) = plan_reversals(&[20.0], 5.0, true);
        assert!(plan.is_empty() && notes.is_empty(), "one row, no turn");
    }

    #[test]
    fn refusals_are_counted_and_named() {
        let (plan, notes) = plan_reversals(&[0.0, 12.0, 20.0, 20.001], 5.0, true);
        assert_eq!(plan, vec![false, true, false]);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("1 of 3") && notes[0].contains("wider"));
        assert!(notes[1].contains("1 of 3") && notes[1].contains("resolution"));
    }
}
