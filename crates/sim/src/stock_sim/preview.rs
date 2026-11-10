//! The preview march (M7.2 A): seeing the machined volume from the eye.
//!
//! Provenance: sphere tracing a signed-distance function is Hart's law
//! (SIGGRAPH 1996, "Sphere Tracing: A Geometric Method for Distributing
//! Implicit Surfaces") - step along the ray by the returned distance,
//! because the nearest surface can be no closer. The deviation classes
//! are `deviate`'s located-not-counted law read at the HIT POINT
//! instead of at a column top: the target's signed distance IS the
//! deviation, exact and wherever the eye lands (the ROADMAP's
//! continuous color map). Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Steps are lower bounds, so material is never skipped past.**
//!   Every distance this march trusts - the band magnitude (a BFS
//!   graph distance, clamped toward the surface), the cut layer's
//!   negated sweep distance, and the gap to the shell's AABB - bounds
//!   the true clearance from below. A step can be timid; it cannot
//!   land inside material it did not see.
//! - **The verdict march's acceleration, made render-side.** Samples
//!   wear the cached parity of their cell column (one scan per column
//!   per frame, the same `shell_crossings` the verdicts use), and a
//!   hit candidate stands only on proof of material at or behind the
//!   surface (`march`'s hit law). What this can still thread -
//!   silently, at the budget's edge - is a wall thinner than one cell
//!   between the sample and its column center; the field's header
//!   names that wall, and the verdict paths (`march`,
//!   `deviate_field`) answer it exactly. A preview is a picture; the
//!   verdict stays the truth.
//! - **No verdict, no paint.** A ray that cannot finish (budget,
//!   non-finite field answer) or that no body stands behind answers
//!   NaN depth and class 3 - never a zero dressed up as a clean bill
//!   of health (the `color_classes` law, extended to pixels).
//! - **Field arm only.** The column arm's distance is vertical, so a
//!   lateral ray would slip past its walls; the sheet render stays
//!   that arm's preview until M7.3 retires it.
//! - **The CPU fallback is the whole module.** The GPU pass traces
//!   the same laws at speed; when a machine cannot compute-shade,
//!   this runs at low resolution and upsamples - never a black
//!   screen (the Prime Directive's "never crash" includes it).

pub mod eikonal;
pub mod pack;
pub mod patch;

mod march;

pub use eikonal::Renormalized;
pub use pack::{pack_cut, pack_field, pack_target, DistanceLattice, FieldLattice, LatticeMeta};
pub use patch::{pack_cut_patch, CutPatch};

use std::collections::HashMap;

use super::field::SdfField;
use super::kernel::StockError;
use crate::geometry::Point3;
use march::{add, scale, unit, Eye, Seen, STEP_EPS};

/// The class gate, in cells, ADDED to `tol`: a pixel whose hit point
/// sits within the march's own landing slack of the model is named
/// clean, never rest or gouge. Widening the gate only ever moves
/// borderline pixels toward clean - the one direction that cannot
/// accuse a good program or absolve a gouge by a whole cell.
const HIT_GATE: f64 = 0.15;

/// The class gate a frame judges by: `tol` plus the march's own
/// landing slack. One law for both eyes - the GPU pass asks here.
#[must_use]
pub fn gate_tol(tol: f64, cell: f64) -> f64 {
    tol.max(0.0) + cell * HIT_GATE
}

/// The hit threshold at a cell size: one law for both eyes.
#[must_use]
pub fn step_eps(cell: f64) -> f64 {
    cell * STEP_EPS
}

/// Where the eye stands: an orthonormal basis built by `look_at`,
/// which refuses a degenerate frame by name (a camera cannot lie).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewCamera {
    pub eye: Point3,
    pub fwd: Point3,
    pub right: Point3,
    pub up: Point3,
    /// Tangent of the half vertical field of view (> 0, finite).
    pub tan_half_fov: f64,
    /// `Some(h)`: an orthographic eye (#609) - parallel rays along
    /// `fwd`, each starting on the eye plane, the frame `h` world units
    /// tall about `eye`. `None`: the perspective cone from `eye`.
    pub ortho_half_height: Option<f64>,
}

impl PreviewCamera {
    /// A camera at `eye` looking at `target`, levelled by `up_hint`.
    pub fn look_at(eye: Point3, target: Point3, up_hint: Point3, tan_half_fov: f64) -> Result<Self, StockError> {
        let refuse = |s: &str| Err(StockError::Invalid(format!("preview camera: {s}")));
        if !(eye.is_finite() && target.is_finite() && up_hint.is_finite()) {
            return refuse("eye, target and up must be finite");
        }
        if !(tan_half_fov.is_finite() && tan_half_fov > 0.0) {
            return refuse("the field of view must be a finite positive tangent");
        }
        let fwd = match unit(target - eye) {
            Some(v) => v,
            None => return refuse("eye and target coincide"),
        };
        let right = match unit(fwd.cross(up_hint)) {
            Some(v) => v,
            None => return refuse("the up hint is parallel to the view direction"),
        };
        let up = right.cross(fwd); // already unit: fwd, right orthonormal
        Ok(PreviewCamera { eye, fwd, right, up, tan_half_fov, ortho_half_height: None })
    }

    /// The same eye made orthographic (#609): parallel rays over a
    /// frame `half_height` tall (each way) about the eye. Refused by
    /// name unless finite and positive.
    pub fn orthographic(self, half_height: f64) -> Result<Self, StockError> {
        if !(half_height.is_finite() && half_height > 0.0) {
            return Err(StockError::Invalid("preview camera: the orthographic half height must be finite and positive".into()));
        }
        Ok(PreviewCamera { ortho_half_height: Some(half_height), ..self })
    }

