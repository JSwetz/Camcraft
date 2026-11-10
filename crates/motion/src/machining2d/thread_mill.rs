//! Helical thread milling: 3-axis interpolation to an internal thread
//! (M2.4 C).
//!
//! Provenance: helical (circular-interpolation) thread milling is
//! documented in machine-tool handbooks and published CAM literature:
//! a small thread mill rides a helix whose lead is the thread pitch,
//! stepping radially outward by a fraction of the thread depth each
//! turn until the finished flank is reached, then retracts back up
//! the path it came (the flutes cut both ways - the handbook's reason
//! a thread mill is ground for it). The radial distance from an
//! internal thread's minor cylinder to its major one is
//! `0.5413 x pitch` - the ISO 724 basic-triangle relation, a public
//! standard. Written clean-room from those two statements. The
//! spindle-sync word a RIGID TAP needs is a different machine's
//! problem and rides the M3.3 machine definitions, per the ROADMAP:
//! every move this kernel posts is plain G1/G2/G3 three-axis
//! interpolation.
//!
//! The law in one line: the drill hole the model shows is the thread's
//! MINOR cylinder (what CAD draws of a threaded bore). The cutter
//! center starts at `R_min - r` - rim tangent to the wall, cutting
//! nothing - and each turn steps out by `infeed` while the helix
//! descends at exactly `pitch` per turn; turns end when BOTH the floor
//! and the major circle `R_min + H - r` are reached, so total radial
//! travel is the standard's depth `H = 0.5413 x pitch` stated once,
//! not guessed. Until the floor arrives the ride is a true-pitch
//! helix; after it, the radius keeps stepping in place - a finishing
//! spiral at depth, which is what a thread that is still short of its
//! major circle needs.
//!
//! Prime Directive: the whole disc must stand inside the minor
//! cylinder to plunge at the axis at all (a mill wider than the drill
//! hole is refused by name); the retract is the recorded ride run
//! BACKWARDS at feed - never a rapid through a fresh flank; stepped
//! holes decline with the counterbore-fold word the boring ride
//! speaks; an unstated pitch is the M1.6 tap refusal's twin.

use crate::cl::{lattice, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::face::GenError;

/// The ISO 724 radial depth of an internal metric thread, in the one
/// constant this kernel borrows from a published standard.
pub const THREAD_DEPTH_PER_PITCH: f64 = 0.5413;

/// One bore to thread: wall = the minor cylinder, depths in world Z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadHole {
    pub xy: Point2,
    /// The model's wall: the thread's minor radius.
    pub radius: f64,
    pub top_z: f64,
    pub floor_z: f64,
    pub stepped: bool,
}

