//! Cutting tools: the parameters that define a cutter and the starter
//! catalog the UI lists.
//!
//! Provenance: end mill / ball nose / bull nose geometry and the
//! diameter-flute-shank-holder stack are standard machine-tool handbook
//! descriptions; the drill point angle and the tap pitch are the handbook
//! cone and the published ISO 261 metric coarse pitches. The catalog's
//! sizes are ordinary metric stock; the speeds and feeds are our own
//! numbers, set for aluminum with carbide tooling at typical chip loads -
//! general published practice, no vendor library copied.
//!
//! The threading insert's crest width (0.866 x pitch) and 60-degree
//! included V are the published metric thread profile's own geometry
//! read as blade practice - machine-tool handbook threading chapters.
//!
//! Pure module: no flecs, no I/O. Commands persist a `Tool` on a `ToolDef`
//! component; the operation layer reads radii and lengths out of it for
//! generation and the shank-reach law in `verify`.

pub use crate::cutting::{ToolMaterial, WorkMaterial};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    EndMill,
    BallNose,
    BullNose,
    /// Twist drill: a conical point (`tip_angle` included degrees) behind
    /// a straight-diameter flute. Only `Drill` reads `tip_angle`.
    Drill,
    /// Rigid tap: a thread-forming cylinder; `pitch` (mm/rev) is what the
    /// tapping cycle feeds at - rpm x pitch. Only `Tap` reads `pitch`.
    Tap,
    /// Boring bar: single-point, hole-sized by the slide, profiled as a
    /// plain cylinder. On a drilling cycle the diameter is the whole
    /// tool (the G86 family sizes by the slide); from M5.2 F the
    /// turning kind also reads `nose_radius` - the ground nose the bar
    /// rides a hole's wall on, exactly the number the OD kinds carry
    /// in the same field.
    Bore,
    /// Turning insert (M5.2 stage A): the lathe's single-point cutter -
    /// the WORK spins, the insert traverses. `nose_radius` (inert for
    /// every other kind) is the circle the cut rides: the facing verdict
    /// sweeps it about the axis, and on a revolved work that revolved
    /// disk IS the cut volume (a sphere's and a disk's revolve share
    /// their meridian section). Only `Turning` reads it.
    Turning,
    /// Grooving/parting blade (M5.2 stage C): a full-width cutter whose
    /// meridian footprint is a rectangle - `width` (inert for every
    /// other kind) is the band it occupies along the axis, and its cut
    /// is every radius OUTSIDE the tip across that band. The blade
    /// verdict reads the band, not a swept ball: a grooved part is
    /// undercut by design and no sphere can judge one honestly (see
    /// `crate::turning::revolve`). Only `Groove` reads `width`.
    Groove,
    /// Threading insert (M5.2 stage D): the V-form blade cut to one
    /// thread's profile. `width` is the V's opening at its crest
    /// line and `tip_angle` the flanks' INCLUDED angle (60 for the
    /// metric forms) - the same two fields the drill's cone and the
    /// tap's pitch ride, now read as the form the helical pass
    /// revolves. Only `Threading` reads either.
    Thread,
    /// Form gauge (M5.2 stage E2): a blade whose edge is a curve -
    /// the meridian arc it stamps. `width` is the ground band along
    /// the axis and `nose_radius` the crown's arc RADIUS in the
    /// work's meridian plane (the sagitta the revolve verdict
    /// bends the floor by - not an envelope ball: a gauge is
    /// judged on the meridian, and `geom` never carries this one).
    /// Only `Form` reads either.
    Form,
    /// Chamfer mill (M2.4 B): a cone widening upward from its virtual
    /// sharp tip, straight land above. `tip_angle` is the cone's
    /// INCLUDED angle across the axis - the same field the drill's
    /// point and the thread's V ride - and the 2D chamfer's congruence
    /// door reads it: a 45-degree chamfer takes a 90-degree cone, the
    /// shop's catalog law. Only `Taper` reads it among the mill kinds.
    Taper,
    /// Touch probe (#108): a spindle-mounted stylus that measures, never
    /// cuts. `diameter` is the stylus BALL's, `flute_len` the stylus
    /// length below the probe body (`shank_*`); `corner_r` is the ball
    /// radius by law (a probe tip is a sphere). It turns no spindle and
    /// removes no stock: only the Probe operation mounts it.
    Probe,
}

impl core::fmt::Display for ToolKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            ToolKind::EndMill => "flat end mill",
            ToolKind::BallNose => "ball-nose",
            ToolKind::BullNose => "bull-nose",
            ToolKind::Drill => "drill",
            ToolKind::Tap => "tap",
            ToolKind::Bore => "boring bar",
            ToolKind::Turning => "turning insert",
            ToolKind::Groove => "grooving blade",
            ToolKind::Thread => "threading insert",
            ToolKind::Form => "form gauge",
            ToolKind::Taper => "chamfer mill",
            ToolKind::Probe => "touch probe",
        };
        f.write_str(s)
    }
}

