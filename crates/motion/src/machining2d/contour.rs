//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::{Entry3D, dive_room, helix_down};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::arcs::{fit_arcs, lead_in, lead_out, push_segs, walk_tangent};
use super::levels::{Cert, Motion, cut_levels, dive_link, substituted_dive};
use super::ring::Ring;
use super::face::GenError;
use super::slots::chord_clears;
/// Entry/exit motion appended to each contour subpath. Every lead that
/// is not [`Lead::None`] keeps the tool from dwelling (and gouging) at a
/// plunge point directly on the finished wall: it starts the cut already
/// moving, in air, and joins the wall tangent-continuous. Specced from
/// published CAM practice (lead-in families: linear, quarter-arc and the
/// tangential half-arc that enters parallel to the path), no vendor
/// material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lead {
    /// Plunge straight onto the path.
    None,
    /// Start `length` behind the path along its tangent, run in, and
    /// overrun the same way on exit.
    Linear { length: f64 },
    /// Quarter arc of `radius`, tangent to the path at entry and exit,
    /// bulging to the air side. The tool starts the arc moving normal to
    /// the path. Posted as one arc move; chord error at `tolerance` only
    /// appears where a path gets flattened downstream (verification,
    /// display, the golden dump).
    Arc { radius: f64 },
    /// Half arc of `radius` (a semicircle), tangent to the path at the
    /// join: the tool plunges `2 * radius` off the wall, in air, moving
    /// *parallel* to the path, curves over and merges into the cut at the
    /// join still moving along it - G1 continuous end to end, which is
    /// what earns the name. Lead-out mirrors it: the tool overruns the
    /// end tangentially and leaves the wall moving parallel to the path.
    /// One posted arc move like [`Lead::Arc`].
    Tangential { radius: f64 },
}

/// Which side of a contour boundary the cutter runs on - a sign law on
/// the offset and one traversal flip, nothing else. Auto is resolved by
/// the selection layer (only there does anything know what a picked ring
/// bounds); inside the generator Auto means Outside, so every
/// explicit-region caller - goldens, direct tests - keeps cutting today's
/// law byte-for-byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Side {
    /// Generator: the Outside law. The app's fill-ratio check on picked
    /// rings may resolve Auto to Inside, and says so in a note.
    #[default]
    Auto,
    /// Cutter inside the boundary: the region erodes by `r + leave` and
    /// each loop's traversal reverses, so material stays on the left and
    /// leads bulge into the bounded air - the wall of a bore. That
    /// bounded air is exactly what certifies the entry family's descents
    /// across the level planes (M8.2 A).
    Inside,
    /// Today's law: the region grows by `r + leave`, the tool grazes the
    /// boundary from the material side - a boss or an outside wall.
    Outside,
}

/// 2D contouring: trim a material region's boundary at successive depths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourParams {
    pub tool_diameter: f64,
    /// Thickness of material deliberately left on the finished faces for
    /// a later finishing pass.
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// Plane tab hops travel in, between the cut and `clearance_z`.
    pub retract_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb travels each offset loop with the region's canonical
    /// orientation (material on the left); conventional reverses it.
    /// Under [`Side::Inside`] the canonical orientation is the loop's
    /// reverse, which the generator XORs in - this field still means the
    /// process, not a winding.
    pub climb: bool,
    /// Which side of the boundary to cut; see [`Side`]. Only the offset
    /// sign and the traversal follow from it - hops, leads and tabs are
    /// side-agnostic.
    pub side: Side,
    pub lead: Lead,
    /// Chord tolerance for arcs: bounds how far a fitted arc may sit
    /// from the tessellation it replaces (both directions), and is the
    /// chord error arcs flatten to wherever a law needs segments.
    pub tolerance: f64,
    /// Tabs: `tab_count` bridges of `tab_width`, holding the part until
    /// the cut drops below `bottom_z + tab_height`.
    pub tab_count: u32,
    pub tab_width: f64,
    pub tab_height: f64,
    /// How a level crossing engages (M8.2 A): the family the pocket
    /// ladders already read. It applies where the descent can be
    /// certified - `Side::Inside`, whose bounded air the eroded region
    /// states exactly. On the wall side it is refused by name in the
    /// path's notes, and the graze this generator has always posted
    /// stands (the grown region would certify a chord across the
    /// part's own interior - a lie, and M8.1's distance law is the
    /// reason it is not told).
    pub entry: Entry3D,
    /// Helix centerline circle radius (mm); read only when `entry` is
    /// Helix, and certified per level by `dive_room`'s margin, not by
    /// the roughing overlap law: a contour orbit descends air, so a
    /// radius above the tool's is legal wherever the bore holds it.
    pub helix_radius: f64,
}

