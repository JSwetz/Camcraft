//! Parametric stock: the raw material solid a setup starts from.
//!
//! Provenance: our own domain model. Box and cylinder stock are defined
//! XY-centered with z in [0, height] - the "stock sits on the table"
//! placement convention described in published CAM documentation, written
//! from general machining knowledge only.
//!
//! The shape is stored as PARAMETERS: the same meshing rule (`stock_mesh`)
//! regenerates identical facets in any build, so project files stay small and
//! undo stays exact without snapshotting whole meshes. Where the solid lives
//! is the entity's `Location`; what it is lives here. Pure module: no flecs,
//! no I/O - the commands layer resolves sources and validates.

use crate::geometry::{Aabb, Point3, Triangle, cube, cylinder};

/// Segment count for cylinder stock: smooth enough to read as round in the
/// viewport, cheap enough to re-mesh on every parameter edit.
pub const CYLINDER_SEGMENTS: u32 = 32;

/// Stock larger than a real machine table by three orders of magnitude is a
/// typo, not a setup. Shared by the command layer and the project loader.
pub const MAX_STOCK_DIM: f64 = 1_000_000.0;

/// The stock solid.
#[derive(Clone, Debug, PartialEq)]
pub enum StockShape {
    /// Full extents in x/y/z; centered in XY, z from 0 to dz.
    Box { dx: f64, dy: f64, dz: f64 },
    Cylinder { diameter: f64, height: f64 },
    /// A copy of another entity's facets as solid stock ("stock from model").
    /// The mesh comes from the world, so `stock_mesh` cannot resolve it; the
    /// command layer clones it at construction.
    FromModel { source: String },
}

impl StockShape {
    /// All dimensions finite and strictly positive (or, for FromModel, a
    /// non-empty source name). The gate commands use before meshing.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        match self {
            StockShape::Box { dx, dy, dz } => positive(*dx) && positive(*dy) && positive(*dz),
            StockShape::Cylinder { diameter, height } => positive(*diameter) && positive(*height),
            StockShape::FromModel { source } => !source.is_empty(),
        }
    }

    /// No dimension above `MAX_STOCK_DIM`; vacuously true for FromModel
    /// (whose geometry came from an already-imported model).
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        match self {
            StockShape::Box { dx, dy, dz } => {
                *dx <= MAX_STOCK_DIM && *dy <= MAX_STOCK_DIM && *dz <= MAX_STOCK_DIM
            }
            StockShape::Cylinder { diameter, height } => {
                *diameter <= MAX_STOCK_DIM && *height <= MAX_STOCK_DIM
            }
            StockShape::FromModel { .. } => true,
        }
    }
}

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

/// Facets of the stock in its local frame; None for FromModel (its mesh is
/// the source entity's facets) and for degenerate dimensions.
#[must_use]
pub fn stock_mesh(shape: &StockShape) -> Option<Vec<Triangle>> {
    if !shape.is_sane() {
        return None;
    }
    match shape {
        StockShape::Box { dx, dy, dz } => Some(
            cube(
                Point3::new(-dx * 0.5, -dy * 0.5, 0.0),
                Point3::new(dx * 0.5, dy * 0.5, *dz),
            )
            .facets
            .to_vec(),
        ),
        StockShape::Cylinder { diameter, height } => {
            Some(cylinder(*diameter, *height, CYLINDER_SEGMENTS).facets.to_vec())
        }
        StockShape::FromModel { .. } => None,
    }
}

/// Shared guards and measurements for fitting stock to a model set: finite
/// non-negative margin, finite bounds, positive extents once the margin is
/// applied. Returns the padded dimensions and the placement that centers the
/// footprint in XY with the bottom at `min.z - margin`. An invisible stock is
/// worse than no stock, so degenerate input yields `None`.
fn fit_measures(bounds: &Aabb, margin: f64) -> Option<([f64; 3], Point3)> {
    if !margin.is_finite() || margin < 0.0 {
        return None;
    }
    let (min, max) = (bounds.min, bounds.max);
    if !(min.is_finite() && max.is_finite()) {
        return None;
    }
    let dims = [
        (max.x - min.x) + 2.0 * margin,
        (max.y - min.y) + 2.0 * margin,
        (max.z - min.z) + 2.0 * margin,
    ];
    if !(positive(dims[0]) && positive(dims[1]) && positive(dims[2])) {
        return None;
    }
    Some((
        dims,
        Point3::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5, min.z - margin),
    ))
}

