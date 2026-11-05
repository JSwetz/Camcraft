//! The truck reference impl of the kernel seam (feature `truck`, bench only):
//! sweep-built primitives, boolean subtract via truck-shapeops `and` with the
//! tool inverted, measure via truck-meshalgo tessellation. truck has no solid
//! offset, so `offset` is `Unsupported`.
//! Provenance: calls truck's public API as shown in its published examples
//! (truck-shapeops `punched-cube-shapeops`, truck-meshalgo rustdoc); no truck
//! internals reimplemented.

use std::panic::{AssertUnwindSafe, catch_unwind};

use truck_meshalgo::tessellation::{MeshedShape, RobustMeshableShape};
use truck_modeling::{Point3, Rad, Solid, Vector3, builder};

use crate::{
    Kernel, KernelError, Measure, P3, finite_point, mesh_signed_volume, positive_len,
};

const NAME: &str = "truck";

/// truck behind the seam. truck is the CURRENT engine, not the one on trial:
/// its known panics (`and` unwraps an empty boundary list, tessellation
/// asserts on its tolerance) are contained the way `app::geometry::import::
/// step` already contains them - input validation first, `catch_unwind` as
/// the backstop. That posture is NOT extended to solidcore, which must not
/// panic at all.
pub struct TruckKernel {
    /// Tolerance handed to truck-shapeops for intersection-curve fitting.
    pub boolean_tol: f64,
    /// Chord tolerance of the tessellation `measure` integrates.
    pub mesh_tol: f64,
}

impl Default for TruckKernel {
    fn default() -> Self {
        Self {
            boolean_tol: 0.01,
            mesh_tol: 0.001,
        }
    }
}

fn contained<T>(op: &'static str, f: impl FnOnce() -> Option<T>) -> Result<T, KernelError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(KernelError::Failed {
            kernel: NAME,
            op,
            detail: "engine returned no result".into(),
        }),
        Err(_) => Err(KernelError::Failed {
            kernel: NAME,
            op,
            detail: "engine panicked (contained)".into(),
        }),
    }
}

impl Kernel for TruckKernel {
    type Solid = Solid;

    fn name(&self) -> &'static str {
        NAME
    }

    fn make_box(&mut self, origin: P3, size: P3) -> Result<Solid, KernelError> {
        finite_point("make_box", origin)?;
        for (axis, v) in ["x", "y", "z"].iter().zip(size) {
            positive_len("make_box", axis, v)?;
        }
        contained("make_box", || {
            let v = builder::vertex(Point3::new(origin[0], origin[1], origin[2]));
            let e = builder::tsweep(&v, Vector3::unit_x() * size[0]);
            let f = builder::tsweep(&e, Vector3::unit_y() * size[1]);
            Some(builder::tsweep(&f, Vector3::unit_z() * size[2]))
        })
    }

    fn make_cylinder(&mut self, base: P3, radius: f64, height: f64) -> Result<Solid, KernelError> {
        finite_point("make_cylinder", base)?;
        positive_len("make_cylinder", "radius", radius)?;
        positive_len("make_cylinder", "height", height)?;
        contained("make_cylinder", || {
            let c = Point3::new(base[0], base[1], base[2]);
            let v = builder::vertex(Point3::new(base[0] + radius, base[1], base[2]));
            // An angle past 2*pi closes the swept wire (truck's convention).
            let w = builder::rsweep(&v, c, Vector3::unit_z(), Rad(7.0));
            let f = builder::try_attach_plane(&[w]).ok()?;
            Some(builder::tsweep(&f, Vector3::unit_z() * height))
        })
    }

    fn boolean_subtract(&mut self, a: Solid, mut b: Solid) -> Result<Solid, KernelError> {
        let tol = self.boolean_tol;
        let r = contained("boolean_subtract", || {
            b.not();
            truck_shapeops::and(&a, &b, tol)
        })?;
        if r.boundaries().is_empty() {
            return Err(KernelError::InvalidResult {
                kernel: NAME,
                op: "boolean_subtract",
                detail: "empty result".into(),
            });
        }
        Ok(r)
    }

    fn offset(&mut self, _s: &mut Solid, _distance: f64) -> Result<Solid, KernelError> {
        Err(KernelError::Unsupported {
            kernel: NAME,
            op: "offset",
        })
    }

    fn measure(&mut self, s: &mut Solid) -> Result<Measure, KernelError> {
        let tol = self.mesh_tol;
        let mesh = contained("measure", || Some(s.robust_triangulation(tol).to_polygon()))?;
        let positions: Vec<P3> = mesh.positions().iter().map(|p| [p.x, p.y, p.z]).collect();
        let mut tris: Vec<[usize; 3]> = Vec::new();
        let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for face in mesh.face_iter() {
            for v in face {
                let q = positions.get(v.pos).ok_or_else(|| KernelError::InvalidResult {
                    kernel: NAME,
                    op: "measure",
                    detail: "mesh index out of range".into(),
                })?;
                for i in 0..3 {
                    min[i] = min[i].min(q[i]);
                    max[i] = max[i].max(q[i]);
                }
            }
            if let Some(first) = face.first() {
                for pair in face.get(1..).unwrap_or_default().windows(2) {
                    if let [b, c] = pair {
                        tris.push([first.pos, b.pos, c.pos]);
                    }
                }
            }
        }
        let volume = mesh_signed_volume(&positions, &tris).ok_or_else(|| {
            KernelError::InvalidResult {
                kernel: NAME,
                op: "measure",
                detail: "non-finite or corrupt tessellation".into(),
            }
        })?;
        if tris.is_empty() || !min.iter().chain(&max).all(|c| c.is_finite()) {
            return Err(KernelError::InvalidResult {
                kernel: NAME,
                op: "measure",
                detail: "empty tessellation".into(),
            });
        }
        Ok(Measure { volume, min, max })
    }
}
