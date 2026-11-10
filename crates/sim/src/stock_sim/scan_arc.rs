//! The envelope scan's exact XY arcs (#413): a G2 / G3's ball-centre
//! path read on the TRUE circle, piece by piece, beside the chord each
//! piece keeps for indexing. The scan-side twin of `dexel_arc` (#402)
//! and `field_arc`. Split beside `scan.rs` by the 400-line law.
//!
//! Provenance: RS-274's helical interpolation (Z linear in the swept
//! angle), the circle-point distance law of plane geometry, the
//! point-triangle distance of Ericson's "Real-Time Collision Detection"
//! (`gouge::dist_pt_tri`), and `dexel_arc`'s piece search. Nothing
//! from vendor material.
//!
//! ## The law here
//!
//! - **The chord indexes, the arc measures.** Each piece keeps the
//!   chord `arc_chords` would have drawn (the same count, the same
//!   points), so every bin, box and ring id the scan builds is
//!   unchanged; but every distance the verdict reads off a piece (to
//!   a lattice point for rest, flank and hidden rest; to a facet for
//!   gouge) is read off the arc. An inscribed chord stood a sagitta inside:
//!   a tangent ride round a convex boss read up to `tol/4` of phantom
//!   gouge, a concave fillet's over-cut hid by as much.
//! - **The bulge bounds what the chord can miss.** Every arc point
//!   lies within `sag + |dz|` of its chord (the perpendicular foot
//!   lands on the chord, Z differs by at most the piece's change; a
//!   piece past half a turn takes the diameter instead), so
//!   a box, a bin walk or a ring prune grown by `bulge` keeps every
//!   piece it held as a chord, and a chord already `bulge` past the
//!   ball cannot hide a contact.
//! - **Level is closed form; helical is searched.** A level piece's
//!   nearest point to a lattice point is at the point's bearing clamped
//!   into the piece. A helical piece, and every facet distance, is a
//!   1-D minimum over the piece found by samples and a golden section
//!   (`dexel_arc::piece_min`): real evaluations only.

use super::dexel_arc::{piece_min, ArcSweep};
use super::gouge::dist_pt_tri;
use super::scan::dist_pt_seg3;

use crate::cl::{arc_chords, ArcMove, Move};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use std::f64::consts::{PI, TAU};

/// One piece of an XY arc's centre path.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScanArc {
    ox: f64,
    oy: f64,
    rad: f64,
    b0: f64,
    dir: f64,
    span: f64,
    z0: f64,
    z1: f64,
    sag: f64,
}

impl ScanArc {
    fn at(&self, t: f64) -> Point3 {
        let b = self.b0 + self.dir * t;
        Point3::new(self.ox + self.rad * b.cos(), self.oy + self.rad * b.sin(), self.z0 + (self.z1 - self.z0) * (t / self.span))
    }

    /// How far the piece can stand from its chord.
    pub(super) fn bulge(&self) -> f64 {
        self.sag + (self.z1 - self.z0).abs()
    }

    /// The distance from `q` to the piece: closed form when level.
    pub(super) fn dist_pt(&self, q: Point3) -> f64 {
        let d = |p: Point3| (q.x - p.x).hypot(q.y - p.y).hypot(q.z - p.z);
        if self.z0 != self.z1 {
            return piece_min(&|t: f64| d(self.at(t)), 0.0, self.span);
        }
        let (wx, wy) = (q.x - self.ox, q.y - self.oy);
        let t = if wx == 0.0 && wy == 0.0 {
            0.0 // on the axis: every point is as near
        } else {
            let u = (self.dir * (wy.atan2(wx) - self.b0)).rem_euclid(TAU);
            if u <= self.span {
                u
            } else if u - self.span < TAU - u {
                self.span
            } else {
                0.0
            }
        };
        d(self.at(t))
    }

    /// The distance from the piece to a facet.
    pub(super) fn dist_tri(&self, tri: &Triangle) -> f64 {
        piece_min(&|t: f64| dist_pt_tri(self.at(t), tri), 0.0, self.span)
    }
}

/// The distance from `q` to segment `seg`, read off its arc when it
/// has one.
pub(super) fn seg_dist(q: Point3, seg: &(Point3, Point3), arc: Option<&ScanArc>) -> f64 {
    match arc {
        Some(a) => a.dist_pt(q),
        None => dist_pt_seg3(q.x, q.y, q.z, seg.0, seg.1),
    }
}

/// The largest bulge among the pieces (0 for an all-straight path).
pub(super) fn max_bulge(arcs: &[Option<ScanArc>]) -> f64 {
    arcs.iter().flatten().map(ScanArc::bulge).fold(0.0, f64::max)
}

/// The centre path of a vertical-axis XY arc `m`, lifted by `lift`:
/// `arc_chords`' chords with Z interpolated, each with its arc piece.
/// A degenerate arc (`ArcSweep::of` says none) is its lone chord,
/// measured as a segment.
pub(super) fn xy_arc_pieces(m: &Move, arc: ArcMove, lift: f64, chord_tol: f64) -> Vec<((Point3, Point3), Option<ScanArc>)> {
    let a2 = Point2::new(m.from.x, m.from.y);
    let b2 = Point2::new(m.to.x, m.to.y);
    let cs = arc_chords(a2, b2, arc, chord_tol);
    let n = cs.len().max(1);
    let dz = m.to.z - m.from.z;
    let z = |j: usize| m.from.z + dz * (j as f64 / n as f64) + lift;
    let sw = ArcSweep::of(m, arc);
    cs.into_iter()
        .enumerate()
        .map(|(j, (ca, cb))| {
            let seg = (Point3::new(ca.x, ca.y, z(j)), Point3::new(cb.x, cb.y, z(j + 1)));
            let piece = sw.as_ref().map(|s| {
                let step = s.span / n as f64;
                ScanArc {
                    ox: s.ox,
                    oy: s.oy,
                    rad: s.rad,
                    b0: s.a0 + s.dir * step * j as f64,
                    dir: s.dir,
                    span: step,
                    z0: z(j),
                    z1: z(j + 1),
                    // Past half a turn the chord no longer holds the
                    // arc's feet: the circle's diameter bounds it.
                    sag: if step <= PI { s.rad * (1.0 - (step * 0.5).cos()) } else { 2.0 * s.rad },
                }
            });
            (seg, piece)
        })
        .collect()
}
