//! The cached probing touch (#108, `crate::cl::ProbeTouch`) in the
//! project file. Our own format; the additive law of the comps and
//! threads keys: absent is none - every cutting path, and every cache
//! before the record existed.

use serde::{Deserialize, Serialize};

use crate::cl::{lattice, Move, ProbeTouch};
use crate::geometry::Point3;

/// One cached probing touch: the move index, the measurement ordinal
/// (absent for a seek) and the probe target on the micron lattice.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProbeTouchDto {
    pub index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<u32>,
    pub target: [f64; 3],
}

impl ProbeTouchDto {
    pub(super) fn from_touch(p: &ProbeTouch) -> Self {
        ProbeTouchDto {
            index: p.index as u64,
            measure: p.measure,
            target: [lattice(p.target.x), lattice(p.target.y), lattice(p.target.z)],
        }
    }
}

/// The touches of a loaded path. A touch that does not sit on its move
/// is not trusted; a path that loses any touch keeps none, and the post
/// refuses a path whose probe op states touches it lost (`posting`) -
/// a touch lost on load must never post as a plain feed.
pub(super) fn load(dtos: &[ProbeTouchDto], moves: &[Move]) -> Vec<ProbeTouch> {
    let touches: Vec<ProbeTouch> = dtos
        .iter()
        .filter_map(|p| {
            Some(ProbeTouch {
                index: usize::try_from(p.index).ok()?,
                measure: p.measure,
                target: Point3::new(p.target[0], p.target[1], p.target[2]),
            })
        })
        .collect();
    if touches.len() == dtos.len() && touches.iter().all(|t| t.is_sane(moves)) {
        touches
    } else {
        Vec::new()
    }
}