    /// The ray through pixel (`px`, `py`) of a `w` x `h` frame (row 0
    /// is the top) as (origin, unit direction): the eye and `ray_dir`
    /// in perspective; in orthographic the pixel's own point on the
    /// eye plane and `fwd` (#609) - the parallel box `Camera::mvp`
    /// draws. The single ray law both eyes and the composite walk.
    pub fn ray(&self, px: u32, py: u32, w: u32, h: u32) -> (Point3, Point3) {
        let Some(hh) = self.ortho_half_height else {
            return (self.eye, self.ray_dir(px, py, w, h));
        };
        let (u, v) = pixel_uv(px, py, w, h);
        let a = hh * (w as f64 / h as f64);
        (add(self.eye, add(scale(self.right, u * a), scale(self.up, v * hh))), self.fwd)
    }

    /// The direction through pixel (`px`, `py`) of a `w` x `h` frame
    /// (row 0 is the top): the single ray law the CPU march and the
    /// GPU parity tests both walk.
    /// In orthographic every ray runs along `fwd` (see `ray`).
    pub fn ray_dir(&self, px: u32, py: u32, w: u32, h: u32) -> Point3 {
        if self.ortho_half_height.is_some() {
            return self.fwd;
        }
        let (u, v) = pixel_uv(px, py, w, h);
        let a = self.tan_half_fov * (w as f64 / h as f64);
        let dir = add(self.fwd, add(scale(self.right, u * a), scale(self.up, v * self.tan_half_fov)));
        unit(dir).unwrap_or(self.fwd) // finite inputs: the norm cannot vanish
    }
}

/// Pixel (`px`, `py`)'s centre in -1..1 frame coordinates (u right,
/// v up; row 0 is the top).
fn pixel_uv(px: u32, py: u32, w: u32, h: u32) -> (f64, f64) {
    let u = 2.0 * ((px as f64 + 0.5) / w as f64) - 1.0;
    let v = 1.0 - 2.0 * ((py as f64 + 0.5) / h as f64);
    (u, v)
}

/// One frame of the machined view. `depth` is the distance along the
/// ray from its `PreviewCamera::ray` origin to the hit (NaN where the
/// march or the world has no answer);
/// `class` is `color_classes`' vocabulary per pixel (0 clean, 1 rest,
/// 2 gouge, 3 unjudged); `shade` is the lambert-times-AO luminance
/// the compositor multiplies into the class color.
#[derive(Clone, Debug)]
pub struct Preview {
    pub w: u32,
    pub h: u32,
    pub depth: Vec<f64>,
    pub class: Vec<f32>,
    pub shade: Vec<f32>,
    /// Pixels whose march could not answer (budget or NaN): the
    /// honesty counter the status line reads, never painted clean.
    pub unresolved: usize,
}

/// Ray-march `stock` (the field AFTER its cuts) from `cam`, classing
/// each hit against `target` (the model's field) within `tol`.
pub fn preview(
    stock: &SdfField,
    target: Option<&SdfField>,
    cam: &PreviewCamera,
    w: u32,
    h: u32,
    tol: f64,
) -> Result<Preview, StockError> {
    let refuse = |s: &str| Err(StockError::Invalid(format!("preview: {s}")));
    if w == 0 || h == 0 {
        return refuse("frame size must be at least 1x1");
    }
    if (w as u64) * (h as u64) > 1 << 22 {
        return refuse("frame exceeds 4M pixels - the fallback wants a thumbnail, not a render farm");
    }
    let cell = stock.cell();
    let b = stock.bounds();
    let pad = stock.cap() + cell;
    let world = (
        Point3::new(b.min.x - pad, b.min.y - pad, b.min.z - pad),
        Point3::new(b.max.x + pad, b.max.y + pad, b.max.z + pad),
    );
    let mut eye = Eye {
        f: stock,
        cols: HashMap::new(),
        solid: (b.min, b.max),
        world,
        cell,
        eps: step_eps(cell),
    };
    // The gate is `tol` plus the march's own landing slack, once for
    // the frame: a borderline pixel is named clean, never accused.
    let tol = gate_tol(tol, cell);
    let n = (w as usize) * (h as usize);
    let mut pv = Preview { w, h, depth: vec![f64::NAN; n], class: vec![3.0; n], shade: vec![0.0; n], unresolved: 0 };
    for py in 0..h {
        for px in 0..w {
            let (origin, dir) = cam.ray(px, py, w, h);
            match march::trace(&mut eye, origin, dir) {
                Seen::Hit { t, p } => {
                    let i = (py as usize) * (w as usize) + px as usize;
                    pv.depth[i] = t;
                    pv.class[i] = deviation(target, p, tol);
                    pv.shade[i] = march::shade(&mut eye, p);
                }
                Seen::Unreadable => pv.unresolved += 1,
                Seen::Empty => {}
            }
        }
    }
    Ok(pv)
}

/// The pixel's deviation class: the TARGET model's EXACT signed
/// distance at the hit point (`true_distance` by magnitude, ray
/// parity by `inside` - the model's field never carries cuts, so its
/// lattice would be the wrong oracle here), gated by `tol` already
/// widened by the march's own landing slack. A model with no facets
/// answers NaN through both oracles and paints 3. Absent body,
/// absent verdict: 3.
fn deviation(target: Option<&SdfField>, p: Point3, tol: f64) -> f32 {
    let Some(t) = target else { return 3.0 };
    if !p.is_finite() {
        return 3.0;
    }
    let d = t.true_distance(p);
    if !d.is_finite() {
        return 3.0;
    }
    let v = if t.inside(p) { -d } else { d };
    if v > tol {
        1.0 // the hit stands outside the model: rest
    } else if v < -tol {
        2.0 // the hit sits under the model's surface: gouge
    } else {
        0.0
    }
}
