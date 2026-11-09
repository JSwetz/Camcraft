//! Lathe-held tool bodies (#369): a holder or boring-bar mesh placed in
//! the turning tool frame by numbers (the model directions that are
//! radial and along the spindle, the model point that is the tip), the
//! clearance bands the revolved-stock law reads off it, and which tools
//! may carry one. Specced from the handbook turning-tool convention (X
//! radial, Z along the spindle); the frame change is plain linear
//! algebra. Our own design; no vendor file or source consulted.
//!
//! The laws here:
//! - **Lathe steel only.** A turning insert, grooving blade, threading
//!   insert, form gauge or boring bar may carry a lathe body; any other
//!   kind is refused by name, at the command, at `ChangeTool` and on
//!   load (`tool_body::fits`).
//! - **Refusals by name** for a placement that is no frame (a zero or
//!   non-finite direction, directions not square, a wild tip) and a mesh
//!   that is no body (empty, non-finite, degenerate, flat along the
//!   spindle, too large, its tip far from it) -
//!   `camcraft_tools::lathe_holder`.

use std::sync::Arc;

use flecs_ecs::prelude::{Entity, World};

use camcraft_tools::lathe_holder::{LatheClearance, LathePlacement};

use crate::geometry::{Point3, Triangle};
use crate::schema::ToolBody;
use crate::tool_body::{body_of, BodyFrame, LATHE_HELD};
use crate::tools::{Tool, ToolKind};

/// The informational note a turning op carries when its tool has no
/// lathe body: only the cutting footprint was judged.
pub const HOLDER_NOTE: &str = "holder not judged - the turning tool carries no modelled body, so only its cutting footprint was judged against the stock";

/// Place model-frame facets in the turning tool frame.
pub fn place(model: &[Triangle], place: &LathePlacement) -> Result<Vec<Triangle>, String> {
    let tris: Vec<[[f64; 3]; 3]> = model.iter().map(tri).collect();
    let placed = place.place(&tris).map_err(|e| e.to_string())?;
    Ok(placed.iter().map(|t| Triangle::new(pt(t[0]), pt(t[1]), pt(t[2]))).collect())
}

/// A lathe body built from facets already placed in the turning tool
/// frame: its clearance bands derived, refused by name when the mesh is
/// no lathe body.
pub fn build(source: &str, place: LathePlacement, placed: Vec<Triangle>) -> Result<ToolBody, String> {
    place.basis().map_err(|e| e.to_string())?;
    let tris: Vec<[[f64; 3]; 3]> = placed.iter().map(tri).collect();
    let clearance = LatheClearance::of(&tris).map_err(|e| e.to_string())?;
    Ok(ToolBody { source: source.to_owned(), facets: Arc::from(placed), frame: BodyFrame::Lathe { place, clearance: Arc::new(clearance) } })
}

/// Can `tool` carry a lathe body? Only a lathe steel can.
pub fn fits(tool: &Tool) -> Result<(), String> {
    if LATHE_HELD.contains(&tool.kind) || tool.kind == ToolKind::Bore {
        Ok(())
    } else {
        Err(format!("a {} is no lathe steel - a lathe-frame body needs a turning, grooving, threading or form insert or a boring bar", tool.kind))
    }
}

/// The clearance bands of the tool entity's lathe body, if it has one.
#[must_use]
pub fn clearance_of(world: &World, entity: &Entity) -> Option<Arc<LatheClearance>> {
    match body_of(world, entity)?.frame {
        BodyFrame::Lathe { clearance, .. } => Some(clearance),
        BodyFrame::Spindle { .. } => None,
    }
}

fn tri(t: &Triangle) -> [[f64; 3]; 3] {
    [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]]
}

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}
