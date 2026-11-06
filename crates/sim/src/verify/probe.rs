//! The cutter as the solid law reads it: the tool assembly as a stack of
//! convex solids of revolution (flat band, point cone, toroidal corner,
//! ball) and the tool frame a move is measured in. The tip shapes are
//! the standard machine-tool handbook side views, the same ones
//! [`Tool::profile`] draws; the frame is plain linear algebra. Our own
//! design; no vendor source.

use crate::geometry::{Point3, Triangle};
use crate::tools::{Tool, ToolGeom, ToolKind};

/// The tool assembly: the cutting atom (radius, corner, point cone)
/// over `flute_len`, then the shank and holder bands - the same stack
/// the reach laws collide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cutter {
    pub radius: f64,
    /// 0 square end, `radius` ball, between bull nose.
    pub corner_r: f64,
    /// Included point angle in degrees (drill, taper); 0 for none.
    pub tip_angle: f64,
    pub flute_len: f64,
    pub shank_radius: f64,
    pub shank_len: f64,
    pub holder_radius: f64,
    pub holder_len: f64,
}

impl Cutter {
    /// The square-ended stack of a [`ToolGeom`]: never smaller than the
    /// real cutter at the same tip (a ball, bull or point sits inside
    /// the flat cylinder), so a verdict read with it can only over-report.
    pub fn flat(g: &ToolGeom) -> Self {
        Self {
            radius: g.radius,
            corner_r: 0.0,
            tip_angle: 0.0,
            flute_len: g.flute_len,
            shank_radius: g.shank_radius,
            shank_len: g.shank_len,
            holder_radius: g.holder_radius,
            holder_len: g.holder_len,
        }
    }

    /// The tool's own tip, by kind - exactly the side view
    /// [`Tool::profile`] draws: ball, bull corner, drill/taper point.
    pub fn of(t: &Tool) -> Self {
        let mut c = Self::flat(&t.geom());
        match t.kind {
            ToolKind::BallNose => c.corner_r = c.radius,
            ToolKind::BullNose => c.corner_r = t.corner_r,
            ToolKind::Drill | ToolKind::Taper => c.tip_angle = t.tip_angle,
            _ => {}
        }
        c
    }

    /// The atoms bottom-up, heights above the tip. Empty when the
    /// numbers describe no tool (the caller fails closed).
    pub(super) fn atoms(&self) -> Vec<Atom> {
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        let r = self.radius;
        if !(r.is_finite() && r > 0.0) || ![self.flute_len, self.shank_len, self.holder_len].into_iter().all(ok) {
            return Vec::new();
        }
        let fl = self.flute_len;
        let mut v = Vec::with_capacity(4);
        let mut z = 0.0;
        let tan_half = (self.tip_angle.to_radians() * 0.5).tan();
        let cr = if self.corner_r.is_finite() { self.corner_r.clamp(0.0, r) } else { 0.0 };
        if self.tip_angle > 0.0 && tan_half.is_finite() && tan_half > 0.0 {
            let top = (r / tan_half).min(fl);
            if top > 0.0 {
                v.push(Atom { z0: 0.0, z1: top, prof: Prof::Cone { k: tan_half, r } });
                z = top;
            }
        } else if cr > 0.0 {
            let top = cr.min(fl);
            if top > 0.0 {
                v.push(Atom { z0: 0.0, z1: top, prof: Prof::Bull { flat: r - cr, cr } });
                z = top;
            }
        }
        if fl > z {
            v.push(Atom { z0: z, z1: fl, prof: Prof::Flat(r) });
        }
        let mut band = |z0: f64, len: f64, br: f64| {
            if len > 0.0 && br.is_finite() && br > 0.0 {
                v.push(Atom { z0, z1: z0 + len, prof: Prof::Flat(br) });
            }
        };
        band(fl, self.shank_len, self.shank_radius);
        band(fl + self.shank_len, self.holder_len, self.holder_radius);
        v
    }
}

/// An atom's radius profile above its own bottom `s = z - z0`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Prof {
    Flat(f64),
    /// Toroidal corner: rho = flat + sqrt(cr^2 - (cr - s)^2); a ball is
    /// `flat = 0`.
    Bull { flat: f64, cr: f64 },
    /// Point cone, capped at the flute radius.
    Cone { k: f64, r: f64 },
}

/// One convex solid of revolution about the tool axis over local
/// heights `[z0, z1]`; its profile is concave and non-decreasing in z.
#[derive(Clone, Copy, Debug)]
pub(super) struct Atom {
    pub z0: f64,
    pub z1: f64,
    pub prof: Prof,
}

impl Atom {
    pub(super) fn rho(&self, z: f64) -> f64 {
        let s = (z - self.z0).max(0.0);
        match self.prof {
            Prof::Flat(r) => r,
            Prof::Bull { flat, cr } => {
                let t = (cr - s).clamp(0.0, cr);
                flat + (cr * cr - t * t).max(0.0).sqrt()
            }
            Prof::Cone { k, r } => (k * s).min(r),
        }
    }

    pub(super) fn rho_max(&self) -> f64 {
        match self.prof {
            Prof::Flat(r) | Prof::Cone { r, .. } => r,
            Prof::Bull { flat, cr } => flat + cr,
        }
    }

    pub(super) fn is_flat(&self) -> bool {
        matches!(self.prof, Prof::Flat(_))
    }
}

/// The tool frame of a tip and axis: local coordinates of world points
/// (tip at the origin, tool axis +z).
#[derive(Clone, Copy)]
pub(super) struct Frame {
    o: Point3,
    u: [f64; 3],
    v: [f64; 3],
    a: [f64; 3],
}

impl Frame {
    pub(super) fn new(o: Point3, axis: Option<[f64; 3]>) -> Self {
        let Some(a) = axis else {
            return Self { o, u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0], a: [0.0, 0.0, 1.0] };
        };
        let e = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = norm(cross(a, e));
        let v = cross(a, u);
        Self { o, u, v, a }
    }

    pub(super) fn local(&self, p: Point3) -> [f64; 3] {
        let d = [p.x - self.o.x, p.y - self.o.y, p.z - self.o.z];
        [dot(d, self.u), dot(d, self.v), dot(d, self.a)]
    }

    pub(super) fn tri(&self, t: &Triangle) -> [[f64; 3]; 3] {
        [self.local(t.a), self.local(t.b), self.local(t.c)]
    }

    /// The world point `h` up the axis from the tip.
    pub(super) fn up(&self, h: f64) -> Point3 {
        Point3::new(self.o.x + self.a[0] * h, self.o.y + self.a[1] * h, self.o.z + self.a[2] * h)
    }

    /// The world displacement of local displacement `d`.
    pub(super) fn end(&self, d: [f64; 3]) -> Point3 {
        let c = |k: usize| self.u[k] * d[0] + self.v[k] * d[1] + self.a[k] * d[2];
        Point3::new(c(0), c(1), c(2))
    }

    pub(super) fn is_upright(&self) -> bool {
        self.a == [0.0, 0.0, 1.0]
    }
}

fn cross(p: [f64; 3], q: [f64; 3]) -> [f64; 3] {
    [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]]
}

fn dot(p: [f64; 3], q: [f64; 3]) -> f64 {
    p[0] * q[0] + p[1] * q[1] + p[2] * q[2]
}

fn norm(p: [f64; 3]) -> [f64; 3] {
    let l = dot(p, p).sqrt();
    if l > 0.0 { [p[0] / l, p[1] / l, p[2] / l] } else { [1.0, 0.0, 0.0] }
}