/// The cutting tool and what rides above it. Radii are coaxial with the
/// spindle; lengths stack upward from the cut edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolGeom {
    pub radius: f64,
    /// The envelope law's ball (M5.2 A): the turning insert's nose
    /// circle, or 0.0 when the tool has no separate cut circle and the
    /// radius above IS it (every mill kind). Inert for the rest of the
    /// laws - the shank bands read the stack below unchanged.
    pub nose_radius: f64,
    /// Usable flute length below the shank.
    pub flute_len: f64,
    pub shank_radius: f64,
    pub shank_len: f64,
    pub holder_radius: f64,
    pub holder_len: f64,
}

/// One rotating cutter, coaxial with the spindle: a cutting body of
/// `diameter`/`flute_len` (corner rounded by `corner_r`), a plain shank,
/// and whatever holder the setup assumes above it. Radii live as
/// diameters here; `ToolGeom` takes the halved coaxial stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub kind: ToolKind,
    pub diameter: f64,
    /// Corner radius: 0 for a square end mill, d/2 for a ball, between
    /// for a bull.
    pub corner_r: f64,
    /// Point angle in degrees, included across the cone: 118 is the
    /// general-purpose twist-drill standard. On a threading insert it
    /// is the flanks' INCLUDED angle of the V (60 for the metric
    /// forms); inert for the rest - every other kind reads it as 0.0
    /// and draws flat.
    pub tip_angle: f64,
    /// Thread pitch in mm per turn (ISO coarse for the catalog taps).
    /// Inert unless kind is Tap, where the cycle feeds at rpm x pitch.
    pub pitch: f64,
    /// Insert nose radius in mm (the ISO corner circle ground into the
    /// tip): 0.8 is common roughing stock. Inert unless kind is
    /// Turning, where it is the circle the facing verdict sweeps, or
    /// Form, where it is the crown's arc radius the revolve verdict
    /// bends its floor by, or Bore, where it is the ground nose the
    /// bar rides a hole's wall on (M5.2 F: the envelope verdict
    /// rolls THIS circle about the centers, as law (f) reads it for
    /// every turning steel) - every other kind reads it as 0.0 and
    /// rolls its own radius.
    pub nose_radius: f64,
    /// Blade width in mm along the axis (the grooving/parting insert's
    /// ground side-to-side size): 3.0 is common cutoff stock. On a
    /// threading insert it is the V's opening width at its crest line
    /// (the handbook's 0.866 x pitch for the metric forms); inert for
    /// the rest - every other kind reads it as 0.0 and sweeps its own
    /// circle.
    pub width: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    /// Free-text material the numbers were picked for ("aluminum").
    pub material_group: String,
    /// Suggested starting spindle speed (rpm) and feed (mm/min) for that
    /// material - the machine operator's starting point, not a limit.
    pub rpm: f64,
    pub feed: f64,
    /// What the cutting edge is made of (M13.2) - the column of the
    /// cutting-data table the feeds-and-speeds suggestion reads.
    /// `None` = not stated, and nothing is suggested for it.
    pub tool_material: Option<ToolMaterial>,
    /// Cutting edges (flutes, z) (M13.2): the feed-per-tooth multiplier
    /// and the chip-thickness reading of the power estimate. `None` =
    /// not stated; a stated count is 1..=32.
    pub flutes: Option<u32>,
    /// Can the tool plunge axially into solid material (M11.3, the
    /// user's safety call)? Public practice: twist drills, ball and bull
    /// noses and the ordinary 2-4 flute end mills cut to center - the
    /// whole starter catalog is `true`; a shell / face mill or an insert
    /// cutter with a center hole is not, and its entries may then only
    /// helix or ramp into solid (or the op is refused by name).
    pub center_cutting: bool,
    /// Lathe tool orientation (#630, [`crate::orientation`]): the
    /// LinuxCNC 1-9 code that says where the control point the
    /// operator touches off stands on the nose circle. `None` = not
    /// stated, read as the kind's default
    /// ([`crate::orientation::default_for`]). On a grooving blade it
    /// names the touched-off corner of the tip edge (#641,
    /// [`crate::orientation::blade_corner`]); inert on every other kind.
    pub orientation: Option<u8>,
}

impl Tool {
    /// The orientation the posts read: the stated code, else the
    /// kind's default; None for a kind that has none.
    #[must_use]
    pub fn lathe_orientation(&self) -> Option<u8> {
        self.orientation.or_else(|| crate::orientation::default_for(self.kind))
    }

