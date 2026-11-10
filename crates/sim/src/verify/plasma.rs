//! Plasma through-cut laws (#672, part of #621): what replaces the gouge
//! and collision checks for a 2D plasma program. A plasma op cuts through,
//! so the hazards are not depth but ORDER and GAPS:
//!
//! - **Cut order (refusal).** A contour cut after a contour that encloses
//!   it (a hole after its outline, an island after its hole): the
//!   enclosing cut has freed the material it sits in.
//! - **Kerf crossing (warning).** A cut whose centre line comes within one
//!   kerf of an earlier cut's: the torch crosses an open kerf, the arc
//!   voltage jumps and torch height control dives - it "can smash the
//!   torch into the material" (LinuxCNC Plasma Primer §12). This law
//!   covers only that torch-height hazard, so it warns (a shared-kerf
//!   common line is legal); a centre line truly crossing a finished
//!   contour outside a lead becomes a refusal when the CL reader lands
//!   (#672 step 2).
//! - **Tip-up (warning).** A rapid that crosses the area of a part already
//!   cut free: a freed part can tip up into the torch (Tormach 1300PL
//!   manual UM10720 §1.4.2, §4.12.8).
//! - **Malformed (refusal).** A non-positive or non-finite kerf, a NaN
//!   point, a cut with fewer than two points (unless a centre-pierced
//!   small hole), a duplicate contour id, a parent that is the contour
//!   itself or no cut carries, an inner contour with no parent, a rapid
//!   list that does not match the cuts, a NaN rapid point - refused by
//!   name.
//!
//! Input is a neutral program - the cuts in order, each the torch-on
//! polyline (arcs chorded by the caller) with its contour id, role and
//! enclosing contour (the CL's `TORCH ON C<id> OUTER|INNER|OPEN
//! [PARENT<id>] [SMALL..]`, #670) - and nothing of the operation's
//! internals.
//!
//! Provenance: our own geometry over the cited plasma practice; no vendor
//! implementation inspected.

use crate::geo2d::Point2;

/// What a contour is: a part's outline (or an island in a hole), a hole,
/// or an open path (a slit, not a closed area).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutRole {
    Outer,
    Inner,
    Open,
}

/// One torch-on span: a contour cut from pierce to torch-off.
#[derive(Clone, Debug, PartialEq)]
pub struct PlasmaCut {
    pub contour: u32,
    pub role: CutRole,
    /// The smallest closed contour enclosing this one (a hole's outline,
    /// an island's hole); `None` for a top-level contour.
    pub parent: Option<u32>,
    /// A centre-pierced small hole: its path may be a single point.
    pub small: bool,
    /// The torch path, lead-in to lead-out, in cut order.
    pub path: Vec<Point2>,
}

/// A plasma program: the kerf, the cuts in cut order, and the rapid that
/// reaches each cut's pierce - `rapids[i]` is (previous torch-off point,
/// pierce point) before `cuts[i]`, `None` when the program starts there.
/// `rapids` has one entry per cut.
#[derive(Clone, Debug, Default)]
pub struct PlasmaProgram {
    pub kerf: f64,
    pub cuts: Vec<PlasmaCut>,
    pub rapids: Vec<Option<(Point2, Point2)>>,
}

/// What the laws find.
#[derive(Clone, Debug, PartialEq)]
pub enum PlasmaFinding {
    /// Refusal: `contour` is cut after `enclosing`, which encloses it.
    CutOrder { contour: u32, enclosing: u32 },
    /// Warning: contour `contour` comes within a kerf of earlier `earlier`.
    KerfCrossing { contour: u32, earlier: u32, at: Point2 },
    /// Warning: the rapid before `contour` crosses freed part `freed`.
    TipUp { contour: u32, freed: u32, at: Point2 },
    /// Refusal: the program cannot be judged.
    Malformed(String),
}

impl PlasmaFinding {
    /// Does this finding block the post?
    #[must_use]
    pub fn refuses(&self) -> bool {
        matches!(self, PlasmaFinding::CutOrder { .. } | PlasmaFinding::Malformed(_))
    }
}