impl ContourParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.retract_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.tolerance,
            self.tab_width,
            self.tab_height,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z < self.top_z {
            // Hops travel at the retract plane; one at or below the
            // material top would drag across solid - a gouge by
            // definition, refused by name. Above clearance is legal in
            // old files and gets clamped, not refused (see `contour`).
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        if self.tolerance <= 0.0 {
            return Err(GenError::Invalid("tolerance must be positive".into()));
        }
        match self.lead {
            Lead::None => {}
            Lead::Linear { length } if length >= 0.0 && length.is_finite() => {}
            Lead::Arc { radius } if radius > 0.0 => {}
            Lead::Tangential { radius } if radius > 0.0 => {}
            _ => return Err(GenError::Invalid("bad lead-in parameters".into())),
        }
        if self.tab_count > 0 && (self.tab_width <= 0.0 || self.tab_height < 0.0) {
            return Err(GenError::Invalid("tabs need positive width".into()));
        }
        if self.entry == Entry3D::Helix && self.helix_radius <= 0.0 {
            // Only positivity: the roughing ladders cap the radius at
            // the tool's so successive turns overlap, but a contour
            // orbit descends an already-open bore and cuts nothing on
            // the way down - fit is certified per level by `dive_room`
            // against the wall, which is where a too-wide disc is
            // refused (loudly, as a substitution).
            return Err(GenError::Invalid("helix radius must be positive".into()));
        }
        Ok(())
    }

    /// Cut levels from `top_z` down to `bottom_z`, always ending exactly
    /// at the bottom (same snap rule as facing rows).
    pub(super) fn levels(&self) -> Vec<f64> {
        cut_levels(self.top_z, self.bottom_z, self.stepdown)
    }
}