/// Box stock that wraps `bounds` on every side by `margin`.
#[must_use]
pub fn fit_box(bounds: &Aabb, margin: f64) -> Option<(StockShape, Point3)> {
    let ([dx, dy, dz], pos) = fit_measures(bounds, margin)?;
    Some((StockShape::Box { dx, dy, dz }, pos))
}

/// Cylinder stock that wraps `bounds`: the diameter is the diagonal of the
/// margin-padded footprint (a circle contains the whole model, which an
/// extent-width one would not), the height spans z plus margins. Written
/// from general machining knowledge, same as `fit_box`.
#[must_use]
pub fn fit_cylinder(bounds: &Aabb, margin: f64) -> Option<(StockShape, Point3)> {
    let ([wx, wy, wz], pos) = fit_measures(bounds, margin)?;
    // Finite positive inputs keep hypot finite (no overflow at real scale).
    let diameter = wx.hypot(wy);
    if !diameter.is_finite() {
        return None;
    }
    Some((StockShape::Cylinder {
        diameter,
        height: wz,
    }, pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Mesh;

    #[test]
    fn box_mesh_is_xy_centered_and_starts_at_z_zero() {
        let shape = StockShape::Box {
            dx: 10.0,
            dy: 10.0,
            dz: 5.0,
        };
        let m = Mesh::from_facets(stock_mesh(&shape).expect("sane box"));
        let b = m.bounds().expect("facets");
        assert!(m.all_finite());
        assert_eq!((b.min.x, b.min.y, b.min.z), (-5.0, -5.0, 0.0));
        assert_eq!((b.max.x, b.max.y, b.max.z), (5.0, 5.0, 5.0));
    }

    #[test]
    fn cylinder_mesh_respects_the_local_convention() {
        let m = Mesh::from_facets(
            stock_mesh(&StockShape::Cylinder {
                diameter: 20.0,
                height: 6.0,
            })
            .expect("sane cylinder"),
        );
        let b = m.bounds().expect("facets");
        assert_eq!(b.min.z, 0.0);
        assert!(b.max.x == 10.0 && b.max.y > 9.9, "{b:?}");
        assert!(m.all_finite());
    }

    #[test]
    fn degenerate_shapes_have_no_mesh_and_are_not_sane() {
        assert!(!StockShape::Box {
            dx: f64::NAN,
            dy: 1.0,
            dz: 1.0
        }
        .is_sane());
        assert!(stock_mesh(&StockShape::Box {
            dx: f64::NAN,
            dy: 1.0,
            dz: 1.0
        })
        .is_none());
        assert!(stock_mesh(&StockShape::Cylinder {
            diameter: -2.0,
            height: 1.0
        })
        .is_none());
        assert!(stock_mesh(&StockShape::FromModel {
            source: "model".into()
        })
        .is_none());
        assert!(!StockShape::FromModel {
            source: String::new()
        }
        .is_sane());
    }

    #[test]
    fn absurd_dimensions_are_unbounded() {
        assert!(
            !StockShape::Box {
                dx: 1.0e7,
                dy: 1.0,
                dz: 1.0
            }
            .is_bounded()
        );
        assert!(!StockShape::Cylinder {
            diameter: f64::INFINITY,
            height: 1.0
        }
        .is_bounded());
        assert!(
            StockShape::Box {
                dx: MAX_STOCK_DIM,
                dy: 1.0,
                dz: 1.0
            }
            .is_bounded()
        );
        assert!(
            StockShape::FromModel {
                source: "m".into()
            }
            .is_bounded()
        );
    }

    #[test]
    fn stock_mesh_is_deterministic() {
        // Undo/redo exactness rests on this: same params, same facets.
        let shape = StockShape::Cylinder {
            diameter: 25.4,
            height: 8.0,
        };
        assert_eq!(stock_mesh(&shape), stock_mesh(&shape));
    }

    #[test]
    fn fit_box_wraps_with_margin() {
        let b = Aabb {
            min: Point3::new(0.0, 0.0, 0.0),
            max: Point3::new(10.0, 4.0, 2.0),
        };
        let (shape, pos) = fit_box(&b, 1.0).expect("fits");
        assert_eq!(
            shape,
            StockShape::Box {
                dx: 12.0,
                dy: 6.0,
                dz: 4.0
            }
        );
        assert_eq!((pos.x, pos.y, pos.z), (5.0, 2.0, -1.0));
        // The placed box must actually contain the model with the margin.
        let m = Mesh::from_facets(stock_mesh(&shape).expect("sane"));
        let local = m.bounds().expect("facets");
        let (lo, hi) = (
            Point3::new(local.min.x + pos.x, local.min.y + pos.y, local.min.z + pos.z),
            Point3::new(local.max.x + pos.x, local.max.y + pos.y, local.max.z + pos.z),
        );
        assert!(
            lo.x <= -0.99 && hi.x >= 11.0 && lo.z <= -0.99 && hi.z >= 2.99,
            "lo {lo:?} hi {hi:?}"
        );
    }

    #[test]
    fn fit_cylinder_contains_the_whole_footprint() {
        // A long thin rectangle: an extent-width cylinder would clip the
        // corners, so the fitted diameter must reach the diagonal.
        let b = Aabb {
            min: Point3::new(-10.0, -1.0, 1.0),
            max: Point3::new(10.0, 1.0, 3.0),
        };
        let (shape, pos) = fit_cylinder(&b, 1.0).expect("fits");
        let StockShape::Cylinder { diameter, height } = shape else {
            panic!("cylinder fit yields a cylinder");
        };
        assert!((height - 4.0).abs() < 1e-12, "{height}");
        assert!((diameter - (22.0f64.hypot(4.0))).abs() < 1e-9, "{diameter}");
        assert!((pos.x).abs() < 1e-12 && pos.y.abs() < 1e-12 && (pos.z - 0.0).abs() < 1e-12, "{pos:?}");
        // The padded rectangle's corners must lie inside the circle.
        let r = diameter * 0.5;
        assert!((11.0f64.hypot(2.0)) <= r + 1e-9);
        // And the mesh honors the convention: XY-centered, z in [0, h].
        let m = Mesh::from_facets(stock_mesh(&shape).expect("sane"));
        let local = m.bounds().expect("facets");
        assert_eq!(local.min.z, 0.0);
        assert!((local.max.z - height).abs() < 1e-9);
    }

    #[test]
    fn fit_cylinder_shares_the_box_guards() {
        let flat = Aabb {
            min: Point3::ZERO,
            max: Point3::new(10.0, 10.0, 0.0),
        };
        assert!(fit_cylinder(&flat, 0.0).is_none());
        assert!(fit_cylinder(&flat, 1.0).is_some());
        assert!(fit_cylinder(
            &Aabb {
                min: Point3::ZERO,
                max: Point3::new(1.0, 1.0, f64::NAN),
            },
            1.0
        )
        .is_none());
        assert!(
            fit_cylinder(
                &Aabb {
                    min: Point3::ZERO,
                    max: Point3::new(1.0, 1.0, 1.0),
                },
                -0.5
            )
            .is_none()
        );
    }

    #[test]
    fn fit_box_refuses_flat_models_and_bad_margins() {
        let flat = Aabb {
            min: Point3::ZERO,
            max: Point3::new(10.0, 10.0, 0.0), // zero-thickness, zero margin
        };
        assert!(fit_box(&flat, 0.0).is_none());
        assert!(fit_box(&flat, 1.0).is_some(), "margin rescues flatness");
        let nan = Aabb {
            min: Point3::ZERO,
            max: Point3::new(1.0, 1.0, f64::NAN),
        };
        assert!(fit_box(&nan, 1.0).is_none());
        assert!(fit_box(&Aabb {
            min: Point3::ZERO,
            max: Point3::new(1.0, 1.0, 1.0)
        }, f64::NAN)
        .is_none());
        assert!(fit_box(
            &Aabb {
                min: Point3::ZERO,
                max: Point3::new(1.0, 1.0, 1.0)
            },
            -0.5
        )
        .is_none());
    }
}
