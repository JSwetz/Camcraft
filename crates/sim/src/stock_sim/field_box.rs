//! The analytic box stock (#633): an axis-aligned block's signed
//! distance in closed form, so a block stock needs no band fill. The
//! BFS of `field_gen::from_mesh` sampled every band cell along the
//! block's walls - which run ten spans deep, the column engine's
//! floorless block - and a 120 mm block at a 0.375 mm cell took ~58 s
//! to build while its cuts took milliseconds. The box's distance is
//! exact and O(1) per cell, so the mesh layer reads it wherever no
//! brick was stored, and `SdfField::block` stores none.
//! Provenance: the textbook signed distance of an axis-aligned box
//! (component-wise exterior distance and inner max); our own design.

use std::collections::HashMap;
use std::sync::Arc;

use super::field::{center_of, SdfField};
use super::kernel::StockError;
use crate::geometry::Point3;

/// An axis-aligned box `[lo, hi]`, read as an unsigned distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnalyticBox {
    pub lo: Point3,
    pub hi: Point3,
}

impl AnalyticBox {
    /// The exact unsigned distance from `p` to the box's surface:
    /// outside, the Euclidean distance to the box; inside, the
    /// distance to the nearest face.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        let (lo, hi) = ([self.lo.x, self.lo.y, self.lo.z], [self.hi.x, self.hi.y, self.hi.z]);
        let q = [p.x, p.y, p.z];
        let mut out2 = 0.0f64;
        let mut inner = f64::INFINITY;
        for a in 0..3 {
            let below = lo[a] - q[a];
            let above = q[a] - hi[a];
            let d = below.max(above);
            if d > 0.0 {
                out2 += d * d;
            }
            inner = inner.min(-d); // depth inside along this axis
        }
        if out2 > 0.0 { out2.sqrt() } else { inner.max(0.0) }
    }

    /// The distance at lattice cell `key`'s centre.
    #[must_use]
    pub fn at_cell(&self, key: [i32; 3], cell: f64) -> f64 {
        self.distance(Point3::new(center_of(key[0], cell), center_of(key[1], cell), center_of(key[2], cell)))
    }
}

impl SdfField {
    /// A block stock's field (#633): the box `[lo, hi]` as its shell
    /// (the parity and bounds source, as for any field) with its band
    /// magnitudes read in closed form - no brick is stored at build.
    pub fn from_box(lo: Point3, hi: Point3, cell: f64, band: u32) -> Result<SdfField, StockError> {
        let m = super::field_gen::box_shell(lo, hi, cell);
        super::field::validate_shell(&m, cell, band)?;
        let mut field = SdfField {
            cell,
            cap: band as f64 * cell,
            mesh: Arc::new(m),
            bricks: Arc::new(HashMap::new()),
            cuts: HashMap::new(),
            cut_ups: HashMap::new(),
            cost: Box::default(),
            analytic: Some(AnalyticBox { lo, hi }),
        };
        field.cost.region = super::field_batch::region_of(&field);
        Ok(field)
    }
}
