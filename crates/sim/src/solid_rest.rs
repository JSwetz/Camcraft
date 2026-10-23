//! Solid-truth material intervals for hidden-rest reporting (M2.2d).
//!
//! Provenance: our own design; the inside/outside test is the classic
//! ray-parity membership rule, standard in any computational-geometry
//! text (e.g. the point-in-mesh surveys in Held & Lyckega, "Approximate
//! cell decomposition of structures"). Nothing derived from vendor
//! binaries or output forensics.
//!
//! Why this module exists. The envelope kernel (`stock_sim`) rides a
//! HEIGHT FIELD: per dexel column it knows the topmost surface, which
//! is exactly what a Z-following ball sees - and exactly what hides a
//! ceiling's shadow. Material sheltered under an overhang never
//! appears in a height-field rest measure: the column's top is the
//! ceiling, so the floor below it is invisible (RECOMMENDED.md names
//! this as M2.2b's open debt). The cure is the SOLID truth: cast the
//! column as a vertical ray and pair its mesh crossings. A closed
//! shell crosses the ray an even number of times and each even-index
//! hit opens a material interval that the next, odd-index hit closes -
//! however deep, however shadowed. Rest is then what material remains
//! outside the swept ball's reach, ceilings included.
//!
//! This module commits to three laws:
//! - (a) CLAIMS: every facet whose closed XY triangle contains the
//!   column makes a claim - INTERIOR (point deep inside) or BOUNDARY
//!   (point on an edge or vertex, within tolerance). At one crossing z
//!   the claims settle into material counts by plane: each interior
//!   claim is its own sheet, and bare boundary claims of one plane
//!   merge to ONE crossing (they are pieces of a single sheet meeting
//!   at an edge). Per-triangle half-open ownership cannot do this - a
//!   shared diagonal would then belong to one owning triangle, but
//!   two coincident sheets stacked flush would collapse to one,
//!   lying about parity. Claims see it through: an ordinary column
//!   is even on any closed shell however triangulated, and a
//!   knife-edge double surface (the column ON two flush sheets at
//!   their own shared edge) lands odd - reported, never guessed.
//! - (b) TOTAL: no input - NaN facets, zero-area facets, open shells -
//!   panics or hangs. An odd crossing count is reported as
//!   `Crossings::Open`, an honest unknown; callers may measure it but
//!   may not call it material.
//! - (c) ORIENTATION-FREE: parity needs a CLOSED shell, not a wound
//!   one. Flipped facets change nothing here - normals group by
//!   LINE, not direction, and inside/outside never reads winding.

use crate::geometry::{Point3, Triangle};

/// Where a vertical ray at `(x, y)` found the shell, z-ascending.
#[derive(Clone, Debug, PartialEq)]
pub enum Crossings {
    /// Even crossings: each pair bounds a material interval
    /// (closed-shell answer).
    Paired(Vec<f64>),
    /// Odd crossings: the column ends inside - open shell, or a
    /// knife-edge boundary column despite law (a)'s claims. `z` is the
    /// crossing list; the last entry is unpaired.
    Open { z: Vec<f64> },
}

impl Crossings {
    /// The material intervals if every crossing is bracketed.
    #[must_use]
    pub fn intervals(&self) -> Option<Vec<(f64, f64)>> {
        match self {
            Crossings::Paired(z) => Some(z.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()),
            Crossings::Open { .. } => None,
        }
    }
}

/// One facet's claim on a column: crossing z, facet plane normal, and
/// whether the claim is INTERIOR (deep in the triangle) or BOUNDARY
/// (on an edge/vertex within tolerance).
pub(crate) struct Claim {
    pub(crate) z: f64,
    pub(crate) n: Point3,
    pub(crate) interior: bool,
}

/// All crossing z of the vertical line through `(x, y)` with `facets`.
#[must_use]
pub fn column_crossings(facets: &[Triangle], x: f64, y: f64) -> Crossings {
    let mut claims: Vec<Claim> = Vec::with_capacity(16);
    for t in facets {
        if let Some(c) = plane_claim(&t.a, &t.b, &t.c, x, y) {
            claims.push(c);
        }
    }
    crossings_from_claims(&claims)
}