    /// Finite, positive where it must be, and geometrically consistent:
    /// a corner radius cannot exceed half the diameter, lengths cannot be
    /// negative. The gate every tool command runs before writing.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let fin = [
            self.diameter,
            self.corner_r,
            self.tip_angle,
            self.pitch,
            self.nose_radius,
            self.width,
            self.flute_len,
            self.shank_d,
            self.shank_len,
            self.holder_d,
            self.holder_len,
            self.rpm,
            self.feed,
        ]
        .into_iter()
        .all(|v| v.is_finite());
        fin && self.diameter > 0.0
            && self.diameter <= 500.0
            && self.corner_r >= 0.0
            && self.corner_r <= self.diameter * 0.5
            && self.tip_angle < 180.0
            && self.pitch >= 0.0
            && self.nose_radius >= 0.0
            && self.width >= 0.0
            && self.flute_len > 0.0
            && self.shank_d >= 0.0
            && self.shank_len >= 0.0
            && self.holder_d >= 0.0
            && self.holder_len >= 0.0
            && self.rpm > 0.0
            && self.feed > 0.0
            && self.flutes.is_none_or(|z| (1..=32).contains(&z))
            && self.orientation.is_none_or(crate::orientation::is_valid)
            && match self.kind {
                // A point that cannot self-center is not a drill, and a
                // tap with no thread pitch has no feed the cycle could
                // derive. Other kinds carry both fields inert.
                ToolKind::Drill => self.tip_angle > 60.0,
                ToolKind::Tap => self.pitch > 0.0,
                // A turning insert with no nose is a mathematical
                // point - it would cut a knife line and verify with a
                // zero-ball refusal - and a nose wider than half the
                // shank is not the ball any insert body could hold.
                // The bounds live HERE, kind-scoped, the drill's and
                // tap's own law: an inert field may carry any sane
                // number through a retype without refusing for
                // geometry the kind never reads.
                ToolKind::Turning => {
                    self.nose_radius > 0.0 && self.nose_radius <= self.diameter * 0.5
                }
                // A blade with no width is a mathematical plane - it
                // would part a knife line and judge a zero-band scan -
                // and a blade as wide as its own body is not the edge
                // any toolholder could ground. The same kind-scoped law
                // as the insert's nose (the note above).
                ToolKind::Groove => self.width > 0.0 && self.width < self.diameter,
                // A thread form that is not a V (no opening, or a
                // full flat) is not steel any grinder cuts, and a V
                // as wide as the body is the insert no holder clamps
                // - the blade's width law with the drill's angle
                // bound, both fields read at once (kind-scoped, as
                // every arm above).
                ToolKind::Thread => {
                    self.width > 0.0
                        && self.width < self.diameter
                        && self.tip_angle > 0.0
                        && self.tip_angle < 180.0
                }
                // A gauge with no crown is a flat blade (mount a
                // Groove), and a band ground wider than the arc's
                // own reach has corners that are no steel at all -
                // the kernel's door words it for a run; the tool
                // says it never got made (kind-scoped, as above).
                ToolKind::Form => {
                    self.width > 0.0
                        && self.width < self.diameter
                        && self.nose_radius > 0.0
                        && self.width <= 2.0 * self.nose_radius
                }
                // A cone that opens no angle at all is a cylinder
                // wearing the name - the chamfer's generatrix would
                // meet the wall nowhere, and the congruence door
                // would divide on nothing (kind-scoped, as above;
                // the global bound keeps it under a flat).
                ToolKind::Taper => self.tip_angle > 0.0,
                // A probe tip is a sphere: the ball radius IS the
                // corner, and a stylus shorter than its own ball is
                // no stylus at all.
                ToolKind::Probe => {
                    (self.corner_r - self.diameter * 0.5).abs() < 1e-9
                        && self.flute_len >= self.diameter
                }
                _ => true,
            }
    }

    /// The corner radius the tip really has (#263). A ball nose's or a
    /// probe's tip is a sphere, so its corner is half the diameter
    /// whatever `corner_r` holds - the radius every reader already cuts
    /// and judges with (`Cutter::of`, [`Tool::profile`], the stock sim's
    /// sphere atom). Every other kind answers its own field; only the
    /// bull nose reads it.
    #[must_use]
    pub fn tip_corner_r(&self) -> f64 {
        match self.kind {
            ToolKind::BallNose | ToolKind::Probe => self.diameter * 0.5,
            _ => self.corner_r,
        }
    }

    /// The same tool as the verifier sees it: coaxial radii upward from
    /// the cutting edge.
    #[must_use]
    pub fn geom(&self) -> ToolGeom {
        ToolGeom {
            radius: self.diameter * 0.5,
            nose_radius: match self.kind {
                // The envelope law rolls the CENTER path with this
                // circle: the turning insert's nose (stage A) and,
                // from M5.2 F, the boring bar's - a bore ride walks
                // the nose center just like the OD rides, so the
                // verdict must roll the same ball. The blade kinds
                // (Groove, Thread, Form) are judged on the
                // meridian, not by a rolled ball: their ground
                // geometry rides the op params, and their ToolGeom
                // nose stays 0.0 by law.
                ToolKind::Turning | ToolKind::Bore => self.nose_radius,
                _ => 0.0,
            },
            flute_len: self.flute_len,
            shank_radius: self.shank_d * 0.5,
            shank_len: self.shank_len,
            holder_radius: self.holder_d * 0.5,
            holder_len: self.holder_len,
        }
    }

    /// The tool's side profile as an axisymmetric outline: (z, radius)
    /// points from the tip center at `z = 0` up the right side of the
    /// revolve to `(flute_len + shank_len + holder_len, 0)` at the top.
    /// The band breakpoints are exactly what the shank-reach law sweeps
    /// (`verify`'s bands start at `flute_len` and `flute_len + shank_len`
    /// and skip the zero-length ones - so does this), which is what lets
    /// the editor's preview draw the very geometry the checker collides.
    ///
    /// Tip by kind: a flat rim on an end mill, a sampled hemispherical
    /// dome on a ball nose, a toroidal corner (clamped to what the
    /// diameter and the flute allow) on a bull nose, a cone on a twist
    /// drill (inert `tip_angle` of 0 reads as the flat it degenerates
    /// to), and a plain cylinder on a tap or boring bar. Degenerate
    /// numbers (a half-typed draft) yield a shorter picture, never a NaN
    /// and never a panic - the drawing Prime Directive mirrors `is_sane`.
    ///
    /// Provenance: our own `Tool` schema; the tip shapes are the standard
    /// machine-tool handbook side views.
    #[must_use]
    pub fn profile(&self) -> Vec<(f64, f64)> {
        let r = self.diameter.max(0.0) * 0.5;
        let zc = self.flute_len.max(0.0);
        let zs = zc + self.shank_len.max(0.0);
        let zh = zs + self.holder_len.max(0.0);
        let cr = self.corner_r.clamp(0.0, r).min(zc);
        let mut p: Vec<(f64, f64)> = Vec::new();
        add_pt(&mut p, 0.0, 0.0);
        match self.kind {
            ToolKind::BallNose | ToolKind::Probe => {
                let steps = 24;
                let top = r.min(zc);
                for i in 1..steps {
                    let z = top * f64::from(i) / f64::from(steps);
                    add_pt(&mut p, z, dome_radius(r, z));
                }
                add_pt(&mut p, top, dome_radius(r, top));
            }
            ToolKind::BullNose => {
                add_pt(&mut p, 0.0, r - cr);
                let steps = 12;
                for i in 1..steps {
                    let t = std::f64::consts::FRAC_PI_2 * f64::from(i) / f64::from(steps);
                    add_pt(&mut p, cr * (1.0 - t.cos()), r - cr + cr * t.sin());
                }
                add_pt(&mut p, cr, r);
            }
            // The insert's nose circle lives in the MERIDIAN of the
            // work's spin, not this side view - the facing verdict
            // reads it through `geom` - so the picture is the plain
            // envelope the shank laws collide with - and so it is for
            // the blade: its truth is the band `width` in the work's
            // meridian, which the turning verdicts read off `Tool`,
            // not off this cylinder.
            ToolKind::EndMill | ToolKind::Tap | ToolKind::Bore | ToolKind::Turning
            | ToolKind::Groove | ToolKind::Thread | ToolKind::Form => add_pt(&mut p, 0.0, r),
            // The chamfer mill's cone is the same picture read the
            // other way in purpose - apex at the tip center opening
            // upward to the full radius - so it shares the drill's
            // line math exactly (a 90-degree cone meets r at z = r).
            ToolKind::Drill | ToolKind::Taper => {
                // The point cone: apex at the tip center, opening at
                // half the included angle, meeting the full radius at
                // z = r / tan(tip/2) - or the flute top first, clamped,
                // with the straight land above drawn by the shared band
                // step. Nonsense angles (0, negative) read as a flat.
                let tan_half = (self.tip_angle.to_radians() * 0.5).tan();
                if tan_half > 0.0 && tan_half.is_finite() {
                    let zr = (r / tan_half).min(zc);
                    add_pt(&mut p, zr, (zr * tan_half).min(r));
                }
            }
        }
        add_pt(&mut p, zc, r);
        let sr = self.shank_d.max(0.0) * 0.5;
        if zs > zc {
            add_pt(&mut p, zc, sr);
            add_pt(&mut p, zs, sr);
        }
        let hr = self.holder_d.max(0.0) * 0.5;
        if zh > zs {
            add_pt(&mut p, zs, hr);
            add_pt(&mut p, zh, hr);
        }
        add_pt(&mut p, zh, 0.0);
        p
    }
}

