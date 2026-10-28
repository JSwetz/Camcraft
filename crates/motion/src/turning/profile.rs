//! The turned profile: the axis frame the lathe reads the model in,
//! and the silhouette extents the facing kernel lives inside (M5.2 A).
//!
//! The frame is not new geometry: the axis is exactly what stage E's
//! [`crate::multiaxis::axis::fit`] claims from the facet normals -
//! one truth for "where the spindle is" across families - with the
//! twins' own two guards restated: a directionless fit is a corrupt
//! mesh, and an axis lying near the table makes world Z heights
//! meaningless AS stations, so it is refused by name, not worked
//! around.
//!
//! Provenance: the meridian section of a surface of revolution as a
//! (station, radius) reading of the mesh is classical descriptive
//! geometry as any machining handbook draws it; written from that
//! and our own design notes. No vendor material consulted.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// World up, spelled once (Point3::new is not a const fn) - the
/// rotary twins' constant, restated because turning must not reach
/// into multiaxis' privates.
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};
/// The axis' up-component below which world Z planes cannot name
/// stations along it (a plane nearly parallel to the axis meets it,
/// if at all, a million stations away): the twins' sin-1-degree
/// boundary, the same number for the same reason.
const LEAN_MIN: f64 = 0.0175;
/// The silhouette's hard point budget: a profile that needs more
/// stations than this before its chords settle is a model or a
/// tolerance problem, not something to grind through silently.
const MAX_SIL_PTS: usize = 1 << 16;

/// The frame the turning kernels walk in: a point on the axis, the
/// unit direction in the world's +z sense (so a station grows with
/// height, whichever way the fit happened to point), and ONE radial
/// direction - every facing pass runs in the plane that pair spans,
/// which is how a turret-mounted insert meets a spinning part.
pub struct Frame {
    origin: Point3,
    dir: Point3,
    radial: Point3,
    /// `dir.z`; strictly above [`LEAN_MIN`], the division's promise.
    uz: f64,
}

impl Frame {
    /// Fit, upright-sign canonicalize, lean-guard and frame in one
    /// call: the model must say "revolved" (axis::fit's own refusal,
    /// by name) and must say it about an axis the height parameters
    /// can walk.
    pub fn about(facets: &[Triangle]) -> Result<Frame, GenError> {
        let ax = crate::multiaxis::axis::fit(facets)?;
        let d = ax.dir;
        let n = (d.dot(d)).sqrt();
        if !n.is_finite() || n <= 0.0 {
            return Err(GenError::Invalid(
                "the fitted axis is not a direction - the mesh is corrupt".into(),
            ));
        }
        let mut u = Point3::new(d.x / n, d.y / n, d.z / n);
        if u.z < 0.0 {
            u = Point3::new(-u.x, -u.y, -u.z);
        }
        if u.z < LEAN_MIN {
            return Err(GenError::Invalid(format!(
                "the model's axis of revolution stands {:.0} degrees off vertical - \
                 turning heights are world Z planes read as stations, and a lying \
                 axis makes them meaningless (the rotary kinds own that posture)",
                90.0 - u.z.clamp(0.0, 1.0).acos().to_degrees()
            )));
        }
        // axis.rs' basis law, verbatim: any vector off the axis, then
        // the direction it completes against.
        let any = if u.z < 0.9 { UP } else { Point3::new(1.0, 0.0, 0.0) };
        let radial = match normalize(u.cross(any)) {
            Some(v) => v,
            None => {
                return Err(GenError::Invalid(
                    "the fitted axis is not a direction - the mesh is corrupt".into(),
                ))
            }
        };
        Ok(Frame {
            origin: ax.origin,
            dir: u,
            radial,
            uz: u.z,
        })
    }

    /// The station a world Z plane meets the axis at.
    pub fn station_of(&self, world_z: f64) -> f64 {
        (world_z - self.origin.z) / self.uz
    }

    /// The world height a point ON the axis stands at that station
    /// (diagnostic wording only - the geometry never needs it).
    pub fn height_of(&self, station: f64) -> f64 {
        self.origin.z + station * self.uz
    }