/// The parity engine, law (a) in one pass: claims sorted, clustered
/// into physical levels within `EPS_Z`, settled to crossings per
/// plane group (every interior claim is a sheet; a group of bare
/// boundary claims is ONE crossing - adjacent pieces of one sheet),
/// then odd/even into `Crossings`. Split out so the envelope kernel
/// can feed it pre-binned claims without touching facets again.
pub(crate) fn crossings_from_claims(claims: &[Claim]) -> Crossings {
    const EPS_Z: f64 = 1e-9;
    const EPS_PAR: f64 = 1e-9; // sin of the angle where normals are "same line"
    if claims.is_empty() {
        return Crossings::Paired(Vec::new());
    }
    let mut sorted: Vec<&Claim> = claims.iter().collect();
    sorted.sort_by(|p, q| p.z.total_cmp(&q.z));
    let mut z: Vec<f64> = Vec::with_capacity(claims.len());
    let mut i = 0;
    while i < sorted.len() {
        // One physical level: every claim within EPS_Z of the cluster's first z.
        let z0 = sorted[i].z;
        let mut sum = 0.0;
        let mut count = 0usize;
        // Plane groups at this level: representative normal, interior claims,
        // any boundary claim.
        let mut groups: Vec<(Point3, usize, bool)> = Vec::new();
        while i < sorted.len() && (sorted[i].z - z0).abs() <= EPS_Z {
            let c = sorted[i];
            match groups
                .iter()
                .position(|(r, _, _)| parallel_line(*r, c.n, EPS_PAR))
            {
                Some(p) => {
                    groups[p].1 += usize::from(c.interior);
                    groups[p].2 |= !c.interior;
                }
                None => groups.push((c.n, usize::from(c.interior), !c.interior)),
            }
            sum += c.z;
            count += 1;
            i += 1;
        }
        let zm = sum / count as f64;
        for (_, inter, boundary) in groups {
            let crossings = if inter > 0 { inter } else if boundary { 1 } else { 0 };
            for _ in 0..crossings {
                z.push(zm);
            }
        }
    }
    if z.len() % 2 == 1 {
        Crossings::Open { z }
    } else {
        Crossings::Paired(z)
    }
}

/// True when both unit normals lie on the same LINE (parallel or
/// antiparallel) - coplanar sheets in any winding (law c).
fn parallel_line(a: Point3, b: Point3, eps: f64) -> bool {
    let c = a.cross(b);
    c.dot(c) <= eps * eps
}