/// Append unless it equals the previous point: the band steps share a z
/// with the wall's top, and duplicates only thicken the polyline.
fn add_pt(p: &mut Vec<(f64, f64)>, z: f64, r: f64) {
    if p.last().copied() != Some((z, r)) {
        p.push((z, r));
    }
}

/// A sphere of radius `rad`, tip at z=0, center at z=rad: the cross-section
/// radius at height z. The `max(0.0)` before the sqrt is the clamped
/// half-typed-draft guard (rounding at the pole cannot go negative;
/// nonsense z can, and the Prime Directive says it renders short, not NaN).
fn dome_radius(rad: f64, z: f64) -> f64 {
    (2.0 * rad * z - z * z).max(0.0).sqrt()
}

/// The starter catalog: name, then tool. Ordinary metric sizes in a
/// 2-flute/4-flute ladder; shanks step up to the next common gauge so
/// the shank-reach law has something wider than the flute to collide
/// with. Display order - the tool library pane lists these top to bottom.
#[must_use]
pub fn catalog() -> Vec<(&'static str, Tool)> {
    let em = |diameter: f64, corner_r: f64, kind: ToolKind, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| Tool {
        kind,
        diameter,
        corner_r,
        tip_angle: 0.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len,
        shank_d,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(),
        rpm,
        feed,
        tool_material: None,
        flutes: None, center_cutting: true, orientation: None,
    };
    // Cycle tools: straight-shank bodies (a jobber drill's shank is its
    // own diameter), no corner radius, 118-degree points, ISO coarse
    // pitches; a tap's feed number is the rpm x pitch starting value.
    let cy = |kind: ToolKind, diameter: f64, tip_angle: f64, pitch: f64, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| {
        Tool { kind, diameter, corner_r: 0.0, tip_angle, pitch, nose_radius: 0.0, width: 0.0, flute_len, shank_d, shank_len: 40.0, holder_d: 44.0, holder_len: 35.0, material_group: "aluminum".into(), rpm, feed, tool_material: None, flutes: None, center_cutting: true, orientation: None }
    };
    // Turning (M5.2 A): the facing insert - a plain toolholder shank,
    // no holder stack (the turret IS the holder), an 0.8 nose like the
    // ISO roughing inserts. Feeds are the handbook's starting values
    // for aluminum facing with carbide.
    let tn = |diameter: f64, nose_radius: f64, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| Tool {
        kind: ToolKind::Turning, diameter, corner_r: 0.0, tip_angle: 0.0, pitch: 0.0,
        nose_radius, width: 0.0, flute_len, shank_d, shank_len: 60.0, holder_d: 0.0, holder_len: 0.0,
        material_group: "aluminum".into(), rpm, feed, tool_material: None, flutes: None, center_cutting: true, orientation: None,
    };
    // Grooving (M5.2 C): the blade insert on the same toolholder body -
    // the ground WIDTH is the tool, the nose and the taper are not
    // modeled (the blade cuts a kerf, not a rolled circle). Feeds are
    // the handbook's starting values for aluminum: grooving rides a
    // facing-like feed, cutoff plunges slow.
    let gt = |diameter: f64, width: f64, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| Tool {
        kind: ToolKind::Groove, diameter, corner_r: 0.0, tip_angle: 0.0, pitch: 0.0,
        nose_radius: 0.0, width, flute_len, shank_d, shank_len: 60.0, holder_d: 0.0, holder_len: 0.0,
        material_group: "aluminum".into(), rpm, feed, tool_material: None, flutes: None, center_cutting: true, orientation: None,
    };
    // Threading (M5.2 D): the V-form insert on the same toolholder
    // body - the GROUND PROFILE (width across the crest, the flanks'
    // included angle) is the tool, the pitch is the THREAD's and lives
    // in the operation. Crest widths are the handbook's 0.866 x P for
    // the two ordinary metric coarse ladders; feeds are the starting
    // values for single-point thread whirling in aluminum.
    let td = |diameter: f64, width: f64, form_angle: f64, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| Tool {
        kind: ToolKind::Thread, diameter, corner_r: 0.0, tip_angle: form_angle, pitch: 0.0,
        nose_radius: 0.0, width, flute_len, shank_d, shank_len: 60.0, holder_d: 0.0, holder_len: 0.0,
        material_group: "aluminum".into(), rpm, feed, tool_material: None, flutes: None, center_cutting: true, orientation: None,
    };
    // Form gauges (M5.2 E2): the crown radius lives in `nose_radius`
    // (the arc of the ground edge in the work's meridian - the
    // field the Turning insert uses for its nose circle, read here
    // as the other ground radius a turning steel carries) and the
    // band width in `width`. A slow feed: forming is a sink, and
    // the crown's corners carry the whole width at once.
    let fg = |diameter: f64, crown_r: f64, width: f64, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| Tool {
        kind: ToolKind::Form, diameter, corner_r: 0.0, tip_angle: 0.0, pitch: 0.0,
        nose_radius: crown_r, width, flute_len, shank_d, shank_len: 60.0, holder_d: 0.0, holder_len: 0.0,
        material_group: "aluminum".into(), rpm, feed, tool_material: None, flutes: None, center_cutting: true, orientation: None,
    };
    let mut v = vec![
        ("EM-0200-2FL", em(2.0, 0.0, ToolKind::EndMill, 6.0, 4.0, 18000.0, 600.0)),
        ("EM-0300-2FL", em(3.0, 0.0, ToolKind::EndMill, 9.0, 4.0, 15000.0, 900.0)),
        ("EM-0400-2FL", em(4.0, 0.0, ToolKind::EndMill, 12.0, 6.0, 12000.0, 1200.0)),
        ("EM-0600-4FL", em(6.0, 0.0, ToolKind::EndMill, 15.0, 8.0, 9500.0, 1800.0)),
        ("EM-0800-4FL", em(8.0, 0.0, ToolKind::EndMill, 20.0, 10.0, 7200.0, 2400.0)),
        ("EM-1000-4FL", em(10.0, 0.0, ToolKind::EndMill, 22.0, 10.0, 5800.0, 3000.0)),
        ("EM-1200-2FL", em(12.0, 0.0, ToolKind::EndMill, 25.0, 12.0, 4800.0, 3600.0)),
        ("BN-0600", em(6.0, 3.0, ToolKind::BallNose, 15.0, 8.0, 9000.0, 1500.0)),
        ("BN-1000", em(10.0, 5.0, ToolKind::BallNose, 22.0, 10.0, 6000.0, 2000.0)),
        ("BB-1000-R1", em(10.0, 1.0, ToolKind::BullNose, 20.0, 10.0, 6500.0, 2200.0)),
        ("DR-0200", cy(ToolKind::Drill, 2.0, 118.0, 0.0, 6.0, 2.0, 15000.0, 150.0)),
        ("DR-0300", cy(ToolKind::Drill, 3.0, 118.0, 0.0, 9.0, 3.0, 12000.0, 120.0)),
        ("DR-0400", cy(ToolKind::Drill, 4.0, 118.0, 0.0, 12.0, 4.0, 10000.0, 130.0)),
        ("DR-0600", cy(ToolKind::Drill, 6.0, 118.0, 0.0, 18.0, 6.0, 7500.0, 115.0)),
        // Chamfer mills (M2.4 B): the 90-degree cone is the everyday
        // 45-degree chamfer tool; single-flute carbide, the handbook's
        // stock sizes. Speeds track the end-mill ladder at the same
        // diameter - a chamfer ride is light, and light wants rpm.
        ("CM-0400-90", cy(ToolKind::Taper, 4.0, 90.0, 0.0, 6.0, 6.0, 12000.0, 1500.0)),
        ("CM-0600-90", cy(ToolKind::Taper, 6.0, 90.0, 0.0, 10.0, 8.0, 10000.0, 2000.0)),
        ("CM-1000-90", cy(ToolKind::Taper, 10.0, 90.0, 0.0, 16.0, 10.0, 6000.0, 3000.0)),
        ("TAP-M4", cy(ToolKind::Tap, 4.0, 0.0, 0.7, 18.0, 4.5, 800.0, 560.0)),
        ("TAP-M6", cy(ToolKind::Tap, 6.0, 0.0, 1.0, 20.0, 6.0, 600.0, 600.0)),
        // The catalog boring bar, with the ground nose a boring tip
        // is bought with (0.4 is the handbook's usual for a Ø10 bar)
        // - inert for the drilling cycles, the ride radius for the
        // boring kernel (M5.2 F). The mill holder stack cy gives is
        // stripped, the tn family's rule: the turret IS the holder.
        // Law (c) reads a band's column along Z about the move's XY,
        // and a bar's steel lies RADIAL on a lathe - an OD column 22
        // over the tip convicts every bore ride for a collision no
        // boring bar has. Its kerf excuses the rest: Ø10 body on a
        // Ø10 shank, the shank band skipped as the inserts' is.
        ("BR-1000", Tool { nose_radius: 0.4, holder_d: 0.0, holder_len: 0.0, ..cy(ToolKind::Bore, 10.0, 0.0, 0.0, 20.0, 10.0, 4000.0, 400.0) }),
        ("TN-1600-0R8", tn(16.0, 0.8, 8.0, 16.0, 3000.0, 150.0)),
        ("GT-0200", gt(16.0, 2.0, 8.0, 16.0, 2500.0, 90.0)),
        ("GT-0300", gt(16.0, 3.0, 8.0, 16.0, 2000.0, 60.0)),
        ("TD-0866-60", td(16.0, 0.866, 60.0, 8.0, 16.0, 1200.0, 90.0)),
        ("TD-1732-60", td(16.0, 1.732, 60.0, 8.0, 16.0, 800.0, 120.0)),
        ("FG-0600-04", fg(16.0, 6.0, 4.0, 8.0, 16.0, 1500.0, 40.0)),
        ("FG-1200-08", fg(16.0, 12.0, 8.0, 8.0, 16.0, 1200.0, 50.0)),
        // The touch probe (#108): a 4 mm ruby ball on a 30 mm stylus, the
        // everyday starter size - the Probe kind's catalog hint.
        ("PRB-0400", probe_tool(4.0, 30.0)),
    ];
    // Cutting data (M13.2): the rotating mills state their edge
    // material and flute count - carbide, as the speeds above were
    // set for; the count the name carries (2FL/4FL), two on the ball
    // and bull noses, one on the single-flute chamfer mills. Drills,
    // taps and the turning steels state nothing: the table does not
    // speak for them.
    for (name, t) in &mut v {
        let z = if name.ends_with("-4FL") {
            Some(4)
        } else if name.starts_with("CM-") {
            Some(1)
        } else if name.starts_with("EM-") || name.starts_with("BN-") || name.starts_with("BB-") {
            Some(2)
        } else {
            None
        };
        if z.is_some() {
            t.flutes = z;
            t.tool_material = Some(ToolMaterial::Carbide);
        }
    }
    v
}

