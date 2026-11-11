//! The mate solver behind the fixture locate (#561): up to three mates,
//! each a face or a point on the part against a face or a point on the
//! fixture, solved as the shop's 3-2-1 - the primary face orients the
//! part (its normal opposed to the fixture face's), the secondary face
//! turns it about that normal, and every mate then pins the translation
//! along the directions still free, in mate order. Pure: datums in, one
//! rigid transform (setup frame) out, or a refusal naming the mate.
//!
//! Provenance: the 3-2-1 locating principle (a primary plane removes
//! three freedoms, a secondary two, a tertiary one) from machine-tool and
//! fixture-design handbooks; the shortest-arc quaternion between two unit
//! vectors and the axis-angle quaternion are textbook rigid-body algebra
//! (Shoemake, SIGGRAPH 1985); Gram-Schmidt for the locked directions.
//! Our own code, no vendor material.

use crate::geometry::{Point3, normalize_quat, quat_mul, quat_rotate_point};
use crate::pick::MateDatum;
use crate::schema::Location;

/// Two normals agree (or a face's tilt matches) this closely (sine).
pub const NORMAL_TOL: f64 = 1e-4;
/// A mate that the earlier ones already fix must hold this closely (mm).
pub const LEN_TOL_MM: f64 = 1e-4;
/// Most mates a location takes: three fix every freedom (3-2-1).
pub const MAX_MATES: usize = 3;

/// The typed nudge applied after the mates: a turn about the primary
/// mate's normal (through its fixture datum), then a shift along the
/// setup's X / Y / Z. Moves the part relative to the fixture - the same
/// relative placement whichever of the two is moved.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LocateOffset {
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    pub turn_deg: f64,
}

impl LocateOffset {
    #[must_use]
    pub fn is_finite(&self) -> bool {
        [self.dx, self.dy, self.dz, self.turn_deg].iter().all(|v| v.is_finite())
    }
}

/// How much turn the mates so far leave free.
#[derive(Clone, Copy)]
enum Spin {
    Free,
    /// Only a turn about this unit axis (the primary face's normal).
    About([f64; 3]),
    Locked,
}

/// The rigid transform (setup frame) that carries the part's datums onto
/// the fixture's: `mates` pairs (part datum, fixture datum), in order.
pub fn solve(mates: &[(MateDatum, MateDatum)], offset: LocateOffset) -> Result<Location, String> {
    if mates.is_empty() {
        return Err("pick at least one mate - a part face or point, then the fixture face or point it sits on".into());
    }
    if mates.len() > MAX_MATES {
        return Err(format!("three mates fully locate a part (floor, jaw, stop) - remove the {} extra", mates.len() - MAX_MATES));
    }
    if !offset.is_finite() {
        return Err("the offset and turn must be finite numbers".into());
    }
    refuse_repeats(mates)?;
    let rot = orient(mates)?;
    let mut t = Translate::default();
    for (i, (p, f)) in mates.iter().enumerate() {
        let k = i + 1;
        match (p, f) {
            (MateDatum::Plane { at: ap, .. }, MateDatum::Plane { at: af, normal: nf }) => {
                let want = dot(*nf, sub(*af, turn(rot, *ap)));
                t.row(*nf, want).map_err(|off| conflict(k, "faces", off))?;
            }
            (MateDatum::Point(a), MateDatum::Point(b)) => {
                let want = sub(*b, turn(rot, *a));
                for (axis, w) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].into_iter().zip(want) {
                    t.row(axis, w).map_err(|off| conflict(k, "points", off))?;
                }
            }
            (MateDatum::Point(a), MateDatum::Plane { at: af, normal: nf }) => {
                let want = dot(*nf, sub(*af, turn(rot, *a)));
                t.row(*nf, want).map_err(|off| conflict(k, "point and face", off))?;
            }
            (MateDatum::Plane { at: ap, normal: np }, MateDatum::Point(b)) => {
                let m = turn(rot, *np);
                let want = dot(m, sub(*b, turn(rot, *ap)));
                t.row(m, want).map_err(|off| conflict(k, "face and point", off))?;
            }
        }
    }
    // The typed nudge: turn about the primary normal through its fixture
    // datum, then shift.
    let (axis, pivot) = primary(mates);
    let half = 0.5 * offset.turn_deg.to_radians();
    let q = normalize_quat([axis[0] * half.sin(), axis[1] * half.sin(), axis[2] * half.sin(), half.cos()])
        .ok_or("the turn does not make a rotation - check the turn angle")?;
    let rot = normalize_quat(quat_mul(q, rot)).ok_or("the mates do not make a usable turn - re-pick them")?;
    let moved = add(turn(q, sub(t.t, pivot)), pivot);
    let pos = add(moved, [offset.dx, offset.dy, offset.dz]);
    if !pos.iter().chain(&rot).all(|v| v.is_finite()) {
        return Err("the mates do not make a finite placement - re-pick them".into());
    }
    Ok(Location { pos: Point3::new(pos[0], pos[1], pos[2]), rot })
}

