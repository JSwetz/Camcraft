//! The file mirror of a tool's modelled body (#368, project v54): the
//! placed facets (tool frame) and the placement they landed with. The
//! envelope is never written - it is re-derived from the facets on
//! load, so file and judgement cannot disagree, and a body the build's
//! envelope law refuses refuses the file by name. Our own format.

use serde::{Deserialize, Serialize};

use super::{dto_to_facets, facets_to_dto};
use crate::schema::ToolBody;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolBodyDto {
    pub source: String,
    pub axis: [f64; 3],
    pub offset: f64,
    /// Placed facets, tool frame (tip at the origin, axis +Z).
    pub facets: Vec<[[f64; 3]; 3]>,
}

impl ToolBodyDto {
    #[must_use]
    pub fn from_body(b: &ToolBody) -> Self {
        Self { source: b.source.clone(), axis: b.axis, offset: b.offset, facets: facets_to_dto(&b.facets) }
    }

    /// The body, its envelope re-derived; a refusal names the tool.
    pub fn to_body(&self, tool: &str) -> Result<ToolBody, String> {
        crate::tool_body::build(&self.source, self.axis, self.offset, dto_to_facets(self.facets.clone()))
            .map_err(|e| format!("tool '{tool}': {e}"))
    }
}
