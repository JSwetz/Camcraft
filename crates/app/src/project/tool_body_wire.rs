//! The file mirror of a tool's modelled body (#368, project v58): the
//! placed facets (tool frame) and the placement they landed with. The
//! envelope is never written - it is re-derived from the facets on
//! load, so file and judgement cannot disagree, and a body the build's
//! envelope law refuses refuses the file by name. Our own format.
//!
//! A lathe body (#369) writes its turning-frame placement under
//! `lathe` and leaves `axis` / `offset` out: a reader that predates it
//! then refuses the file (a missing field) rather than reading
//! lathe-placed facets as a spindle body. Its clearance bands are
//! re-derived on load, like the envelope.

use serde::{Deserialize, Serialize};

use super::{dto_to_facets, facets_to_dto};
use crate::schema::ToolBody;
use crate::tool_body::BodyFrame;
use camcraft_tools::lathe_holder::LathePlacement;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolBodyDto {
    pub source: String,
    /// A milling body's placement; absent on a lathe body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    /// #369: a lathe body's placement; absent on a milling body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lathe: Option<LathePlacementDto>,
    /// Placed facets, tool frame (milling: tip at the origin, axis +Z;
    /// lathe: tip at the origin, +x radial, +z along the spindle).
    pub facets: Vec<[[f64; 3]; 3]>,
}

/// The turning-frame placement on file (#369).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LathePlacementDto {
    pub radial: [f64; 3],
    pub along: [f64; 3],
    pub tip: [f64; 3],
}

impl ToolBodyDto {
    #[must_use]
    pub fn from_body(b: &ToolBody) -> Self {
        let facets = facets_to_dto(&b.facets);
        match &b.frame {
            BodyFrame::Spindle { axis, offset, .. } => {
                Self { source: b.source.clone(), axis: Some(*axis), offset: Some(*offset), lathe: None, facets }
            }
            BodyFrame::Lathe { place, .. } => Self {
                source: b.source.clone(),
                axis: None,
                offset: None,
                lathe: Some(LathePlacementDto { radial: place.radial, along: place.along, tip: place.tip }),
                facets,
            },
        }
    }

    /// The body, its envelope or clearance re-derived; a refusal names
    /// the tool.
    pub fn to_body(&self, tool: &str) -> Result<ToolBody, String> {
        let facets = dto_to_facets(self.facets.clone());
        let built = match (self.axis, self.offset, self.lathe) {
            (Some(axis), Some(offset), None) => crate::tool_body::build(&self.source, axis, offset, facets),
            (None, None, Some(l)) => {
                crate::tool_body_lathe::build(&self.source, LathePlacement { radial: l.radial, along: l.along, tip: l.tip }, facets)
            }
            _ => Err("the tool body states neither one spindle placement (axis and offset) nor one lathe placement".to_owned()),
        };
        built.map_err(|e| format!("tool '{tool}': {e}"))
    }
}