/// Offset-contour `region` (a material cross-section): every level walks
/// the cutter-center loops of the region grown by `r + leave`, so the
/// tool edge grazes the boundary with `leave = 0` and stops a finishing
/// allowance short of it otherwise. The plan phrases this as
/// `- (r - leave)`; that is the same curve written from the void side -
/// here the region is material, so both outer walls and island/hole
/// walls come out of one positive offset. [`Side::Inside`] negates that
/// one term - the region erodes and each loop traverses reversed, so the
/// cutter rides the boundary from the void side with material still on
/// the left (Auto is this Outside law inside the generator). Loop order
/// is deterministic (largest absolute area first, canonically oriented,
/// and traversed per the climb/side XOR).
///
/// Between subpaths (tab hops) and between loops of one level the tool
/// retracts to the hop plane - `retract_z`, clamped to `clearance_z` so
/// a legacy retract parked above clearance cannot make the hop climb
/// higher than the clearance move it replaced, with the clamp echoed in
/// the panel - rapids across in air, and plunges again. A level change
/// still lifts to full `clearance_z`: the new level engages material the
/// previous one never touched, and crosses into it by the `entry`
/// family (M8.2 A) - every arm certified against the one center this
/// ladder shares, on the bore side where that center is bounded air.
/// Rapids inside the footprint below the
/// safe plane are the verifier's business (step 7), flagged, never
/// hidden.
pub fn contour(region: &Region, p: &ContourParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty footprint".into()));
    }
    let r = p.tool_diameter * 0.5;
    let inside = p.side == Side::Inside;
    let d = r + p.stock_to_leave;
    let center = region.offset(if inside { -d } else { d })?;
    if center.is_empty() {
        return Err(GenError::Empty(if inside {
            "the bore is narrower than the tool - switch Side to Outside or pick a smaller tool"
                .into()
        } else {
            "offset boundary collapsed".into()
        }));
    }
    let zs = p.levels();
    let mut loops: Vec<&Loop> = center.loops().iter().collect();
    loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));

    // The rings every level plans to walk, as closed polylines - the
    // link chords of M8.1's plan law get proven against these. Tabbed
    // levels walk a subset, which errs conservative: an unproven chord
    // hops rather than dragging across a ring line.
    let plan: Vec<Vec<Point2>> = loops
        .iter()
        .map(|l| {
            let mut v: Vec<Point2> = if p.climb != inside {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            if v.len() > 1 {
                let (first, last) = (v[0], v[v.len() - 1]);
                if first.x != last.x || first.y != last.y {
                    v.push(first);
                }
            }
            v
        })
        .collect();
    // The shared motion envelope: dive_link and substituted_dive read
    // their lifts and feeds through it, so the contour's arms speak
    // the pocket ladders' word verbatim (hop = retract clamped at
    // clearance, exactly what `hop` below computes).
    let m = Motion::new(p.clearance_z, Some(p.retract_z), p.feed, p.plunge_feed)
        .with_entry(p.entry, p.helix_radius);
    let mut refused_said = false;
    let mut tabbed_said = false;
    let cert = Cert::Region(&center);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    let hop = p.retract_z.min(p.clearance_z);
    let mut first_entry = true;
    for (zi, &z) in zs.iter().enumerate() {
        let tabbed = p.tab_count > 0 && z < p.bottom_z + p.tab_height;
        // A tab-hop template traverses at `hop` between windows, and a
        // standing tab is material AT the window down to the level -
        // below `bottom_z + tab_height` the hop plane itself runs
        // through the tab. On tabbed levels the traverse therefore
        // rides the tab top when that is higher. Sane files are
        // unaffected twice over: validate keeps `retract >= top_z`,
        // and a tab no taller than the cut ends at or below the
        // surface, so the raise is a no-op unless the tab truly stands
        // above the plane - which is exactly when it must.
        let hop_z = if tabbed {
            hop.max(p.bottom_z + p.tab_height)
        } else {
            hop
        };
        for (li, l) in loops.iter().enumerate() {
            // Canonical orientation is material-on-the-left travel;
            // conventional climbing reverses it. Air is the other side.
            // An Inside region was eroded, not grown, so its stored
            // winding puts the (outside) material on the right - flip
            // with the sign and the rule, and every air_side-derived
            // lead, holds again.
            let pts: Vec<Point2> = if p.climb != inside {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            let ring = Ring::new(pts);
            let keeps = if tabbed {
                ring.tab_keeps(p.tab_count as usize, p.tab_width)
            } else {
                vec![(0.0, ring.len)]
            };
            for (k, &(a, e)) in keeps.iter().enumerate() {
                let poly = ring.arc_between(a, e);
                if poly.len() < 2 {
                    continue;
                }
                let walk = fit_arcs(&poly, p.tolerance);
                // The lead meets the wall on the wall's own tangent at
                // the joint - the fitted walk's true circle tangent, not
                // a chord across it, or the join kinks at the very seam
                // the fitter just smoothed.
                let t0 = walk_tangent(&walk, &poly, true);
                let (entry, lead) = lead_in(poly[0], t0, air_side(t0, p.climb), p);
                let e2 = Point2::new(entry.x, entry.y);
                if k == 0 && li > 0 && !first_entry {
                    // Loop to loop within one level: the tab-hop
                    // template - up to the hop plane, across, plunge.
                    let cur = b.at();
                    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
                    b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, hop_z), None);
                    b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, z), Some(p.plunge_feed));
                } else if k == 0 {
                    // The level's first engagement: a crossing (the
                    // tool last cut at `prev_z`) or the path's first
                    // entry, where nothing has cut and prev_z falls
                    // back to clearance - the level-0 law M8.1 wrote
                    // into both roughing ladders, voiced for the wall
                    // here (the `li > 0` shape this arm replaces read
                    // the family from crossings only). New material
                    // engages from the one plane safe over all of it.
                    let cur = b.at();
                    if !first_entry {
                        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
                    }
                    let prev_z = if zi == 0 { p.clearance_z } else { zs[zi - 1] };
                    if inside && tabbed {
                        // M8.2 C: the arm stands down where a tab is
                        // material. dive_room and chord_clears certify
                        // against the eroded region - the FULL-depth
                        // air - but a standing tab is material INSIDE
                        // that region at this level: a helix orbit or
                        // a ramp chord certified here could run
                        // straight through the tab it was told to
                        // leave. Tabbed levels therefore take the
                        // shape this generator posted before the
                        // certification existed: clearance traverse,
                        // axial dive at the lead-in, the same window
                        // the walking cut already reaches by wall-
                        // parallel feed. Sane and provably no worse
                        // than the pre-A byte; the tab gets its
                        // cleanup at the level above, where it is air.
                        if p.entry != Entry3D::Plunge && !tabbed_said {
                            b.note(
                                "entry: no arm stands where a tab is material no \
                                 certification can see - axial dive at the lead-in on \
                                 tabbed levels",
                            );
                            tabbed_said = true;
                        }
                        let cur = b.at();
                        b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, cur.z), None);
                        b.push(
                            MoveKind::Plunge,
                            Point3::new(e2.x, e2.y, z),
                            Some(p.plunge_feed),
                        );
                    } else if inside {
                        // The bore side: the eroded region states the
                        // bounded air exactly, so every arm certifies
                        // against it with the pocket ladders' own
                        // machinery - the dive stands in the floor's
                        // room, never dwells on the finished wall.
                        match p.entry {
                            Entry3D::Plunge => match dive_room(&center, 0.0) {
                                Some(d) => {
                                    let cur = b.at();
                                    if cur.x != d.x || cur.y != d.y {
                                        b.push(
                                            MoveKind::Rapid,
                                            Point3::new(d.x, d.y, p.clearance_z),
                                            None,
                                        );
                                    }
                                    b.push(
                                        MoveKind::Plunge,
                                        Point3::new(d.x, d.y, z),
                                        Some(p.plunge_feed),
                                    );
                                    dive_link(&mut b, &cert, &plan, d, e2, z, &m);
                                }
                                None => substituted_dive(
                                    &mut b,
                                    &cert,
                                    &plan,
                                    e2,
                                    z,
                                    &m,
                                    "the disc fits nowhere here",
                                ),
                            },
                            Entry3D::Ramp => {
                                // One chord test covers the whole
                                // interpolated descent: the region is
                                // one at every plane, and the
                                // destination's room bounds every
                                // plane above it (M8.1's superset law,
                                // here at its simplest - a planar
                                // contour's single center).
                                let cur = b.at();
                                let leg = Point2::new(cur.x, cur.y);
                                if chord_clears(&center, leg, e2) {
                                    b.push(
                                        MoveKind::Rapid,
                                        Point3::new(leg.x, leg.y, prev_z),
                                        None,
                                    );
                                    b.push(MoveKind::Feed, Point3::new(e2.x, e2.y, z), Some(p.feed));
                                } else {
                                    substituted_dive(
                                        &mut b,
                                        &cert,
                                        &plan,
                                        e2,
                                        z,
                                        &m,
                                        "the ramp's chord is blocked",
                                    );
                                }
                            }
                            Entry3D::Helix => {
                                let cur = b.at();
                                let leg = Point2::new(cur.x, cur.y);
                                // The bore orbits about `d + (rh, 0)`
                                // (the anchor is the circle's near
                                // side), so the widened disc spans 2
                                // radii from the anchor - see
                                // `entry::dive_room` (M2.5 F law fix).
                                match dive_room(&center, 2.0 * p.helix_radius + 1e-3) {
                                    Some(d) => {
                                        if leg.x != d.x || leg.y != d.y {
                                            b.push(
                                                MoveKind::Rapid,
                                                Point3::new(d.x, d.y, p.clearance_z),
                                                None,
                                            );
                                        }
                                        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, prev_z), None);
                                        helix_down(&mut b, d, prev_z, z, p.helix_radius, p.feed);
                                        dive_link(&mut b, &cert, &plan, d, e2, z, &m);
                                    }
                                    None => substituted_dive(
                                        &mut b,
                                        &cert,
                                        &plan,
                                        e2,
                                        z,
                                        &m,
                                        "the widened disc fits nowhere at this level",
                                    ),
                                }
                            }
                        }
                    } else {
                        // The wall side (explicit Outside, and every
                        // direct Auto): the graze this generator has
                        // always posted, and the only one that can be
                        // certified - the grown region holds the
                        // part's own interior, so a dive_room or a
                        // chord on it would certify descents THROUGH
                        // the body (M8.1's rule: not a law to build a
                        // dive on). The other two arms are refused by
                        // name - once, because the reason is
                        // structural, not level-dependent.
                        if p.entry != Entry3D::Plunge && !refused_said {
                            let arm = match p.entry {
                                Entry3D::Ramp => "ramp",
                                Entry3D::Helix => "helix",
                                Entry3D::Plunge => "plunge",
                            };
                            b.note(format!(
                                "entry: the {arm} needs bounded air only a bore (Side::Inside) \
                                 has - grazing dive at the lead-in, unchanged"
                            ));
                            refused_said = true;
                        }
                        let cur = b.at();
                        b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, cur.z), None);
                        b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, z), Some(p.plunge_feed));
                    }
                } else {
                    let cur = b.at();
                    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
                    b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, hop_z), None);
                    b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, z), Some(p.plunge_feed));
                }
                first_entry = false;
                push_segs(&mut b, &lead, z, p.feed);
                push_segs(&mut b, &walk, z, p.feed);
                let n = poly.len();
                let t1 = walk_tangent(&walk, &poly, false);
                push_segs(
                    &mut b,
                    &lead_out(poly[n - 1], t1, air_side(t1, p.climb), p),
                    z,
                    p.feed,
                );
            }
        }
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Contour along an open chain: a run whose ends stop mid-edge instead of
/// a loop around material. The cutter center rides the chain's own
/// points: plunge at one end, feed through every vertex at each level,
/// retract at the other. Climb travels the chain in stored order,
/// conventional reversed, and the same air-side rule sends the lead-in
/// and lead-out off the ends - an open run has no material to its side to
/// keep off, so the ends themselves are where the lead lives. The tab
/// fields do not apply (there is nothing left held between passes), and
/// the level ladder, clearance plane and lead geometry are `contour`'s
/// reused.
///
/// Half the tool overhangs the run on either side; a verifier told about
/// a surrounding region reads that as the wall it is, and says so.
pub fn contour_along_chain(
    pts: &[Point2],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    if pts.len() < 2 {
        return Err(GenError::Empty(
            "a centerline run needs at least two points".into(),
        ));
    }
    let poly: Vec<Point2> = if p.climb {
        pts.to_vec()
    } else {
        let mut v = pts.to_vec();
        v.reverse();
        v
    };
    let zs = p.levels();
    let walk = fit_arcs(&poly, p.tolerance);
    let t0 = walk_tangent(&walk, &poly, true);
    let (entry, lead) = lead_in(poly[0], t0, air_side(t0, p.climb), p);
    let n = poly.len();
    let t1 = walk_tangent(&walk, &poly, false);
    let out = lead_out(poly[n - 1], t1, air_side(t1, p.climb), p);

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    // An open run bounds no area, so no dive on it can be certified
    // against air that exists: the chosen entry family is refused by
    // name once, and every level dives at the lead-in's start as the
    // chain has always done (M8.2 A - the same word the wall side of
    // `contour` speaks for its unbounded outside).
    if p.entry != Entry3D::Plunge {
        let arm = match p.entry {
            Entry3D::Ramp => "ramp",
            Entry3D::Helix => "helix",
            Entry3D::Plunge => "plunge",
        };
        b.note(format!(
            "entry: an open centerline run bounds no air to certify the {arm} against - dive \
             at the run start, unchanged"
        ));
    }
    for (k, &z) in zs.iter().enumerate() {
        if k > 0 {
            let cur = b.at();
            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
        }
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, cur.z), None);
        b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(p.plunge_feed));
        push_segs(&mut b, &lead, z, p.feed);
        push_segs(&mut b, &walk, z, p.feed);
        push_segs(&mut b, &out, z, p.feed);
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Which side of travel is air: canonical (climb) travel keeps material on
/// the left, so air is right; reversed travel flips both.
pub(super) fn air_side(t: Point2, climb: bool) -> Point2 {
    if climb {
        Point2::new(t.y, -t.x)
    } else {
        Point2::new(-t.y, t.x)
    }
}

pub(super) fn unit(dx: f64, dy: f64) -> Point2 {
    let m = (dx * dx + dy * dy).sqrt();
    if m == 0.0 || !m.is_finite() {
        Point2::new(1.0, 0.0)
    } else {
        Point2::new(dx / m, dy / m)
    }
}