/// Judge a plasma program. Findings come in cut order; a malformed
/// program is one refusal and nothing else.
#[must_use]
pub fn judge(p: &PlasmaProgram) -> Vec<PlasmaFinding> {
    if let Err(why) = well_formed(p) {
        return vec![PlasmaFinding::Malformed(why)];
    }
    let mut out = Vec::new();
    for (i, c) in p.cuts.iter().enumerate() {
        // Cut order: anything enclosing this contour already cut.
        if let Some(parent) = c.parent
            && p.cuts[..i].iter().any(|e| e.contour == parent)
        {
            out.push(PlasmaFinding::CutOrder { contour: c.contour, enclosing: parent });
        }
        // Tip-up: the rapid into this cut over a closed outline already cut.
        if let Some(Some((a, b))) = p.rapids.get(i) {
            for e in p.cuts[..i].iter().filter(|e| e.role == CutRole::Outer) {
                if let Some(at) = crosses_area(*a, *b, &e.path) {
                    out.push(PlasmaFinding::TipUp { contour: c.contour, freed: e.contour, at });
                }
            }
        }
        // Kerf crossing: within a kerf of an earlier cut's centre line.
        for e in &p.cuts[..i] {
            if let Some(at) = near(&c.path, &e.path, p.kerf) {
                out.push(PlasmaFinding::KerfCrossing { contour: c.contour, earlier: e.contour, at });
            }
        }
    }
    out
}

/// The program's shape, refused by name when it cannot be judged.
fn well_formed(p: &PlasmaProgram) -> Result<(), String> {
    if !(p.kerf.is_finite() && p.kerf > 0.0) {
        return Err(format!("plasma kerf {} is not a positive width", p.kerf));
    }
    if p.rapids.len() != p.cuts.len() {
        return Err(format!("{} rapids for {} cuts - one rapid entry per cut", p.rapids.len(), p.cuts.len()));
    }
    let finite = |q: &Point2| q.x.is_finite() && q.y.is_finite();
    let mut ids = std::collections::HashSet::new();
    for (c, r) in p.cuts.iter().zip(&p.rapids) {
        if !ids.insert(c.contour) {
            return Err(format!("contour {} appears twice", c.contour));
        }
        if c.path.is_empty() || (c.path.len() < 2 && !c.small) {
            return Err(format!("contour {} has no cut path", c.contour));
        }
        if !c.path.iter().all(finite) {
            return Err(format!("contour {} has a non-finite point", c.contour));
        }
        if let Some((a, b)) = r
            && !(finite(a) && finite(b))
        {
            return Err(format!("the rapid into contour {} has a non-finite point", c.contour));
        }
    }
    for c in &p.cuts {
        match c.parent {
            Some(parent) if parent == c.contour => return Err(format!("contour {} names itself as its parent", c.contour)),
            Some(parent) if !ids.contains(&parent) => {
                return Err(format!("contour {} names parent {parent}, which no cut carries", c.contour));
            }
            None if c.role == CutRole::Inner => return Err(format!("inner contour {} has no parent", c.contour)),
            _ => {}
        }
    }
    Ok(())
}

/// The first point of `a` whose segment comes within `gap` of `b`'s path.
fn near(a: &[Point2], b: &[Point2], gap: f64) -> Option<Point2> {
    for s in a.windows(2) {
        for t in b.windows(2) {
            if seg_dist(s[0], s[1], t[0], t[1]) < gap {
                return Some(s[0]);
            }
        }
    }
    None
}

/// Where segment a-b enters the area closed by `ring` (its endpoints
/// inside, or it crossing the ring); `None` when it stays outside.
fn crosses_area(a: Point2, b: Point2, ring: &[Point2]) -> Option<Point2> {
    if ring.len() < 3 {
        return None;
    }
    if inside(a, ring) {
        return Some(a);
    }
    if inside(b, ring) {
        return Some(b);
    }
    let n = ring.len();
    for k in 0..n {
        let (c, d) = (ring[k], ring[(k + 1) % n]);
        if seg_dist(a, b, c, d) == 0.0 {
            return Some(c);
        }
    }
    None
}

/// Even-odd point in polygon.
fn inside(p: Point2, ring: &[Point2]) -> bool {
    let mut odd = false;
    let n = ring.len();
    for k in 0..n {
        let (a, b) = (ring[k], ring[(k + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                odd = !odd;
            }
        }
    }
    odd
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn pt_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - qx).powi(2) + (p.y - qy).powi(2)).sqrt()
}

/// Distance between segments a-b and c-d (0 when they cross).
fn seg_dist(a: Point2, b: Point2, c: Point2, d: Point2) -> f64 {
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return 0.0;
    }
    pt_seg(a, c, d).min(pt_seg(b, c, d)).min(pt_seg(c, a, b)).min(pt_seg(d, a, b))
}