/// A touch probe of stylus ball `ball_d` mm over a stylus `stylus_len` mm
/// long (#108): the ball is the whole tip (corner = radius), the probe
/// body rides as the shank band. `rpm` and `feed` are nominal - the
/// probe turns no spindle, and the Probe operation states its own
/// seek and measure feeds - but every tool states them positive.
#[must_use]
pub fn probe_tool(ball_d: f64, stylus_len: f64) -> Tool {
    Tool {
        kind: ToolKind::Probe,
        diameter: ball_d,
        corner_r: ball_d * 0.5,
        tip_angle: 0.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: stylus_len,
        shank_d: 20.0,
        shank_len: 40.0,
        holder_d: 0.0,
        holder_len: 0.0,
        material_group: String::new(),
        rpm: 1.0,
        feed: 300.0,
        tool_material: None,
        flutes: None,
        center_cutting: false,
        orientation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sane_and_geom_matches() {
        for (name, t) in catalog() {
            assert!(t.is_sane(), "{name}");
            let g = t.geom();
            assert_eq!(g.radius, t.diameter * 0.5);
            assert_eq!(g.shank_radius, t.shank_d * 0.5);
        }
    }

    #[test]
    fn degenerate_tools_are_refused() {
        let base = catalog().remove(0).1;
        assert!(!Tool { diameter: 0.0, ..base.clone() }.is_sane());
        assert!(!Tool { diameter: f64::NAN, ..base.clone() }.is_sane());
        assert!(!Tool { corner_r: base.diameter, ..base.clone() }.is_sane());
        assert!(!Tool { flute_len: -1.0, ..base.clone() }.is_sane());
        assert!(!Tool { rpm: 0.0, ..base }.is_sane());
        // Kind-conditional: a drill point must self-center, a tap must
        // have a thread to derive its feed from.
        let drill = catalog().into_iter().find(|(n, _)| *n == "DR-0400").unwrap().1;
        assert!(!Tool { tip_angle: 200.0, ..drill.clone() }.is_sane());
        assert!(!Tool { tip_angle: 45.0, ..drill.clone() }.is_sane());
        assert!(!Tool { pitch: -1.0, ..drill }.is_sane());
        let tap = catalog().into_iter().find(|(n, _)| *n == "TAP-M6").unwrap().1;
        assert!(!Tool { pitch: 0.0, ..tap }.is_sane());
    }

    /// #263: a ball's corner is its ball. A ball nose saved with any
    /// corner field still answers half its diameter - the radius the
    /// readers cut with - while a bull nose answers its own field.
    #[test]
    fn a_ball_noses_corner_is_half_its_diameter_whatever_the_field_holds() {
        let tool = |name: &str| catalog().into_iter().find(|(n, _)| *n == name).unwrap().1;
        let ball = tool("BN-0600");
        for stale in [0.0, 1.0, ball.diameter * 0.5] {
            let b = Tool { corner_r: stale, ..ball.clone() };
            assert_eq!(b.tip_corner_r(), ball.diameter * 0.5, "corner field {stale}");
        }
        let bull = Tool { kind: ToolKind::BullNose, corner_r: 1.0, ..ball };
        assert_eq!(bull.tip_corner_r(), 1.0);
    }

    /// The preview-drawing law: the profile's band breakpoints are the
    /// same z's `verify` sweeps for shank and holder reach, and each band
    /// carries that band's radius. If the checker and the picture ever
    /// disagree about where a band starts, this fails.
    #[test]
    fn profile_segments_match_the_collision_bands() {
        let near = |p: &[(f64, f64)], z: f64, r: f64| {
            p.iter().any(|&(pz, pr)| (pz - z).abs() < 1e-9 && (pr - r).abs() < 1e-9)
        };
        for (name, t) in catalog() {
            let g = t.geom();
            let p = t.profile();
            assert!(p.len() >= 3, "{name}: an outline needs vertices");
            assert_eq!(p[0], (0.0, 0.0), "{name}: tip center first");
            assert!(
                near(&p, g.flute_len, g.shank_radius),
                "{name}: the shank band must start where verify says"
            );
            assert!(
                near(&p, g.flute_len + g.shank_len, g.holder_radius),
                "{name}: the holder band must start at flute + shank"
            );
            assert!(
                near(&p, g.flute_len + g.shank_len + g.holder_len, 0.0),
                "{name}: the outline must end on the axis at the top"
            );
            // z is non-decreasing along the outline - the picture never
            // runs backwards, whatever the numbers are.
            assert!(
                p.windows(2).all(|w| w[0].0 <= w[1].0 + 1e-12),
                "{name}: profile z ran backwards: {p:?}"
            );
        }
    }

    #[test]
    fn profile_degeneracies_draw_short_never_wrong() {
        let base = catalog()[3].1.clone();
        // A zero-length holder contributes nothing: the picture tops out
        // at the shank, exactly as verify skips that band.
        let no_holder = Tool { holder_len: 0.0, ..base.clone() };
        let p = no_holder.profile();
        let z_max = p.iter().map(|q| q.0).fold(0.0f64, f64::max);
        assert_eq!(z_max, base.flute_len + base.shank_len);
        // A half-typed draft (all zeros) is a point, not a panic, and no
        // NaN rides anywhere in the polyline.
        let blank = Tool {
            diameter: 0.0,
            corner_r: 0.0,
            flute_len: 0.0,
            shank_d: 0.0,
            shank_len: 0.0,
            holder_d: 0.0,
            holder_len: 0.0,
            ..base.clone()
        };
        let p = blank.profile();
        assert_eq!(p, vec![(0.0, 0.0)], "an empty tool is one point");
        // A negative typed length clamps to zero rather than pointing down.
        let neg = Tool { shank_len: -5.0, ..base };
        assert!(neg.profile().iter().all(|q| q.0 >= 0.0 && q.0.is_finite()));
    }

    #[test]
    fn tip_shapes_read_from_the_kind() {
        // Ball: a dome - center on the axis, widening to full radius at
        // z = d/2, all strictly inside the cutting envelope before that.
        let ball = catalog().into_iter().find(|(n, _)| *n == "BN-0600").unwrap().1;
        let p = ball.profile();
        let r = ball.diameter * 0.5;
        assert!(p.iter().any(|&(z, rad)| (z - r).abs() < 1e-9 && (rad - r).abs() < 1e-9));
        assert!(
            p.iter()
                .all(|&(z, rad)| z >= ball.flute_len - 1e-9 || rad <= r + 1e-9),
            "the cutting body must not grow outside the tool: {p:?}"
        );
        assert!(p[1].1 < p[2].1, "a dome opens widening, not narrowing");
        // Bull: flat center, a rounded corner, full radius by z = corner_r.
        let bull = catalog().into_iter().find(|(n, _)| *n == "BB-1000-R1").unwrap().1;
        let p = bull.profile();
        let (r, cr) = (bull.diameter * 0.5, bull.corner_r);
        assert_eq!(p[1], (0.0, r - cr), "flat runs from the axis to the corner");
        assert!(p.iter().any(|&(z, rad)| (z - cr).abs() < 1e-9 && (rad - r).abs() < 1e-9));
        // End mill: nothing between the tip and the wall.
        let em = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").unwrap().1;
        let p = em.profile();
        assert_eq!(&p[..2], &[(0.0, 0.0), (0.0, em.diameter * 0.5)]);
        // Drill: a cone from the axis, meeting full radius at the
        // handbook height r / tan(tip/2), then the straight land.
        let dr = catalog().into_iter().find(|(n, _)| *n == "DR-0400").unwrap().1;
        let p = dr.profile();
        let (r, zr) = (dr.diameter * 0.5, dr.diameter * 0.5 / (118.0_f64.to_radians() * 0.5).tan());
        assert!(p[1].0 > 0.0 && p[1].1 > 0.0, "a drill opens from a point, not a flat rim");
        assert!(p.iter().any(|&(z, rad)| (z - zr).abs() < 1e-6 && (rad - r).abs() < 1e-6));
        // Tap: a plain cylinder - pitch is a feed number, never drawn.
        let tap = catalog().into_iter().find(|(n, _)| *n == "TAP-M6").unwrap().1;
        let p = tap.profile();
        assert_eq!(&p[..2], &[(0.0, 0.0), (0.0, tap.diameter * 0.5)]);
        // Chamfer mill: the cone the 2D chamfer's congruence door
        // reads - a 90-degree included angle opens at 45 degrees from
        // the axis, meeting the full radius at z = r / tan(45) = r,
        // the handbook picture of the shop's own catalog page.
        let cm = catalog().into_iter().find(|(n, _)| *n == "CM-0400-90").unwrap().1;
        let p = cm.profile();
        assert!(p[1].0 > 0.0 && p[1].1 > 0.0, "a cone opens from its virtual sharp, not a flat rim");
        assert!(p.iter().any(|&(z, rad)| (z - cm.diameter * 0.5).abs() < 1e-6 && (rad - cm.diameter * 0.5).abs() < 1e-6));
        assert!(!Tool { tip_angle: 0.0, ..cm.clone() }.is_sane(), "a zero cone is a cylinder in disguise");
    }
}
