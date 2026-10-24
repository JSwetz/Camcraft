//! Split from `types.rs` by M5.0.1; structure only.

use crate::machining2d::PocketStrategy;

use crate::operation::types::params::OpParams;
use crate::operation::types::sel::lead_sane;


impl OpParams {
    /// Shallow sanity for command construction, no world: finite numbers,
    /// positive increments, in-range slots. Ordering needs resolved
    /// heights and runs in [`crate::operation::depth_sane`] wherever a command holds the
    /// world; the generators own the deep rules (stepover vs diameter,
    /// lead sizes), which still fire inside `GenerateOperation`'s
    /// constructor.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        match self {
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && clearance_z.is_sane()
                    && cut_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && top_z.is_none_or(|t| t.is_sane())
                    && stepdown.is_none_or(|s| s.is_finite() && s > 0.0)
            }
            OpParams::Contour {
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                lead,
                tolerance,
                tab_width,
                tab_height,
                ..
            } => {
                nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*tolerance)
                    && nneg(*tab_width)
                    && nneg(*tab_height)
                    && lead_sane(lead)
            }
            OpParams::Pocket {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Pocket3D {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                max_engagement,
                ..
            } => {
                // Structural only, like the pocket's: the helix radius
                // is checked against the tool radius by the generator,
                // which is the only reader that owns a tool. A Plunge
                // entry never consults it, so any positive finite value
                // - the default 1.0 among them - is sane here. Same
                // law for the engagement angle: it must be finite
                // always, and inside its own (0, 90] only when the
                // Adaptive strategy actually reads it - the generator
                // repeats the bound as the refusal of record.
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (max_engagement.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive)
                            || (*max_engagement > 0.0 && *max_engagement <= 90.0)))
            }
            OpParams::Finish3D {
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                min_step,
                ramp_angle,
                ..
            } => {
                // No stepdown to check: the scallop law derives it from
                // tolerance and the tool radius, and the generator's
                // level cap refuses a tolerance that would need more
                // rings than the machine of record may hold.
                pos(*tolerance)
                    && stepover.is_none_or(pos)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && min_step.is_none_or(pos)
                    // (0, 90): a flat ramp never descends - that is
                    // a traverse with a name it doesn't earn; a
                    // vertical one is a plunge, and the run's first
                    // contact already owns those. Both refusals
                    // happen at the gate, never at the emitter.
                    && (0.0..90.0).contains(ramp_angle)
            }
            OpParams::Slot {
                a,
                b,
                width,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                stepover,
                feed,
                plunge_feed,
                ..
            } => {
                a.x.is_finite()
                    && a.y.is_finite()
                    && b.x.is_finite()
                    && b.y.is_finite()
                    && pos(*width)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*stepover)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                peck,
                dwell,
                feed,
                plunge_feed,
                holes,
                ..
            } => {
                // An empty hole list is sane here: a new Drill op exists
                // before its holes are detected, and creation must be
                // allowed to get the panel on screen. Generation is the
                // gate that refuses it, by name.
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_none_or(|b| b.is_sane())
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && peck.is_finite()
                    && *peck >= 0.0
                    && dwell.is_finite()
                    && *dwell >= 0.0
                    && holes.iter().all(|h| h.is_usable())
            }
        }
    }

}
