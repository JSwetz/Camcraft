//! The signature of an operation's cached path (issue #109 review B1):
//! a bit-exact hash of every field a move carries, computed ONCE when a
//! cache is built ([`crate::schema::Toolpath::new`]) so the per-frame
//! staleness question reads a stored word instead of cloning and
//! hashing every predecessor's path.
//!
//! Provenance: our own design (a content hash memoised at the write, the
//! same idea as M5.3 A's geometry signature); no vendor material.

use crate::cl::Move;

/// Identity of a cached path: every field a move carries, bit-exact.
/// The generators are deterministic functions of what the stamp
/// digests, so equal paths here means the rest mask builds identically.
pub fn of(moves: &[Move]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"camcraft-rest-path-v1");
    h.write_u64(moves.len() as u64);
    for m in moves {
        h.write_u8(match m.kind {
            crate::cl::MoveKind::Rapid => 0,
            crate::cl::MoveKind::Feed => 1,
            crate::cl::MoveKind::Plunge => 2,
        });
        for p in [m.from, m.to] {
            h.write(&p.x.to_bits().to_le_bytes());
            h.write(&p.y.to_bits().to_le_bytes());
            h.write(&p.z.to_bits().to_le_bytes());
        }
        match m.feed {
            Some(f) => {
                h.write_u8(1);
                h.write(&f.to_bits().to_le_bytes());
            }
            None => h.write_u8(0),
        }
        match &m.arc {
            Some(a) => {
                h.write_u8(1);
                h.write(&a.center.x.to_bits().to_le_bytes());
                h.write(&a.center.y.to_bits().to_le_bytes());
                h.write_u8(u8::from(a.ccw));
                // M11.2 A: only a vertical plane adds a byte, so every
                // XY arc hashes exactly as it always did.
                if !a.plane.is_xy() {
                    h.write_u8(a.plane.g_code());
                }
            }
            None => h.write_u8(0),
        }
    }
    h.finish()
}
