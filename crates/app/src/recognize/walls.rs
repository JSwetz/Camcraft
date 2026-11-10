//! The wall read of a floor (#35): which spans of a level face's outer
//! ring have a wall rising from them into the face (a concave edge, the
//! depression rule), and how high the walls standing on it reach.
//!
//! Provenance: our own code over this repo's welded mesh topology; the
//! concave-edge rule after Joshi & Chang (CAD 20(2), 1988), see
//! `recognize`. No vendor material consulted.

use crate::geo2d::Point2;
use crate::geometry::feature::{Face, Features, Loop};
use crate::geometry::{transform_point, Point3, Triangle};
use crate::schema::Location;
use std::collections::{HashMap, VecDeque};

/// Weld lattice, 1 µm - the factor `geometry::feature` welds on, so a
/// ring vertex of the feature topology keys to the same lattice point
/// as the facet corners it was welded from.
const S: f64 = 1e6;
/// A face this close to +Z is level (the `list_features` "up" word).
const LEVEL: f64 = 0.999_999;
/// Height slack above a floor before a facet counts as standing on it.
const Z_TOL: f64 = 1e-4;
/// The least horizontal lean (unit normal toward the floor) a facet
/// needs to count as a wall facing into it: a vertical wall reads 1, a
/// first fillet strip still reads well above this.
const WALL_LEAN: f64 = 0.05;
/// Smallest depth worth calling a pocket, mm.
const MIN_DEPTH: f64 = 1e-3;

type Key = (i64, i64, i64);

fn key(p: Point3) -> Option<Key> {
    let q = |v: f64| {
        let s = (v * S).round();
        (s.is_finite() && s.abs() < 9.0e15).then_some(s as i64)
    };
    Some((q(p.x)?, q(p.y)?, q(p.z)?))
}

fn edge_key(a: Key, b: Key) -> (Key, Key) {
    if a <= b { (a, b) } else { (b, a) }
}

/// One floor's ring, read for walls.
pub(super) struct Floor {
    /// The outer ring, world XY, counter-clockwise from above.
    pub(super) ring: Vec<Point2>,
    /// `wall[i]`: a wall stands on the span `ring[i] -> ring[i + 1]`.
    pub(super) wall: Vec<bool>,
    pub(super) z: f64,
    pub(super) top: f64,
    pub(super) area: f64,
}

/// The facets in world space with their winding normals and a welded
/// edge -> facets map (keys in the model's local frame, where the
/// feature topology welds).
pub(super) struct Mesh {
    world: Vec<[Point3; 3]>,
    normal: Vec<Option<Point3>>,
    keys: Vec<Option<[Key; 3]>>,
    edges: HashMap<(Key, Key), Vec<u32>>,
    /// Facets with three distinct finite corners and a normal.
    pub(super) usable: usize,
}