/// The turn the face-to-face mates fix, in mate order: the first opposes
/// its normals by the shortest turn, the next turns about that normal,
/// any later one must already agree. Point mates never turn the part.
fn orient(mates: &[(MateDatum, MateDatum)]) -> Result<[f64; 4], String> {
    let mut rot = [0.0, 0.0, 0.0, 1.0];
    let mut spin = Spin::Free;
    for (i, (p, f)) in mates.iter().enumerate() {
        let (MateDatum::Plane { normal: np, .. }, MateDatum::Plane { normal: nf, .. }) = (p, f) else { continue };
        let k = i + 1;
        let cur = turn(rot, *np);
        let target = scale(*nf, -1.0);
        match spin {
            Spin::Free => {
                rot = normalize_quat(quat_mul(shortest_arc(cur, target), rot))
                    .ok_or_else(|| format!("mate {k}: the faces do not make a usable turn - re-pick them"))?;
                spin = Spin::About(*nf);
            }
            Spin::About(axis) => {
                let (ca, ta) = (dot(cur, axis), dot(target, axis));
                if (ca - ta).abs() > NORMAL_TOL {
                    return Err(format!(
                        "mate {k}: the faces cannot meet face to face while the first face mate holds - they stand at \
                         different angles to it; pick faces that match (a jaw square to the floor takes a side square to the bottom)"
                    ));
                }
                let c = sub(cur, scale(axis, ca));
                let g = sub(target, scale(axis, ta));
                if norm(c) > NORMAL_TOL && norm(g) > NORMAL_TOL {
                    let theta = dot(axis, cross(c, g)).atan2(dot(c, g));
                    let h = 0.5 * theta;
                    let q = [axis[0] * h.sin(), axis[1] * h.sin(), axis[2] * h.sin(), h.cos()];
                    rot = normalize_quat(quat_mul(q, rot))
                        .ok_or_else(|| format!("mate {k}: the faces do not make a usable turn - re-pick them"))?;
                    spin = Spin::Locked;
                }
                // Else parallel to the first face: no turn to take; its
                // translation row decides whether it holds.
            }
            Spin::Locked => {
                if norm(sub(cur, target)) > NORMAL_TOL {
                    return Err(format!(
                        "mate {k}: the faces cannot meet face to face - the earlier mates already fix the part's turn"
                    ));
                }
            }
        }
    }
    Ok(rot)
}

/// The turn axis and pivot of the typed nudge: the first face mate's
/// fixture normal and point; else +Z through mate 1's fixture datum.
fn primary(mates: &[(MateDatum, MateDatum)]) -> ([f64; 3], [f64; 3]) {
    let face = mates.iter().find_map(|(p, f)| match (p, f) {
        (MateDatum::Plane { .. }, MateDatum::Plane { at, normal }) => Some((*normal, *at)),
        _ => None,
    });
    face.unwrap_or_else(|| {
        let at = match mates.first().map(|m| m.1) {
            Some(MateDatum::Plane { at, .. } | MateDatum::Point(at)) => at,
            None => [0.0; 3],
        };
        ([0.0, 0.0, 1.0], at)
    })
}

/// Degenerate picks: two mates on one part point (or one fixture point),
/// which add nothing and hide a mis-pick.
fn refuse_repeats(mates: &[(MateDatum, MateDatum)]) -> Result<(), String> {
    let same = |a: &MateDatum, b: &MateDatum| match (a, b) {
        (MateDatum::Point(x), MateDatum::Point(y)) => norm(sub(*x, *y)) <= LEN_TOL_MM,
        (MateDatum::Plane { at: x, normal: m }, MateDatum::Plane { at: y, normal: n }) => {
            norm(sub(*m, *n)) <= NORMAL_TOL && dot(*m, sub(*x, *y)).abs() <= LEN_TOL_MM
        }
        _ => false,
    };
    for (j, (pj, fj)) in mates.iter().enumerate() {
        for (i, (pi, fi)) in mates.iter().enumerate().take(j) {
            let side = if same(pi, pj) {
                "part"
            } else if same(fi, fj) {
                "fixture"
            } else {
                continue;
            };
            return Err(format!(
                "mate {} picks the same {side} point or face as mate {} (coincident) - pick a distinct one",
                j + 1,
                i + 1
            ));
        }
    }
    Ok(())
}

fn conflict(k: usize, what: &str, off: f64) -> String {
    format!(
        "mate {k}: the {what} cannot touch while the earlier mates hold - {off:.3} mm apart; \
         check the picks, or their order (the floor first, then the jaw, then the stop)"
    )
}

/// The translation, pinned one direction at a time.
#[derive(Default)]
struct Translate {
    t: [f64; 3],
    /// Orthonormal directions already pinned by earlier rows.
    locked: Vec<[f64; 3]>,
}

impl Translate {
    /// Make `m . t == want` (`m` unit) by moving only along directions no
    /// earlier row pinned; Err(miss in mm) when none is left and it
    /// does not already hold.
    fn row(&mut self, m: [f64; 3], want: f64) -> Result<(), f64> {
        let d = want - dot(m, self.t);
        let mut f = m;
        for l in &self.locked {
            f = sub(f, scale(*l, dot(f, *l)));
        }
        let fl = norm(f);
        if !(fl.is_finite() && d.is_finite()) {
            return Err(f64::NAN);
        }
        if fl <= NORMAL_TOL {
            return if d.abs() <= LEN_TOL_MM { Ok(()) } else { Err(d.abs()) };
        }
        let fh = scale(f, 1.0 / fl);
        self.t = add(self.t, scale(fh, d / fl));
        self.locked.push(fh);
        Ok(())
    }
}

/// The shortest turn taking unit `from` onto unit `to`; a half turn about
/// an axis square to `from` when they are opposed.
fn shortest_arc(from: [f64; 3], to: [f64; 3]) -> [f64; 4] {
    let w = 1.0 + dot(from, to);
    if w > 1e-12 {
        let v = cross(from, to);
        return [v[0], v[1], v[2], w];
    }
    let axis = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .map(|a| sub(a, scale(from, dot(a, from))))
        .max_by(|a, b| norm(*a).total_cmp(&norm(*b)))
        .unwrap_or([1.0, 0.0, 0.0]);
    let l = norm(axis);
    [axis[0] / l, axis[1] / l, axis[2] / l, 0.0]
}

fn turn(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let r = quat_rotate_point(q, Point3::new(v[0], v[1], v[2]));
    [r.x, r.y, r.z]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
