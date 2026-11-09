//! A CPU software rasteriser (#182): flat-shaded triangles through a
//! depth buffer and plain line segments, under a fixed orthographic
//! camera, into an RGB image and its PNG bytes - so a headless session
//! (the MCP server, CI) can see the simulated stock with no GPU.
//!
//! Provenance: the textbook scanline-free triangle fill by edge
//! functions over pixel centers (Pineda, "A Parallel Algorithm for
//! Polygon Rasterization", SIGGRAPH 1988), a z-buffer (Catmull 1974),
//! Lambert shading, and a DDA line walk; the PNG container through the
//! `png` crate (W3C PNG specification). Our own code.
//!
//! ## The laws here
//!
//! - **Deterministic.** One thread, a fixed camera fitted to the scene's
//!   own bounds, no clocks, no hashing order: the same scene is the same
//!   bytes, every run, every machine with IEEE-754 doubles.
//! - **Degenerate input is skipped, never a panic.** A non-finite vertex,
//!   a zero-area projection, a segment off the canvas: drawn as nothing.
//!   Every pixel index is bounds-checked; loops are capped by the canvas.

use crate::geometry::Point3;

/// The smallest and largest image edge accepted, in pixels.
pub const MIN_EDGE: u32 = 16;
pub const MAX_EDGE: u32 = 2048;

/// Where the camera looks from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewDir {
    /// From front-right-above (+X, -Y, +Z): the CAM isometric.
    Iso,
    /// Straight down the -Z axis, +X right, +Y up.
    Top,
}

/// An orthographic camera fitted to a set of points.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    right: Point3,
    up: Point3,
    /// Toward the viewer: larger depth is nearer.
    toward: Point3,
    light: Point3,
    mid_x: f64,
    mid_y: f64,
    scale: f64,
    width: u32,
    height: u32,
}

fn unit(p: Point3) -> Point3 {
    let n = p.dot(p).sqrt();
    if n > 0.0 && n.is_finite() {
        Point3::new(p.x / n, p.y / n, p.z / n)
    } else {
        Point3::new(0.0, 0.0, 1.0)
    }
}

impl Camera {
    /// Fit the view so every finite point of `pts` lands on a
    /// `width` x `height` canvas with a 6 % margin. `None` when there is
    /// no finite point to frame, or the size is outside the accepted edge.
    #[must_use]
    pub fn fit(dir: ViewDir, pts: impl Iterator<Item = Point3>, width: u32, height: u32) -> Option<Self> {
        if !(MIN_EDGE..=MAX_EDGE).contains(&width) || !(MIN_EDGE..=MAX_EDGE).contains(&height) {
            return None;
        }
        let (toward, right, up) = match dir {
            ViewDir::Iso => {
                let t = unit(Point3::new(1.0, -1.0, 1.0));
                let r = unit(Point3::new(0.0, 0.0, 1.0).cross(t));
                (t, r, t.cross(r))
            }
            ViewDir::Top => (Point3::new(0.0, 0.0, 1.0), Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)),
        };
        let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for p in pts.filter(|p| p.is_finite()) {
            let (sx, sy) = (p.dot(right), p.dot(up));
            x0 = x0.min(sx);
            x1 = x1.max(sx);
            y0 = y0.min(sy);
            y1 = y1.max(sy);
        }
        if !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite()) {
            return None;
        }
        let (w, h) = (f64::from(width), f64::from(height));
        let span_x = (x1 - x0).max(1e-9);
        let span_y = (y1 - y0).max(1e-9);
        let scale = ((w * 0.88) / span_x).min((h * 0.88) / span_y);
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        // A key light from above-left of the viewer, fixed per view.
        let light = unit(Point3::new(
            toward.x * 0.6 - right.x * 0.35 + up.x * 0.5,
            toward.y * 0.6 - right.y * 0.35 + up.y * 0.5,
            toward.z * 0.6 - right.z * 0.35 + up.z * 0.5,
        ));
        Some(Self { right, up, toward, light, mid_x: (x0 + x1) * 0.5, mid_y: (y0 + y1) * 0.5, scale, width, height })
    }

    /// Screen x, screen y (pixels, y down) and depth of a world point.
    fn project(&self, p: Point3) -> (f64, f64, f64) {
        let sx = f64::from(self.width) * 0.5 + (p.dot(self.right) - self.mid_x) * self.scale;
        let sy = f64::from(self.height) * 0.5 - (p.dot(self.up) - self.mid_y) * self.scale;
        (sx, sy, p.dot(self.toward))
    }
}

/// An RGB image with a depth buffer.
pub struct Canvas {
    cam: Camera,
    rgb: Vec<u8>,
    depth: Vec<f64>,
}