impl Mesh {
    pub(super) fn new(tris: &[Triangle], loc: &Location) -> Self {
        let mut m = Mesh {
            world: Vec::with_capacity(tris.len()),
            normal: Vec::with_capacity(tris.len()),
            keys: Vec::with_capacity(tris.len()),
            edges: HashMap::new(),
            usable: 0,
        };
        for (i, t) in tris.iter().enumerate() {
            let w = [t.a, t.b, t.c].map(|p| transform_point(p, loc.pos, loc.rot));
            let ks = match (key(t.a), key(t.b), key(t.c)) {
                (Some(a), Some(b), Some(c)) if a != b && b != c && a != c => Some([a, b, c]),
                _ => None,
            };
            let n = ks.and_then(|_| unit_normal(&w));
            if let (Some(ks), Some(_)) = (ks, n) {
                m.usable += 1;
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    m.edges.entry(edge_key(ks[a], ks[b])).or_default().push(i as u32);
                }
            }
            m.world.push(w);
            m.normal.push(n);
            m.keys.push(if n.is_some() { ks } else { None });
        }
        m
    }

    /// `face` (index `fi`) read as a floor: level, looking up, with at
    /// least one wall on its outer ring. None for anything else.
    pub(super) fn floor_of(&self, f: &Features, fi: usize, face: &Face, outer: &Loop) -> Option<Floor> {
        let up = face.facets.iter().find_map(|&t| *self.normal.get(t as usize)?)?;
        if up.z < LEVEL {
            return None;
        }
        // The ring's lattice keys in the local frame, where it welded.
        let mut keys = Vec::with_capacity(outer.verts.len());
        for &v in &outer.verts {
            let p = f.verts.get(v as usize)?;
            keys.push(key(Point3::new(p[0], p[1], p[2]))?);
        }
        if keys.len() < 3 {
            return None;
        }
        // World positions: each ring vertex read off the facet corner
        // that welds to it, so the placement is the facets' own.
        let world = self.world_ring(face, &keys)?;
        let z = world.iter().map(|p| p.z).sum::<f64>() / world.len() as f64;
        if !z.is_finite() {
            return None;
        }
        let mut pts: Vec<Point2> = world.iter().map(|p| Point2::new(p.x, p.y)).collect();
        let signed = super::shape::signed_area(&pts);
        if !signed.is_finite() || signed.abs() < 1e-9 {
            return None;
        }
        if signed < 0.0 {
            pts.reverse();
            keys.reverse();
        }
        let n = pts.len();
        let mut wall = Vec::with_capacity(n);
        let mut seeds = Vec::new();
        for i in 0..n {
            let (Some(&ka), Some(&kb), Some(pa), Some(pb)) = (keys.get(i), keys.get((i + 1) % n), pts.get(i), pts.get((i + 1) % n)) else {
                return None;
            };
            let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
            let len = dx.hypot(dy);
            let mut is_wall = false;
            if len > 0.0 && len.is_finite() {
                let left = (-dy / len, dx / len);
                for &t in self.edges.get(&edge_key(ka, kb)).map_or(&[][..], |v| v.as_slice()) {
                    if f.facet_face.get(t as usize).is_some_and(|&g| g as usize == fi) {
                        continue;
                    }
                    if self.rises_into(t, z, left) {
                        is_wall = true;
                        seeds.push(t);
                    }
                }
            }
            wall.push(is_wall);
        }
        if seeds.is_empty() {
            return None;
        }
        let top = self.wall_top(&seeds, z, &pts, f, fi)?;
        if top - z < MIN_DEPTH {
            return None;
        }
        Some(Floor { ring: pts, wall, z, top, area: face.area })
    }

    /// The ring's vertices in world space, read off the facet corners
    /// that weld to them (every ring vertex is a corner of the face).
    fn world_ring(&self, face: &Face, keys: &[Key]) -> Option<Vec<Point3>> {
        let mut at: HashMap<Key, Point3> = HashMap::new();
        for &t in &face.facets {
            let (Some(Some(ks)), Some(w)) = (self.keys.get(t as usize), self.world.get(t as usize)) else {
                continue;
            };
            for (k, p) in ks.iter().zip(w.iter()) {
                at.entry(*k).or_insert(*p);
            }
        }
        keys.iter().map(|k| at.get(k).copied()).collect()
    }

    /// Facet `t` stands on the floor at `z` and leans into it.
    fn rises_into(&self, t: u32, z: f64, left: (f64, f64)) -> bool {
        let (Some(Some(n)), Some(w)) = (self.normal.get(t as usize), self.world.get(t as usize)) else {
            return false;
        };
        let cz = (w[0].z + w[1].z + w[2].z) / 3.0;
        n.z < LEVEL && cz > z + Z_TOL && n.x * left.0 + n.y * left.1 > WALL_LEAN
    }

    /// How high the walls reach: a flood from the wall facets over every
    /// facet that is neither level nor below the floor, held near the
    /// floor's footprint so it cannot wander over the rest of the part.
    fn wall_top(&self, seeds: &[u32], z: f64, ring: &[Point2], f: &Features, fi: usize) -> Option<f64> {
        let (mut lo, mut hi) = ((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY));
        for p in ring {
            lo = (lo.0.min(p.x), lo.1.min(p.y));
            hi = (hi.0.max(p.x), hi.1.max(p.y));
        }
        let margin = 0.25 * (hi.0 - lo.0).max(hi.1 - lo.1) + 1.0;
        if !margin.is_finite() {
            return None;
        }
        let near = |w: &[Point3; 3]| {
            let (cx, cy) = ((w[0].x + w[1].x + w[2].x) / 3.0, (w[0].y + w[1].y + w[2].y) / 3.0);
            cx >= lo.0 - margin && cx <= hi.0 + margin && cy >= lo.1 - margin && cy <= hi.1 + margin
        };
        let mut seen = vec![false; self.world.len()];
        let mut queue: VecDeque<u32> = VecDeque::new();
        let mut top = f64::NEG_INFINITY;
        for &s in seeds {
            if let Some(v) = seen.get_mut(s as usize)
                && !*v
            {
                *v = true;
                queue.push_back(s);
            }
        }
        while let Some(t) = queue.pop_front() {
            let (Some(Some(ks)), Some(w)) = (self.keys.get(t as usize), self.world.get(t as usize)) else {
                continue;
            };
            top = top.max(w[0].z.max(w[1].z).max(w[2].z));
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                for &g in self.edges.get(&edge_key(ks[a], ks[b])).map_or(&[][..], |v| v.as_slice()) {
                    let Some(v) = seen.get_mut(g as usize) else { continue };
                    if *v || f.facet_face.get(g as usize).is_some_and(|&h| h as usize == fi) {
                        continue;
                    }
                    let (Some(Some(n)), Some(gw)) = (self.normal.get(g as usize), self.world.get(g as usize)) else {
                        continue;
                    };
                    let min_z = gw[0].z.min(gw[1].z).min(gw[2].z);
                    if n.z.abs() < LEVEL && min_z >= z - Z_TOL && near(gw) {
                        *v = true;
                        queue.push_back(g);
                    }
                }
            }
        }
        top.is_finite().then_some(top)
    }
}

fn unit_normal(w: &[Point3; 3]) -> Option<Point3> {
    let (u, v) = (w[1] - w[0], w[2] - w[0]);
    let n = Point3::new(u.y * v.z - u.z * v.y, u.z * v.x - u.x * v.z, u.x * v.y - u.y * v.x);
    let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
    (len.is_finite() && len > 1e-12).then(|| Point3::new(n.x / len, n.y / len, n.z / len))
}
