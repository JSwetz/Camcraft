//! The stock a later carried setup's verdicts judge against (#319): when
//! the body machined in earlier setups is carried into this one (#317),
//! the rapid, entry, reach and linking laws must never see less material
//! than really stands - so where the carried body stands outside the
//! setup's DECLARED stock, the job's stock becomes the box enclosing both.
//!
//! Provenance: our own design per #308's research decision (the carried
//! body is the incoming stock; a law that cannot read it judges a
//! superset or refuses by name); plain re-clamp practice, no vendor
//! material consulted.
//!
//! ## The laws here
//!
//! - **Never less than reality.** The box encloses the declared stock and
//!   the incoming body's bounds (themselves clamped to the uncut origin
//!   block carried through the turns - both supersets of the body). Every
//!   law reading the job's stock (`stock_facets`, `stock_aabb`, the top)
//!   then judges at least the real material; verify's prior acquittal
//!   (#96, carried by #318) still clears a dive into what was really cut.
//! - **Unchanged where the declared block already covers.** When the
//!   declared stock is a box enclosing the carried block (or the incoming
//!   body), nothing is substituted: the verdict is the one before #319,
//!   byte for byte - and so is every single-setup and clip-entered job,
//!   whose chain crosses no carried boundary.
//! - **A mismatch is said by name.** Carried material standing outside the
//!   declared stock rides the path as a note ([`CARRIED_OUTSIDE_DECLARED`])
//!   naming both setups: the judge covered it, the op's machining region
//!   (the declared stock) did not.
//! - **Declared material beyond the carried block is present.** Where the
//!   declared stock reaches past the uncut carried block in XY, the
//!   carried grid cannot speak for that material: the rest mask and
//!   verify's prior are not read, and the raw stock judges.

use crate::geometry::{Point3, Triangle, cube};
use crate::machining2d::GenError;
use crate::model;
use crate::stock::StockShape;
use flecs_ecs::prelude::World;

/// The note's head when carried material stands outside the declared stock.
pub const CARRIED_OUTSIDE_DECLARED: &str = "the stock carried from an earlier setup stands outside this setup's declared stock";

/// Containment slack: float noise of a carried corner, never material.
const TOL: f64 = 1e-6;

/// What a carried setup changes in the job; the default changes nothing.
#[derive(Default)]
pub(super) struct Judged {
    /// The box the stock-reading laws judge instead of the declared stock.
    pub stock: Option<(Vec<Triangle>, (Point3, Point3))>,
    pub note: Option<String>,
    /// The declared stock reaches beyond the carried block in XY: that
    /// material is present (it is declared), and the carried grid - the
    /// rest mask and verify's prior - does not cover it, so neither is
    /// read; the raw stock judges (more material, never less).
    pub distrust_prior: bool,
}

/// Does `outer` hold `inner` in XY and below its top? The bottom is not
/// read: nothing judged from above meets material under the floor.
fn holds(outer: (Point3, Point3), inner: (Point3, Point3)) -> bool {
    let ((o0, o1), (i0, i1)) = (outer, inner);
    i0.x >= o0.x - TOL && i0.y >= o0.y - TOL && i1.x <= o1.x + TOL && i1.y <= o1.y + TOL && i1.z <= o1.z + TOL
}

/// What `op_name`'s job judges in `setup`: the default when the declared
/// stock stands (no carried boundary, or it already covers).
///
/// # Errors
/// A carry the lattice refuses or an origin setup with no stock, named:
/// the stock this setup starts from is unknown, so nothing judges it.
pub(super) fn judged(world: &World, op_name: &str, setup: Option<&str>, tool_r: f64) -> Result<Judged, GenError> {
    let named = |e: String| GenError::Invalid(format!("{op_name}: {e}"));
    let Some((origin, block)) = crate::rest::carried_stock_block(world, op_name, setup).map_err(named)? else {
        return Ok(Judged::default());
    };
    let declared = model::stock_world_aabb_for(world, setup).map(|b| (b.min, b.max));
    let distrust_prior = declared.is_some_and(|(d0, d1)| {
        d0.x < block.0.x - TOL || d0.y < block.0.y - TOL || d1.x > block.1.x + TOL || d1.y > block.1.y + TOL
    });
    let unchanged = Judged { distrust_prior, ..Judged::default() };
    let boxed = crate::setup::stock_entity(world, setup)
        .and_then(|e| model::stock_def_of(world, &e))
        .is_some_and(|s| matches!(s, StockShape::Box { .. }));
    let covers = |b: (Point3, Point3)| boxed && declared.is_some_and(|d| holds(d, b));
    if covers(block) {
        return Ok(unchanged);
    }
    // The real body, when the grid can cut it; the uncut block otherwise
    // (a superset either way - an unreadable body is never read as air).
    let body = match crate::rest::carried_stock_body(world, op_name, setup, tool_r, block) {
        Ok(Some(b)) => b,
        // No material carried at all: any declared stock covers it.
        Ok(None) => return Ok(unchanged),
        Err(e) => {
            eprintln!("camcraft: {op_name}: the carried body is unreadable, its uncut block is judged: {e}");
            block
        }
    };
    if covers(body) {
        return Ok(unchanged);
    }
    let (lo, hi) = match declared {
        Some((d0, d1)) => (
            Point3::new(d0.x.min(body.0.x), d0.y.min(body.0.y), d0.z.min(block.0.z)),
            Point3::new(d1.x.max(body.1.x), d1.y.max(body.1.y), d1.z.max(body.1.z)),
        ),
        None => (Point3::new(body.0.x, body.0.y, block.0.z), body.1),
    };
    let note = declared.filter(|d| !holds(*d, body)).map(|(d0, d1)| {
        let side = [d0.x - body.0.x, d0.y - body.0.y, body.1.x - d1.x, body.1.y - d1.y].into_iter().fold(0.0, f64::max);
        let above = (body.1.z - d1.z).max(0.0);
        let to = setup.unwrap_or_default();
        format!(
            "{CARRIED_OUTSIDE_DECLARED} (from setup '{origin}' into setup '{to}': up to {side:.2} mm beside it, {above:.2} mm above its top) - rapids, entries and links were judged against both, but only the declared stock was machined; enlarge setup '{to}'s stock to machine the rest"
        )
    });
    Ok(Judged { stock: Some((cube(lo, hi).facets.to_vec(), (lo, hi))), note, distrust_prior })
}