    /// The world point at radius `rho` from the axis, at `station`,
    /// in the plane the frame chose.
    pub fn point(&self, rho: f64, station: f64) -> Point3 {
        Point3::new(
            self.origin.x + self.dir.x * station + self.radial.x * rho,
            self.origin.y + self.dir.y * station + self.radial.y * rho,
            self.origin.z + self.dir.z * station + self.radial.z * rho,
        )
    }

    /// The axis's own point at a station - the helical arc's center
    /// line (M5.2 D: threading's XY circles stand on it).
    pub(crate) fn axis_point(&self, station: f64) -> Point3 {
        Point3::new(
            self.origin.x + self.dir.x * station,
            self.origin.y + self.dir.y * station,
            self.origin.z + self.dir.z * station,
        )
    }

    /// The frame's angle-0 point, seen from the axis's other side:
    /// the half-turn endpoint of a threading arc, exact about the
    /// axis line whatever the lean (the plumb door still refuses a
    /// tilted bar for the XY-circle law, not this mirror's).
    pub(crate) fn antipode(&self, rho: f64, station: f64) -> Point3 {
        let a = self.point(rho, station);
        let x = self.axis_point(station);
        Point3::new(2.0 * x.x - a.x, 2.0 * x.y - a.y, 2.0 * x.z - a.z)
    }

    /// How far the axis leans off world Z in the XY sense - |dir.xy|
    /// of the unit direction. Facing and the twins tolerate the fit's
    /// LEAN_MIN degrees; a helix of XY circles cannot (M5.2 D).
    pub(crate) fn lean(&self) -> f64 {
        (self.dir.x * self.dir.x + self.dir.y * self.dir.y).sqrt()
    }
}

impl Frame {
    /// A vertex read as (station, radius): the meridian section's
    /// own coordinates, the pair every turning kernel thinks in.
    pub fn sr(&self, q: Point3) -> (f64, f64) {
        let d = q - self.origin;
        let s = d.dot(self.dir);
        let w = Point3::new(
            d.x - self.dir.x * s,
            d.y - self.dir.y * s,
            d.z - self.dir.z * s,
        );
        (s, (w.dot(w)).sqrt())
    }
}

/// The body's own extents in the frame: the radius no material
/// exceeds and the station no material tops.
pub struct Profile {
    pub r_max: f64,
    pub z_top: f64,
}

/// Sweep the facets once. The distance to a line is CONVEX, and the
/// body is the hull of all its vertices, so the WHOLE-BODY maximum
/// is taken at a vertex: `r_max` and `z_top` are exact from vertices
/// alone. That is all this fold promises: per station it says
/// nothing. A band's material can stand inside the station window
/// while every vertex of the facet that carries it stands outside
/// it - a cylinder wall between its two rim rings - and an
/// infeed radius read from window-filtered vertices is then too
/// SMALL, which is a rapid through skin. [`silhouette`]'s edge
/// sweep is the per-station answer.
pub fn profile_of(facets: &[Triangle], f: &Frame) -> Result<Profile, GenError> {
    let (mut r_max, mut z_top) = (0.0f64, f64::NEG_INFINITY);
    for t in facets {
        for q in [t.a, t.b, t.c] {
            if !q.is_finite() {
                return Err(GenError::Invalid(
                    "the model carries a non-finite vertex - the mesh is corrupt".into(),
                ));
            }
            let (s, rho) = f.sr(q);
            r_max = r_max.max(rho);
            z_top = z_top.max(s);
        }
    }
    Ok(Profile { r_max, z_top })
}

