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
//!
//! A lathe body's insert zone (#396) is an optional `insert_zone` key
//! beside `lathe`, absent when there is none, so a zoneless body's bytes
//! are unchanged and no version rung is spent: a reader that predates
//! it skips the unknown key (serde's default) and judges the whole body,
//! plate and all - an over-report that blocks the post, never one that
//! passes a rub.
//!
//! A drawn spun holder (#93) also writes its outline under `spun`, which
//! is re-drawn on load (a spindle-frame body; never beside `lathe`).

use serde::{Deserialize, Serialize};

use super::{dto_to_facets, facets_to_dto};
use crate::schema::ToolBody;
use camcraft_tools::spun::{SpunPoint, SpunProfile};
use crate::tool_body::BodyFrame;
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

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
    /// #396: a lathe body's insert zone (tool frame); absent when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_zone: Option<InsertZoneDto>,
    /// Placed facets, tool frame (milling: tip at the origin, axis +Z;
    /// lathe: tip at the origin, +x radial, +z along the spindle).
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

/// The turning-frame placement on file (#369).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LathePlacementDto {
    pub radial: [f64; 3],
    pub along: [f64; 3],
    pub tip: [f64; 3],
}

/// The insert zone on file (#396): the box's corners, tool frame.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct InsertZoneDto {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

impl ToolBodyDto {
    #[must_use]
    pub fn from_body(b: &ToolBody) -> Self {
        let facets = facets_to_dto(&b.facets);
        let spun = b.spun.as_ref().map(|s| s.points().iter().map(|p| [p.z, p.r, p.arc]).collect());
        match &b.frame {
            BodyFrame::Spindle { axis, offset, .. } => Self {
                source: b.source.clone(),
                axis: Some(*axis),
                offset: Some(*offset),
                lathe: None,
                insert_zone: None,
                facets,
                spun,
            },
            BodyFrame::Lathe { place, zone, .. } => Self {
                source: b.source.clone(),
                axis: None,
                offset: None,
                lathe: Some(LathePlacementDto { radial: place.radial, along: place.along, tip: place.tip }),
                insert_zone: zone.map(|z| InsertZoneDto { lo: z.lo, hi: z.hi }),
                facets,
                spun: None,
            },
        }
    }

    /// The body, its envelope or clearance re-derived; a refusal names
    /// the tool.
    pub fn to_body(&self, tool: &str) -> Result<ToolBody, String> {
        if let Some(pts) = &self.spun {
            if self.lathe.is_some() || self.insert_zone.is_some() {
                return Err(format!("tool '{tool}': a drawn holder profile is a spindle body, not a lathe one"));
            }
            let points = pts.iter().map(|p| SpunPoint { z: p[0], r: p[1], arc: p[2] }).collect();
            let spun = SpunProfile::new(points).map_err(|e| format!("tool '{tool}': holder profile: {e}"))?;
            return Ok(crate::tool_body::build_spun(spun));
        }
        let facets = dto_to_facets(self.facets.clone());
        let zone = self.insert_zone.map(|z| InsertZone { lo: z.lo, hi: z.hi });
        let built = match (self.axis, self.offset, self.lathe) {
            (Some(_), Some(_), None) if zone.is_some() => Err("an insert zone needs a lathe placement, not a spindle one".to_owned()),
            (Some(axis), Some(offset), None) => crate::tool_body::build(&self.source, axis, offset, facets),
            (None, None, Some(l)) => {
                crate::tool_body_lathe::build(&self.source, LathePlacement { radial: l.radial, along: l.along, tip: l.tip }, zone, facets)
            }
            _ => Err("the tool body states neither one spindle placement (axis and offset) nor one lathe placement".to_owned()),
        };
        built.map_err(|e| format!("tool '{tool}': {e}"))
    }
}
