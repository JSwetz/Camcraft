//! The one region law (M11.4 A): what an operation's selection encloses,
//! whatever was picked - a face, its boundary edge loop, the wall patch -
//! followed by the single area-controls seam every kind's region passes
//! through. Provenance: our own design notes on 2.5D pocketing (a
//! boundary plus the islands that stand inside it, the textbook pocket
//! definition of any machining handbook); no vendor material consulted.
//!
//! The pocket family on a PART cuts inside the enclosed area and around
//! the part material standing in it: the enclosed area is the union of
//! the picked rings' interiors (a face's hole loops are not trusted as
//! the island list - its edge loop has none), and the islands are read
//! from the part itself at every plane the ladder cuts, the verifier's
//! own section law (`slice::material_above`). A floor face, its rim
//! chain, the rim at the top of the wall and the wall patch therefore
//! resolve to one region, byte for byte, and the strategy the user chose
//! runs on it - nothing here looks at the region's shape.

use super::*;

/// The area controls of M11.5 (tool containment, boundary offset, edge
/// extension, extra include/exclude boundaries) attach HERE, in one
/// place, for every kind. Today the default is the identity, so every
/// region and every golden is exactly what it was.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AreaControls {}

/// The seam itself: every region an operation cuts (selection-built or
/// source-built, Face included) passes through here last.
pub(super) fn apply_area_controls(region: Region, _controls: &AreaControls) -> Result<Region, GenError> {
    Ok(region)
}

/// The part's facets and the ladder planes, top down.
pub(super) type PocketLadder = (Vec<Triangle>, Vec<f64>);

/// The part facets and ladder planes the pocket region law reads, or
/// `None` where it does not apply: not a pocket-family kind, or picks
/// that touch the setup's stock (stock is the material to remove, not a
/// part to keep - the depth verify's `part_solid` rule).
pub(super) fn pocket_ladder(
    world: &World,
    setup: Option<&str>,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<Option<PocketLadder>, GenError> {
    let (top, bottom, stepdown) = match params {
        OpParams::Pocket { top_z, bottom_z, stepdown, .. }
        | OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, .. } => (*top_z, *bottom_z, *stepdown),
        _ => return Ok(None),
    };
    let stock_name = crate::setup::stock_entity_name(world, setup);
    if items.is_empty() || items.iter().any(|g| g.entity == stock_name) {
        return Ok(None);
    }
    let sel = SrcRef::Selection(items.to_vec());
    let material = material_of(world, setup, &sel)?;
    if material.facets.is_empty() {
        return Ok(None);
    }
    // Unresolvable heights are the generator's refusal of record (named
    // there, in its precedence); the region law simply stands down.
    let (Some(t), Some(b)) = (resolve_height(world, setup, Some(&sel), top), resolve_height(world, setup, Some(&sel), bottom))
    else {
        return Ok(None);
    };
    Ok(Some((material.facets, ladder_planes(t, b, stepdown))))
}

/// Below this (mm^2) the part material inside the enclosed area is
/// tessellation noise along a shared wall, not an island.
const ISLAND_AREA_MIN: f64 = 1e-4;

/// The pocket family's selection region on a part: the enclosed area of
/// `loops` minus the part material standing in it at each ladder plane.
/// Where no material stands inside, the answer is exactly today's
/// `Region::new(loops)` (byte-stable for every existing pick). A plane
/// where the part fills the whole area is a named refusal - the pocket's
/// Bottom sits below the selection's floor - never a cut into the part.
pub(super) fn pocket_region_on_part(
    loops: Vec<Loop>,
    part: &[Triangle],
    planes: &[f64],
) -> Result<Region, GenError> {
    let mut enclosed = Region::empty();
    for l in &loops {
        enclosed = enclosed.union(&Region::new(vec![l.clone()])?)?;
    }
    if enclosed.is_empty() {
        return Err(GenError::Empty("nothing in the selection bounds an area".into()));
    }
    let mut standing = Region::empty();
    for &z in planes {
        let at = slice::material_above(part, z).map_err(from_slice)?;
        let inside = enclosed.intersect(&at)?;
        if inside.area().abs() < ISLAND_AREA_MIN {
            continue;
        }
        if enclosed.difference(&inside)?.area().abs() < ISLAND_AREA_MIN {
            return Err(GenError::Invalid(format!(
                "the part fills the whole selection at Z {z:.3} - Bottom Z sits below the selected pocket's floor; raise Bottom Z to the floor"
            )));
        }
        standing = standing.union(&inside)?;
    }
    if standing.area().abs() < ISLAND_AREA_MIN {
        return Ok(Region::new(loops)?);
    }
    Ok(enclosed.difference(&standing)?)
}

/// The planes a pocket ladder cuts at, top down, the last one `bottom`
/// exactly - the `machining2d` level law, restated here because the
/// region is built before the generator runs (and the motion crate keeps
/// its helper private). Degenerate spans and steps collapse to the one
/// bottom plane; the generator refuses them by name later.
pub(super) fn ladder_planes(top: f64, bottom: f64, stepdown: f64) -> Vec<f64> {
    let span = top - bottom;
    let walkable = span.is_finite() && span > 0.0 && stepdown > 0.0;
    if !walkable {
        return vec![bottom];
    }
    let tol = span * 1e-9;
    let mut zs = Vec::new();
    let mut z = top;
    while z > bottom + tol && zs.len() < 100_000 {
        zs.push(z);
        z -= stepdown;
    }
    zs.push(bottom);
    zs
}

#[cfg(test)]
mod law {
    use super::*;

    #[test]
    fn a_ring_and_its_mirror_share_one_canonical_rim() {
        // The dedupe key a face loop and its reversed edge chain meet on.
        let ring = [(0, 0), (5, 0), (7, 3), (4, 8), (-1, 4)];
        let mut rev = ring.to_vec();
        rev.reverse();
        rev.rotate_left(2);
        assert_eq!(canon_ring(&ring), canon_ring(&rev));
        let other = [(0, 0), (5, 0), (4, 8), (7, 3), (-1, 4)];
        assert_ne!(canon_ring(&ring), canon_ring(&other), "a different ring is not a mirror");
    }

    #[test]
    fn the_area_controls_seam_is_the_identity_today() {
        let r = Region::new(vec![
            Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), Point2::new(4.0, 3.0)])
                .expect("loop"),
        ])
        .expect("region");
        let out = apply_area_controls(r.clone(), &AreaControls::default()).expect("identity");
        assert_eq!(out.loops(), r.loops());
    }

    #[test]
    fn ladder_planes_match_the_level_law_and_never_hang() {
        assert_eq!(ladder_planes(20.0, 0.0, 5.0), vec![20.0, 15.0, 10.0, 5.0, 0.0]);
        assert_eq!(ladder_planes(20.0, 2.0, 5.0), vec![20.0, 15.0, 10.0, 5.0, 2.0]);
        assert_eq!(ladder_planes(1.0, 1.0, 5.0), vec![1.0]);
        assert_eq!(ladder_planes(10.0, 0.0, 0.0), vec![0.0]);
        assert_eq!(ladder_planes(f64::NAN, 0.0, 1.0), vec![0.0]);
        assert!(ladder_planes(1e9, 0.0, 1e-9).len() <= 100_001);
    }
}