/// The turned silhouette (M5.2 B): the envelope radius as an
/// ascending (station, radius) polyline, the step faces of the bar
/// appearing as its VERTICALS (one station, two radii - the same
/// jump a facing ladder would leave as a shoulder).
///
/// The envelope is asked of EDGES, not window-filtered vertices -
/// the stubble law carried into sampling: an edge crossing a station
/// plane meets it at exactly the tangency, so a wall spanning the
/// window between its rim rings says its full radius at every
/// station inside, which is true (the wall IS the skin there) where
/// the stage-A vertex reading was silent. Radius along an edge is
/// CONVEX in the station (distance to a line, along a line), so on
/// any interval between vertex stations - where the set of crossing
/// edges is constant - the envelope is a max of convex functions:
/// convex. Chording a convex graph never sinks below the material,
/// which is why the adaptive law needs only the sagitta test:
/// sample the interval's midpoint until chord minus envelope stands
/// within `tol`, never past a hard point budget.
///
/// A station interval no edge crosses is a GAP in the body along the
/// axis; profile turning machines one bar between its end faces, so
/// that model refuses by name rather than walk two silhouettes as if
/// they were one.
pub fn silhouette(facets: &[Triangle], f: &Frame, tol: f64) -> Result<Vec<(f64, f64)>, GenError> {
    if facets.is_empty() {
        return Err(GenError::Empty("the model has no faces to turn".into()));
    }
    if tol <= 0.0 || !tol.is_finite() {
        return Err(GenError::Invalid(format!(
            "the silhouette's chord tolerance must be a positive finite size, not {tol}"
        )));
    }
    // One pass: every edge gets its station window; an edge whose
    // two stations coincide is the boundary of a STEP FACE (or a rim
    // ring) and answers only AT that station, with its wider vertex.
    // A stored edge keeps its lo-station endpoint FIRST: rho_at
    // measures t from lo along a to b, and an edge stored the other
    // way round reads the mirror profile - both orientations used
    // to ride in from the two triangles sharing an edge, and the
    // envelope's max made every slanted line a W (invisible on the
    // blade fixtures, whose varying edges are all constant radius).
    let mut edges: Vec<(Point3, Point3, f64, f64)> = Vec::with_capacity(facets.len() * 3);
    let mut stations: Vec<f64> = Vec::with_capacity(facets.len() * 6);
    let mut verticals: Vec<(f64, f64)> = Vec::new();
    for t in facets {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            for q in [a, b] {
                if !q.is_finite() {
                    return Err(GenError::Invalid(
                        "the model carries a non-finite vertex - the mesh is corrupt".into(),
                    ));
                }
            }
            let (sa, ra) = f.sr(a);
            let (sb, rb) = f.sr(b);
            stations.push(sa);
            stations.push(sb);
            if sa == sb {
                verticals.push((sa, ra.max(rb)));
            } else if sa < sb {
                edges.push((a, b, sa, sb));
            } else {
                edges.push((b, a, sb, sa));
            }
        }
    }
    stations.sort_unstable_by(f64::total_cmp);
    stations.dedup();
    if stations.len() < 2 {
        return Err(GenError::Invalid(
            "the model is flat - there is no length to turn along".into(),
        ));
    }
    verticals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));

    // The radius an edge's crossing of a plane carries: the point ON
    // the edge at that station, read for its distance to the axis.
    // The clamp is the convexity promise, never a lie.
    let rho_at = |a: Point3, b: Point3, lo: f64, hi: f64, s: f64| -> f64 {
        let t = ((s - lo) / (hi - lo)).clamp(0.0, 1.0);
        f.sr(Point3::new(
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        ))
        .1
    };

    // Interval crossing-sets. A window [lo, hi] covers interval i
    // exactly when lo <= s_i and s_{i+1} <= hi; lo and hi ARE station
    // values (same floats, same arithmetic), so the index walk is
    // exact - no epsilon anywhere but the sagitta test.
    let n = stations.len();
    let mut crosses: Vec<Vec<usize>> = vec![Vec::new(); n - 1];
    for (k, &(_, _, lo, hi)) in edges.iter().enumerate() {
        let i0 = stations.partition_point(|&s| s < lo);
        let i1 = stations.partition_point(|&s| s <= hi);
        for slot in &mut crosses[i0..i1.saturating_sub(1)] {
            slot.push(k);
        }
    }
    for (i, c) in crosses.iter().enumerate() {
        if c.is_empty() {
            return Err(GenError::Invalid(format!(
                "no material crosses the station interval [{:.3}, {:.3}] - the model is \
                 not one continuous turned body, and profile turning machines one bar",
                stations[i], stations[i + 1]
            )));
        }
    }
    let env = |i: usize, s: f64| -> f64 {
        let mut r: f64 = 0.0;
        for &k in &crosses[i] {
            let (a, b, lo, hi) = edges[k];
            r = r.max(rho_at(a, b, lo, hi, s));
        }
        r
    };
    let v_at = |s: f64| -> f64 {
        let lo = verticals.partition_point(|&(sv, _)| sv < s);
        let hi = verticals.partition_point(|&(sv, _)| sv <= s);
        let mut r: f64 = 0.0;
        for &(_, rv) in &verticals[lo..hi] {
            r = r.max(rv);
        }
        r
    };

    // Walk: at each station the chain enters at the left interval's
    // limit, rises to the envelope VALUE (a step face or rim ring's
    // wider vertex when it stands above both limits), and leaves at
    // the right limit; equal neighbours drop, so a plain vertex
    // states itself once and a shoulder states its vertical pair.
    // Between stations, the open interval bisects on the convex
    // sagitta law, left half before right, so the walk is born in
    // order - a global sort could not tell a shoulder's descending
    // vertical from an up-facing step's ascending one.
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut budget = MAX_SIL_PTS;
    for i in 0..n {
        let s = stations[i];
        let l = if i > 0 { Some(env(i - 1, s)) } else { None };
        let r = if i + 1 < n { Some(env(i, s)) } else { None };
        let here = l.unwrap_or(0.0).max(r.unwrap_or(0.0)).max(v_at(s));
        if let Some(v) = l.filter(|&v| v < here) {
            pts.push((s, v));
        }
        pts.push((s, here));
        if let Some(v) = r.filter(|&v| v < here) {
            pts.push((s, v));
        }
        // The interval's own chord endpoints - what the sagitta
        // recursion subdivides against; the stitch's `here` may
        // stand above them (a step face) and must not inflate
        // their chord.
        let Some(f0) = r else { break };
        bisect(
            &crosses[i],
            &edges,
            f,
            s,
            stations[i + 1],
            f0,
            env(i, stations[i + 1]),
            tol,
            &mut pts,
            &mut budget,
        )?;
    }
    Ok(pts)
}

