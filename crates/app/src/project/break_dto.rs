//! The cached edge break (#303, `crate::cl::EdgeBreak`) in the project
//! file. Our own format; the additive law of the comps, threads and
//! probes keys: absent is none - every path but a chamfer's or a
//! deburr's, and every cache before the record existed (such a cached
//! chamfer is judged against the sharp model until it regenerates -
//! stricter, never looser).

use serde::{Deserialize, Serialize};

use crate::cl::{lattice, EdgeBreak};
use crate::geometry::Point3;

/// One cached edge break: the edge polyline on the micron lattice, the
/// bevel's depth at the edge and its slope.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EdgeBreakDto {
    pub edge: Vec<[f64; 3]>,
    pub depth: f64,
    pub slope: f64,
}

impl EdgeBreakDto {
    pub(super) fn from_break(b: &EdgeBreak) -> Self {
        EdgeBreakDto {
            edge: b.edge.iter().map(|p| [lattice(p.x), lattice(p.y), lattice(p.z)]).collect(),
            depth: b.depth,
            slope: b.slope,
        }
    }
}

/// The breaks of a loaded path; a record that fails the sanity door is
/// dropped (its columns are then judged against the sharp model).
pub(super) fn load(dtos: &[EdgeBreakDto]) -> Vec<EdgeBreak> {
    dtos.iter()
        .map(|d| EdgeBreak {
            edge: d.edge.iter().map(|p| Point3::new(p[0], p[1], p[2])).collect(),
            depth: d.depth,
            slope: d.slope,
        })
        .filter(EdgeBreak::is_sane)
        .collect()
}
