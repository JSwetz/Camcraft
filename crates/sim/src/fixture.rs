//! Fixture collision predicates: what "the tool swept into the
//! workholding" means in closed form - boxes against tool cylinders,
//! bands and rapids, conservative by construction. Pure geometry; the
//! entity side (which boxes a setup declares) is the app's `fixture`
//! module, re-exporting this whole.
//!
//! Provenance: general workholding-collision practice (machine-tool
//! handbooks), analytic cylinder/AABB intersection from published
//! computational geometry; the predicates are ours. Crate home per
//! M4.1 R4: camcraft-sim's `verify` is their main reader.

use crate::geometry::Point3;

/// The margin every fixture predicate adds before deciding "no contact",
/// so coincident faces and float-dust gaps classify as hits. 1 nm -
/// orders of magnitude below any tolerance a machine or a tool library
/// carries, far above accumulated round-off in these closed-form tests.
pub const FIXTURE_HIT_TOL: f64 = 1e-9;

/// Fixtures beyond a real machine table by three orders of magnitude are
/// a typo, not workholding - and the derived table's underside sits this
/// far down, which is "unreachable from below" expressed in millimeters.
pub const MAX_FIXTURE_DIM: f64 = 1_000_000.0;

/// The virtual name of the machine table row (the list and the sheet
/// show it; the commands refuse to move, tag, or delete it, because it
/// is not an entity - it is what the `.mch` says the table is).
pub const MACHINE_TABLE: &str = "Machine/Table";

/// One solid's world-space body box, always axis-aligned (see module
/// header: conservative AABBs). The verifier's collision currency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixtureBox {
    pub min: Point3,
    pub max: Point3,
}

/// Inclusive point containment, faces grown by `tol`: on a face is
/// inside (steel contact is contact). The min/max split is normalized,
/// so a degenerately stored box can never dodge the test.
#[must_use]
pub fn point_inside(b: &FixtureBox, p: Point3, tol: f64) -> bool {
    let t = tol.abs();
    let (x0, x1) = (b.min.x.min(b.max.x) - t, b.max.x.max(b.min.x) + t);
    let (y0, y1) = (b.min.y.min(b.max.y) - t, b.max.y.max(b.min.y) + t);
    let (z0, z1) = (b.min.z.min(b.max.z) - t, b.max.z.max(b.min.z) + t);
    p.x.is_finite()
        && p.y.is_finite()
        && p.z.is_finite()
        && p.x >= x0
        && p.x <= x1
        && p.y >= y0
        && p.y <= y1
        && p.z >= z0
        && p.z <= z1
}

/// Does the vertical cylinder `(cx0, cy0)`, radius `r`, z span `[z0, z1]`
/// meet the box? The XY test is the exact distance from the axis to the
/// box's XY rectangle (zero inside it), grown by [`FIXTURE_HIT_TOL`]
/// before comparison; the z test is interval overlap, inclusive by the
/// same tol (a band face resting on steel touches, and touching is
/// colliding). Non-finite input fails CLOSED as a hit - clearance that
/// cannot be proven is not clearance (Prime Directive).
#[must_use]
pub fn cylinder_hits_box(cx0: f64, cy0: f64, z0: f64, z1: f64, r: f64, b: &FixtureBox) -> bool {
    if !(cx0.is_finite()
        && cy0.is_finite()
        && z0.is_finite()
        && z1.is_finite()
        && r.is_finite()
        && b.min.is_finite()
        && b.max.is_finite())
    {
        return true;
    }
    // The z overlap must be STRICT beyond tolerance, not a graze:
    // a clamp milled flush with the work, its top exactly at the
    // band's bottom plane, is a face contact - the machine stops, but
    // nothing penetrates anything. An overlap deeper than float dust
    // is the crash. Erring toward HIT in XY (disc touching the rect
    // counts); here the flush-contact false positive would drown the
    // signal, so contact-without-penetration is silent by law.
    let (bz0, bz1) = (b.min.z.min(b.max.z), b.max.z.max(b.min.z));
    let (lo_z, hi_z) = (z0.min(z1), z0.max(z1));
    if bz1 <= lo_z + FIXTURE_HIT_TOL || bz0 >= hi_z - FIXTURE_HIT_TOL {
        return false;
    }
    let (bx0, bx1) = (b.min.x.min(b.max.x), b.max.x.max(b.min.x));
    let (by0, by1) = (b.min.y.min(b.max.y), b.max.y.max(b.min.y));
    let dx = if cx0 < bx0 {
        bx0 - cx0
    } else if cx0 > bx1 {
        cx0 - bx1
    } else {
        0.0
    };
    let dy = if cy0 < by0 {
        by0 - cy0
    } else if cy0 > by1 {
        cy0 - by1
    } else {
        0.0
    };
    dx.hypot(dy) <= r.max(0.0) + FIXTURE_HIT_TOL
}