/// The interval's sagitta recursion: midpoint first, halves after,
/// only when the chord still stands over the envelope by more than
/// `tol`. Convexity says the error never needs a sign test.
#[allow(clippy::too_many_arguments)]
fn bisect(
    set: &[usize],
    edges: &[(Point3, Point3, f64, f64)],
    f: &Frame,
    x0: f64,
    x1: f64,
    f0: f64,
    f1: f64,
    tol: f64,
    pts: &mut Vec<(f64, f64)>,
    budget: &mut usize,
) -> Result<(), GenError> {
    let m = x0 + (x1 - x0) * 0.5;
    if m <= x0 || m >= x1 {
        return Ok(()); // no float room left inside the interval
    }
    if *budget == 0 {
        return Err(GenError::Invalid(format!(
            "the turned profile does not settle its chord at tolerance {tol} - \
             coarsen the model or widen the tolerance"
        )));
    }
    let mut fm: f64 = 0.0;
    for &k in set {
        let (a, b, lo, hi) = edges[k];
        let t = ((m - lo) / (hi - lo)).clamp(0.0, 1.0);
        fm = fm.max(
            f.sr(Point3::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
                a.z + (b.z - a.z) * t,
            ))
            .1,
        );
    }
    *budget -= 1;
    if (f0 + f1) * 0.5 - fm > tol {
        bisect(set, edges, f, x0, m, f0, fm, tol, pts, budget)?;
        pts.push((m, fm));
        bisect(set, edges, f, m, x1, fm, f1, tol, pts, budget)?;
    } else {
        pts.push((m, fm));
    }
    Ok(())
}

fn normalize(v: Point3) -> Option<Point3> {
    let n2 = v.dot(v);
    if n2.is_finite() && n2 > 0.0 {
        let n = n2.sqrt();
        Some(Point3::new(v.x / n, v.y / n, v.z / n))
    } else {
        None
    }
}