impl Canvas {
    /// A canvas of the camera's size, filled with `background`.
    #[must_use]
    pub fn new(cam: Camera, background: [u8; 3]) -> Self {
        let n = (cam.width as usize).saturating_mul(cam.height as usize);
        let mut rgb = Vec::with_capacity(n.saturating_mul(3));
        for _ in 0..n {
            rgb.extend_from_slice(&background);
        }
        Self { cam, rgb, depth: vec![f64::NEG_INFINITY; n] }
    }

    fn put(&mut self, x: i64, y: i64, z: f64, c: [u8; 3], test: bool) {
        let (w, h) = (i64::from(self.cam.width), i64::from(self.cam.height));
        if x < 0 || y < 0 || x >= w || y >= h {
            return;
        }
        let Ok(k) = usize::try_from(y * w + x) else { return };
        let Some(d) = self.depth.get_mut(k) else { return };
        if test {
            if z <= *d {
                return;
            }
            *d = z;
        }
        if let Some(px) = self.rgb.get_mut(k * 3..k * 3 + 3) {
            px.copy_from_slice(&c);
        }
    }

    /// Fill one triangle, flat-shaded: `base` lit by the camera's fixed
    /// light through the facet normal (two-sided - a sheet's walls face
    /// either way).
    pub fn triangle(&mut self, a: Point3, b: Point3, c: Point3, base: [u8; 3]) {
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            return;
        }
        let n = unit((b - a).cross(c - a));
        let lit = 0.3 + 0.7 * n.dot(self.cam.light).abs();
        let shade = |v: u8| -> u8 { (f64::from(v) * lit).round().clamp(0.0, 255.0) as u8 };
        let col = [shade(base[0]), shade(base[1]), shade(base[2])];
        let (p0, p1, p2) = (self.cam.project(a), self.cam.project(b), self.cam.project(c));
        let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0);
        if !area.is_finite() || area.abs() < 1e-12 {
            return;
        }
        let (w, h) = (f64::from(self.cam.width), f64::from(self.cam.height));
        let x_lo = p0.0.min(p1.0).min(p2.0).floor().clamp(0.0, w) as i64;
        let x_hi = p0.0.max(p1.0).max(p2.0).ceil().clamp(0.0, w) as i64;
        let y_lo = p0.1.min(p1.1).min(p2.1).floor().clamp(0.0, h) as i64;
        let y_hi = p0.1.max(p1.1).max(p2.1).ceil().clamp(0.0, h) as i64;
        let edge = |p: (f64, f64, f64), q: (f64, f64, f64), x: f64, y: f64| (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
        for y in y_lo..y_hi {
            let py = y as f64 + 0.5;
            for x in x_lo..x_hi {
                let px = x as f64 + 0.5;
                let w0 = edge(p1, p2, px, py) / area;
                let w1 = edge(p2, p0, px, py) / area;
                let w2 = edge(p0, p1, px, py) / area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p0.2 + w1 * p1.2 + w2 * p2.2;
                self.put(x, y, z, col, true);
            }
        }
    }

    /// A one-pixel segment drawn over everything (no depth test): an
    /// overlay the eye must never lose behind the stock.
    pub fn overlay_line(&mut self, a: Point3, b: Point3, c: [u8; 3]) {
        if !(a.is_finite() && b.is_finite()) {
            return;
        }
        let (p, q) = (self.cam.project(a), self.cam.project(b));
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        // Capped at the canvas diagonal's worth of steps: a segment that
        // runs far off-canvas is walked no longer than one that fits.
        let cap = 2.0 * f64::from(self.cam.width + self.cam.height);
        let steps = dx.abs().max(dy.abs()).ceil().clamp(1.0, cap);
        let n = steps as i64;
        for i in 0..=n {
            let t = i as f64 / steps;
            let (x, y) = (p.0 + dx * t, p.1 + dy * t);
            self.put(x.floor() as i64, y.floor() as i64, 0.0, c, false);
        }
    }

    /// The image's RGB bytes, row-major from the top-left.
    #[must_use]
    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }

    /// Pixels whose depth was written by a triangle.
    #[must_use]
    pub fn covered(&self) -> usize {
        self.depth.iter().filter(|d| d.is_finite()).count()
    }

    /// The image as PNG bytes (8-bit RGB, default compression - the
    /// encoder is deterministic for the same input).
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.cam.width, self.cam.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| format!("png header: {e}"))?;
        wr.write_image_data(&self.rgb).map_err(|e| format!("png data: {e}"))?;
        wr.finish().map_err(|e| format!("png finish: {e}"))?;
        Ok(out)
    }
}
