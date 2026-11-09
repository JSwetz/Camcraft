//! The file mirror of a tool's modelled body (#368, project v58): the
//! placed facets (tool frame) and the placement they landed with. The
//! envelope is never written - it is re-derived from the facets on
//! load, so file and judgement cannot disagree, and a body the build's
//! envelope law refuses refuses the file by name. A drawn spun holder
//! (#93) also writes its outline, which is re-drawn on load. Our own
//! format.

use serde::{Deserialize, Serialize};

use super::{dto_to_facets, facets_to_dto};
use crate::schema::ToolBody;
use camcraft_tools::spun::{SpunPoint, SpunProfile};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolBodyDto {
    pub source: String,
    pub axis: [f64; 3],
    pub offset: f64,
    /// Placed facets, tool frame (tip at the origin, axis +Z).
    pub facets: Vec<[[f64; 3]; 3]>,
    /// #93, additive (no version step): the drawn outline of a spun
    /// holder profile, `[z, r, arc]` per point. When present it is the
    /// truth - the body is re-drawn from it and the facets (its revolve,
    /// written too) are not read. A reader without it judges those
    /// facets, whose ring vertices sit on the conservative outline, so
    /// its envelope is never smaller than the drawn solid - dropping the
    /// key can only over-report, never under-read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spun: Option<Vec<[f64; 3]>>,
}

impl ToolBodyDto {
    #[must_use]
    pub fn from_body(b: &ToolBody) -> Self {
        Self {
            source: b.source.clone(),
            axis: b.axis,
            offset: b.offset,
            facets: facets_to_dto(&b.facets),
            spun: b.spun.as_ref().map(|s| s.points().iter().map(|p| [p.z, p.r, p.arc]).collect()),
        }
    }

    /// The body, its envelope re-derived; a refusal names the tool.
    pub fn to_body(&self, tool: &str) -> Result<ToolBody, String> {
        if let Some(pts) = &self.spun {
            let points = pts.iter().map(|p| SpunPoint { z: p[0], r: p[1], arc: p[2] }).collect();
            let spun = SpunProfile::new(points).map_err(|e| format!("tool '{tool}': holder profile: {e}"))?;
            return Ok(crate::tool_body::build_spun(spun));
        }
        crate::tool_body::build(&self.source, self.axis, self.offset, dto_to_facets(self.facets.clone()))
            .map_err(|e| format!("tool '{tool}': {e}"))
    }
}