impl ThreadHole {
    fn all_finite(&self) -> bool {
        self.xy.x.is_finite()
            && self.xy.y.is_finite()
            && self.radius.is_finite()
            && self.top_z.is_finite()
            && self.floor_z.is_finite()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThreadParams {
    pub holes: Vec<ThreadHole>,
    /// Lead per turn, mm - the tool's pitch, handed down by the app's
    /// dispatch (the tap law reads the same tool field).
    pub pitch: f64,
    pub tool_diameter: f64,
    /// Radial step per turn; the ride lands on the major circle
    /// whatever this is (turns count up to cover BOTH jobs).
    pub infeed: f64,
    pub clearance_z: f64,
    pub retract_z: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// CCW when true (the standard read for a right-hand thread).
    pub climb: bool,
}

impl ThreadParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.pitch,
            self.tool_diameter,
            self.infeed,
            self.clearance_z,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        finite &= self.holes.iter().all(ThreadHole::all_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.holes.is_empty() {
            return Err(GenError::Invalid("thread milling needs at least one hole".into()));
        }
        if self.pitch <= 0.0 {
            return Err(GenError::Invalid(
                "a thread mill needs a positive pitch to time its helix against".into(),
            ));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        let h = THREAD_DEPTH_PER_PITCH * self.pitch;
        if self.infeed <= 0.0 || self.infeed > h {
            return Err(GenError::Invalid(format!(
                "infeed must sit between 0 and the {} mm thread depth this pitch stands at",
                h
            )));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.clearance_z < self.holes.iter().map(|x| x.top_z).fold(0.0_f64, f64::max) {
            return Err(GenError::Invalid("clearance plane below a hole entry".into()));
        }
        for x in &self.holes {
            if x.radius <= 0.0 {
                return Err(GenError::Invalid("hole radius must be positive".into()));
            }
            if x.floor_z >= x.top_z {
                return Err(GenError::Invalid(
                    "a threaded hole must have its floor below its entry".into(),
                ));
            }
            // #261 part 2: a refusal about this hole points at its mouth.
            let mouth = Point3::new(x.xy.x, x.xy.y, x.top_z);
            if x.stepped {
                return Err(GenError::invalid_at(
                    format!(
                        "hole at ({}, {}) carries a counterbore fold - thread milling cuts the \
                         wall it can see and would silently miss the ring it cannot",
                        x.xy.x, x.xy.y
                    ),
                    mouth,
                ));
            }
            // The air door: the whole disc must stand inside the minor
            // cylinder to plunge at the axis, and the tangent start
            // circle R_min - r must be a real circle the helix can
            // begin its orbit on.
            if self.tool_diameter * 0.5 >= x.radius {
                return Err(GenError::invalid_at(
                    format!(
                        "a {} mm thread mill has no air to enter the {} mm drill hole at ({}, {})",
                        self.tool_diameter,
                        x.radius * 2.0,
                        x.xy.x,
                        x.xy.y
                    ),
                    mouth,
                ));
            }
        }
        Ok(())
    }
}

/// One recorded ride move - the kernel collects these so the retract
/// can post them BACKWARDS, the thread mill's own law.
#[derive(Clone, Copy)]
enum Step {
    Line(Point3),
    Arc { to: Point3, center: Point2, ccw: bool },
}

fn run_steps(b: &mut PathBuilder, steps: &[Step], feed: f64, reverse: bool, ride_start: Point3) {
    if !reverse {
        for s in steps {
            match s {
                Step::Line(to) => b.push(MoveKind::Feed, *to, Some(feed)),
                Step::Arc { to, center, ccw } => {
                    b.push_arc(MoveKind::Feed, *to, Some(feed), *center, *ccw);
                }
            }
        }
        return;
    }
    // The mirror ride: step i ran start_i -> end_i forward; walking
    // backwards the tool lands on start_i, and an arc reversed swaps
    // its ends and its turn direction about the same center.
    let end_of = |i: usize| -> Point3 {
        match &steps[i] {
            Step::Line(to) | Step::Arc { to, .. } => *to,
        }
    };
    for i in (0..steps.len()).rev() {
        let start = if i == 0 { ride_start } else { end_of(i - 1) };
        match &steps[i] {
            Step::Line(_) => b.push(MoveKind::Feed, start, Some(feed)),
            Step::Arc { center, ccw, .. } => {
                b.push_arc(MoveKind::Feed, start, Some(feed), *center, !ccw);
            }
        }
    }
}

/// Helical thread milling over a hole list. Per hole: rapid down the
/// axis's air, feed out to the tangent circle, the ride (helix with
/// per-turn infeed, then an in-place finishing spiral), the reversed
/// ride as retract, and a clearance rapid at the axis.
pub fn thread_mill(p: &ThreadParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let r = p.tool_diameter * 0.5;
    let h = THREAD_DEPTH_PER_PITCH * p.pitch;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));

    for x in &p.holes {
        let rho_end = x.radius + h - r; // major circle, minus one tool
        let rho_0 = x.radius - r; // tangent: rim on the wall, no cut yet
        let span = x.top_z - x.floor_z;
        let n_z = (span / p.pitch).ceil().max(1.0) as u32;
        let n_r = ((h / p.infeed).ceil().max(1.0)) as u32;
        // Turn 0 is the tangent ride (infeed count starts at one);
        // the turns then run until BOTH jobs are done. Turns past the
        // floor-count ride flat at depth, stepping out - the finishing
        // spiral the last fraction of the depth needs.
        let turns = n_z.max(n_r).max(1) + 1;

        // Air moves first: down the axis (the door proved the whole
        // disc fits inside the hole), then the tangent feed-out.
        b.push(
            MoveKind::Rapid,
            Point3::new(lattice(x.xy.x), lattice(x.xy.y), p.clearance_z),
            None,
        );
        b.push(
            MoveKind::Rapid,
            Point3::new(lattice(x.xy.x), lattice(x.xy.y), x.top_z),
            None,
        );
        b.push(
            MoveKind::Feed,
            Point3::new(lattice(x.xy.x + rho_0), lattice(x.xy.y), x.top_z),
            Some(p.plunge_feed),
        );

        // The ride. Per turn t: a radial infeed step (t > 0, only
        // while the radius still travels), then two half-arcs whose
        // Z drops pitch/2 each until the floor, flat after it.
        let mut ride: Vec<Step> = Vec::with_capacity(turns as usize * 3);
        for t in 0..turns {
            // This turn's circle; the z the turn opens at is the
            // previous arc's end (top_z for turn 0).
            let rho = if t == 0 {
                rho_0
            } else {
                (rho_0 + p.infeed * t as f64).min(rho_end.max(rho_0))
            };
            if t > 0 {
                let z = ride
                    .last()
                    .map(|s| match s {
                        Step::Line(to) | Step::Arc { to, .. } => to.z,
                    })
                    .unwrap_or(x.top_z);
                ride.push(Step::Line(Point3::new(
                    lattice(x.xy.x + rho),
                    lattice(x.xy.y),
                    z,
                )));
            }
            for j in 1..=2 {
                // Half-arc j of turn t: forward half count k = 2t + j;
                // the lead is exact pitch per turn until the floor,
                // then the ride goes flat and only spirals out.
                let k = (2 * t + j) as f64;
                let z = (x.top_z - p.pitch * k / 2.0).max(x.floor_z);
                let px = if j % 2 == 1 { x.xy.x - rho } else { x.xy.x + rho };
                ride.push(Step::Arc {
                    to: Point3::new(lattice(px), lattice(x.xy.y), z),
                    center: x.xy,
                    ccw: p.climb,
                });
            }
        }
        // The last arc must land EXACTLY on the floor when the depth
        // job finishes first, and on the major circle when the radius
        // job does - both are true by construction: z clamps at
        // floor_z, rho clamps at rho_end.
        let ride_start = b.at();
        run_steps(&mut b, &ride, p.feed, false, ride_start);
        // Retract: the same ride backwards, at feed - a thread mill
        // leaves through its own groove (the handbook's reason its
        // flutes cut both ways).
        run_steps(&mut b, &ride, p.plunge_feed, true, ride_start);
        // Up the axis's air.
        b.push(
            MoveKind::Rapid,
            Point3::new(lattice(x.xy.x), lattice(x.xy.y), x.top_z),
            None,
        );
        b.push(
            MoveKind::Rapid,
            Point3::new(lattice(x.xy.x), lattice(x.xy.y), p.clearance_z),
            None,
        );
    }

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}