/// The claim of `abc` on the column at `(x, y)`, or None if the
/// column misses the facet's XY triangle or the projection is
/// degenerate (vertical wall seen edge-on, zero area) - walls are
/// handled by the columns their faces bound, never by a ray down
/// their plane. Claims carry the strictness that law (a) needs.
pub(crate) fn plane_claim(
    a: &Point3,
    b: &Point3,
    c: &Point3,
    x: f64,
    y: f64,
) -> Option<Claim> {
    let d = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    // Degeneracy skin scales with the projection's own footprint.
    let span = (b.x - a.x)
        .abs()
        .max((c.x - a.x).abs())
        .max((b.y - a.y).abs())
        .max((c.y - a.y).abs());
    if !d.is_finite() || !span.is_finite() || d.abs() <= 1e-12 * span.max(1.0) * span.max(1.0) {
        return None;
    }
    let px = x - a.x;
    let py = y - a.y;
    if !(px.is_finite() && py.is_finite()) {
        return None;
    }
    let u = (px * (c.y - a.y) - py * (c.x - a.x)) / d;
    let v = ((b.x - a.x) * py - (b.y - a.y) * px) / d;
    const EPS_B: f64 = 1e-12;
    // Closed containment, then the strictness that splits interior
    // from boundary claims. The barycentric tolerance is dimension-
    // free, so it is a fraction of the facet, not of the model.
    if u < -EPS_B || v < -EPS_B || u + v > 1.0 + EPS_B {
        return None;
    }
    let interior = u > EPS_B && v > EPS_B && u + v < 1.0 - EPS_B;
    let z = (1.0 - u - v) * a.z + u * b.z + v * c.z;
    let n = (*b - *a).cross(*c - *a);
    let l2 = n.dot(n);
    if !z.is_finite() || l2 <= 0.0 || !l2.is_finite() {
        return None;
    }
    let inv = 1.0 / l2.sqrt();
    Some(Claim {
        z,
        n: Point3::new(n.x * inv, n.y * inv, n.z * inv),
        interior,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// One horizontal square (side 2, centered), split on a diagonal;
    /// `up` picks the winding (law c: parity must not care).
    fn sheet(z: f64, up: bool) -> Vec<Triangle> {
        let (a, b, c, d) = (p(-1., -1., z), p(1., -1., z), p(1., 1., z), p(-1., 1., z));
        if up {
            vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
        } else {
            vec![Triangle::new(a, d, c), Triangle::new(a, c, b)]
        }
    }

    /// Closed axis-aligned box [-1,1]^2 x [z0, z1]: 12 triangles.
    fn closed_box(z0: f64, z1: f64) -> Vec<Triangle> {
        let mut f = sheet(z0, false);
        f.extend(sheet(z1, true));
        let wall = |q: [Point3; 4]| {
            vec![
                Triangle::new(q[0], q[1], q[2]),
                Triangle::new(q[0], q[2], q[3]),
            ]
        };
        f.extend(wall([p(-1., -1., z0), p(1., -1., z0), p(1., -1., z1), p(-1., -1., z1)]));
        f.extend(wall([p(1., -1., z0), p(1., 1., z0), p(1., 1., z1), p(1., -1., z1)]));
        f.extend(wall([p(1., 1., z0), p(-1., 1., z0), p(-1., 1., z1), p(1., 1., z1)]));
        f.extend(wall([p(-1., 1., z0), p(-1., -1., z0), p(-1., -1., z1), p(-1., 1., z1)]));
        f
    }

    fn flip(f: &[Triangle]) -> Vec<Triangle> {
        f.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect()
    }

    #[test]
    fn closed_box_columns_pair_to_one_interval() {
        let f = closed_box(0.0, 10.0);
        let c = column_crossings(&f, 0.0, 0.0);
        // (0,0) sits ON both sheets' shared diagonal - law (a)'s
        // boundary merge, the exact case a half-open rule loses.
        assert_eq!(c, Crossings::Paired(vec![0.0, 10.0]));
        assert_eq!(c.intervals(), Some(vec![(0.0, 10.0)]));
    }

    #[test]
    fn an_open_sheet_is_unknown_not_a_lie() {
        // Law (b): one sheet is not material - it is an unknown.
        let c = column_crossings(&sheet(5.0, true), 0.4, 0.1);
        assert_eq!(c, Crossings::Open { z: vec![5.0] });
        assert_eq!(c.intervals(), None);
    }

    #[test]
    fn ceiling_over_floor_finds_the_sheltered_material() {
        // The hidden-rest shape: a ceiling plate 9..10 over a floor
        // slab 0..2. The height field of either alone would see only
        // its own top; the column pairs [0,2],[9,10] and the slab's
        // material reads as material BELOW the plate, where a Z-
        // following ball never sees it.
        let mut f = closed_box(9.0, 10.0);
        f.extend(closed_box(0.0, 2.0));
        let c = column_crossings(&f, 0.4, 0.1); // off all diagonals
        assert_eq!(c.intervals(), Some(vec![(0.0, 2.0), (9.0, 10.0)]));
    }

    #[test]
    fn winding_is_irrelevant() {
        // Law c: flip EVERY facet of a closed box; crossings identical.
        let f = closed_box(0.0, 10.0);
        for (x, y) in [(0.3, 0.2), (0.0, 0.0), (1.0, -0.5), (-1.0, 1.0), (2.0, 0.0)] {
            let a = column_crossings(&f, x, y);
            let b = column_crossings(&flip(&f), x, y);
            assert_eq!(a, b, "winding changed the column at ({x},{y})");
        }
    }

    #[test]
    fn columns_on_edges_and_vertices_stay_even() {
        // Law (a) witnesses: through shared edges and corners of a
        // closed box, parity must stay even and the box must still
        // read as ONE interval - boundary merge, not double count.
        let f = closed_box(0.0, 10.0);
        for (x, y) in [
            (0.0, 0.0),   // the shared diagonal of both sheets
            (1.0, 0.0),   // mid of the boundary edge x=1
            (1.0, 1.0),   // up a vertical corner
            (-1.0, -1.0), // another corner
            (0.5, -1.0),  // on a bottom-edge line of the top sheet
        ] {
            match column_crossings(&f, x, y) {
                Crossings::Paired(z) => {
                    assert_eq!(z, vec![0.0, 10.0], "column at ({x},{y})")
                }
                Crossings::Open { z } => panic!("odd parity at ({x},{y}): {z:?}"),
            }
        }
    }

    #[test]
    fn flush_stacked_shells_are_reported_not_guessed() {
        // The knife edge law (a) promises to REPORT: two boxes sharing
        // the plane z=5 as flush sheets. Ordinary columns pair to one
        // material interval; on the flush pair's own shared diagonal
        // the level is ambiguous and must come out Open, never a lie.
        let mut f = closed_box(0.0, 5.0);
        f.extend(closed_box(5.0, 10.0));
        let inside = column_crossings(&f, 0.4, 0.1);
        assert_eq!(inside, Crossings::Paired(vec![0.0, 5.0, 5.0, 10.0]));
        assert_eq!(inside.intervals(), Some(vec![(0.0, 5.0), (5.0, 10.0)]));
        // A column riding both sheets' boundary edges makes only
        // BOUNDARY claims at z=5 - and the merge cannot tell "two
        // faces of one sheet" from "one face of each of two flush
        // sheets". It merges (that is the honest half of the law:
        // when in doubt report unknown, never assert material), so
        // the corner reads Open, not a false solid interval.
        match column_crossings(&f, 1.0, 0.0) {
            Crossings::Open { z } => assert_eq!(z, vec![0.0, 5.0, 10.0]),
            Crossings::Paired(z) => panic!("a knife edge was guessed: {z:?}"),
        }
    }

    #[test]
    fn degenerate_input_is_total() {
        // Law (b) fuzz floor: NaNs, zero-area facets, empty mesh.
        let junk = vec![
            Triangle::new(p(f64::NAN, 0., 0.), p(1., 0., 0.), p(0., 1., 0.)),
            Triangle::new(p(0., 0., 0.), p(0., 0., 0.), p(0., 0., 0.)),
            Triangle::new(p(0., 0., 1.), p(0., 0., 1.), p(0., 0., 1.)),
        ];
        let _ = column_crossings(&junk, 0.0, 0.0);
        let _ = column_crossings(&[], 0.0, 0.0);
        let _ = column_crossings(&junk, f64::NAN, 0.0);
    }
}