/// Does the rapid segment `p0`->`p1` carry a tool of radius `tool_r`
/// through the box? XY: the segment against the box's footprint inflated
/// by `tool_r` - an L-infinity inflation, and the radius disc is inside
/// that, so a swept-disc touch always registers and only corner grazing
/// over-reports (the err-toward-HIT direction). z: the span must dip
/// STRICTLY into the box by more than [`FIXTURE_HIT_TOL`] on both ends -
/// the one stated err-toward-silence (module law): a retract from
/// exactly the mounting surface passes through its own kerf in the
/// PART, not through the steel below it. Vertical rapids are not exempt
/// (there is no kerf through steel). Non-finite fails closed.
#[must_use]
pub fn rapid_hits_box(p0: &Point3, p1: &Point3, tool_r: f64, b: &FixtureBox) -> bool {
    if !(p0.is_finite()
        && p1.is_finite()
        && tool_r.is_finite()
        && b.min.is_finite()
        && b.max.is_finite())
    {
        return true;
    }
    let (bz0, bz1) = (b.min.z.min(b.max.z), b.max.z.max(b.min.z));
    let (lo_z, hi_z) = (p0.z.min(p1.z), p0.z.max(p1.z));
    if lo_z >= bz1 - FIXTURE_HIT_TOL || hi_z <= bz0 + FIXTURE_HIT_TOL {
        return false;
    }
    let grow = tool_r.max(0.0) + FIXTURE_HIT_TOL;
    let (x0, x1) = (b.min.x.min(b.max.x) - grow, b.max.x.max(b.min.x) + grow);
    let (y0, y1) = (b.min.y.min(b.max.y) - grow, b.max.y.max(b.min.y) + grow);
    // Slab clip: the segment meets the rect iff the entry parameter
    // never passes the exit parameter. A degenerate axis (zero run)
    // clips to "is the constant inside".
    let mut tmin = 0.0f64;
    let mut tmax = 1.0f64;
    for (pa, pb, lo, hi) in [(p0.x, p1.x, x0, x1), (p0.y, p1.y, y0, y1)] {
        let d = pb - pa;
        if d.abs() <= f64::EPSILON {
            if pa < lo || pa > hi {
                return false;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo - pa) / d, (hi - pa) / d);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        tmin = tmin.max(ta);
        tmax = tmax.min(tb);
        if tmin > tmax {
            return false;
        }
    }
    true
}

/// The XY-swept band against a fixture box (M3.2 stage B, law (c2)):
/// the vertical cylinder of radius `r` carried along the segment
/// (x0,y0)->(x1,y1) between z planes `bz0`/`bz1` - the shape a shank,
/// holder or flute column actually traces while a move runs. Testing
/// it at law points alone could be read straight past a small vise
/// sitting between two far endpoints. The z rule is
/// [`cylinder_hits_box`]'s (strict overlap beyond tolerance: flush
/// contact stops the machine but is not a crash); the XY rule is the
/// rapid law's slab clip against the box grown by the radius - a
/// segment reaching within `r` of the rect counts, touching included,
/// erring toward HIT as both parent laws do. A degenerate segment (a
/// pure plunge) reads as the point cylinder it is.
#[must_use]
#[allow(clippy::too_many_arguments)] // flat sweep primitive: segment + band + box, same shape as cylinder_hits_box
pub fn band_hits_box(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    bz0: f64,
    bz1: f64,
    r: f64,
    b: &FixtureBox,
) -> bool {
    if !(x0.is_finite()
        && y0.is_finite()
        && x1.is_finite()
        && y1.is_finite()
        && bz0.is_finite()
        && bz1.is_finite()
        && r.is_finite()
        && b.min.is_finite()
        && b.max.is_finite())
    {
        return true;
    }
    let (bz_lo, bz_hi) = (b.min.z.min(b.max.z), b.max.z.max(b.min.z));
    let (lo_z, hi_z) = (bz0.min(bz1), bz0.max(bz1));
    if bz_hi <= lo_z + FIXTURE_HIT_TOL || bz_lo >= hi_z - FIXTURE_HIT_TOL {
        return false;
    }
    let grow = r.max(0.0) + FIXTURE_HIT_TOL;
    let (xg0, xg1) = (b.min.x.min(b.max.x) - grow, b.max.x.max(b.min.x) + grow);
    let (yg0, yg1) = (b.min.y.min(b.max.y) - grow, b.max.y.max(b.min.y) + grow);
    // The rapid law's own slab clip, over XY only - the z window was
    // just decided. The path meets the grown rect iff the entry
    // parameter never passes the exit parameter.
    let mut tmin = 0.0f64;
    let mut tmax = 1.0f64;
    for (pa, pb, lo, hi) in [(x0, x1, xg0, xg1), (y0, y1, yg0, yg1)] {
        let d = pb - pa;
        if d.abs() <= f64::EPSILON {
            if pa < lo || pa > hi {
                return false;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo - pa) / d, (hi - pa) / d);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        tmin = tmin.max(ta);
        tmax = tmax.min(tb);
        if tmin > tmax {
            return false;
        }
    }
    true
}
